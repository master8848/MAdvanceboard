# 14 — DB Index Plan (Current State: No Secondary Indexes)

Question asked: do we have indexes? Answer: **only PRIMARY KEYs, zero
secondary indexes.** Verified in `core-rust/src/personal.rs:303-330` +
`core-rust/src/session.rs:103-135`. Landed: the 6-index + `context_counts`
DDL below is now `IF NOT EXISTS` inside `PersonalDict::SQLITE_SCHEMA`
(`core-rust/src/personal.rs`), with `USING INDEX` analytics readers +
acceptance tests in `personal.rs` / `session.rs` / `store.rs`.

## 1. Current schema (frozen, audited)

```sql
personal(key TEXT PRIMARY KEY, word TEXT, category TEXT, count, acc, rej,
         last_seen, deleted);              -- rowid alias on key, auto-index
bigrams(prev TEXT, word TEXT, count, PRIMARY KEY(prev, word));
session_events(id INTEGER PRIMARY KEY AUTOINCREMENT, ts, seq, ctx, chosen,
               shown_json, action);
```

- Loads are full scans by design: `SELECT … FROM personal` (`:388`),
  `SELECT prev,word,count FROM bigrams` (`:430`),
  `SELECT … FROM session_events ORDER BY id` (`session.rs:142`) — all rows
  into RAM at open, then hot path never touches disk (`06:3`). Indexes do
  **nothing** for these; do not add for load.
- Saves are `DELETE + reinsert all` per call (`personal.rs:343-346`,
  `session.rs:115`) over a short-lived `Connection` (`:332`, `session.rs:103`)
  with `PRAGMA journal_mode=WAL` re-executed every open. No index maintenance
  cost today because there is nothing to maintain — but also O(n) flash wear
  per accept (`06:7`). Index plan only pays off **after** `06` lands (single
  handle + dirty-set + `INSERT … ON CONFLICT DO UPDATE`); before that, new
  indexes just slow the full rewrite.

## 2. Index set (add exactly these, in this order, after 06)

```sql
-- 06 lands first: single Connection, WAL+NORMAL+MEMORY pragmas once.
CREATE INDEX IF NOT EXISTS idx_personal_word
  ON personal(word);                        -- OOV promotion check, dedupe
CREATE INDEX IF NOT EXISTS idx_personal_cat_count
  ON personal(category, count DESC);        -- per-tab top-up, stack order 08
CREATE INDEX IF NOT EXISTS idx_personal_last_seen
  ON personal(last_seen DESC);              -- TTL sweep, LFU eviction, 7d clip
CREATE INDEX IF NOT EXISTS idx_bigrams_prev
  ON bigrams(prev);                         -- tiered bigram 05#8: prev fan-out
CREATE INDEX IF NOT EXISTS idx_events_ts
  ON session_events(ts DESC);               -- 14d routine mining (13/P2), KSR
CREATE INDEX IF NOT EXISTS idx_events_chosen_action
  ON session_events(chosen, action);        -- replay harness 09#1, lift A/B
-- 13/P1 only:
CREATE TABLE IF NOT EXISTS context_counts(
  word TEXT NOT NULL, bucket INTEGER NOT NULL, count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(word, bucket)) WITHOUT ROWID; -- 8 buckets, ~1-2k rows, <50KB
```

Why each (and why not more):

- `word`: `learn()` dedupe + `is_blocked()` + OOV `≥2/7d` promotion are all
  point lookups by word today done in RAM; index makes the post-`06` lazy-load
  / partial-fetch path possible without full scan if RAM cache ever shards.
- `(category, count)`: `08` drag-drop stack + per-tab breakdown (`00`) both
  filter by category then order by count. Composite avoids sort.
- `last_seen`: eviction + retention sweeps (`11` clipboard TTL pattern,
  personal 30d reject decay `09#1`) are range deletes — without index they are
  full scans on every daily sweep.
- `bigrams(prev)`: leftmost-prefix of existing PK already covers `(prev,word)`
  lookups, so this is a **covering-index note, not a new index** — verify with
  `EXPLAIN QUERY PLAN`, add only if SQLite does not use PK prefix for
  `WHERE prev=?` fan-out on the NDK SQLite version.
- `events(ts)`, `(chosen,action)`: `09#1` replay + `09#7` KSR + `13/P2`
  routine mining are all `WHERE ts > now-14d` / `WHERE chosen=? AND
  action='accepted'` — today's `ORDER BY id` scan is O(5000) per query, fine
  once, fatal if run per keystroke. These two keep analytics off the hot path.
- `WITHOUT ROWID` on `context_counts`: tiny PK-value table, halves pages.
  Only if NDK SQLite supports it (minSdk26 ships SQLite 3.19 — yes).

Explicit non-goals: no FTS5 (substring clipboard search `11:3` is ≤100 rows,
`LIKE %…%` in RAM is fine), no trigram, no `memmap2` (`05:18`), no partial
index until eviction policy freezes, no auto-`ANALYZE` (run once after bulk
pack import, never per keystroke).

## 3. Migration + perf rules (respect absolute-minimum-resource mandate)

- `CREATE INDEX IF NOT EXISTS` inside the same `execute_batch(SCHEMA)` —
  idempotent, no version table, no separate migration job. First open after
  update pays one O(n log n) build (~10k rows, <50ms once), then free.
- Measure on-device: `EXPLAIN QUERY PLAN` for each new query before/after,
  `dbstat` page count, 1h-trace battery delta (`09#7`). Kill rule: any index
  that grows DB >5% or slows dirty-flush >2ms p95 gets dropped — full scans
  at 10k rows are already <1ms, indexes are for the 14d/30d sweeps, not the
  keystroke path.
- Ordering with other plans: `06` (single handle + upsert) → this file
  (indexes) → `09#1` replay harness → `13/P1` temporal. Doing indexes before
  `06` is wasted (DELETE+reinsert rebuilds them every save).

## 4. Acceptance

- `SELECT name, sql FROM sqlite_master WHERE type='index'` shows the 6 above,
  zero others.
- All analytics queries (`14d routines`, `replay`, `KSR`) show `USING INDEX`
  in `EXPLAIN QUERY PLAN`; all hot-path queries show `USING PRIMARY KEY` or
  pure-RAM (no DB hit).
- Fresh-install open still <100ms on minSdk26 reference device; 30-day DB
  (10k personal + 5k events + 2k context) <5MB total.
