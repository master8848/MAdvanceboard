# 16 — Docs Refresh: Configurable Board, Not “9-Button Only” (Task)

Living docs still say “9-button keyboard” everywhere. Truth: board is
config-dependent — 9 default (research direction), 12 / 16 / QWERTY per
category via `02` `resolve(cat)`. Symbol / second-symbol / numbers / emoji
paths that QWERTY buries in mode-switches move into category tabs +
thoughtful gestures (`01`). Fix wording once, keep it fixed.

## 1. Scope (living docs only — SPEC/history frozen)

- Edit: `README.md`, `docs/OVERVIEW.md`, `docs/INTENT.md` (§1 title/problem,
  §4 NOT-list, §7 MVP/V2), `docs/USER.md` (§1 title), `docs/ENGINEERING.md`
  (9-key-encode → layout-driven), `core-rust/README.md` + `Cargo.toml`
  description, `android/README.md`, `packs/README.md` §examples.
- Do NOT edit: `SPEC.md`, `INTEGRATION_REPORT.md`, `FINAL_RECONCILE.md`,
  `android/QA_REPORT.md`, `FIX_REPORT.md`, `LEAN_QWERTY_AUDIT.md` — note
  drift instead (they are read-only history).

## 2. Wording rule (apply everywhere)

- Title pattern: `Configurable predictive keyboard (9-key default)`.
- First paragraph must state: 9-key T9 pad is the default research vehicle;
  actual board resolves per category — 9/12 simple, 16 Nepali/complex,
  full QWERTY fallback; all share one `suggest()` engine, only the
  key→`seq` encoder changes.
- Category paragraph: symbol / numbers / emoji / code / math are tabs, not
  QWERTY long-press mazes; gestures are actions (accept/space/delete/hide),
  never swipe-typing (`01`).
- Voice: remove from “non-goals” — now Tier-3 extension per `15`
  (system engine default + optional API). Gesture-typing stays non-goal.

## 3. Concrete edits (checklist)

- [ ] `README.md:1-4` title + lede → configurable framing.
- [ ] `README.md:18-19` What-line + `124` License line → drop “9-key” noun.
- [ ] `docs/OVERVIEW.md:3-6` What/Why → same framing, 2 lines.
- [ ] `docs/INTENT.md:1` title, `:7` research bet, `:38` NOT-list
  (voice → gesture-typing only), `:68/:70` MVP (9/12-key) vs V2 (voice `15`).
- [ ] `docs/USER.md:3` “How 9-key typing works” → “How pad typing works
  (9-key default, layout per category)”.
- [ ] `docs/ENGINEERING.md:10` mapping line → layout registry pointer.
- [ ] `core-rust/README.md:1`, `lib.rs:1`, `mapping.rs:1`, `Cargo.toml:5`,
  `android/README.md:3/10`, `packs/README.md:8` → “layout-driven engine,
  t9-9 default” phrasing.
- [ ] `just docs-check` passes (fence balance + link surface).

## 4. Acceptance

- `grep -ri "9-key predictive keyboard\|9-button keyboard" README.md docs/
  core-rust/README.md android/README.md` → zero hits outside `layouts/`,
  `t9-9` frozen-reference names, and historical quotes.
- New contributor reading README + OVERVIEW answers “which board shows?”
  with “depends on category config, 9 default” without opening SPEC.
