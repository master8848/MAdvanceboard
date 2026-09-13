//! Session logger: accepted/rejected events to JSONL, export/import.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::personal::now_ts;
use crate::store::{SESSION_RING_CAP, SESSION_TABLE_KEEP};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SessionEvent {
    pub ts: i64,
    /// Digit sequence that produced the candidates.
    pub seq: String,
    /// Previous word context.
    pub ctx: String,
    /// Word the user committed (accepted) or dismissed (rejected).
    pub chosen: String,
    /// Candidates that were shown.
    pub shown: Vec<String>,
    /// `"accepted"` or `"rejected"`.
    pub action: String,
}

#[derive(Clone, Debug, Default)]
pub struct SessionLogger {
    /// In-memory ring: the last [`SESSION_RING_CAP`] events. Bounded so a
    /// long typing session can never grow RAM without limit; SQLite holds
    /// the deeper window ([`SESSION_TABLE_KEEP`]).
    events: Vec<SessionEvent>,
    /// Events not yet flushed to SQLite. Drained by
    /// [`SessionLogger::flush_pending_to_conn`] (called from
    /// [`Store::flush`](crate::store::Store::flush) on the 2 s coalesced
    /// cadence). Capped at [`SESSION_TABLE_KEEP`]; overflow is counted in
    /// `dropped_unflushed` and screamed to stderr — never dropped silently.
    pending: Vec<SessionEvent>,
    dropped_unflushed: u64,
}

