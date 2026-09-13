# 20 — Dictionaries Manager + Quick-Sheet + Medical Canon

> Split 2026-09-13 from old `18-settings-ia-reorg.md`. This file is the single owner of `medical`.

## Problem

Tabs `E/ने/JS/Rs/HTML/😀/∑/⚕/★/123` in 48dp `ScrollableTabRow` (`CategoryTabs.kt:137-160`) are cryptic; hide-tab buried; `medical` missing from `DEFAULT_CATEGORIES:31-33` + `LayoutStore.KNOWN_CATS:31-42`; IDs drift (`words/ne` vs `E/ने/★` vs `English/नेपाली` vs docs `EN/NE` `USER.md:29-38`).

## Fix

### Step 1 — Unified searchable tab manager

Single list (built-ins + customs): `badge | full name (English/नेपाली/…) | enable toggle | drag priority | layout badge | learns badge`. Replaces stack + customs pages + popup duplication. Drop manual Refresh (`collectAsState`/resume reload). Keep Up/Down accessible path. Friends see two toggles first (`English [on], नेपाली [on]` + `Hide code tabs`) — power-user drag UI stays below, not above.

### Step 2 — Canonicalize labels + medical

Canonicalize all labels through `CategoryLabels.kt:21-77`; add `medical` to `DEFAULT_CATEGORIES`/`KNOWN_CATS` or document why hidden. `canonicalId(label(id))==id` round-trip must hold. Priority under hard tab filter only tie-breaks within a tab — do not sell drag-drop as language prioritization (see `08`).

### Step 3 — Quick-sheet (current tab only)

In-keyboard `⚙` sheet edits current tab only (layout pills, enable/disable, undo, incognito, clipboard). Dictionaries row is the single editor; sheet never forks state.

## Tests

* Per-tab layout override works for `medical`; round-trip holds.
* Coach + trial row survive process death (cf. `dictus-android#19`).
