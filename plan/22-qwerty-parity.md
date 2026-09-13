# 22 — QWERTY Parity: Gboard-Grade Suggestions + Autocorrect

> NEW 2026-09-13. The missing half of the plan: QWERTY must work same as Gboard (suggestions while typing) or non-technical friends cannot daily-drive this keyboard. T9 files (`17`/`18`) never touch QWERTY behavior; this file owns it.

## Problem

Today QWERTY exists only as floating FAB fallback (`KbInputMethodService.kt:89-90,948-956`): `refreshQwertySuggestions:1042-1069` does `encode(raw)->suggest(seq)->map{word}` — typo/unencodable input yields `[]`, engine never returns a verbatim literal, `Suggestion` (`stack.rs:687-695`) has no `is_literal/is_correction/confidence`. Space commits the literal always (inverse of Gboard — never autocorrects), no backspace-restore, no double-space period, no auto-caps. Neighbor-OFF policy (`stack.rs:600-606`) kills typo tolerance, and QWERTY re-encodes through T9 digits, losing `q/w`, `o/p` geometry.

## Target contract (Gboard parity)

### Step 1 — Engine: literal slot + corrections + confidence (additive, no T9 fork)

- Keep `suggest_policy_at:stack.rs:1246` frozen for T9. Add `suggest_qwerty_at(raw,ctx,tab,limit,now) -> {literal, corrections:[{word,score,delta,is_correction}], confident:bool}`:
  - `literal = raw` (always present — AOSP complaint if missing).
  - `seq = encode(raw)`; candidates from `suggest_inner` with `SuggestOpts{include_prefix:true, include_neighbor:true(qwerty-graph), hard_tab_filter:true}`.
  - QWERTY typo model is letter-level insert/delete/substitute/transpose on the QWERTY adjacency graph (`mapping.rs:97`, `layout.rs:100`) **pre-encode**, not T9 digit-neighbors post-collapse.
  - `confident = S(top) − S(literal_scored_as_OOV) > autocorrect_threshold` (new weight, default ~1.0; settings `Corrections & suggestions: aggressive / conservative / off`). Separate from `hide_threshold=-0.5` (display) — never conflate.
  - Strip slots: `[literal left | correction center-bold | completion right] + overflow`. Center is prediction only if confident, else literal repeated unstyled.
- Cache key for QWERTY is `(layout,raw)` — digit cache (`05`) misses on every backspace-restore cycle. Do not apply digit-neighbor tuning to QWERTY confidence; measure false-correction rate separately from `top-3@4/KSPW` (`01`).

### Step 2 — IME state machine: space / punct / double-space / backspace

- Composing vs committed: keystrokes go to `setComposingText` (underlined), strip live-updates per key. One `suggest` per keystroke (today QWERTY costs 2 FFI `encode+suggest:1037-1040` — fix to one).
- `onSpace`: if `confident && autocorrect_on` commit top correction, else literal; record `lastAuto{literal,corrected,ts}` only when a correction is taken + `lastSpaceTs` for double-space. Punct (`.,!?`) commits pick + punct, no extra space. Never autocorrect in password/URI fields.
- Double-space → period: `space` within ~800ms after `word+" "` rewrites to `word+". "`, restarts caps. Add auto-caps (`capsNext` exists `:112,:909` — wire it).
- `onDelete` (muscle memory):
  - While composing: delete 1 composing char, re-suggest.
  - Immediately after auto-correct commit (fresh `lastAuto`, cursor at end): first `⌫` deletes `correction+" "`, restores `literal` as composing + shows `literal` as revert hint; second `⌫` edits literal. Counts as `reject(corrected)`. No restore after cursor move / timeout.
  - Tap candidate commits it + auto-space (if enabled); long-press = remove/forgot. After `commit+" "`, strip shows bigram followers, not corrections.
- Physical `Space/⌫` keys are authoritative for QWERTY. Pad flings (`01`: `↑`=space+commit, `→`=accept) must call the **same** `onQwertySpace/onDelete` (with `lastAuto` logic), not a separate `commitCandidate(top)` — two space paths invite accidental autocorrect. Tap-commits for emoji/code must not clear `lastAuto`.

### Step 3 — Settings surface

`Corrections & suggestions` card (in `19` Typing section): `Aggressive / Conservative / Suggest-only / Off`. Suggest-only = space commits literal, user taps correction. Per-field safety (never in password/URI) is automatic, not a setting.

## Alternatives considered

- Shared `suggest()` with T9 digit-neighbors for QWERTY: rejected — loses QWERTY geometry, mistunes `w_keyfit` (raising it hurts T9 per `rank.rs:20-22`).
- Full neural rescorer (Gboard LSTM/CIFG): rejected at 10k vocab / 50ms / no-INTERNET — freq+bigram+personal suffices; revisit only if budget changes (`26` #6).
- Gesture-typing (glide): stays non-goal. Taps + corrections only.

## Tests

* Typo `teh` → center `the` bold, space commits `the`, `⌫` restores `teh` composing. Suggest-only mode: space commits `teh`.
* `q/w`, `o/p` fat-finger corrected (QWERTY-graph proof); T9 `top-3@4/KSPW` unchanged.
* Double-space → `. ` + caps; password field never autocorrects; FFI count 1/keystroke.
