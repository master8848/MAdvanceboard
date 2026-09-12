//! Local durability for the personal dictionary + session log.
//!
//! Design (plan `06-persistence-power.md`, file layout `docs/SYNC.md` §3):
//!
//! * The in-RAM [`PersonalDict`](crate::personal::PersonalDict) is the source
//!   of truth for `suggest`; SQLite is durability only and never on the read
//!   path. One [`Store`] owns one `rusqlite::Connection` and lives behind the
//!   existing `Mutex<Inner>` in `Predictor` (already single-writer — safe).
//! * Hot path never touches disk: mutations only mark a dirty-set in RAM.
//!   [`Store::flush`] writes `INSERT … ON CONFLICT DO UPDATE` for dirty rows
//!   only (never `DELETE` + reinsert) plus one `wal.log` append, at most once
//!   per [`FLUSH_COALESCE`] (2 s). The hourly compact rewrites
//!   `personal.jsonl` / `bigrams.jsonl`, snapshots `snapshot-<ts>.gz`, and
//!   truncates `wal.log`.
//! * `VACUUM` runs only on a schema-version bump, never on the steady path.
//! * Every I/O failure is returned as `Err(String)` with the path + operation
//!   named — never swallowed, never a silent empty store.
//!
//! Cross-device merge/recovery (`sync.rs`) is separate, future work; this
//! module covers local durability + single-file recovery only.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::personal::PersonalDict;
use crate::session::SessionLogger;

/// On-disk schema version. Bumped only with a migration; the bump is the
/// sole trigger for `VACUUM` (see [`Store::open`]).
pub const SCHEMA_VERSION: u32 = 1;

/// Minimum interval between coalesced flushes: keystrokes only dirty RAM,
/// disk sees at most one batch per window.
pub const FLUSH_COALESCE: Duration = Duration::from_secs(2);

/// Steady-state cadence for `wal_checkpoint(TRUNCATE)` + snapshot + compact.
pub const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(3600);

/// `PRAGMA journal_size_limit` cap (1 MiB) so the WAL can never grow
/// unbounded between hourly checkpoints.
pub const JOURNAL_SIZE_LIMIT_BYTES: i64 = 1024 * 1024;

/// Extra schema owned by the store: key/value metadata (schema version).
/// The personal/bigram/session tables live in
/// [`PersonalDict::SQLITE_SCHEMA`](crate::personal::PersonalDict::SQLITE_SCHEMA).
const META_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS meta (
        k TEXT PRIMARY KEY,
        v TEXT NOT NULL
    );
";

/// Outcome of a [`Store::flush`] call: exact rows written, so callers (and
/// power tracing) can see flash wear per batch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlushStats {
    pub personal_rows: usize,
    pub bigram_rows: usize,
    pub session_rows: usize,
    /// Bytes appended to `wal.log` (`0` when no sync dir is configured).
    pub wal_bytes: u64,
    pub checkpointed: bool,
}

/// How a personal dictionary was recovered at startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// `personal.jsonl` parsed cleanly; WAL replayed on top (if present).
    Clean { wal_lines: usize },
    /// `personal.jsonl` was corrupt: restored newest `snapshot-<ts>.gz` and
    /// replayed `wal.log`. The detail string names the corrupt file(s) and
    /// any skipped WAL line numbers — surface it to the user, do not drop it.
    RestoredFromSnapshot { detail: String },
    /// Both snapshot and WAL were unusable: started with an **empty**
    /// personal table so base packs stay functional. The detail string must
    /// be shown to the user (explicit notify requirement, `docs/SYNC.md` §5).
    EmptyAfterCorruption { detail: String },
}

/// Recovered personal dictionary + how it was obtained (never an `Err` for
/// corruption itself — corruption is an expected branch with a status; `Err`
/// is reserved for I/O failures that prevent even attempting recovery).
#[derive(Debug)]
pub struct Recovery {
    pub dict: PersonalDict,
    pub status: RecoveryStatus,
}

/// Single SQLite handle + flush/checkpoint clocks.
///
/// `rusqlite::Connection` is `Send` (not `Sync`); `Store` travels behind the
/// existing `Mutex<Inner>`, preserving the single-writer discipline.
/// Session-log caps (plan/06): in-memory ring holds the last 500 events;
/// the SQLite window keeps the newest 5000 rows after each flush.
pub const SESSION_RING_CAP: usize = 500;
pub const SESSION_TABLE_KEEP: i64 = 5000;

