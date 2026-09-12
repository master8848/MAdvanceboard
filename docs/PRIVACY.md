# Privacy

## Permissions

- The IME declares **only** `BIND_INPUT_METHOD` (`android/README.md`). No
  `INTERNET` permission in the shell; sync transport is a file stub.
- No network calls from the keyboard process. Sync is user-driven
  file export/import (see `SYNC.md`).

## No learning in password fields

`KbInputMethodService` checks `EditorInfo.inputType` (`isPasswordField()`)
and skips `Predictor.learn` for password / visible-password / web-password
variations. Nothing typed there touches the personal dictionary, bigrams,
or session log.

## Category opt-outs

Packs can set `privacy.learn: false`. Shipped opt-outs: `numbers`, `math`.
Words committed under those tabs are inserted but never counted.

## Opt-in sync

- Personal-dictionary sync/export is **opt-in only** (`SyncPreferences`,
  off by default).
- Enabling it only writes local files (`personal.jsonl`,
  `bigrams.jsonl`, snapshots, WAL) to a folder you choose. No account,
  no server.

## Tombstones (blocked words)

Blocking a word writes `{del: true}`. Tombstones replicate on sync and are
never resurrected by merge — a delete wins if its `ts` is newest. Unblock
is an explicit user action that bumps `ts`.

## Incognito / what stays local

- Learning lives in on-device storage (SQLite via `core-rust`
  `rusqlite` bundled, WAL + hourly compact). It never leaves the device
  unless you export.
- Session logs (`log.jsonl`: accepted/rejected events) are local replay
  material for import; export includes them only in the session file you
  explicitly save.
- Bigrams are personal-only (`log-prob … backoff 0`, SPEC §2). No shared
  or server model.

## Data safety summary (Play declaration)

- Collected: nothing transmitted. Stored on-device: typed-word counters
  (`freq_personal`, `acc/rej`, `ts`), personal bigrams, blocked-word flags.
- Shared: nothing. Location, contacts, files/media: not accessed.
- Deletion: Settings → Personal dictionary → Clear, or uninstall; exported
  files you created must be deleted by you.

Related: `USER.md` (block/forget), `SYNC.md` (file format + merge).
