# 00 — Phase-0 Go/No-Go Gate

Question: does 9-key single-tap + shared `suggest()` clear a bar worth building on?

## Thresholds (freeze before measuring)

- `KSPW ≤ 1.5` on EN+NE freq-weighted set (type-until-in-top-3 + 1 select tap). Lenient; fails only if ranking/prefix broken.
- `top-3 @ 4-digit prefix ≥ 85%` exact-seq, neighbor ON. Operating point is 4 digits, never 3 (3-digit: 8^3=512 buckets for 5k words, measured top-3 ~24% — hopeless by theory).
- `learning lift ≥ 10pts` top-3@4 on repeat-word subset, paired A/B frozen-base vs +personal replay (2 accepts + 1 bigram context).
- Report neighbor ON vs OFF separately (1-edit expands candidates ~8x at 4 digits: typo-tolerance vs precision cost).

## Why 4 digits

Measured on real packs (5150 EN + distractors): 3-digit avg 11-13 words/prefix (max 64-91, singletons ~6-9%); 4-digit avg 2.2-2.6 (max 13-16, singletons ~38-42%); full-seq 38% collide. Freq-only rank simulation: top-3@4 ~84.4%, KSPW ~1.26. So 85% is calibrated — neither free nor impossible; bigram (0.8) + category (0.3) + personal (1.2) add the missing points. Rank-4/5 mass ~11.7% is the learning target.

## Harness (script + frozen weights, no new deps)

1. Corpus: `packs/words_en.json` + `packs/nepali.json` romanized + code/medical as distractors. Freq-weighted sampling via pack `freq` (Zipf tails). 80/20 split, 20% held-out never trains personal. Freeze split + seed in repo.
2. Query `DictionaryStack::suggest(ctx="", digits=prefix_k, tab, 30)` (`core-rust/src/stack.rs:226`) with `RankWeights::default()` (`core-rust/src/rank.rs:28`). Record rank at k=4 and full length. Python T9 port must parity-check `hello→43556`, `कमल→267` (`core-rust/src/mapping.rs:58`).
3. Learning arm: replay 2 accepts + bigram, re-query. Δ top-3@4.
4. Per-tab breakdown: EN vs NE vs js. `t9-12` changes only bottom control row → zero collision change; `t9-16` splits pq/rs/wx/yz: 4-digit avg 2.6→2.0, singletons 38→52%, exact collisions 38→33%. Weak lever — do last.

## GO / NO-GO

- GO all three → proceed to Tier-1.
- NO-GO → apply cheapest-first, re-run: (1) prefix 0.9 toggle (`core-rust/src/stack.rs:314`), (2) category boost 0.3→0.6 / hard tab filter for code tabs, (3) personal+bigram (already weighted, just include), (4) explicit `seq` overrides for top-20 colliding keywords (`DictEntry.explicit`), (5) per-category 16-key last.
- Blocker before blessing numbers: fix `HashMap` nondeterministic 200-truncate (`core-rust/src/stack.rs:364`) — total-order sort `(freq desc, word asc, lang asc)` before truncate. See `04-engine-determinism.md`.
