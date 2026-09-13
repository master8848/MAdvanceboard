# 01 — Gestures Reserved for Actions (No Swipe-Typing)

Decision: taps = typing only. All gestures = non-writing functions: switch category, accept suggestion, QWERTY toggle, delete/space, symbols, hide.

## Zone separation (don't overload same swipe)

- `Pad` (9 keys): 4-direction flings only. No tab-switch here.
- `CategoryTabs` row: horizontal scroll/swipe only. No delete here.
- `SuggestionBar`: tap + horizontal fling only.
- QWERTY FAB: button-first, gesture secondary (avoids Samsung spacebar-language-switch false triggers).

## Core map — 9-key default

- tap key → emit digit, predict. No change.
- fling `←` on Pad → delete. Short: revert last keypress; continue sliding left: word-by-word select-preview, release commits. Slide back `→` shrinks. Undo in SuggestionBar for 5s.
- fling `↑` on Pad → space + commit top candidate (`accept()` → learning). If not composing: plain space.
- fling `→` on Pad → accept top suggestion (= tap candidate #1). Symmetric with `←`=delete.
- fling `↓` on Pad → hide keyboard. Never symbols.
- long-press ~400ms no-move → symbols sheet for that key (`SPEC.md:14-25` alt table). Release commits.
- SuggestionBar: tap = accept that word (+auto-space setting); fling `→/←` = accept #1 / cycle overflow; tap `∨` / swipe-up = expand-all sheet (30, paged).
- CategoryTabs: tap / h-scroll = switch category (sets dict filter + layout resolve). Swipe stays in strip.
- QWERTY FAB tap = toggle `Pad↔QWERTY`. No single-finger Pad gesture for this. Optional advanced: 2-finger swipe `↑`.

## Amendment 2026-09-13 — QWERTY flings must share the space path (see 22)

Pad `↑` (space+commit) and `→` (accept #1) must call the SAME `onQwertySpace/onDelete` used by physical Space/⌫ (with `lastAuto` correction-restore logic), not a separate `commitCandidate(top)`. Two space paths with different confidence handling invite accidental autocorrect on QWERTY. Physical Space/⌫ stay authoritative; flings are duplicates. Tap-commits for emoji/code must not clear `lastAuto` needed for backspace-restore.

Rejected: Pad `←→` for category switch (conflicts delete/accept); `↓` for symbols.

## 12-key override

Bottom row `[Sym|Shift|Space/⌫]` is authoritative → disable Pad `←/↑` flings by default. Keep `→` accept, Tabs/Bar unchanged. Setting `Gesture Tuning → Allow pad flings in 12-key` default off, threshold +30% if on. QWERTY view: same (no flings except `→` accept).

## Per-category deltas (keep `←/↓/Tabs` identical everywhere)

- `numbers`: no prediction. `→`/tap noop, `↑` = `0`/space.
- `emoji`: tap commits immediately. Long-press = tone/variant picker.
- `js/rust/html/math`: tap commits keyword/latex; long-press = symbol variant/preview (`{}` vs `function`, `\frac{}{}` skeleton); `↑` commits + space or `;` per pack setting.
- `★personal`: same as EN/NE.

## Detection (ship defaults, tunable in Settings → Gesture Tuning)

- fling `>24dp`, `>200dp/s`, dominant-axis `>1.4:1`. Word-step `20dp/word`, char-step `10dp`. Long-press 400ms (300-600 setting), slop `8dp` cancels to tap.
- Gesture must start+end inside Pad bounds (except Tabs/Bar). Edge `8dp` dead-zone. Tap wins if `<200ms` + `<8dp`; else evaluate on `ACTION_UP`. Live preview only for delete-selection.
- Haptic tick on threshold cross. All gestures duplicated by visible UI.
- Log false-trigger rate to `log.jsonl` for tuning.

## Discoverability + accessibility

- First-run 3-step coach (`←` delete / `↑` space / `→` accept), Skip + replayable. Faint footer `← del · ↑ space · → accept` first 3 days (dismissable).
- If TalkBack/ExploreByTouch/SwitchAccess/VoiceAccess on: disable Pad flings. Expose focusable 48dp buttons: Delete, Space, Accept-#1, Next-Category, QWERTY, Sym. D-pad (TV) / VR controller: arrows move focus `Tabs→Bar→Pad→FAB`, Center=tap, Long-OK=symbols, Back=delete. Gaze/dwell: 800ms=tap, 1500ms=long-press, on-screen Delete/Space/Accept cluster, no gaze-fling.
- Borrow AOSP `PointerTracker` thresholds; follow SwiftKey `ExploreByTouch` precedent.
