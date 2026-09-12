# 10 — Execution Order (Multi-Tier Dependencies)

User compiles later. Plan only, in this sequence. Each step = its file. Gate blocks downstream.

## Tier-0 — Gate (do first, 1-2 days)

1. `00-gates.md` — freeze corpus split + seed, run top-1/top-3 @3/@4/full, KSPW, neighbor ON/OFF, per-tab breakdown, learning arm. Fix `HashMap` tie-order first or numbers are meaningless. GO → continue; NO-GO → cheapest-first alternatives in file, re-run.

## Tier-1 — Correctness (parallel after GO)

2. `04-engine-determinism.md` — total-order truncate, 1h clock buckets, bigram backoff 0, personal-OOV index, golden + fuzz harness. Unblocks all measurement.
3. `03-nepali-transliteration.md` — `tr` first-class, `encode(tr)` pipeline change, matras non-emitting, download 2.4M-row HF + Leipzig intersect → 5k+ `w+tr` pack. Unblocks NE gate + `02`.
4. `02-layout-global-percat.md` — `resolve(cat)`, `set_default/set_cat_layout`, per-layout FST + precomputed seqs, global + per-tab settings. Depends on 03 for NE default (t9-16 vs t9-9+`tr`).
5. `01-gestures-action-only.md` — Pad `←` delete / `↑` space+commit / `→` accept / `↓` hide, Tabs scroll, Bar tap/fling, FAB toggle, 12-key override, AT fallback. UI-only, needs `02` layout ids for per-cat deltas.

## Tier-2 — Performance (after Tier-1)

6. `05-suggest-optimization.md` — FST posting + `StartsWith` automaton, generative neighbors, heap top-30, alloc kills, drop `suggest` from `learn`/`reject`, prefix cache, quantized scores. Re-bench on 6613 + 10k personal.
7. `06-persistence-power.md` — single handle + WAL/NORMAL, dirty-set 2s flush, ring-cap session log, settle 10k LFU vs 20k LRU. Power trace after.

## Tier-3 — Extension (after Tier-2)

8. `07-code-math-palettes.md` — keep Tier-0 vocab for all tabs; add Tier-1 palette behind `CategoryProvider` for `js/rust/html/math` only (symbols, snippets `$1/$0`, matrix navigation). `words/ne/emoji/numbers/medical` stay pure.
9. `08-dict-stack-ordering.md` — drag-drop 10-90 UI, enable/layout/learn badges, placement popup, order round-trips export. Depends on 07 priority-vs-availability rule.

## Tier-4 — Spikes (any time after Tier-0, time-boxed)

10. `09-research-open-questions.md` — weights grid-search, `V` stability, alias caps, doc-index code completion, structural math tasks, longitudinal/power. Feed back into tiers; don't block Tier-1/2.

## What stays deferred

- `sync.rs` merge/recovery (design frozen `docs/SYNC.md`, unbuilt), `.kbpack` zip + WASM sandbox + SHA UI, `uniffi-bindgen 0.32` regen + `StubPredictor` replace, JDK assemble + on-device smoke. Platform (iOS companion split, TV/VR gaze) after core gate — keyboard layout is the test harness, not the product.
