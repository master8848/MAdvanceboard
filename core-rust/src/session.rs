//! Session logger: accepted/rejected events to JSONL, export/import.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::personal::now_ts;

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
    events: Vec<SessionEvent>,
}

impl SessionLogger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log_accepted(&mut self, seq: &str, ctx: &str, chosen: &str, shown: &[String]) {
        self.events.push(SessionEvent {
            ts: now_ts(),
            seq: seq.to_string(),
            ctx: ctx.to_string(),
            chosen: chosen.to_string(),
            shown: shown.to_vec(),
            action: "accepted".to_string(),
        });
    }

    pub fn log_rejected(&mut self, seq: &str, ctx: &str, chosen: &str, shown: &[String]) {
        self.events.push(SessionEvent {
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
            self.events.push(e);
            n += 1;
        }
        Ok(n)
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
    // Single-writer discipline: short-lived connection, WAL mode, one
    // transaction per call. JSONL remains the interchange/export format;
    // this table is local persistence only.

    fn open_db(path: &Path) -> Result<rusqlite::Connection, String> {
        let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(crate::personal::PersonalDict::SQLITE_SCHEMA)
            .map_err(|e| e.to_string())?;
        Ok(conn)
    }

    /// Persist all in-memory events to the `session_events` table
    /// (clears the table first, then inserts in order).
    pub fn save_to_sqlite(&self, path: &Path) -> Result<(), String> {
        let mut conn = Self::open_db(path)?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM session_events", [])
            .map_err(|e| e.to_string())?;
        {
            let mut st = tx
                .prepare(
                    "INSERT INTO session_events
                     (ts, seq, ctx, chosen, shown_json, action)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|e| e.to_string())?;
            for e in &self.events {
                let shown = serde_json::to_string(&e.shown).map_err(|e| e.to_string())?;
                st.execute(rusqlite::params![
                    e.ts, e.seq, e.ctx, e.chosen, shown, e.action
                ])
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load all events from the `session_events` table, ordered by row id.
    pub fn load_from_sqlite(path: &Path) -> Result<Self, String> {
        let conn = Self::open_db(path)?;
        let mut st = conn
            .prepare(
                "SELECT ts, seq, ctx, chosen, shown_json, action
                 FROM session_events ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| {
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
        for row in rows {
            let (ts, seq, ctx, chosen, shown_json, action) =
                row.map_err(|e| e.to_string())?;
            let shown: Vec<String> =
                serde_json::from_str(&shown_json).map_err(|e| e.to_string())?;
            log.events.push(SessionEvent {
                ts,
                seq,
                ctx,
                chosen,
                shown,
                action,
            });
        }
        Ok(log)
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
}
