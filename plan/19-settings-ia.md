# 19 — Settings IA: 4 Sections + Search + Quick-Sheet

> Split 2026-09-13 from old `18-settings-ia-reorg.md`. Dictionaries manager → `20`, onboarding → `21`, strip → `18`.

## Problem

Settings split across 3 places with no search: (a) 8-page in-app Settings (`SettingsActivity.kt:80-104` routes `home,appearance,layout,suggestions,clipboard,dictionary,gestures,tuning,privacy` + `DictStackSettings.kt:50-193` + `ClipboardSettings.kt:32-138`), (b) in-keyboard `PlacementPopup`/`SlimToolbar`/`⚙→Layout only` (`ImeScreen.kt:218-260`), (c) system enable wizard (`OnboardingActivity.kt:53-90`, enable-only). `Gestures` vs `Tuning` split with wrong hint (`GestureConfig.kt:110-111`); manual `Refresh` everywhere; live-vs-restart wording differs 3x; docs stale (`USER.md:75-76` personal search, `TUNING.md:36-40` weight sliders — real Tuning is 5 gesture sliders only `SettingsActivity.kt:735-739`).

Model: Gboard 2025 reorg `15.8.4` (≤7 cards, `Layout / Shortcuts / Key tap / Corrections & suggestions`), HeliBoard (`Languages & layouts / Correction / Glide / Theme / Toolbar / Clipboard + Backup`), FlorisBoard (live preview on top), Android docs (IME must link to App page).

## New IA — 4 sections + search

### Step 1 — Four sections

* `Typing`: strip + auto-space + layout global + per-tab overrides (incl. `medical`) + QWERTY hint + 12-key opt-in + code terminator. One place for "how typing feels". Friend-defaults (see `24`): hide `js/rust/html/math/medical` behind "Show code tabs" for non-technical users — dev noise must not greet friends.
* `Dictionaries`: see `20` (single editor).
* `Appearance`: theme + footer-hint + coach replay + incognito/clipboard mirrors.
* `Advanced`: detection sliders + gesture log + export/import (stack + packs JSON) + sync opt-in + reset-to-defaults. Buried — not a card friends tap.

### Step 2 — Search + quick-sheet

* Top-level search `filter()` over titles (even dumb filter beats maze).
* In-keyboard `⚙` → bottom-sheet (layout for *current tab only* 3 pills, enable/disable current tab, undo, incognito, clipboard) instead of deep-link to full Layout page. Long-press placement = Details → jumps to Dictionaries row (avoid two editors diverging).

## Tests

* All 8→4 routes resolve; search filters every row; no dead doc links.
* Quick-sheet edits current tab only; Dictionaries row is single editor.
