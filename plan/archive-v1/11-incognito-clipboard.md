# 11 — Incognito + Clipboard History (Pins, 7-Day Expiry)

## 1. Learn framing (owner decision — always local, no off-switch)

- Learn is always local: personal dict + bigrams + session log live in on-device SQLite (`core-rust/src/personal.rs:282`, `session.rs:112`), no `INTERNET` in IME (`android/README.md:36`), sync is opt-in file export only (`docs/SYNC.md:1`). There is no server to switch off from — agreed, no user-facing "disable learning" toggle.
- `privacy.learn:false` for `numbers`/`math` (`KbInputMethodService.kt:464`, `USER.md:57`) is **storage hygiene, not privacy**: every OTP/phone number is unique — learning them pollutes personal + leaks sensitive numbers into future suggestions. Keep as internal constant, remove from UI as a toggle. Don't generalize to user-switchable per-pack learning.
- Real off-switches (automatic, not settings):
  - password fields (`isPasswordField()`, `KbInputMethodService.kt:485`) → skip learn + bigram + log + clipboard capture, always.
  - email/URI variations (`isEmailOrUri()`, `:508`) → suggest but suppress learn (addresses are high-entropy, low-reuse).
  - non-TEXT `InputMode` (NUMBER/PHONE/DATETIME, `:472`) → commit raw, no prediction/learn.

## 2. Incognito mode (was forgotten — now explicit)

Triggers (OR): system `IME_FLAG_NO_PERSONALIZED_LEARNING` (browsers, private mode) + `privateImeOptions` token `incognito/noPersonalizedLearning` (`KbInputMethodService.kt:499`) + manual keyboard toggle (new incognito key in toolbar, mask icon) + auto-on for password fields.

While incognito:
- `shouldLearnNow()` false → no `learn/record_reject/record_bigram`, no session `log_accepted/log_rejected`.
- Snippet 2-min helper (`07-code-math-palettes.md`) cleared immediately on entry + on exit — no TTL carryover.
- No clipboard capture (see §3). Existing history hidden (not deleted) until exit.
- Visual: dark suggestion strip + mask badge + "Incognito — nothing saved" snackbar on entry. No persistence of the flag itself across restarts (always start non-incognito unless app requests it).

Code gap: `FIX_REPORT.md:25` gate exists (`:405`, `:278`), `LEAN_QWERTY_AUDIT.md:32` flagged missing — verify `learnByCategory` + incognito + input-class are all consulted in one `shouldLearnNow()` before closing.

## 3. Clipboard history (new — needed even for basic testing)

Why: without it every paste-test requires retyping. What:

- Capture: plaintext committed via this IME + system clipboard changes while keyboard active, **except** password fields + incognito sessions (never stored, never shown). Cap item 5k chars, history cap 100 unpinned items (ring).
- Retention: default **7 days**, Settings → Clipboard → preset `1h / 24h / 7d / 30d` + custom hours. Unpinned items auto-delete past TTL (WorkManager daily sweep, same 24h budget as sync). **Pinned items exempt** from TTL (persist until unpinned/deleted) — pins are the user's explicit "keep."
- UI: clipboard tab/row (next to category tabs) + long-press paste menu. Tap = paste, long-press = pin/unpin + delete + preview. Pins section on top, recency below. Search filter (substring). Clear-all (except pins? confirm dialog: "Clear pins too?").
- Privacy: on-device only, never in `export_session`/sync files (`docs/SYNC.md`1938 files are personal+bigrams+session only — clipboard excluded unless user explicitly exports). Clear on uninstall. `Clear` in Settings wipes unpinned + optionally pins.
- Acceptance: copy 3 items, pin 1, advance clock 8 days → unpinned gone, pinned stays; password-field copy never appears; incognito copy never appears; TTL change 7d→24h applies on next sweep without data loss for pins.
