# 05 — Suggest-Path Optimization (FST + Generation + Heap)

Target: 5-15k vocab + 10k personal, p95 <50ms on minSdk26, deterministic (see `04-engine-determinism.md`). Order by impact.

## P1 hot path

1. **Use the FST you build.** Today `rebuild_index` builds `fst::Set` of seqs but `suggest` scans full `Vec`; `decode_prefix` linearly scans whole set (`stream.next()` + `starts_with`, O(N) `core-rust/src/stack.rs:170`). Replace with `seq → posting-list` map (`HashMap<seq, Vec<idx>>` or `fst::Map<seq,u64>` offset into sorted postings) + FST `StartsWith` automaton query, not linear filter. Per-layout precomputed seqs at insert (`seq_t99/t912/t916`) — kills `seq_of` closure `encode_word` alloc per entry per keystroke (~15k allocs).
2. **Generative neighbor expansion, not scan.** Today `is_one_edit_neighbor(digits,eseq)` per entry = O(N·L) (`core-rust/src/mapping.rs:97`, `core-rust/src/layout.rs:100`). Enumerate variants instead: `len × ~8 neighbors` (≈50 lookups for 7-digit input) → exact `HashMap` hit. Exact + prefix via FST + neighbor via generation = O(|digits|+matches), SPEC's stated complexity.
3. **Real top-30 heap.** Today: full sort merged → truncate 200 → score → full sort scored → truncate (`core-rust/src/stack.rs:364-417`). Replace second sort with `BinaryHeap` / `select_nth_unstable` capped at 30. Keep 200 pre-cap (SPEC) but frequency-ordered only after FST/match stage, deterministically ordered (see 04 #1).
4. **Kill per-keystroke allocs.** Hot loop clones `eseq/word/cat`, `to_lowercase()` per candidate, `format!("{}\x1f{}")` per entry. Precompute `norm_key`, intern `cat/lang` as `u8` enums, reuse buffers. Biggest lean win after #1-2.
5. **Remove `suggest` from `learn`/`reject`.** `core-rust/src/predictor.rs:103-114,126-133` runs full 3-candidate `suggest` inside `Mutex` just for session `shown`. Doubles latency, holds lock. Log `shown` from caller's last suggest result, or empty + backfill. `learn/record_reject/record_bigram` become O(1) HashMap ops.
6. **Prefix-result cache.** Typing appends one digit: cache `digits → Vec<idx>` (cap 8-16, keyed `(layout_id, digits)`). Next keystroke filters cached set + one FST prefix page. Near-zero cost, big p95 win on long words.
7. **Quantized static scores.** Precompute `log10(freq_base+1)` as `u16` fixed-point (×1000) at load. Per-keystroke scoring becomes integer adds + 2 floats (recency/bigram). No model file, minSdk26-safe.
8. **Tiered bigram.** Keep add-one unigram/bigram in RAM (tiny). `prev_counts` derivable via `SUM GROUP BY` on load or incremental; compute `bigram_counts` lazily only for ≤200 scored candidates, not all matches (`core-rust/src/personal.rs:165`).

## Keep / skip

- Keep `fst::Set` (~10s KB for 5-10k seqs). Don't add `memmap2` until ~50k+ words; heap-built FST is NDK-simpler (`INTEGRATION_REPORT.md:190`).
- DAWG/FST suffices; no TRIE rewrite, no embeddings/quantized LM.
- Measure on full 6613-word packs + personal 10k, not 225-word toy. Re-run `~19µs` debug bench (`INTEGRATION_REPORT.md:170`) after each change.