impl SessionLogger {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, e: SessionEvent) {
        if self.events.len() >= SESSION_RING_CAP {
            let overflow = self.events.len() + 1 - SESSION_RING_CAP;
            self.events.drain(..overflow);
        }
        self.events.push(e.clone());
        if self.pending.len() as i64 >= SESSION_TABLE_KEEP {
            self.pending.remove(0);
            self.dropped_unflushed += 1;
            eprintln!(
                "kbcore session: pending queue overflow, dropped oldest unflushed event \
                 (total dropped: {}); flush is not keeping up",
                self.dropped_unflushed
            );
        }
        self.pending.push(e);
    }

    pub fn log_accepted(&mut self, seq: &str, ctx: &str, chosen: &str, shown: &[String]) {
        self.push(SessionEvent {
            ts: now_ts(),
            seq: seq.to_string(),
            ctx: ctx.to_string(),
            chosen: chosen.to_string(),
            shown: shown.to_vec(),
            action: "accepted".to_string(),
        });
    }

    pub fn log_rejected(&mut self, seq: &str, ctx: &str, chosen: &str, shown: &[String]) {
        self.push(SessionEvent {
            ts: now_ts(),
            seq: seq.to_string(),
            ctx: ctx.to_string(),
            chosen: chosen.to_string(),
            shown: shown.to_vec(),
            action: "rejected".to_string(),
        });
    }

    /// Export the session as JSONL (one event per line).
    pub fn export_jsonl(&self) -> String {
        let mut out = String::new();
        for e in &self.events {
            if let Ok(line) = serde_json::to_string(e) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }

    /// Import a session: replays the JSONL log, appending events.
    /// Returns the number of events imported.
    pub fn import_jsonl(&mut self, s: &str) -> Result<usize, String> {
        let mut n = 0;
        for (i, line) in s.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let e: SessionEvent =
                serde_json::from_str(line).map_err(|e| format!("line {i}: {e}"))?;
            self.push(e);
            n += 1;
        }
        Ok(n)
    }

    /// Events dropped from the unflushed queue by overflow (see [`Self::push`]).
    /// Non-zero means the 2 s flush is not keeping up — surfaced, never silent.
    pub fn dropped_unflushed(&self) -> u64 {
        self.dropped_unflushed
    }

    /// Number of events awaiting flush.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn events(&self) -> &[SessionEvent] {
        &self.events
    }

    // ---- SQLite event-log table (same DB file as the personal store) ----
    //
    // Single-writer discipline: the one `Store` connection behind
    // `Predictor`'s `Mutex` is the only writer. Events are INSERTed
    // append-only (never `DELETE` + rewrite); the table is pruned to the
    // newest [`SESSION_TABLE_KEEP`] rows after each flush. JSONL remains
    // the interchange/export format; this table is local persistence only.

    fn open_db(path: &Path) -> Result<rusqlite::Connection, String> {
        let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(crate::personal::PersonalDict::SQLITE_SCHEMA)
            .map_err(|e| e.to_string())?;
        Ok(conn)
    }

    /// Insert one event row on an open connection (append-only).
    fn insert_event(
        conn: &rusqlite::Connection,
        e: &SessionEvent,
    ) -> Result<(), String> {
        let shown = serde_json::to_string(&e.shown).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO session_events
             (ts, seq, ctx, chosen, shown_json, action)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![e.ts, e.seq, e.ctx, e.chosen, shown, e.action],
        )
        .map_err(|e| format!("session: append event: {e}"))?;
        Ok(())
    }

    /// Prune the table to the newest [`SESSION_TABLE_KEEP`] rows.
    fn prune(conn: &rusqlite::Connection) -> Result<(), String> {
        conn.execute(
            "DELETE FROM session_events WHERE id NOT IN
             (SELECT id FROM session_events ORDER BY id DESC LIMIT ?1)",
            rusqlite::params![SESSION_TABLE_KEEP],
        )
        .map_err(|e| format!("session: prune beyond {SESSION_TABLE_KEEP}: {e}"))?;
        Ok(())
    }

    /// Flush pending events to an already-open connection (see
    /// [`Store::flush`](crate::store::Store::flush)). Returns rows written.
    /// The pending queue drains only after commit; on `Err` it is retained
    /// for retry — no silent loss.
    pub(crate) fn flush_pending_to_conn(
        &mut self,
        conn: &mut rusqlite::Connection,
    ) -> Result<usize, String> {
        if self.pending.is_empty() {
            return Ok(0);
        }
        let tx = conn
            .transaction()
            .map_err(|e| format!("session: begin flush tx: {e}"))?;
        for e in &self.pending {
            Self::insert_event(&tx, e)?;
        }
        let n = self.pending.len();
        Self::prune(&tx)?;
        tx.commit()
            .map_err(|e| format!("session: commit flush tx: {e}"))?;
        self.pending.clear();
        Ok(n)
    }

    /// Persist pending in-memory events to the `session_events` table
    /// (append-only, then prune to the newest rows). Compat path for callers
    /// without a `Store`; the steady path is the coalesced flush via `Store`.
    pub fn save_to_sqlite(&mut self, path: &Path) -> Result<(), String> {
        let mut conn = Self::open_db(path)?;
        self.flush_pending_to_conn(&mut conn)?;
        Ok(())
    }

    /// Load the newest ring window from the `session_events` table
    /// (at most [`SESSION_RING_CAP`] events, oldest first). SQLite is never
    /// on the suggest path — this runs once at startup.
    pub fn load_from_sqlite(path: &Path) -> Result<Self, String> {        let conn = Self::open_db(path)?;
        let mut st = conn
            .prepare(
                "SELECT ts, seq, ctx, chosen, shown_json, action
                 FROM session_events ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(rusqlite::params![SESSION_RING_CAP as i64], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut log = Self::new();
        // DESC + LIMIT reads newest-first; reverse to restore oldest-first
        // ring order. Loaded rows are history, not pending: clear the queue
        // so the next flush does not re-insert them.
        let mut loaded: Vec<SessionEvent> = Vec::new();
        for row in rows {
            let (ts, seq, ctx, chosen, shown_json, action) =
                row.map_err(|e| e.to_string())?;
            let shown: Vec<String> =
                serde_json::from_str(&shown_json).map_err(|e| e.to_string())?;
            loaded.push(SessionEvent {
                ts,
                seq,
                ctx,
                chosen,
                shown,
                action,
            });
        }
        loaded.reverse();
        for e in loaded {
            log.push(e);
        }
        log.pending.clear();
        Ok(log)
    }

    // ---- Analytics queries (plan/14): each served by one secondary index.
    //
    // Hot path never calls these (suggest is pure RAM); they back 14d
    // routine mining, the replay harness, and KSR measurement. Every query
    // below must show `USING INDEX` in `EXPLAIN QUERY PLAN`
    // (locked by `analytics_queries_use_indexes`).

    /// Decode one event row (shared by the analytics readers below).
    fn row_to_event(
        ts: i64,
        seq: String,
        ctx: String,
        chosen: String,
        shown_json: String,
        action: String,
    ) -> Result<SessionEvent, String> {
        let shown: Vec<String> =
            serde_json::from_str(&shown_json).map_err(|e| format!("session: decode shown_json: {e}"))?;
        Ok(SessionEvent { ts, seq, ctx, chosen, shown, action })
    }

    /// 14d routine mining (plan/13 P2) + KSR (plan/09#7): newest `limit`
    /// events newer than `since_ts`. Served by `idx_events_ts`.
    pub fn events_since(
        conn: &rusqlite::Connection,
        since_ts: i64,
        limit: i64,
    ) -> Result<Vec<SessionEvent>, String> {
        let mut st = conn
            .prepare(
                "SELECT ts, seq, ctx, chosen, shown_json, action
                 FROM session_events WHERE ts > ?1 ORDER BY ts DESC LIMIT ?2",
            )
            .map_err(|e| format!("session: events_since prepare: {e}"))?;
        let rows = st
            .query_map(rusqlite::params![since_ts, limit], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| format!("session: events_since query: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            let (ts, seq, ctx, chosen, shown_json, action) =
                row.map_err(|e| format!("session: events_since row: {e}"))?;
            out.push(Self::row_to_event(ts, seq, ctx, chosen, shown_json, action)?);
        }
        Ok(out)
    }

    /// Replay harness (plan/09#1) + lift A/B: newest `limit` events for
    /// one `(chosen, action)` pair. Served by
    /// `idx_events_chosen_action`.
    pub fn events_by_chosen_action(
        conn: &rusqlite::Connection,
        chosen: &str,
        action: &str,
        limit: i64,
    ) -> Result<Vec<SessionEvent>, String> {
        let mut st = conn
            .prepare(
                "SELECT ts, seq, ctx, chosen, shown_json, action
                 FROM session_events
                 WHERE chosen = ?1 AND action = ?2 ORDER BY id DESC LIMIT ?3",
            )
            .map_err(|e| format!("session: events_by_chosen_action prepare: {e}"))?;
        let rows = st
            .query_map(rusqlite::params![chosen, action, limit], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| format!("session: events_by_chosen_action query: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            let (ts, seq, ctx, chosen, shown_json, action) =
                row.map_err(|e| format!("session: events_by_chosen_action row: {e}"))?;
            out.push(Self::row_to_event(ts, seq, ctx, chosen, shown_json, action)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_roundtrip() {
        let mut log = SessionLogger::new();
        log.log_accepted("43556", "say", "hello", &["hello".into(), "hell".into()]);
        log.log_rejected("43556", "say", "hell", &["hello".into()]);
        let s = log.export_jsonl();
        assert_eq!(s.lines().count(), 2);
        let mut log2 = SessionLogger::new();
        assert_eq!(log2.import_jsonl(&s).unwrap(), 2);
        assert_eq!(log2.events()[0].action, "accepted");
        assert_eq!(log2.events()[1].chosen, "hell");
    }

    #[test]
    fn import_rejects_garbage() {
        let mut log = SessionLogger::new();
        assert!(log.import_jsonl("not json\n").is_err());
        assert!(log.is_empty());
    }

    #[test]
    fn ring_caps_memory_at_500() {
        let mut log = SessionLogger::new();
        for i in 0..(SESSION_RING_CAP + 50) {
            log.log_accepted("1", "", &format!("w{i}"), &[]);
        }
        assert_eq!(log.len(), SESSION_RING_CAP);
        // Ring keeps the newest: first survivor is w50.
        assert_eq!(log.events()[0].chosen, "w50");
        assert_eq!(log.events()[SESSION_RING_CAP - 1].chosen, "w549");
    }

    #[test]
    fn sqlite_event_log_roundtrip() {
        let path = std::env::temp_dir().join("kbcore_session_test.sqlite");
        let _ = std::fs::remove_file(&path);
        let mut log = SessionLogger::new();
        log.log_accepted("43556", "say", "hello", &["hello".into(), "hell".into()]);
        log.log_rejected("43556", "say", "hell", &["hello".into()]);
        log.save_to_sqlite(&path).unwrap();
        let loaded = SessionLogger::load_from_sqlite(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.events()[0].action, "accepted");
        assert_eq!(loaded.events()[1].chosen, "hell");
        // JSONL interchange format unchanged.
        assert_eq!(loaded.export_jsonl(), log.export_jsonl().lines().fold(
            String::new(),
            |mut acc, l| {
                // Compare semantically: ts differs? No — ts persisted, so
                // the JSONL must round-trip exactly.
                acc.push_str(l);
                acc.push('\n');
                acc
            }
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn append_only_save_then_prune() {        // Two saves must accumulate (append-only, no DELETE+rewrite), and a
        // flush past the keep window must prune the oldest rows.
        let path = std::env::temp_dir().join("kbcore_session_append.sqlite");
        let _ = std::fs::remove_file(&path);
        let mut log = SessionLogger::new();
        log.log_accepted("1", "", "first", &[]);
        log.save_to_sqlite(&path).unwrap();
        log.log_accepted("1", "", "second", &[]);
        log.save_to_sqlite(&path).unwrap();
        let loaded = SessionLogger::load_from_sqlite(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.events()[1].chosen, "second");
        // Loaded history is not pending: a save right after load writes 0 rows.
        let mut loaded = loaded;
        assert_eq!(loaded.pending_len(), 0);
        loaded.save_to_sqlite(&path).unwrap();
        let reloaded = SessionLogger::load_from_sqlite(&path).unwrap();
        assert_eq!(reloaded.len(), 2, "reload-save must not duplicate rows");
        let _ = std::fs::remove_file(&path);
    }

    /// Plan/14 acceptance, part 3: session analytics queries are
    /// index-served. Volume is realistic (3k rows + ANALYZE) so the
    /// planner's choice is meaningful.
    #[test]
    fn analytics_queries_use_indexes() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::personal::PersonalDict::SQLITE_SCHEMA)
            .unwrap();
        {
            let tx = conn.transaction().unwrap();
            {
                let mut ins = tx
                    .prepare(
                        "INSERT INTO session_events
                         (ts, seq, ctx, chosen, shown_json, action)
                         VALUES (?1, ?2, ?3, ?4, '[]', ?5)",
                    )
                    .unwrap();
                for i in 0..3000 {
                    ins.execute(rusqlite::params![
                        1_700_000_000i64 + i as i64,
                        "43556",
                        "",
                        format!("w{:03}", i % 100),
                        if i % 2 == 0 { "accepted" } else { "rejected" },
                    ])
                    .unwrap();
                }
            }
            tx.commit().unwrap();
        }
        conn.execute_batch("ANALYZE").unwrap();

        let plan_of = |sql: &str| -> Vec<String> {
            conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .unwrap()
                .query_map([], |r| r.get::<_, String>(3))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        let p = plan_of(
            "SELECT ts, seq, ctx, chosen, shown_json, action
             FROM session_events WHERE ts > 1700002000 ORDER BY ts DESC LIMIT 50",
        );
        assert!(
            p.iter()
                .any(|d| d.contains("USING INDEX") && d.contains("idx_events_ts")),
            "14d routine window must use idx_events_ts: {p:?}",
        );
        let p = plan_of(
            "SELECT ts, seq, ctx, chosen, shown_json, action
             FROM session_events
             WHERE chosen = 'w000' AND action = 'accepted' ORDER BY id DESC LIMIT 50",
        );
        assert!(
            p.iter().any(|d| d.contains("USING INDEX")
                && d.contains("idx_events_chosen_action")),
            "replay lookup must use idx_events_chosen_action: {p:?}",
        );

        // Functional locks on the helpers behind those plans.
        let recent = SessionLogger::events_since(&conn, 1_700_002_950, 100).unwrap();
        assert_eq!(recent.len(), 49);
        assert!(recent.windows(2).all(|w| w[0].ts >= w[1].ts), "ts desc");
        assert!(SessionLogger::events_since(&conn, 1_800_000_000, 10).unwrap().is_empty());
        // Fixture parity: i % 100 == 0 => i even => action 'accepted', so
        // ("w000", "accepted") hits and ("w000", "rejected") is empty.
        let replay =
            SessionLogger::events_by_chosen_action(&conn, "w000", "accepted", 100).unwrap();
        assert_eq!(replay.len(), 30);
        assert!(replay.iter().all(|e| e.chosen == "w000" && e.action == "accepted"));
        assert!(SessionLogger::events_by_chosen_action(&conn, "w000", "rejected", 10)
            .unwrap()
            .is_empty());
    }
}
