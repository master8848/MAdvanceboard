# 12 — Snippet vs Learn Gates (Owner Decision: Exempt Snippet)

Owner call: snippet is current-document scratch, learn is keyboard-long memory.
So split the single `shouldLearnNow()` gate into two. Plan only, no code.

## Definitions

- **Persistent learn** = `PersonalDict.entries` + `bigrams` + `session_events`
  (`core-rust/src/personal.rs:47-50`, `session.rs:112`). SQLite-backed,
  survives restart, drives future `suggest()` via +1.2 personal / +0.8 bigram
  (`core-rust/src/rank.rs:32-33`). Cap 10k LFU.
- **Snippet 2-min memory** = `07-code-math-palettes.md:12` ephemeral helper.
  `if` → suggest `if (` → then next 2-3 syntax tokens
  (`{ } : ; ` " '`) in same layout. RAM only, max 2-3 tokens, ~2min TTL,
  delete after expansion commits. Never SQLite, never sync, never feeds rank
  weights beyond the visible strip.

## Decision

Exempt snippet from `privacy.learn:false`. Rationale: snippet dies on its own
(TTL + delete-after-expand), so OTP/phone pollution risk that justifies
`numbers` no-learn does not apply the same way — but per-tab usefulness does.

## Gates (freeze these names)

- `learnNow = !password && !incognito && isTextClass && privacy.learn(tab)`
  - `password`: `isPasswordField()` (`KbInputMethodService.kt:485`) → always false.
  - `incognito`: system `FLAG_NO_PERSONALIZED_LEARNING` + `privateImeOptions`
    token + manual mask-key (`KbInputMethodService.kt:499`, `11:2`) → false.
  - `isTextClass`: `InputMode.TEXT` only (`:472`). NUMBER/PHONE/DATETIME → false.
  - `privacy.learn(tab)`: internal constant, `numbers=false, math=false`
    (`:464`). Storage hygiene, not a user toggle (`11:1`).
  - `email/URI` variation → suggest but `learnNow=false` (high-entropy, low-reuse).
- `snippetNow = !password && !incognito && isTextClass && tab != numbers`
  - `numbers` blocked: digit-commit layout cannot show `{ }`, and OTP digits
    lingering even 2min in RAM is risk for zero benefit.
  - `math/js/rust/html` allowed even though `math` has `learn=false`:
    `\frac{}{}`, `^{}/_{}`, `{ } ;` helpers are the whole point of the tab.
  - `email/URI`: allow snippet (syntax chars still useful), block learn.
  - Password + incognito: both gates false, no difference. Snippet buffer
    cleared immediately on entry + on exit (`11:2`), no TTL carryover.

Truth table:

| case | learnNow | snippetNow |
|---|---|---|
| normal EN/NE/js | true | true |
| numbers tab | false | false |
| math tab | false | true |
| password field | false | false |
| incognito | false | false |
| email/URI | false | true |
| NUMBER/PHONE/DATETIME | false | false |

## Lifecycle (snippet buffer)

- Max 3 entries: `(trigger, next-tokens[], expires_at)`. `expires_at` = 1h-quantized
  `now + 2min` — reuse existing `quantize_ts` determinism rule (`04`), so
  `suggest()` stays byte-identical for same `(dict, ctx, digits, tab, layout, hour)`.
- Clear on: expansion commit, tab switch, IME hide, incognito entry/exit,
  password field focus, TTL sweep on next keystroke (no timer thread — check
  expiry lazily to avoid wakeups, see `06` power budget).
- Never written to `session_events.shown_json`, never exported in JSONL/sync.
  Session log records the committed snippet text only if `learnNow` true,
  same as any other commit.

## Acceptance (plan-level, no code)

- Type OTP in `numbers` → nothing in personal, nothing in snippet strip.
- Type `if` in `js` → `if (` suggested; accept → `{ }` strip for 2min; switch
  to EN → strip gone; reopen `js` after 3min → gone.
- Type `\fr` in `math` → `\frac{}{}` suggested though nothing learned.
- Enter incognito mid-snippet → buffer wiped, history hidden, dark strip.
- Password field → neither gate fires, no clipboard capture (`11:3`).

## Decision (frozen 2026-09-13)

`snippetNow` allows everywhere except `numbers` — simpler, one exclusion. The `tab in (js,rust,html,math)`-strict alternative is rejected: no second gate to maintain, negligible cost.
