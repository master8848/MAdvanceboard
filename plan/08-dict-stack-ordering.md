# 08 — Dictionary Stack Ordering (Drag-Drop Priorities)

Invariant: `base 0` fixed bottom, `personal 100` fixed top, extensions float `10-90` user-ordered (`SPEC.md:87`, `core-rust/src/stack.rs:79`). Reorder = reassign `priority` + `rebuild_index()` + tab order. No format change.

## Behavior (already in code, expose in UI)

- Lookup unions enabled packs + personal, dedupes `(norm(word), lang)`, display keeps highest-prio pack's `cat`, frequencies sum with personal weighted separately (`core-rust/src/stack.rs:309-350`, `docs/TUNING.md:32`). Verified: `stack_priority_display_cat`, `category_boost_prefers_active_tab` (`core-rust/src/stack.rs:479-531`).
- Priority affects `suggest()` only: freq-sum, display-`cat`, `cat_boost` (`core-rust/src/stack.rs:209`), final tie-break (`core-rust/src/stack.rs:409`). Palette availability (see `07-code-math-palettes.md`) ignores priority.
- Personal rules (`SPEC.md:93`): never overwritten by pack updates (`personal.jsonl` separate); OOV `≥2 accepts/7d → personal`; Block → tombstone `{del:true}` hidden everywhere, survives sync, exempt from eviction (`core-rust/src/personal.rs:101`); decay `×0.98` monthly (demote only); cap 10k LFU vs 20k LRU — settle in `06-persistence-power.md`.
- Placement search: long-press → `packId • freq • accepts` (`core-rust/src/stack.rs:432`, `SPEC.md:92`). Answers "where is this word from".

## UI

- Settings → Packs → drag-drop list = stack order (top = higher prio within 10-90) + enable toggle + per-pack `layout` override (see `02-layout-global-percat.md`) + `privacy.learn` badge (`numbers`, `math` false). Sideload `+ .kbpack / URL` → SHA-256 confirm → enable + set priority (`docs/EXTENSIONS.md:83`). `paid` informational-only, never gates loading; unsigned allowed with warning.
- User-reorderable `CategoryTabs` mirror pack order (plus `numbers`, `★personal` pinned). `onCategoryChanged` keeps legacy `EN/NE/nepali` compat (`FINAL_RECONCILE.md:20`).

## Acceptance

- Drag `medical` above `js`: `fun`-class collisions change display `cat` + ranking deterministically; personal words still outrank (`+1.2`). Export/import round-trips order.