pub struct Store {
    conn: rusqlite::Connection,
    path: PathBuf,
    sync_dir: Option<PathBuf>,
    last_flush: Option<Instant>,
    last_checkpoint: Instant,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `rusqlite::Connection` has no `Debug`; the path identifies the
        // handle in diagnostics (explicit, not a silent opaque struct).
        f.debug_struct("Store")
            .field("path", &self.path)
            .field("sync_dir", &self.sync_dir)
            .field("last_flush", &self.last_flush)
            .field("last_checkpoint", &self.last_checkpoint)
            .finish()
    }
}

impl Store {
    /// Open (or create) the database at `path`, apply the power-friendly
    /// pragmas, ensure the schema, and `VACUUM` only on a version bump.
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        "store: create parent dir {} for {}: {e}",
                        parent.display(),
                        path.display()
                    )
                })?;
            }
        }
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| format!("store: open {}: {e}", path.display()))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| format!("store: busy_timeout {}: {e}", path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA temp_store=MEMORY;
             PRAGMA cache_size=-4096;
             PRAGMA journal_size_limit=1048576;",
        )
        .map_err(|e| format!("store: pragmas {}: {e}", path.display()))?;
        conn.execute_batch(PersonalDict::SQLITE_SCHEMA)
            .map_err(|e| format!("store: schema {}: {e}", path.display()))?;
        conn.execute_batch(META_SCHEMA)
            .map_err(|e| format!("store: meta schema {}: {e}", path.display()))?;

        let stored: Option<String> = conn
            .query_row("SELECT v FROM meta WHERE k='schema_version'", [], |r| {
                r.get(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(|e| format!("store: read schema_version {}: {e}", path.display()))?;
        match stored
            .as_deref()
            .map(str::parse::<u32>)
            .transpose()
            .map_err(|e| {
                format!(
                    "store: corrupt schema_version {stored:?} in {}: {e}",
                    path.display()
                )
            })? {
            None => {
                conn.execute(
                    "INSERT INTO meta (k, v) VALUES ('schema_version', ?1)",
                    rusqlite::params![SCHEMA_VERSION.to_string()],
                )
                .map_err(|e| format!("store: stamp schema_version {}: {e}", path.display()))?;
            }
            Some(v) if v < SCHEMA_VERSION => {
                // Sole VACUUM trigger: a version bump (migration repack).
                conn.execute("VACUUM", [])
                    .map_err(|e| format!("store: VACUUM {}: {e}", path.display()))?;
                conn.execute(
                    "UPDATE meta SET v=?1 WHERE k='schema_version'",
                    rusqlite::params![SCHEMA_VERSION.to_string()],
                )
                .map_err(|e| {
                    format!("store: bump schema_version {}: {e}", path.display())
                })?;
            }
            Some(v) if v > SCHEMA_VERSION => {
                return Err(format!(
                    "store: {} has schema_version {v} (newer than supported {SCHEMA_VERSION}); \
                     refusing to open so a newer app's data is never misread",
                    path.display()
                ));
            }
            Some(_) => {}
        }

        Ok(Self {
            conn,
            path: path.to_path_buf(),
            sync_dir: None,
            last_flush: None,
            last_checkpoint: Instant::now(),
        })
    }

    /// Attach the sync folder (`personal.jsonl`, `bigrams.jsonl`,
    /// `snapshot-<ts>.gz`, `wal.log` per `docs/SYNC.md` §3). `None` disables
    /// WAL/snapshot I/O (SQLite durability still applies).
    pub fn set_sync_dir(&mut self, dir: Option<PathBuf>) {
        self.sync_dir = dir;
    }

    pub fn db_path(&self) -> &Path {
        &self.path
    }

    /// True when the coalesce window has elapsed since the last flush
    /// (or no flush has ever run while dirty rows exist).
    pub fn flush_due(&self) -> bool {
        match self.last_flush {
            None => true,
            Some(t) => t.elapsed() >= FLUSH_COALESCE,
        }
    }

    /// Flush dirty personal rows + bigrams (upsert-only) and pending session
    /// events (append-only), then append the personal delta to `wal.log`.
    /// Dirty sets are cleared only after the transaction commits; on error
    /// they are retained so no accepted word is ever lost silently.
    pub fn flush(
        &mut self,
        dict: &mut PersonalDict,
        log: &mut SessionLogger,
    ) -> Result<FlushStats, String> {
        let (personal_rows, bigram_rows, wal_lines) = dict.flush_dirty_to_conn(&mut self.conn)?;
        let session_rows = log.flush_pending_to_conn(&mut self.conn)?;
        let mut stats = FlushStats {
            personal_rows,
            bigram_rows,
            session_rows,
            wal_bytes: 0,
            checkpointed: false,
        };
        if !wal_lines.is_empty() {
            stats.wal_bytes = Self::append_wal_lines(
                self.sync_dir.as_deref(),
                &wal_lines,
                &self.path,
            )?;
        }
        self.last_flush = Some(Instant::now());
        if self.last_checkpoint.elapsed() >= CHECKPOINT_INTERVAL {
            self.checkpoint_now()?;
            stats.checkpointed = true;
        }
        Ok(stats)
    }

    /// `PRAGMA wal_checkpoint(TRUNCATE)` — recycles the WAL file so standby
    /// power/pages stay flat. Runs hourly via [`Store::flush`]; callers may
    /// also invoke it directly (e.g. before snapshot/export).
    pub fn checkpoint_now(&mut self) -> Result<(), String> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|e| {
                format!(
                    "store: wal_checkpoint(TRUNCATE) {}: {e}",
                    self.path.display()
                )
            })?;
        self.last_checkpoint = Instant::now();
        Ok(())
    }

    /// Hourly compact of the sync folder: rewrite `personal.jsonl` +
    /// `bigrams.jsonl` from the given snapshots, write `snapshot-<ts>.gz`,
    /// truncate `wal.log`. Takes pre-rendered JSONL (so callers behind a
    /// `Mutex` can clone under a short immutable borrow, then compact under
    /// the store borrow). Returns the snapshot path, or `None` when no sync
    /// dir is configured. Every write is atomic (tmp + rename); every I/O
    /// failure names the file and operation.
    pub fn compact_sync_folder(
        &mut self,
        personal_jsonl: &str,
        bigrams_jsonl: &str,
    ) -> Result<Option<PathBuf>, String> {
        let dir = match self.sync_dir.clone() {
            None => return Ok(None),
            Some(d) => d,
        };
        fs::create_dir_all(&dir)
            .map_err(|e| format!("store: create sync dir {}: {e}", dir.display()))?;
        atomic_write(&dir.join("personal.jsonl"), personal_jsonl.as_bytes()).map_err(|e| {
            format!("store: write {}/personal.jsonl: {e}", dir.display())
        })?;
        atomic_write(&dir.join("bigrams.jsonl"), bigrams_jsonl.as_bytes()).map_err(|e| {
            format!("store: write {}/bigrams.jsonl: {e}", dir.display())
        })?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let snap = dir.join(format!("snapshot-{ts}.gz"));
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(personal_jsonl.as_bytes())
            .map_err(|e| format!("store: gzip {}: {e}", snap.display()))?;
        let bytes = enc
            .finish()
            .map_err(|e| format!("store: gzip finish {}: {e}", snap.display()))?;
        atomic_write(&snap, &bytes)
            .map_err(|e| format!("store: write {}: {e}", snap.display()))?;
        // Truncate (do not delete) so concurrent readers never see a missing file.
        fs::write(dir.join("wal.log"), b"")
            .map_err(|e| format!("store: truncate {}/wal.log: {e}", dir.display()))?;
        self.checkpoint_now()?;
        Ok(Some(snap))
    }

    fn append_wal_lines(
        sync_dir: Option<&Path>,
        lines: &[String],
        db_path: &Path,
    ) -> Result<u64, String> {
        let dir = match sync_dir {
            None => return Ok(0),
            Some(d) => d,
        };
        fs::create_dir_all(dir)
            .map_err(|e| format!("store: create sync dir {}: {e}", dir.display()))?;
        let wal = dir.join("wal.log");
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal)
            .map_err(|e| format!("store: open {} (db {}): {e}", wal.display(), db_path.display()))?;
        let mut bytes: u64 = 0;
        for line in lines {
            f.write_all(line.as_bytes())
                .map_err(|e| format!("store: append {}: {e}", wal.display()))?;
            f.write_all(b"\n")
                .map_err(|e| format!("store: append {}: {e}", wal.display()))?;
            bytes += line.len() as u64 + 1;
        }
        // Durability, not just page cache: a power cut must not lose acked taps.
        f.sync_all()
            .map_err(|e| format!("store: fsync {}: {e}", wal.display()))?;
        Ok(bytes)
    }
}

