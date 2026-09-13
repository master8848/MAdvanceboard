# 17 — Single-Press Core Fallback (`len<=2` never empty)

> Split 2026-09-13: core half stays here; Android/strip half → `18-strip-muscle-memory.md`; settings halves → `19/20/21`; new QWERTY contract → `22-qwerty-parity.md`.

## Problem

Pressing once the key containing `S` (key `7=pqrs`, digit `7`) shows wrong/empty strip. User expects `S` words.

Mapping is correct (`mapping.rs:33` `S->7`, `key_label(7)=pqrs` `mapping.rs:140`). Engine `suggest("7")` does prefix-match (`stack.rs:1566-1576,1950-2086`, no len-gate, `hide_threshold=-0.5` `rank.rs:32` cannot hide same-tab rows). Root cause is data + UI:

1. Zero single-letter `7` rows — `packs/sources/words_en.txt` only `a 600000` (`seq=2`); no `s/i/p/q/r`. All `7` hits are multi-letter prefixes (`some/school/sad…`).
2. Android blank-strip + space-eats-press: `KbInputMethodService.onPadCode:815-841` composes `7`, `refreshSuggestions:1010-1034` maps to words only, `SuggestionStrip:57-61` renders `[]` as blank, `onPadSpace:878-890` commits bare `" "` when `top==null`, `acceptTopCandidate:1751-1759` noop on empty. Only long-press `1780-1804` gives letters.
3. Cross-tab isolation (`stack.rs:1089-1095`) drops English `7*` when active tab != `words`. (EN folds to `words` via `tab_key:568-576`, so this bites non-EN tabs.)

Reference behavior (Nokia T9 / TT9 / Type Nine iOS / Samsung `US8826167` / JP flick): prefix `WHERE code STARTSWITH seq ORDER BY freq`, center most-frequent letter, hold-for-letter picker, `*` Next-cycle. Never empty.

## Fix (P0, days)

Scope rule: synthetic fallback applies **only** when `len<=2 && !qwertyFallback && t9-layouts`. QWERTY single-letter is the literal (see `22`), never a `pqrs` chip. Fallback chips are layout-derived, not dict-derived — the **one explicit exception** to cross-tab isolation (`stack.rs:1089-1095`); they commit via `commitRaw` (no-learn) so typing `s` under emoji never learns `s`-as-emoji.

### Step 1 — Core synthetic fallback for `len<=2`

In `suggest_inner`/`build_union` inject `key_label(last-digit)` chars as `keyfit=1.0` candidates with fixed freq (`a:600k` parity, `i:500k`, others low). Guarantees `suggest("7",words)` contains `s+p/q/r`, `suggest("2")` contains `a`.

- Exempt fallback rows from the 200 pre-truncate (`stack.rs:1832-1839` freq-desc truncate kills low-freq synthetics inside 644-row `7*` unions) — append post-truncate instead of relying on inflated parity freqs.
- Give synthetics an explicit `cat/lang/priority` so `merge_row:1886-1904` can't double-sum them with seeded rows.
- Prefer runtime fallback over pack edits (avoids pack-edit drift). Do NOT do both seed-rows AND synthetics — pick one (fallback) to avoid `merge_row` double-count.

### Step 2 — Short-exact boost

`len==digits.len()` + `len==1` outranks longer prefixes (`s` before `some`); keep freq tie-break after. Do not raise global `w_keyfit` (hurts per `rank.rs:20-22`; current exact-vs-prefix margin is only `0.02` — targeted bonus only).

### Step 3 — Cache-key versioning

Prefix cache (`05`) keyed `(layout_id, digits)` must include fallback-version/pack-set once synthetics land, or len-1 results go stale across pack updates.

## Tests

* `stack.rs` + `per_tab` gate: `suggest("7","words")` contains `s`; `suggest("2")` contains `a`; len-1 ordering (`s` before `some`).
* No regression: len>=3 ordering unchanged; neighbor-OFF policy untouched (`stack.rs:600-606`).
* QWERTY typing `s`/`a` shows literal, never `pqrs` chips (see `22`).

## Out of scope

Hold-picker radial + `*` cycling (needs control remap — `*` is delete today) + bigram re-rank → P1 spikes (see `26-spikes.md`). Strip rendering + space/backspace contract → `18`.
