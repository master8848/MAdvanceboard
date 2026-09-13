//! Personal dictionary: learned words with counts + last_seen,
//! LFU cap, block tombstones, password-mode bypass, SQLite persistence.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// LFU cap for the personal dictionary: 10k entries (tombstones exempt).
///
/// Cap-dispute ruling (plan `06-persistence-power.md`, one commit): SPEC §4
/// says 20k LRU, code said 10k LFU. Keeping **10k LFU** because (a) the power
/// budget in the same plan is sized for "full 10k personal in RAM ~2-3MB" —
/// 20k doubles that resident set for no measured gain; (b) LFU protects
/// high-frequency learned words from recency churn (typing locality bursts
/// would evict good words under LRU); (c) the existing eviction path is
/// tested (`lfu_cap_evicts_coldest`, `tombstones_exempt_from_eviction`) and a
/// policy flip would invalidate the tuning gate without new data. If a
/// longitudinal study shows recency beats frequency, flip `PERSONAL_CAP` +
/// `evict_if_needed` together with the gate re-run — not piecemeal.
pub const PERSONAL_CAP: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersonalEntry {
    pub word: String,
    pub category: String,
    /// Dedupe language for the suggest union key `(norm(word), lang)`.
    /// Derived deterministically from `category` on learn/load
    /// (`NE` -> `ne`, else `en`); old JSONL lines without it default to
    /// `en`. Not stored in SQLite (recomputed from `category` on load).
    #[serde(default = "default_lang")]
    pub lang: String,
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

fn default_lang() -> String {
    "en".to_string()
}

/// Fixed bigram vocabulary size (plan/09#2 desk-spike outcome).
///
/// `bigram_term` smooths with add-one mass `1/(N+V)`, so a growing
/// `V=entries.len()` makes every unseen-pair score drift more negative over
/// months of use even when behavior is unchanged (measured day0→day6 drift
/// −0.67 score pts under dynamic V, +0.00 under fixed-10k; Lidstone-α barely
/// moves it, so do not tune α). Fixed `V = 10_000` freezes the unseen mass:
/// cold-start backoff (`count_prev == 0` → exactly `0.0`) is unaffected, and
/// long-term scores stop bleeding as the personal dict grows.
pub const BIGRAM_FIXED_VOCAB: u64 = 10_000;

/// Deterministic category -> dedupe-lang rule (only `en`/`ne` exist today).
fn lang_of_category(category: &str) -> String {
    if category.eq_ignore_ascii_case("ne") {
        "ne".to_string()
    } else {
        "en".to_string()
    }
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
    /// Keys mutated since the last durable flush. The hot path (`learn` /
    /// `forget` / `record_reject`) only inserts here — never touches disk.
    /// [`Store::flush`](crate::store::Store::flush) upserts exactly these
    /// rows (`INSERT … ON CONFLICT DO UPDATE`), then clears the sets, so a
    /// keystroke costs O(1) RAM + amortized O(dirty) flash.
    dirty: HashSet<String>,
    dirty_bigrams: HashSet<(String, String)>,
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
    /// password mode or empty input. The accept timestamp is floored to
    /// its 1h bucket ([`now_quantized`]) so `recency = exp(-dt/7d)` is
    /// stable within the hour: two accepts in the same hour land on the
    /// same `last_seen`, and repeats stay byte-identical (plan/04).
    pub fn learn(&mut self, word: &str, category: &str) -> bool {
        self.learn_at(word, category, now_ts())
    }

    /// Deterministic learn at an explicit wall-clock `now`: the stored
    /// `last_seen` is `quantize_ts(now)`. Tests and replay harnesses pass
    /// a frozen hour bucket; negative timestamps fail loudly via
    /// [`quantize_ts`].
    pub fn learn_at(&mut self, word: &str, category: &str, now: i64) -> bool {
        if self.password_mode || word.trim().is_empty() {
            return false;
        }
        let ts = quantize_ts(now).unwrap_or_else(|e| panic!("PersonalDict::learn_at: {e}"));
        let k = key_of(word);
        match self.entries.get_mut(&k) {
            Some(e) => {
                e.count += 1;
                e.acc += 1;
                e.last_seen = ts;
                e.deleted = false;
                if !category.is_empty() {
                    e.category = category.to_string();
                    e.lang = lang_of_category(category);
                }
            }
            None => {
                self.entries.insert(
                    k.clone(),
                    PersonalEntry {
                        word: word.to_string(),
                        category: category.to_string(),
                        lang: lang_of_category(category),
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
        self.dirty.insert(k);
        true
    }

    /// Block a word: tombstone `{del:true}`; hidden everywhere,
    /// survives sync, exempt from LFU eviction. Timestamp quantized to
    /// the 1h bucket (see [`Self::learn`]).
    pub fn forget(&mut self, word: &str) -> bool {
        self.forget_at(word, now_ts())
    }

    /// Deterministic block at an explicit wall-clock `now` (see
    /// [`Self::learn_at`]).
    pub fn forget_at(&mut self, word: &str, now: i64) -> bool {
        let ts = quantize_ts(now).unwrap_or_else(|e| panic!("PersonalDict::forget_at: {e}"));
        let k = key_of(word);
        match self.entries.get_mut(&k) {
            Some(e) => {
                e.deleted = true;
                e.last_seen = ts;
            }
            None => {
                self.entries.insert(
                    k.clone(),
                    PersonalEntry {
                        word: word.to_string(),
                        category: String::new(),
                        lang: default_lang(),
                        count: 0,
                        acc: 0,
                        rej: 0,
                        last_seen: ts,
                        deleted: true,
                    },
                );
            }
        }
        self.dirty.insert(k);
        true
    }

    /// Record a rejection (user deleted the committed word or picked
    /// another candidate). New-row timestamps use the 1h bucket (see
    /// [`Self::learn`]); existing rows keep their `last_seen` (a reject
    /// is not an accept).
    pub fn record_reject(&mut self, word: &str) {
        self.record_reject_at(word, now_ts())
    }

    /// Deterministic reject at an explicit wall-clock `now` (see
    /// [`Self::learn_at`]).
    pub fn record_reject_at(&mut self, word: &str, now: i64) {
        let k = key_of(word);
        if let Some(e) = self.entries.get_mut(&k) {
            e.rej += 1;
        } else {
            let ts = quantize_ts(now).unwrap_or_else(|e| panic!("PersonalDict::record_reject_at: {e}"));
            self.entries.insert(
                k.clone(),
                PersonalEntry {
                    word: word.to_string(),
                    category: String::new(),
                    lang: default_lang(),
                    count: 0,
                    acc: 0,
                    rej: 1,
                    last_seen: ts,
                    deleted: false,
                },
            );
        }
        self.dirty.insert(k);
    }

    pub fn record_bigram(&mut self, prev: &str, word: &str) {
        if self.password_mode || prev.is_empty() || word.is_empty() {
            return;
        }
        let k = (prev.to_lowercase(), word.to_lowercase());
        *self.bigrams.entry(k.clone()).or_insert(0) += 1;
        *self.prev_counts.entry(prev.to_lowercase()).or_insert(0) += 1;
        self.dirty_bigrams.insert(k);
    }

    pub fn get(&self, word: &str) -> Option<&PersonalEntry> {
        self.entries.get(&key_of(word))
    }

    /// Live (non-tombstone) personal entries, sorted by `(word, lang)` so    /// the suggest union consumes them in a deterministic order.
    /// This is the personal-OOV index: learned QWERTY/OOV words with no
    /// static entry participate in `suggest` through this iterator.
    pub fn live_entries(&self) -> Vec<&PersonalEntry> {
        let mut v: Vec<&PersonalEntry> =
            self.entries.values().filter(|e| !e.deleted).collect();
        v.sort_by(|a, b| a.word.cmp(&b.word).then_with(|| a.lang.cmp(&b.lang)));
        v
    }

    /// `(count(prev,w), count(prev), V)` with `V` fixed at
    /// [`BIGRAM_FIXED_VOCAB`] (plan/09#2): the unseen mass never drifts as
    /// the dict grows.
    pub fn bigram_counts(&self, prev: &str, word: &str) -> (u64, u64, u64) {
        let pw = self
            .bigrams
            .get(&(prev.to_lowercase(), word.to_lowercase()))
            .copied()
            .unwrap_or(0);
        let p = self.prev_counts.get(&prev.to_lowercase()).copied().unwrap_or(0);
        (pw, p, BIGRAM_FIXED_VOCAB)
    }

    /// Learned followers of `prev`: `(word, count)` with count desc, then
    /// word asc (deterministic). Backs the next-word connection surface
    /// ([`crate::stack::DictionaryStack::suggest_next_at`]); empty when
    /// `prev` was never committed before a word (cold-start = no data,
    /// never a guess).
    pub fn top_followers(&self, prev: &str) -> Vec<(String, u64)> {
        let prev = prev.to_lowercase();
        let mut v: Vec<(String, u64)> = self
            .bigrams
            .iter()
            .filter(|((p, _), _)| *p == prev)
            .map(|((_, w), c)| (w.clone(), *c))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
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

    /// Strict JSONL parse: every non-blank line must decode, otherwise
    /// `Err` names the first bad line number. Used by startup recovery
    /// ([`crate::store::recover_personal`]) so a corrupt `personal.jsonl`
    /// triggers snapshot-restore instead of silently loading partial data.
    /// (`from_jsonl` stays lenient for loose interchange input.)
    pub fn from_jsonl_strict(s: &str) -> Result<Self, String> {
        let mut d = Self::new();
        for (i, line) in s.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let e: PersonalEntry = serde_json::from_str(line)
                .map_err(|e| format!("line {}: {e}", i + 1))?;
            if e.word.trim().is_empty() {
                return Err(format!("line {}: empty word", i + 1));
            }
            d.entries.insert(key_of(&e.word), e);
        }
        Ok(d)
    }

    /// Personal bigram counts as JSONL (`{"prev","word","count"}` per line),
    /// the `bigrams.jsonl` sync-folder file (`docs/SYNC.md` §3).
    pub fn bigrams_jsonl(&self) -> String {
        let mut pairs: Vec<(&(String, String), &u64)> = self.bigrams.iter().collect();
        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut out = String::new();
        for ((prev, word), count) in pairs {
            let line = serde_json::json!({"prev": prev, "word": word, "count": count});
            out.push_str(&line.to_string());
            out.push('\n');
        }
        out
    }

    /// True when mutations since the last flush exist (flush work to do).
    pub fn is_flush_dirty(&self) -> bool {
        !self.dirty.is_empty() || !self.dirty_bigrams.is_empty()
    }

    /// Apply one `wal.log` line (a `PersonalEntry` JSON row) during recovery
    /// replay: last-writer-wins per key, tombstones preserved. Replayed rows
    /// are marked dirty so the next flush persists them to SQLite too.
    pub(crate) fn apply_wal_line(&mut self, line: &str) -> Result<(), String> {
        let e: PersonalEntry =
            serde_json::from_str(line).map_err(|e| format!("bad WAL row: {e}"))?;
        if e.word.trim().is_empty() {
            return Err("bad WAL row: empty word".to_string());
        }
        let k = key_of(&e.word);
        self.dirty.insert(k.clone());
        self.entries.insert(k, e);
        Ok(())
    }

    // ---- SQLite persistence (rusqlite, bundled static build) ----
    //
    // Single-writer discipline: the one `Store` connection behind
    // `Predictor`'s `Mutex` is the only writer. Flushes upsert dirty rows
    // only (`INSERT … ON CONFLICT(key) DO UPDATE`); there is deliberately no
    // `DELETE FROM personal` anywhere — a power cut mid-flush can never lose
    // more than the uncommitted batch, and the dirty set is retained on
    // error so the next flush retries it.

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

    /// Upsert one entry row. Shared by dirty-flush and full-save paths.
    fn upsert_entry(conn: &rusqlite::Connection, k: &str, e: &PersonalEntry) -> Result<(), String> {
        conn.execute(
            "INSERT INTO personal
             (key, word, category, count, acc, rej, last_seen, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(key) DO UPDATE SET
               word=excluded.word, category=excluded.category,
               count=excluded.count, acc=excluded.acc, rej=excluded.rej,
               last_seen=excluded.last_seen, deleted=excluded.deleted",
            rusqlite::params![
                k,
                e.word,
                e.category,
                e.count as i64,
                e.acc as i64,
                e.rej as i64,
                e.last_seen,
                if e.deleted { 1i64 } else { 0i64 },
            ],
        )
        .map_err(|e| format!("personal: upsert {k:?}: {e}"))?;
        Ok(())
    }

    /// Flush dirty rows to an already-open connection (see
    /// [`Store::flush`](crate::store::Store::flush)). Returns
    /// `(personal_rows, bigram_rows)` written, plus the JSONL lines for the
    /// `wal.log` append. Dirty sets clear only after commit; on `Err` they
    /// are retained for retry — no silent loss.
    pub(crate) fn flush_dirty_to_conn(
        &mut self,
        conn: &mut rusqlite::Connection,
    ) -> Result<(usize, usize, Vec<String>), String> {
        if !self.is_flush_dirty() {
            return Ok((0, 0, Vec::new()));
        }
        let tx = conn
            .transaction()
            .map_err(|e| format!("personal: begin flush tx: {e}"))?;
        let mut personal_rows = 0;
        let mut wal_lines = Vec::with_capacity(self.dirty.len());
        // Deterministic row order so WAL / tests see stable output.
        let mut keys: Vec<String> = self.dirty.iter().cloned().collect();
        keys.sort();
        for k in &keys {
            if let Some(e) = self.entries.get(k) {
                Self::upsert_entry(&tx, k, e)?;
                wal_lines.push(
                    serde_json::to_string(e)
                        .map_err(|e| format!("personal: encode WAL row {k:?}: {e}"))?,
                );
                personal_rows += 1;
            }
        }
        let mut bigram_rows = 0;
        {
            let mut st = tx
                .prepare(
                    "INSERT INTO bigrams (prev, word, count) VALUES (?1, ?2, ?3)
                     ON CONFLICT(prev, word) DO UPDATE SET count=excluded.count",
                )
                .map_err(|e| format!("personal: prepare bigram upsert: {e}"))?;
            let mut bkeys: Vec<(String, String)> = self.dirty_bigrams.iter().cloned().collect();
            bkeys.sort();
            for (prev, word) in &bkeys {
                if let Some(c) = self.bigrams.get(&(prev.clone(), word.clone())) {
                    st.execute(rusqlite::params![prev, word, *c as i64])
                        .map_err(|e| {
                            format!("personal: upsert bigram ({prev:?}, {word:?}): {e}")
                        })?;
                    bigram_rows += 1;
                }
            }
        }
        tx.commit()
            .map_err(|e| format!("personal: commit flush tx: {e}"))?;
        self.dirty.clear();
        self.dirty_bigrams.clear();
        Ok((personal_rows, bigram_rows, wal_lines))
    }

    /// Persist all entries + bigrams to a SQLite file (upsert-all, no
    /// `DELETE`). Compat path for callers without a `Store`; the steady
    /// path is dirty-flush via `Store`.
    pub fn save_to_sqlite(&self, path: &Path) -> Result<(), String> {
        let mut conn = Self::open_db(path)?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for (k, e) in &self.entries {
            Self::upsert_entry(&tx, k, e)?;
        }
        {
            let mut st = tx
                .prepare(
                    "INSERT INTO bigrams (prev, word, count) VALUES (?1, ?2, ?3)
                     ON CONFLICT(prev, word) DO UPDATE SET count=excluded.count",
                )
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
                // `lang` is not a SQLite column (schema frozen); recompute
                // it deterministically from `category` (same rule as learn).
                let lang = lang_of_category(&category);
                d.entries.insert(
                    k,
                    PersonalEntry {
                        word,
                        category,
                        lang,
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
    fn learn_timestamps_floor_to_hour_bucket() {
        // plan/04: `last_seen` is bucketed, so two accepts 40 min apart
        // in the same hour land on the same stamp and score identically.
        let mut a = PersonalDict::new();
        let mut b = PersonalDict::new();
        assert!(a.learn_at("hello", "EN", 3600));
        assert!(b.learn_at("hello", "EN", 3600 + 2400));
        assert_eq!(
            a.get("hello").unwrap().last_seen,
            b.get("hello").unwrap().last_seen,
            "same-hour accepts must share a last_seen bucket"
        );
        assert_eq!(a.get("hello").unwrap().last_seen, 3600);
        // Next hour is a distinct bucket (decay across days preserved).
        let mut c = PersonalDict::new();
        assert!(c.learn_at("hello", "EN", 7200));
        assert_eq!(c.get("hello").unwrap().last_seen, 7200);
        // Recency is stable within the hour for a fixed quantized now.
        let now = quantize_ts(3600 + 3000).unwrap();
        assert_eq!(
            crate::rank::recency_term(now, a.get("hello").unwrap().last_seen),
            crate::rank::recency_term(now, b.get("hello").unwrap().last_seen),
        );
        // Reject/forget paths bucket identically.
        let mut d = PersonalDict::new();
        d.record_reject_at("nope", 3600 + 100);
        assert_eq!(d.get("nope").unwrap().last_seen, 3600);
        let mut e = PersonalDict::new();
        e.forget_at("spam", 3600 + 3599);
        assert_eq!(e.get("spam").unwrap().last_seen, 3600);
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
    fn save_never_deletes_preexisting_rows() {
        // Upsert-only proof: saving a smaller dict into a DB that already
        // holds rows must leave those rows intact (the old DELETE+reinsert
        // would have wiped them).
        let dir = std::env::temp_dir().join("kbcore_personal_upsert.sqlite");
        let _ = std::fs::remove_file(&dir);
        let mut d = PersonalDict::new();
        d.learn("keepme", "EN");
        d.save_to_sqlite(&dir).unwrap();
        let mut d2 = PersonalDict::new();
        d2.learn("newcomer", "EN");
        d2.save_to_sqlite(&dir).unwrap();
        let loaded = PersonalDict::load_from_sqlite(&dir).unwrap();
        assert!(loaded.get("keepme").is_some(), "upsert must not delete");
        assert!(loaded.get("newcomer").is_some());
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn strict_jsonl_rejects_corruption_with_line_number() {
        let mut d = PersonalDict::new();
        d.learn("hello", "EN");
        let mut s = d.to_jsonl();
        s.push_str("{bad json\n");
        let err = PersonalDict::from_jsonl_strict(&s).unwrap_err();
        assert!(err.contains("line 2"), "unexpected: {err}");
        // Lenient reader still loads the good prefix (interchange compat).
        assert!(PersonalDict::from_jsonl(&s).get("hello").is_some());
    }

    #[test]
    fn dirty_set_tracks_mutations() {
        let mut d = PersonalDict::new();
        assert!(!d.is_flush_dirty());
        d.learn("a", "EN");
        d.record_bigram("say", "a");
        assert!(d.is_flush_dirty());
    }

    #[test]
    fn bigrams_jsonl_roundtrip_shape() {
        let mut d = PersonalDict::new();
        d.record_bigram("say", "hello");
        let s = d.bigrams_jsonl();
        assert_eq!(s.lines().count(), 1);
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["prev"], "say");
        assert_eq!(v["word"], "hello");
        assert_eq!(v["count"], 1);
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
    fn bigram_vocab_fixed_at_10k_as_dict_grows() {
        // Plan/09#2: V must not grow with the dict (dynamic V drifted the
        // unseen-pair score −0.67 pts over 6 simulated days; fixed is 0.00).
        use crate::rank::bigram_term;
        let mut d = PersonalDict::new();
        let (_, _, v0) = d.bigram_counts("the", "quick");
        assert_eq!(v0, BIGRAM_FIXED_VOCAB);
        for i in 0..50 {
            d.learn(&format!("word{i:03}"), "EN");
        }
        let (cpw, cp, v1) = d.bigram_counts("the", "quick");
        assert_eq!((cpw, cp), (0, 0));
        assert_eq!(v1, BIGRAM_FIXED_VOCAB, "V must stay fixed as entries grow");
        // Cold-start backoff still exactly 0 regardless of V.
        assert_eq!(bigram_term(0, 0, v1), 0.0);
        // And the smoothed unseen score is now growth-invariant: growing
        // the dict further cannot move it.
        d.record_bigram("the", "quick");
        let s_before = bigram_term(0, 1, d.bigram_counts("the", "zorblax").2);
        for i in 50..150 {
            d.learn(&format!("word{i:03}"), "EN");
        }
        let s_after = bigram_term(0, 1, d.bigram_counts("the", "zorblax").2);
        assert_eq!(s_before, s_after, "unseen score must not drift with growth");
        assert!(s_after < 0.0);
    }

    #[test]
    fn jsonl_roundtrip() {
        let mut d = PersonalDict::new();
        d.learn("hello", "EN");
        let s = d.to_jsonl();
        let d2 = PersonalDict::from_jsonl(&s);
        assert_eq!(d2.get("hello").unwrap().count, 1);
    }

    #[test]
    fn top_followers_rank_count_desc_word_asc() {
        let mut d = PersonalDict::new();
        assert!(d.top_followers("hello").is_empty());
        d.record_bigram("hello", "zebra");
        d.record_bigram("hello", "apple");
        d.record_bigram("hello", "apple");
        d.record_bigram("hi", "there");
        assert_eq!(
            d.top_followers("hello"),
            vec![
                ("apple".to_string(), 2),
                ("zebra".to_string(), 1),
            ]
        );
        assert_eq!(d.top_followers("hi").len(), 1);
        assert!(d.top_followers("unknown").is_empty());
    }
}