/// Recover the personal dictionary from a sync folder.
///
/// 1. `personal.jsonl` parses strictly → `Clean` (WAL replayed on top).
/// 2. Else newest `snapshot-<ts>.gz` + `wal.log` replay → `RestoredFromSnapshot`.
/// 3. Else empty dict (base packs stay functional) → `EmptyAfterCorruption`.
///
/// Corruption details are data in the returned [`RecoveryStatus`], meant to be
/// shown to the user — never logged-and-dropped.
pub fn recover_personal(sync_dir: &Path) -> Result<Recovery, String> {
    let main = sync_dir.join("personal.jsonl");
    let main_src = fs::read_to_string(&main).map_err(|e| {
        // A missing folder/file on first run is not corruption: start empty
        // but say so explicitly (caller notifies; base stays functional).
        if e.kind() == std::io::ErrorKind::NotFound {
            return format!(
                "store: no {} yet (first run); starting with empty personal",
                main.display()
            );
        }
        format!("store: read {}: {e}", main.display())
    });
    // Distinguish "first run" (NotFound → clean empty) from real I/O errors.
    let main_src = match main_src {
        Err(msg) if msg.contains("first run") => {
            return Ok(Recovery {
                dict: PersonalDict::new(),
                status: RecoveryStatus::Clean { wal_lines: 0 },
            });
        }
        Err(msg) => return Err(msg),
        Ok(s) => s,
    };
    match PersonalDict::from_jsonl_strict(&main_src) {
        Ok(mut dict) => {
            let wal_lines = replay_wal(sync_dir, &mut dict)?;
            Ok(Recovery {
                dict,
                status: RecoveryStatus::Clean { wal_lines },
            })
        }
        Err(main_err) => match restore_snapshot(sync_dir)? {
            Some((mut dict, snap_name)) => {
                let wal_applied = replay_wal(sync_dir, &mut dict).unwrap_or(0);
                Ok(Recovery {
                    dict,
                    status: RecoveryStatus::RestoredFromSnapshot {
                        detail: format!(
                            "personal.jsonl corrupt ({main_err}); restored {snap_name}, \
                             replayed {wal_applied} WAL line(s)"
                        ),
                    },
                })
            }
            None => Ok(Recovery {
                dict: PersonalDict::new(),
                status: RecoveryStatus::EmptyAfterCorruption {
                    detail: format!(
                        "personal.jsonl corrupt ({main_err}); no usable snapshot or WAL in {}; \
                         started with empty personal, base packs unaffected",
                        sync_dir.display()
                    ),
                },
            }),
        },
    }
}

