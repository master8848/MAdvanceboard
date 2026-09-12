# Sync — JSONL Export / Import / File-Sync

No accounts, no server. Sync is file-based: you pick a folder (local /
WebDAV / Drive), the app reads/writes JSONL there. Opt-in only.

## 1. Row schema

Every dict row (base, pack, personal, session delta) is one JSON object
per line:

```json
{"w":"काठमाडौं","lang":"ne","cat":"NE","seq":"524...","freq":120,"acc":5,"rej":1,"ts":1726000000,"del":false,"id":"ne:काठमाडौं"}
```

| Field | Meaning |
|---|---|
| `w` | surface form |
| `lang` | language tag |
| `cat` | category tab |
| `seq` | T9 digit sequence |
| `freq` | base + personal count snapshot |
| `acc` / `rej` | monotonic accept / reject counters |
| `ts` | HLC timestamp (device clock + counter) |
| `del` | tombstone flag (blocked). Canonical name per SPEC §5; readers also
accept legacy `deleted` (SPEC §4) — core via `serde(alias)`, Android via
`SyncMerge.fromJsonl` compat. |
| `id` | word id `"<lang>:<normalized-word>"`, dedupe key |

## 2. Export / import (session replay)

- Export: `session-<date>.jsonl.gz` = personal delta + `log.jsonl` of
  events `{ts, seq, ctx, chosen, shown, action}` (accepted/rejected).
- Import replays the log through the ranking function to rebuild counters
  deterministically (`core-rust/session.rs`, `Predictor::export_session`).
- Pack rows are never in the personal export; pack updates never overwrite
  `personal.jsonl`.

## 3. File-sync layout

```
<sync-folder>/personal.jsonl   # current personal table, compacted hourly
<sync-folder>/bigrams.jsonl    # personal bigram counts
<sync-folder>/snapshot-<ts>.gz # hourly compacted snapshot
<sync-folder>/wal.log          # append-only writes since snapshot (2 s batches)
```

Every write is append-to-WAL, then batch-compact hourly.

## 4. Merge rule

Per word-id: **last-write-wins on the record, max-merge on counters.**

1. Group by `id`. Keep the record with max `ts`.
2. `acc / rej / freq = max(a, b)` element-wise (counters are monotonic;
   decay bumps `ts` so it survives).
3. `del` tombstone wins iff its `ts` is newest — deletes propagate, never
   resurrected by stale replicas.

Android `sync` module does LWW per-word merge in a 24 h `WorkManager` job.

## 5. Auto-recovery

On startup:

1. Load `personal.jsonl`. If corrupt → restore newest `snapshot-<ts>.gz`,
   replay `wal.log`.
2. If both corrupt → start with an **empty personal** table, keep base
   packs functional, notify the user.
3. `device_id` breaks `ts` ties (HLC: single-device `ts + deviceId`
   suffices; no server clock trust).

Related: `PRIVACY.md` (opt-in, tombstones), `USER.md` (block/forget),
`TUNING.md` (decay/cap before sync).
