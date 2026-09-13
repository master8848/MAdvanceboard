# 02 — Global Layout + Per-Category Override (9/12/16)

Requirement: one `layout_id` resolved per keystroke, but mapping under same shell may differ per need. Global default + per-cat override, user-changeable.

## Model

- `LayoutRegistry { specs, default: String, cat_map: HashMap<cat, layout_id> }` (`core-rust/src/layout.rs:289`). Add `resolve(cat) -> &LayoutSpec` (fallback `default`, then `t9-9`). Source from `layouts/cat_map.json` or `PackFile.layout: Option<String>` → `DictEntry.layout_id`.
- `Predictor`: add `set_default_layout() / set_cat_layout(cat, layout)`. `suggest_inner` picks `seq_of(e)` per-entry mapping + per-layout neighbor graph (`core-rust/src/stack.rs:250`, `core-rust/src/predictor.rs:54`). Build per-layout FST indexes (today single shared index + comment "future per-layout index can reuse this hook" `core-rust/src/stack.rs:194`).
- UI: Settings → Layout → Global (t9-9 / t9-12 / t9-16) + per-tab override list (e.g. EN→t9-9, NE→t9-9 default, numbers→t9-12). Persist in DataStore. Changing layout re-encodes display labels only; engine re-resolves per `active_tab`.

## Which layout for what

- Simple Latin/code/emoji/numbers: t9-9 (3x3) or t9-12 (3x4, bottom Sym/Space/Delete). t9-12 text codes identical to t9-9 → zero collision change, only comfort/keys.
- Nepali/complex scripts: t9-16 (4x4, splits `pq|rs`, `wx|yz→0/A`, extra `B` symbols). Measured pre-`tr`: exact collisions 38→33%, 4-digit avg 2.6→2.0. Under the `tr`-model (see `03` — only Latin split matters, Devanagari split is fallback) the default moves back to **t9-9**; t9-16 stays opt-in. Re-run gate before keeping any divergence (see `23`).
- Custom: `register_json` (`core-rust/src/layout.rs:323`) stays for user `.json` layouts. Validate: no digit in `symbols`, symmetric adjacency, all codes have adjacency (`core-rust/src/layout.rs:129`).

## Precompute (perf, ties to `05-suggest-optimization.md`)

- At insert, store `seq_t99, seq_t912, seq_t916` (or compact). Today `seq_of` closure calls `mapping.encode_word` → one `String` alloc per entry per keystroke (~15k allocs). Precompute once.
- `t9-12` note: `a@b → 2*2` Sym code (`layout.rs:396` test); `contains_code` linear scan per char — intern codes, reuse buffers.

## Acceptance

- Switching Global or per-cat layout changes labels + `suggest_for_layout` results deterministically; unknown/empty id → `t9-9` (`core-rust/src/layout.rs:333`). Golden vectors per layout (`hello→43556` t9-9/t9-12, `fun→386` t9-9 vs `396` t9-16, `pqrs→7788` t9-16).