/// Newest `snapshot-<ts>.gz` that gunzips + strict-parses, or `None`.
fn restore_snapshot(sync_dir: &Path) -> Result<Option<(PersonalDict, String)>, String> {
    let mut snaps: Vec<PathBuf> = fs::read_dir(sync_dir)
        .map_err(|e| format!("store: list {}: {e}", sync_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("snapshot-") && n.ends_with(".gz"))
                .unwrap_or(false)
        })
        .collect();
    snaps.sort();
    for snap in snaps.iter().rev() {
        let bytes = fs::read(snap)
            .map_err(|e| format!("store: read {}: {e}", snap.display()))?;
        let mut dec = GzDecoder::new(&bytes[..]);
        let mut s = String::new();
        {
            use std::io::Read;
            if dec.read_to_string(&mut s).is_err() {
                continue; // corrupt gzip: try the next older snapshot
            }
        }
        if let Ok(dict) = PersonalDict::from_jsonl_strict(&s) {
            let name = snap
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("snapshot.gz")
                .to_string();
            return Ok(Some((dict, name)));
        }
    }
    Ok(None)
}

/// Apply `wal.log` lines on top of `dict` (last-writer-wins per key via
/// normal learn-path overwrite). Returns lines applied. A malformed WAL line
/// is an explicit `Err` naming the line number — the caller decides, but the
/// [`recover_personal`] path treats WAL errors as non-fatal (snapshot data is
/// kept) while still reporting them in the status detail.
fn replay_wal(sync_dir: &Path, dict: &mut PersonalDict) -> Result<usize, String> {
    let wal = sync_dir.join("wal.log");
    let src = match fs::read_to_string(&wal) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("store: read {}: {e}", wal.display())),
    };
    let mut n = 0;
    for (i, line) in src.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        dict.apply_wal_line(line)
            .map_err(|e| format!("store: {} line {}: {e}", wal.display(), i + 1))?;
        n += 1;
    }
    Ok(n)
}

