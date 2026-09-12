# 06 — Persistence + Power (Single Handle, Batch, LRU)

Hot path never touches disk. In-memory personal is source of truth for `suggest`; SQLite is durability only.

## From (today) → to

- Today: `open_db` per save (`core-rust/src/personal.rs:275`, `core-rust/src/session.rs:103`): `open + PRAGMA journal_mode + CREATE TABLE×3 + DELETE×2 + reinsert 10k rows + commit`. O(n) flash wear per accept. `SessionLogger.events` unbounded `Vec`; `save_to_sqlite` rewrites whole `session_events` table (`core-rust/src/session.rs:112`).
- To: one `Connection` behind existing `Mutex<Inner>` (already single-writer — safe). Pragmas: `journal_mode=WAL; synchronous=NORMAL; temp_store=MEMORY; cache_size=-4096; journal_size_limit=1MB; busy_timeout=5s`.
- Dirty-set + 2s coalesced flush (core thread or Android WorkManager-scheduled), `INSERT … ON CONFLICT(key) DO UPDATE` only dirty rows. Never `DELETE+reinsert`. Hourly `wal_checkpoint(TRUNCATE)` + snapshot `.gz`; `VACUUM` only on version bump.
- Session log: in-memory ring cap 500, append-only INSERTs, prune `DELETE WHERE id NOT IN (SELECT id … LIMIT 5000)`. JSONL stays interchange/export (`core-rust/src/session.rs:57`, `core-rust/src/personal.rs:211`).

## Cache

- Full 10k personal in RAM ~2-3MB — keep it all, SQLite never on read path. If front cache wanted for eviction bookkeeping: 1024 entries (512-2048 range), tombstones exempt. Hand-roll (`HashMap` + clock) — don't add `lru` dep.
- Settle cap dispute in one commit: 20k LRU (SPEC `SPEC.md:98`, matches typing locality) or 10k LFU (code `core-rust/src/personal.rs:10`). Either fits budget.

## Sync files (ties to future `sync.rs`, not this step)

- Layout already frozen: `personal.jsonl`, `bigrams.jsonl`, `snapshot-<ts>.gz`, `wal.log` (`docs/SYNC.md:38`). Every write append-to-WAL, batch-compact hourly. Merge LWW + max-merge + tombstone-wins-newest + HLC `ts+deviceId` (`docs/SYNC.md:49`). This file covers local durability only; merge/recovery is separate work.
