# 09 — Open Research Questions (Spikes, Not Commits)

For genuinely unsure / novel areas. Each is a time-boxed spike with falsifiable outcome, not a build task. Solo order: #2, #3, #5 first (desk, no users), then #1 (needs corpus, not live users), then #4, #6, #7 (need behavior/testers).

## 1. Weight tuning — grid search, not hand-tuning

- `RankWeights::default()` (`core-rust/src/rank.rs:28`), `hide -0.5`, neighbor cost `0.4` (`SPEC.md:36`), OOV `≥2/7d`, 5s reject window, 30d reject decay — all guesses, no data.
- Needs: Phase-0 corpus + replay harness (`core-rust/src/session.rs`) re-running same keystrokes through different weight sets. Metric first: `top-3@4` = after 4 keystrokes, intended word in top-3?
- Hold out ~20% sessions as validation never tuned against (else overfit to own corpus). Sweep one weight at a time first (coordinate-wise) → elasticity chart (which weight moves metric most per unit). Full 7-param grid is expensive — do coordinate first. Near-zero elasticity across wide range → fix and drop from tunable set. Keep per-user sliders (`docs/TUNING.md:36`) + Reset.

## 2. Bigram `V` non-stationarity

- `V=entries.len()` (`core-rust/src/personal.rs:172`) grows → `bigram_term` more negative over time even after `04-engine-determinism.md` backoff fix. Spike: fixed `V` (e.g. 10k) vs dynamic vs Lidstone variants. Measure cold-start vs 7-day lift stability.

## 3. Schwa-deletion + variant explosion (NE)

- Casual romanizations (`paani/pani`, `shabda/sabda`, `nepal` not `nepala`) need alias generation (`shabdakosha` IAST→casual, `nepali_unicoder` overrides). Spike: how many aliases per headword before 4-digit buckets regress? Cap aliases (e.g. 3) + rank by Leipzig freq.

## 4. Local-scope token/snippet packs for code categories (rescoped — characters, not logic)

Scope is narrowed: keyboard makes *characters* cheaper, not *logic*. No multi-token intent guessing, no next-line prediction, no tiny LM. Three pieces only:

- Static per-language token pack (keywords, common stdlib names, common punctuation clusters `();`, `=>`, `->`) ranked by corpus frequency — just another vocab plugin, same architecture as medical/Nepali packs.
- Lightweight *local* frequency table scanning current file's tokens (not parsing, not LSP) so identifiers typed once (`userId`, `fetchData`) suggest again — sliding window over last N files/edits with decay (e.g. halve every edit-session), stale files don't pollute.
- Optional single-level snippet expansion (`if`→`if () {}` with tab stops) — macro expansion, not generation. No sentence context.

Spike question: does doc-level token table beat frequency-alone for identifier recall, staying under 50ms with plain hash map — not "can LM fit on phone." Snippet tab-stop UX is separate usability test: 5-10 testers timed on canonical snippets (function signature, if-block, import line), watch confusion points.

## 5. Structural math navigation

- Spike: `\frac{}{}` / matrix tabstop model (`Tab→&`, `Enter→\\`), auto-frac, symbol panel recall. Flat token is baseline; success = task-completion time for 5 canonical equations, not top-3.

## 6. Novel input ideas (only if Phase-0 NO-GO)

- Chording (multi-press) vs single-tap: changes disambiguation math + dict structure entirely — prototype, don't retrofit.
- Deliberate 16-key for NE vs 9-key + `tr`: motor cost vs 0.6 words/bucket gain — A/B with mis-press rate (neighbor ON reduces failed commits ≥30%? `docs/INTENT.md:49`).
- Prefix-caching + quantized static scores (`05-suggest-optimization.md` #6-7) already cover "unique solution" space without ML. Skip embeddings/neural LM at 10k vocab + 50ms + no-INTERNET constraint.

## 7. Longitudinal + power

- 7-day learning lift + KSPW from exported `log.jsonl` (`docs/SYNC.md:29`). 1h typing trace power delta; background ≤24h periodic only (`android/README.md:20`). Spelling-atrophy guard: placement popup + Pin/Block/Info usage rate (`docs/INTENT.md:63`).