/// Atomic file write: tmp in the same directory + rename, so a power cut
/// never leaves a half-written `personal.jsonl` / snapshot.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("snapshot")
    ));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn open_applies_pragmas_and_versions_once() {
        let d = tmp_dir("kbcore_store_open");
        let db = d.join("kb.sqlite");
        let store = Store::open(&db).unwrap();
        let mode: String = store
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        let sync: String = store
            .conn
            .query_row("PRAGMA synchronous", [], |r| r.get::<_, i64>(0))
            .map(|v| v.to_string())
            .unwrap();
        assert_eq!(sync, "1"); // NORMAL
        let ver: String = store
            .conn
            .query_row("SELECT v FROM meta WHERE k='schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, SCHEMA_VERSION.to_string());
        // Reopen: no VACUUM, version stable, data intact.
        drop(store);
        let store2 = Store::open(&db).unwrap();
        let ver2: String = store2
            .conn
            .query_row("SELECT v FROM meta WHERE k='schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver2, SCHEMA_VERSION.to_string());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn newer_schema_refuses_to_open() {
        let d = tmp_dir("kbcore_store_newer");
        let db = d.join("kb.sqlite");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(META_SCHEMA).unwrap();
            conn.execute(
                "INSERT INTO meta (k, v) VALUES ('schema_version', '999')",
                [],
            )
            .unwrap();
        }
        let err = Store::open(&db).unwrap_err();
        assert!(err.contains("newer than supported"), "unexpected: {err}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn flush_writes_only_dirty_rows() {
        let d = tmp_dir("kbcore_store_flush");
        let db = d.join("kb.sqlite");
        let sync = d.join("sync");
        let mut store = Store::open(&db).unwrap();
        store.set_sync_dir(Some(sync.clone()));
        let mut dict = PersonalDict::new();
        dict.learn("alpha", "EN");
        dict.learn("beta", "EN");
        let mut log = SessionLogger::new();
        let s1 = store.flush(&mut dict, &mut log).unwrap();
        assert_eq!(s1.personal_rows, 2);
        assert!(!dict.is_flush_dirty());
        // Second flush with one new word writes exactly one row.
        dict.learn("gamma", "EN");
        let s2 = store.flush(&mut dict, &mut log).unwrap();
        assert_eq!(s2.personal_rows, 1);
        assert_eq!(s2.bigram_rows, 0);
        // WAL got both batches; DB holds all three words.
        let wal = fs::read_to_string(sync.join("wal.log")).unwrap();
        assert_eq!(wal.lines().count(), 3);
        let loaded = PersonalDict::load_from_sqlite(&db).unwrap();
        assert_eq!(loaded.get("alpha").unwrap().count, 1);
        assert_eq!(loaded.get("gamma").unwrap().count, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compact_snapshot_and_recover_clean() {
        let d = tmp_dir("kbcore_store_compact");
        let db = d.join("kb.sqlite");
        let sync = d.join("sync");
        let mut store = Store::open(&db).unwrap();
        store.set_sync_dir(Some(sync.clone()));
        let mut dict = PersonalDict::new();
        dict.learn("hello", "EN");
        let mut log = SessionLogger::new();
        store.flush(&mut dict, &mut log).unwrap();
        let snap = store
            .compact_sync_folder(&dict.to_jsonl(), &dict.bigrams_jsonl())
            .unwrap()
            .unwrap();
        assert!(snap.exists());
        assert!(sync.join("personal.jsonl").exists());
        assert!(sync.join("bigrams.jsonl").exists());
        assert_eq!(fs::read_to_string(sync.join("wal.log")).unwrap(), "");
        let rec = recover_personal(&sync).unwrap();
        assert!(matches!(rec.status, RecoveryStatus::Clean { .. }));
        assert_eq!(rec.dict.get("hello").unwrap().count, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupt_jsonl_restores_snapshot_plus_wal() {
        let d = tmp_dir("kbcore_store_recover");
        let sync = d.join("sync");
        fs::create_dir_all(&sync).unwrap();
        // Snapshot with one word + WAL with a second word.
        let mut dict = PersonalDict::new();
        dict.learn("snapword", "EN");
        let jsonl = dict.to_jsonl();
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(jsonl.as_bytes()).unwrap();
        fs::write(sync.join("snapshot-100.gz"), enc.finish().unwrap()).unwrap();
        let mut dict2 = PersonalDict::new();
        dict2.learn("walword", "EN");
        fs::write(sync.join("wal.log"), dict2.to_jsonl()).unwrap();
        fs::write(sync.join("personal.jsonl"), b"{corrupt!!!").unwrap();
        let rec = recover_personal(&sync).unwrap();
        assert!(
            matches!(rec.status, RecoveryStatus::RestoredFromSnapshot { .. }),
            "status: {:?}",
            rec.status
        );
        assert!(rec.dict.get("snapword").is_some());
        assert!(rec.dict.get("walword").is_some());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn power_coalesced_flush_writes_dirty_only() {
        // Power/flash-wear evidence (plan 06): with a full 10k personal
        // table resident, a single accept must cost O(dirty) rows — not a
        // 10k-row rewrite. Timings are printed for the power-trace record;
        // the hard assertion is on rows written (timings vary by host).
        let d = tmp_dir("kbcore_store_power");
        let db = d.join("kb.sqlite");
        let mut store = Store::open(&db).unwrap();
        let mut dict = PersonalDict::with_cap(10_000);
        for i in 0..10_000 {
            dict.learn(&format!("w{i:05}"), "EN");
        }
        let mut log = SessionLogger::new();
        let t0 = std::time::Instant::now();
        let base = store.flush(&mut dict, &mut log).unwrap();
        let base_dt = t0.elapsed();
        assert_eq!(base.personal_rows, 10_000);
        dict.learn("one-more-word", "EN");
        let t1 = std::time::Instant::now();
        let inc = store.flush(&mut dict, &mut log).unwrap();
        let inc_dt = t1.elapsed();
        println!(
            "power: full 10k flush = {} rows in {:?}; single-accept flush = {} row(s) in {:?} \
             (flash writes per keystroke reduced ~{}x)",
            base.personal_rows,
            base_dt,
            inc.personal_rows,
            inc_dt,
            base.personal_rows.max(1) / inc.personal_rows.max(1),
        );
        assert_eq!(inc.personal_rows, 1);
        assert_eq!(inc.bigram_rows, 0);
        assert_eq!(inc.session_rows, 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn double_corruption_starts_empty_with_notify_detail() {
        let d = tmp_dir("kbcore_store_double");
        let sync = d.join("sync");
        fs::create_dir_all(&sync).unwrap();
        fs::write(sync.join("personal.jsonl"), b"{corrupt").unwrap();
        fs::write(sync.join("snapshot-1.gz"), b"not gzip at all").unwrap();
        fs::write(sync.join("wal.log"), b"{also corrupt").unwrap();
        let rec = recover_personal(&sync).unwrap();
        match rec.status {
            RecoveryStatus::EmptyAfterCorruption { detail } => {
                assert!(detail.contains("empty personal"), "detail: {detail}");
                assert!(detail.contains("base packs unaffected"), "detail: {detail}");
            }
            other => panic!("expected empty-after-corruption, got {other:?}"),
        }
        assert!(rec.dict.is_empty());
        let _ = fs::remove_dir_all(&d);
    }
}
