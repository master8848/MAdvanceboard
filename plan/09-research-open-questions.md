# 09 — Open Research Questions (Spikes, Not Commits)

For genuinely unsure / novel areas. Each is a time-boxed spike with falsifiable outcome, not a build task. Solo order: #2, #3, #5 first (desk, no users), then #1 (needs corpus, not live users), then #4, #6, #7 (need behavior/testers).

## 1. Weight tuning — grid search, not hand-tuning

- `RankWeights::default()` (`core-rust/src/rank.rs:28`), `hide -0.5`, neighbor cost `0.4` (`SPEC.md:36`), OOV `≥2/7d`, 5s reject window, 30d reject decay — all guesses, no data.
- Needs: Phase-0 corpus + replay harness (`core-rust/src/session.rs`) re-running same keystrokes through different weight sets. Metric first: `top-3@4` = after 4 keystrokes, intended word in top-3?
- Hold out ~20% sessions as validation never tuned against (else overfit to own corpus). Sweep one weight at a time first (coordinate-wise) → elasticity chart (which weight moves metric most per unit). Full 7-param grid is expensive — do coordinate first. Near-zero elasticity across wide range → fix and drop from tunable set. Keep per-user sliders (`docs/TUNING.md:36`) + Reset.

## 2. Bigram `V` non-stationarity

- Mechanism: Laplace/add-one unseen mass ≈ `1/(N+V)`, so as personal dict grows `V=entries.len()` (`core-rust/src/personal.rs:172`) grows, mass shrinks — unseen word score drifts more negative over months though usage unchanged, even after `04-engine-determinism.md` backoff fix.
- Compare three: fixed V (ceiling e.g. 10k, stop growing) vs dynamic V (current) vs Lidstone tunable alpha (instead of implicit alpha=1). Timeline simulation: empty → replay 7 days growing dict, plot top-3 / perplexity over timeline. Want fast early climb (cold start) without slow bleed later (stability).

## 3. Schwa-deletion + variant explosion (NE) — alias cap vs collision trade curve

- Generate aliases via IAST→casual rules + manual overrides (`shabdakosha`, `nepali_unicoder`). For cap = 0,1,2,3 per headword, count 9-key buckets with >N colliding candidates (N=4-5, suggestion-noise limit). Plot collisions-per-bucket vs cap. Within collided bucket, Leipzig freq breaks ties — not alphabetical/insertion order.

## 4. Local-scope token/snippet packs for code categories (rescoped — characters, not logic)

Scope is narrowed: keyboard makes *characters* cheaper, not *logic*. No multi-token intent guessing, no next-line prediction, no tiny LM. Three pieces only:

- Static per-language token pack (keywords, common stdlib names, common punctuation clusters `();`, `=>`, `->`) ranked by corpus frequency — just another vocab plugin, same architecture as medical/Nepali packs.
- Lightweight *local* frequency table scanning current file's tokens (not parsing, not LSP) so identifiers typed once (`userId`, `fetchData`) suggest again — sliding window over last N files/edits with decay (e.g. halve every edit-session), stale files don't pollute.
- Optional single-level snippet expansion (`if`→`if () {}` with tab stops) — macro expansion, not generation. No sentence context.

Spike question: does doc-level token table beat frequency-alone for identifier recall, staying under 50ms with plain hash map — not "can LM fit on phone." Snippet tab-stop UX is separate usability test: 5-10 testers timed on canonical snippets (function signature, if-block, import line), watch confusion points.

## 5. Structural math navigation — usability, not corpus

- No corpus. 5 canonical equations (quadratic formula, matrix, summation, integral, derivative), testers enter each two ways: flat token stream vs frac/matrix tab-stop model (`Tab→&`, `Enter→\\`, auto-frac, panel). Time + correction count. Success = time reduction on tab-stop version, not top-3 — math entry is structural-navigation, not prediction.

## 6. Novel input ideas — gated on Phase-0 fail, don't spend time yet

- Chording = separate prototype track, not 9-key tweak: chord maps directly to intent, no sequential disambiguation across digit sequence. Different data model.
- 16-key vs 9-key+tr for Nepali: A/B same testers/phrases, WPM *and* mis-press rate. Neighbor fuzzy must clear ≥30% failed-commit reduction bar (`docs/INTENT.md:49`) or extra 7 keys not worth complexity.
- Embeddings/neural LM: settled "no" at 10k vocab / 50ms / no-INTERNET — quantized model won't beat well-tuned freq+bigram. Documented decision, revisit only if vocab/latency budget changes drastically. Prefix-cache + quantized scores (`05-suggest-optimization.md`) already cover unique-solution space.

## 7. Longitudinal + power — two independent measurements

- Learning lift: export `log.jsonl` (`docs/SYNC.md:29`) across week, compute KSR = `(chars − keystrokes)/chars` (AAC metric) day-0 vs day-7 per user, look at distribution not just mean (lift varies by vocab idiosyncrasy).
- Power: 1h typing trace prediction-on vs off (same device/script), OS battery/CPU stats; separately verify background sync stays ≤24h periodic without extra radio wakes (`android/README.md:20`).
- Spelling guard = usage-rate question: how often popup / Pin/Block/Info actually opened. Near-zero → transparency feature fails safeguard job regardless of popup content (`docs/INTENT.md:63`).
