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

Blocking a word writes `{del: true}`. Tombstones are local-only
(single-device scope, no cross-device merge): a blocked word stays
hidden on this device until you explicitly unblock it. Unblock
is an explicit user action that bumps `ts`.

## Incognito / what stays local

- **Incognito** (system `IME_FLAG_NO_PERSONALIZED_LEARNING` /
  `privateImeOptions`, manual 🎭 mask-key toggle, or automatic in
  password fields): no `learn`/`record_reject`/`record_bigram`, no
  session `log_accepted`/`log_rejected`, no clipboard capture. The
  snippet buffer, candidates, and the visible clipboard clear on entry
  (persisted clipboard history is HIDDEN, not deleted, until exit); the
  snippet buffer clears again on exit (no TTL carryover either way).
  Visual: dark suggestion strip + 🎭 badge + "Incognito — nothing
  saved" banner. The manual flag is never persisted (always starts OFF
  unless the app requests incognito).
- **Clipboard history** (7-day default, Settings → Clipboard presets
  `1h / 24h / 7d / 30d` + custom hours): on-device only in
  `kb_clipboard_prefs` (cleared on uninstall), RAM-first, TTL-enforced
  on every keyboard start and every capture (no background worker).
  Pinned items persist until unpinned/deleted; unpinned cap 100 (ring),
  item cap 5k chars. Pastes commit raw — never learned, never logged.
  Password-field and incognito copies are never stored, never shown.
  Clipboard items are excluded from sync/export files.

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

Related: `USER.md` (block/forget), `SYNC.md` (file format, local only).
