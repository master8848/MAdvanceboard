# Sync — JSONL Export / Import (local only, single-device)

No accounts, no server, no multi-device merge. Sync is file-based local
export/import: you pick a folder (local / WebDAV / Drive), the app
reads/writes JSONL there. Opt-in only. Single-device is the supported
scope: importing a file applies its rows as-is; nothing reconciles
counters across devices.

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

## 4. Merge rule: none (local only)

There is NO multi-device merge. Import applies rows as-is (upsert by
`id`); export writes the local table. An earlier whole-row LWW design
was removed because it silently dropped accept/reject counters — see
`SyncMerge.kt` (its `mergeWord`/`mergeAll` now throw
`UnsupportedOperationException` instead of reconciling).

`del` tombstones (blocked words) are carried in export/import as rows
and stay hidden locally, but they are NOT reconciled across devices:
each device keeps its own block set.

## 5. Auto-recovery

On startup:

1. Load `personal.jsonl`. If corrupt → restore newest `snapshot-<ts>.gz`,
   replay `wal.log`.
2. If both corrupt → start with an **empty personal** table, keep base
   packs functional, notify the user.
3. `device_id` is recorded per row for provenance only (which device
   wrote it). It breaks NO ties — there is no merge. HLC `ts` orders the
   local log.

Related: `PRIVACY.md` (opt-in, tombstones), `USER.md` (block/forget),
`TUNING.md` (decay/cap before sync).
