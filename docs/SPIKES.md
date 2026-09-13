# Research Spikes — Phase 4 notes (plan/09)

## 0. Voice V1 — no spike needed (decision recorded, plan/15)

- Voice V1 is system-`SpeechRecognizer`-first with zero audio retention;
  the custom-API engine (`AudioRecord` + `INTERNET` + key storage + spill
  lifecycle) is deferred per plan/24 alternatives. No falsifiable unknown
  justifies a time-box here — policy contract (`VoiceInputPolicy`) is
  unit-covered; the remaining risk is service-wiring + device behavior,
  which is build/test work, not research. Blocks: nothing (friend-ready
  #6 tracks the wiring follow-up).

Time-boxed, falsifiable, never blocking. Each spike ends with a verdict
that feeds back into its owner tier; a spike that fails to reach a
verdict in its box is recorded as INCONCLUSIVE (box kept, question
kept), never silently extended into build work. Desk-first order per
`plan/09`: #2, #3, #5 → #1 → #4, #6, #7.

## 1. Weight tuning — grid search, not hand-tuning (box: 3d)

- Question: which of `RankWeights::default()` (7 params), `hide -0.5`,
  neighbor cost `0.4`, OOV ≥2/7d, 5 s reject window, 30 d reject decay
  actually move `top-3@4`?
- Method: Phase-0 corpus + `session.rs` replay; hold out ~20% sessions as
  validation; coordinate-wise sweep → elasticity chart. Full 7-param grid
  only if ≥2 weights show high elasticity.
- Falsifiable: "weight W moves validation top-3@4 ≥1 pt per ±20% change"
  else W is fixed and dropped from the tunable set. Near-zero elasticity
  everywhere → keep ship defaults + Reset, kill the Tuning-sliders debate.
- Feeds: `docs/TUNING.md` + Advanced sliders. Blocks: nothing.

## 2. Bigram `V` non-stationarity (box: 2d, desk)

- Question: does growing `V=entries.len()` bleed unseen-word mass over
  months (score drifts more negative with unchanged usage)?
- Method: timeline simulation empty → replay 7 days growing dict; plot
  top-3 / perplexity for fixed-V (ceiling 10k) vs dynamic-V (current) vs
  Lidstone tunable alpha.
- Falsifiable: dynamic-V loses ≥2 pts top-3 by day 7 vs fixed-V → adopt
  fixed-V ceiling; else keep current + document the non-finding.
- Feeds: `04` determinism/backoff. Blocks: nothing.

## 3. Schwa-deletion alias cap vs collision curve (box: 2d, desk)

- Question: for alias cap 0/1/2/3 per headword (IAST→casual rules +
  manual overrides), how do 9-key buckets with >4–5 colliding candidates
  grow?
- Method: generate aliases, count collided buckets per cap, plot
  collisions-per-bucket; Leipzig freq breaks ties inside collided
  buckets (never alphabetical/insertion).
- Falsifiable: pick the cap at the knee (collision growth >2× per +1 cap
  → stop). No knee → cap 1 + revisit after gate re-run.
- Feeds: `03` + `23` NE ranking. Blocks: nothing.

## 4. Ephemeral snippet helper vs static-alone (box: 3d + 5–10 testers)

- Question: does the 2-min follow-token helper beat the static token pack
  for identifier recall at <50 ms (plain hash map)?
- Method: timed canonical snippets (function signature, if-block, import
  line), static-alone vs static+helper; watch confusion, not just speed.
- Falsifiable: helper wins median time ≥15% with no confusion regression
  → keep; else static-alone + kill the buffer (code stays simpler).
- Feeds: `07` palettes + `12` lifecycle owner. Blocks: nothing.

## 5. Structural math navigation (box: 2d, desk + testers)

- Question: does the frac/matrix tab-stop model (`Tab→&`, `Enter→\\`,
  auto-frac, panel) beat the flat token stream on 5 canonical equations
  (quadratic, matrix, summation, integral, derivative)?
- Method: time + correction count, same testers both ways. No corpus.
- Falsifiable: tab-stop wins median time with fewer corrections on ≥4/5
  equations → keep; else flat stream + kill tab-stops.
- Feeds: `07` math tab. Blocks: nothing.

## 6. Novel input ideas (box: gated — spend nothing until Phase-0 fails)

- Chording: separate prototype track (direct intent map, not 9-key
  tweak) — only if gate NO-GO forces a rethink.
- 16-key vs 9-key+`tr` for Nepali: A/B same testers/phrases, WPM *and*
  mis-press rate; neighbor fuzzy must clear ≥30% failed-commit reduction
  (`docs/INTENT.md`) or the extra keys aren't worth it.
- Embeddings/neural LM: settled NO at 10k vocab / 50 ms / no-INTERNET —
  revisit only if the budget changes drastically.
- Feeds: `02` layout set / `03` NE. Blocks: nothing (explicitly gated).

## 7. Longitudinal + power (box: 1wk passive + 1d analysis)

- Learning lift: export `log.jsonl` across a week, KSR =
  `(chars − keystrokes)/chars` day-0 vs day-7 per user; report the
  distribution, not just the mean (lift varies by vocab idiosyncrasy).
- Power: 1 h typing trace prediction-on vs off (same device/script),
  OS battery/CPU stats; background sync stays ≤24 h periodic, no extra
  radio wakes.
- Spelling-guard usage rate: popup Pin/Block/Info open rate — near-zero
  → the transparency feature fails its safeguard job regardless of
  popup content (`docs/INTENT.md`).
- Falsifiable: lift <5 pts / no KSR movement → H3 rejected as framed;
  drain delta measurable → `05`/`06` revisit. Blocks: nothing.
