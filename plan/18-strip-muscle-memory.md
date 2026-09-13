# 18 — Strip + Space/Backspace Muscle Memory (T9 pad)

> Split 2026-09-13: this file owns all T9 strip rendering + space/backspace behavior (was half of old `17` + strip section of old `18`). QWERTY has its own contract in `22-qwerty-parity.md`. Settings IA → `19`, dictionaries → `20`, onboarding → `21`.

## Problem

Even with correct engine results (`17`), the strip can still feel broken: blank on single-press, bare-space commit, raw digits shown as composing text, no literal slot, no undo. Muscle memory from Nokia/Gboard: tap-space types, backspace deletes one step, eyes stay on keys.

## Contract

### Step 1 — Strip rendering (never empty, leftmost #1)

- When `query.len==1` or result empty: render `key_label` chars (`pqrs` for `7` via `T9Mapping::key_label`) as tappable chips committing via `commitRaw/onSymbolPick 1813-1821`. Chips are a **view of the core union from `17`**, not a second ranking path (single code path; keeps `lastShown` learn in `commitCandidateWithTerminator:1430-1463` consistent).
- Always render the core union — never chips-only-when-empty (avoids layout flicker at len1→len2).
- Ordering: **leftmost position 0 = best scored** (`cmp_scored:668-676` + `17` short-exact boost makes len-1 #1 the top letter anyway). Rejected: "center best" rocker idiom (Nokia/Samsung `US8826167`) — wrong for touch `LazyRow`; position 0 is fastest tap/TalkBack target. Literal-always-first also rejected — breaks top-3@4.
- 3 visible + `...` overflow; tap commits (`pickSuggestionManually`); long-press → pin/block (Gboard trash); frequency re-rank on select. `*` cycles (`227 car>cap`) and hold-`7` for OOV spelling stay P1 (need control remap).
- Composing header: show top candidate preview, never raw digits (`onPadCode:833` `setComposingText(seq)` leaks `7` into the field — fix to preview; keep digit seq internally). Debug `7 (pqrs)` header is not permanent UI.

### Step 2 — Space (commit #1, never bare)

- 1-press + Space (or fling-`↑`/`→`, or tap-#1): commit strip #1 = top fallback letter (`7→s`, `2→a`) + terminator (`autoSpaceUi`). **Never bare `" "` while `seq` non-empty.** Eyes-free tap-space types letters.
- 2-press / N-press + Space: identical rule — commit #1 ranked (exact-len2 first, then prefix). Bare space only when `seq` and `qwertyBuffer` both empty (word break). Code tabs keep `COMMIT_PLUS_TERMINATOR` (`;`) path (`1719-1728`).
- Space after single-tap commits top chip, not `" "` (Android test in `17`).

### Step 3 — Backspace (one digit, cascade)

- `⌫` while composing: delete exactly one digit, re-query; cascade per press; `seq` empty → `finishComposingText` (current `1499-1503` correct). Never delete whole composing, never touch committed text.
- `⌫` after commit (`seq` empty): delete 1 committed char + reject iff within 5s (current `1512-1515` + SPEC §2 correct). Never resurrect composing; restore path is strip Undo 5s / fling-`←` word-preview (`05` gestures + `onPadFlingDelete:1551-1565`), not `⌫`.
- Temporal re-rank (`23` P1) stays gated to len>=2/scored-200 — never in len-1/empty strip (per `13` lesson: no zero-prefix routine push in main strip).

## Tests

* Single tap → chips non-empty; space after single tap commits top chip, not `" "`.
* Backspace cascade: `4355→435→43`, each step re-queries; empty → composing finished.
* No digit residue after commit (audit `commitRaw:1483-1489` vs bare `commitText:1491-1493`).

## Order

`17` core before this file. Both before `26` spikes / routine row / voice.
