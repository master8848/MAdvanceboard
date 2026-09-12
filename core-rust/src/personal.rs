//! Personal dictionary: learned words with counts + last_seen,
//! LFU cap, block tombstones, password-mode bypass, SQLite persistence.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// LFU cap for the personal dictionary (task requirement: 10k).
pub const PERSONAL_CAP: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersonalEntry {
    pub word: String,
    pub category: String,
    pub count: u64,
    pub acc: u64,
    pub rej: u64,
    pub last_seen: i64,
    /// Canonical wire name is `del` (SPEC §5, `docs/SYNC.md`); `deleted`
    /// (SPEC §4) is accepted on read for compat. The Rust field name stays
    /// `deleted` so JSONL blobs and call sites are untouched.
    #[serde(rename = "del", alias = "deleted")]
    pub deleted: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PersonalDict {
    entries: HashMap<String, PersonalEntry>,
    bigrams: HashMap<(String, String), u64>,
    prev_counts: HashMap<String, u64>,
    /// When true (e.g. password field), `learn` is a no-op: nothing is
    /// stored, satisfying the password-mode bypass requirement.
    pub password_mode: bool,
    pub cap: usize,
}

fn key_of(word: &str) -> String {
    word.to_lowercase()
}

pub fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Determinism quantum for scoring: wall-clock time is quantized to 1h
/// buckets at the `Predictor` level before it reaches ranking, so
/// `recency = exp(-dt/7d)` is stable within the hour and suggestions never
/// depend on per-second timing. Decay across days is preserved (intended).
pub const CLOCK_QUANTUM_SECS: i64 = 3600;

/// Floor `ts` to its 1h bucket. Returns an error for negative timestamps
/// instead of silently wrapping, so clock bugs surface explicitly.
pub fn quantize_ts(ts: i64) -> Result<i64, String> {
    if ts < 0 {
        return Err(format!("quantize_ts: negative timestamp {ts}"));
    }
    Ok(ts - ts % CLOCK_QUANTUM_SECS)
}

/// Current wall-clock time, quantized to the 1h scoring bucket.
/// Falls back to the Unix epoch (0) only when the system clock is
/// unavailable; callers needing strictness should use
/// `quantize_ts(now_ts())` and propagate the `Err`.
pub fn now_quantized() -> i64 {
    quantize_ts(now_ts()).unwrap_or(0)
}

impl PersonalDict {
    pub fn new() -> Self {
        Self {
            cap: PERSONAL_CAP,
            ..Default::default()
        }
    }

    pub fn with_cap(cap: usize) -> Self {
        Self {
            cap,
            ..Default::default()
        }
    }

    /// Learn (or re-accept) a word. Returns `false` when bypassed by
    /// password mode or empty input.
    pub fn learn(&mut self, word: &str, category: &str) -> bool {
        if self.password_mode || word.trim().is_empty() {
            return false;
        }
        let k = key_of(word);
        let ts = now_ts();
        match self.entries.get_mut(&k) {
            Some(e) => {
                e.count += 1;
                e.acc += 1;
                e.last_seen = ts;
                e.deleted = false;
                if !category.is_empty() {
                    e.category = category.to_string();
                }
            }
            None => {
                self.entries.insert(
                    k.clone(),
                    PersonalEntry {
                        word: word.to_string(),
                        category: category.to_string(),
                        count: 1,
                        acc: 1,
                        rej: 0,
                        last_seen: ts,
                        deleted: false,
                    },
                );
            }
        }
        self.evict_if_needed(Some(&k));
        true
    }

    /// Block a word: tombstone `{del:true}`; hidden everywhere,
    /// survives sync, exempt from LFU eviction.
    pub fn forget(&mut self, word: &str) -> bool {
        let k = key_of(word);
        let ts = now_ts();
        match self.entries.get_mut(&k) {
            Some(e) => {
                e.deleted = true;
                e.last_seen = ts;
            }
            None => {
                self.entries.insert(
                    k,
                    PersonalEntry {
                        word: word.to_string(),
                        category: String::new(),
                        count: 0,
                        acc: 0,
                        rej: 0,
                        last_seen: ts,
                        deleted: true,
                    },
                );
            }
        }
        true
    }

    /// Record a rejection (user deleted the committed word or picked
    /// another candidate).
    pub fn record_reject(&mut self, word: &str) {
        let k = key_of(word);
        if let Some(e) = self.entries.get_mut(&k) {
            e.rej += 1;
        } else {
            self.entries.insert(
                k,
                PersonalEntry {
                    word: word.to_string(),
                    category: String::new(),
                    count: 0,
                    acc: 0,
                    rej: 1,
                    last_seen: now_ts(),
                    deleted: false,
                },
            );
        }
    }

    pub fn record_bigram(&mut self, prev: &str, word: &str) {
        if self.password_mode || prev.is_empty() || word.is_empty() {
            return;
        }
        let k = (prev.to_lowercase(), word.to_lowercase());
        *self.bigrams.entry(k).or_insert(0) += 1;
        *self.prev_counts.entry(prev.to_lowercase()).or_insert(0) += 1;
    }

    pub fn get(&self, word: &str) -> Option<&PersonalEntry> {
        self.entries.get(&key_of(word))
    }

    /// `(count(prev,w), count(prev), V)`.
    pub fn bigram_counts(&self, prev: &str, word: &str) -> (u64, u64, u64) {
        let pw = self
            .bigrams
            .get(&(prev.to_lowercase(), word.to_lowercase()))
            .copied()
            .unwrap_or(0);
        let p = self.prev_counts.get(&prev.to_lowercase()).copied().unwrap_or(0);
        (pw, p, self.entries.len().max(1) as u64)
    }

    pub fn is_blocked(&self, word: &str) -> bool {
        self.get(word).map(|e| e.deleted).unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.entries.values().filter(|e| !e.deleted).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn evict_if_needed(&mut self, exclude: Option<&str>) {
        let live = self.entries.values().filter(|e| !e.deleted).count();
        if live <= self.cap {
            return;
        }
        // LFU: evict lowest (count, oldest last_seen) among non-tombstones,
        // excluding the just-learned word (a new word must not evict itself
        // when all timestamps tie within the same wall-clock second) and
        // breaking residual ties by key so the victim is deterministic
        // (HashMap iteration order is not).
        let victim = self
            .entries
            .iter()
            .filter(|(_, e)| !e.deleted)
            .filter(|(k, _)| exclude.map(|x| k.as_str() != x).unwrap_or(true))
            .min_by(|(ka, ea), (kb, eb)| {
                (ea.count, ea.last_seen, ka).cmp(&(eb.count, eb.last_seen, kb))
            })
            .map(|(k, _)| k.clone());
        if let Some(k) = victim {
            self.entries.remove(&k);
        }
    }

    pub fn to_jsonl(&self) -> String {
        let mut out = String::new();
        let mut entries: Vec<&PersonalEntry> = self.entries.values().collect();
        entries.sort_by(|a, b| a.word.cmp(&b.word));
        for e in entries {
            if let Ok(line) = serde_json::to_string(e) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }

    pub fn from_jsonl(s: &str) -> Self {
        let mut d = Self::new();
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(e) = serde_json::from_str::<PersonalEntry>(line) {
                d.entries.insert(key_of(&e.word), e);
            }
        }
        d
    }

    // ---- SQLite persistence (rusqlite, bundled static build) ----
    //
    // Single-writer discipline: each save/load opens its own short-lived
    // `Connection`, enables WAL mode, and runs one transaction to
    // completion. `Predictor` already serializes access behind a `Mutex`,
    // so there is exactly one writer at a time; no pool, no shared
    // connection across threads.

    pub const SQLITE_SCHEMA: &str = "
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS personal (
            key TEXT PRIMARY KEY,
            word TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT '',
            count INTEGER NOT NULL DEFAULT 0,
            acc INTEGER NOT NULL DEFAULT 0,
            rej INTEGER NOT NULL DEFAULT 0,
            last_seen INTEGER NOT NULL DEFAULT 0,
            deleted INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS bigrams (
            prev TEXT NOT NULL,
            word TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (prev, word)
        );
        CREATE TABLE IF NOT EXISTS session_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            seq TEXT NOT NULL DEFAULT '',
            ctx TEXT NOT NULL DEFAULT '',
            chosen TEXT NOT NULL DEFAULT '',
            shown_json TEXT NOT NULL DEFAULT '[]',
            action TEXT NOT NULL DEFAULT ''
        );
    ";

    fn open_db(path: &Path) -> Result<rusqlite::Connection, String> {
        let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(Self::SQLITE_SCHEMA)
            .map_err(|e| e.to_string())?;
        Ok(conn)
    }

    /// Persist entries + bigrams to a SQLite file.
    pub fn save_to_sqlite(&self, path: &Path) -> Result<(), String> {
        let mut conn = Self::open_db(path)?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM personal", [])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM bigrams", [])
            .map_err(|e| e.to_string())?;
        {
            let mut st = tx
                .prepare(
                    "INSERT INTO personal
                     (key, word, category, count, acc, rej, last_seen, deleted)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .map_err(|e| e.to_string())?;
            for (k, e) in &self.entries {
                st.execute(rusqlite::params![
                    k,
                    e.word,
                    e.category,
                    e.count as i64,
                    e.acc as i64,
                    e.rej as i64,
                    e.last_seen,
                    if e.deleted { 1i64 } else { 0i64 },
                ])
                .map_err(|e| e.to_string())?;
            }
        }
        {
            let mut st = tx
                .prepare("INSERT INTO bigrams (prev, word, count) VALUES (?1, ?2, ?3)")
                .map_err(|e| e.to_string())?;
            for ((prev, word), c) in &self.bigrams {
                st.execute(rusqlite::params![prev, word, *c as i64])
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load entries + bigrams from a SQLite file.
    pub fn load_from_sqlite(path: &Path) -> Result<Self, String> {
        let conn = Self::open_db(path)?;
        let mut d = Self::new();
        {
            let mut st = conn
                .prepare(
                    "SELECT key, word, category, count, acc, rej, last_seen, deleted
                     FROM personal",
                )
                .map_err(|e| e.to_string())?;
            let rows = st
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                        r.get::<_, i64>(6)?,
                        r.get::<_, i64>(7)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (k, word, category, count, acc, rej, last_seen, deleted) =
                    row.map_err(|e| e.to_string())?;
                d.entries.insert(
                    k,
                    PersonalEntry {
                        word,
                        category,
                        count: count.max(0) as u64,
                        acc: acc.max(0) as u64,
                        rej: rej.max(0) as u64,
                        last_seen,
                        deleted: deleted != 0,
                    },
                );
            }
        }
        {
            let mut st = conn
                .prepare("SELECT prev, word, count FROM bigrams")
                .map_err(|e| e.to_string())?;
            let rows = st
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (prev, word, count) = row.map_err(|e| e.to_string())?;
                let c = count.max(0) as u64;
                *d.prev_counts.entry(prev.clone()).or_insert(0) += c;
                d.bigrams.insert((prev, word), c);
            }
        }
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_quantization_buckets() {
        // 1h buckets: any two timestamps in the same hour quantize equal.
        assert_eq!(quantize_ts(3600), Ok(3600));
        assert_eq!(quantize_ts(3600 + 3599), Ok(3600));
        assert_eq!(quantize_ts(7200), Ok(7200));
        assert_eq!(quantize_ts(0), Ok(0));
        // Negative timestamps are an explicit error, never silent wrap.
        assert!(quantize_ts(-1).is_err());
    }

    #[test]
    fn learn_and_get() {
        let mut d = PersonalDict::new();
        assert!(d.learn("Hello", "EN"));
        let e = d.get("hello").unwrap();
        assert_eq!(e.count, 1);
        assert_eq!(e.acc, 1);
        d.learn("hello", "EN");
        assert_eq!(d.get("HELLO").unwrap().count, 2);
    }

    #[test]
    fn password_mode_bypass() {
        let mut d = PersonalDict::new();
        d.password_mode = true;
        assert!(!d.learn("secret", "EN"));
        assert!(d.get("secret").is_none());
    }

    #[test]
    fn forget_tombstones() {
        let mut d = PersonalDict::new();
        d.learn("spam", "EN");
        d.forget("spam");
        assert!(d.is_blocked("spam"));
        // Re-learning unblocks (fresh accept clears tombstone).
        d.learn("spam", "EN");
        assert!(!d.is_blocked("spam"));
    }

    #[test]
    fn lfu_cap_evicts_coldest() {
        let mut d = PersonalDict::with_cap(3);
        d.learn("a", "EN");
        d.learn("b", "EN");
        d.learn("c", "EN");
        d.learn("a", "EN"); // a is hot
        d.learn("d", "EN"); // evicts b or c (count 1, oldest)
        assert_eq!(d.len(), 3);
        assert!(d.get("a").is_some());
        assert!(d.get("d").is_some());
    }

    #[test]
    fn tombstones_exempt_from_eviction() {
        let mut d = PersonalDict::with_cap(2);
        d.learn("a", "EN");
        d.learn("b", "EN");
        d.forget("x"); // tombstone beyond cap must survive
        d.learn("c", "EN");
        assert!(d.is_blocked("x"));
    }

    #[test]
    fn sqlite_roundtrip() {
        let dir = std::env::temp_dir().join("kbcore_personal_test.sqlite");
        let _ = std::fs::remove_file(&dir);
        let mut d = PersonalDict::new();
        d.learn("hello", "EN");
        d.forget("spam");
        d.record_bigram("say", "hello");
        d.record_reject("hello");
        d.save_to_sqlite(&dir).unwrap();
        let loaded = PersonalDict::load_from_sqlite(&dir).unwrap();
        assert_eq!(loaded.get("hello").unwrap().count, 1);
        assert_eq!(loaded.get("hello").unwrap().rej, 1);
        assert!(loaded.is_blocked("spam"));
        assert_eq!(loaded.bigram_counts("say", "hello").0, 1);
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn tombstone_serde_compat_del_deleted() {
        // Canonical wire name `del`; `deleted` accepted on read.
        let e: PersonalEntry = serde_json::from_str(
            r#"{"word":"x","category":"EN","count":1,"acc":1,"rej":0,"last_seen":1,"del":true}"#,
        )
        .unwrap();
        assert!(e.deleted);
        let e2: PersonalEntry = serde_json::from_str(
            r#"{"word":"x","category":"EN","count":1,"acc":1,"rej":0,"last_seen":1,"deleted":true}"#,
        )
        .unwrap();
        assert!(e2.deleted);
        let line = serde_json::to_string(&e).unwrap();
        assert!(line.contains("\"del\":true"));
    }

    #[test]
    fn jsonl_roundtrip() {
        let mut d = PersonalDict::new();
        d.learn("hello", "EN");
        let s = d.to_jsonl();
        let d2 = PersonalDict::from_jsonl(&s);
        assert_eq!(d2.get("hello").unwrap().count, 1);
    }
}
