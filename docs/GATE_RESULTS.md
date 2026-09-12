# Tier-0 Gate Experiment Results (NO-GO follow-up)

Frozen gate baseline (`core-rust/tests/gate.rs`, seed `0x9E3779B9…`, op 4 digits,
limit 30): **top-3@4 ON = 0.34 / OFF = 0.47, KSPW = 0.852 (PASS), lift = 100.0pts
(PASS)** → Tier-0 NO-GO on the accuracy leg. Thresholds frozen; nothing below
changes them.

Harness: `core-rust/tests/gate_exp.rs` (8 tests, structural asserts only —
never thresholds). Engine arms: `SuggestOpts` / `TargetProbe`
(`core-rust/src/stack.rs`, commits `4a0d9fc`, `ceb05b9`, `4a8271b`).
Reproduce: `cargo test -p kbcore --test gate_exp -- --nocapture`.

## exp0 — miss split (neighbor ON, EN+NE, freq-weighted)

Scope weight 3358419, miss weight 2201527 (65.6% of scope; n=1944 misses):

| class | % of MISS | % of scope |
|---|---|---|
| outside union (alias/encoding gap) | 0.0 | 0.0 |
| cut by pre-truncate 200 | **0.0** | 0.0 |
| scored but ranked out of top-3 | **100.0** | 65.6 |
| hidden by threshold / blocked | 0.0 | 0.0 |

Miss union size: mean 54.6, max 159 — the 200-cap **never binds** at the
operating point. Conclusion: misses are 100% a ranking problem (distractors
outscore the target in small unions), not truncation, not aliases, not the
hide threshold.

## exp1 — cheapest-first remedies (top-3@4 deltas, freq-weighted)

| remedy | scope EN+NE | words | NE | js | medical |
|---|---|---|---|---|---|
| 1a prefix toggle (exact-only vs ON) | 0.344 → 0.158 (−0.187) | 0.795 → 0.871 (+0.076) | 0.263 → 0.028 (−0.234) | — | — |
| 1b cat-boost 0.3 → 0.6 (ON) | 0.344 → 0.347 (+0.003) | +0.014 | +0.001 | — | — |
| 1c hard tab filter (ON) | — | 0.795 → 0.932 (+0.137) | 0.263 → 0.274 (+0.011) | 0.374 → 0.963 (+0.589) | 0.446 → 0.994 (+0.548) |
| 1d neighbor OFF vs ON | 0.344 → **0.469 (+0.125)** | +0.082 (→0.877) | +0.132 (→0.395) | OFF wins (gate: 0.37→0.65) | OFF wins (gate: 0.45→0.67) |

Notes:
- 1a: prefix mass is essential for NE but hurts EN precision — supports
  per-tab handling, not a global toggle.
- 1b: elasticity ≈ 0. Dead lever, do not spend on it.
- 1c: cross-tab noise dominates code tabs. EN+NE unaffected-to-positive.
- 1d: OFF-arm parity assert holds (2318 scope prefixes byte-identical between
  `suggest_no_neighbor_at` and the `neighbor_off()` opts arm). OFF wins on
  all four tabs.
- Explicit-seq overrides for top-20 colliders were **not applied** (data lives
  in `packs/`, out of scope); see exp2 ceiling instead.

## exp2 — seq-override ceiling (OFF arm, EN+NE)

OFF top-3@4 = 0.469, miss weight 1782648. Top miss prefixes: 7262, 7264, 7274,
5262, 7729 (NE-heavy, ~1.4–2.0% of miss each). Top-20 prefixes hold 23.6% of
miss mass (12.5% of total) → perfect-fix upper bound **OFF 0.469 → 0.594
(assumes zero collateral)**. Even the ceiling leaves a 25pt gap to 85%.

## 09#2 — bigram-V stability (7-day replay, weight-applied, w=0.8)

| day | V | dynamic unseen | fixed-10k unseen | Lidstone-0.5 | Lidstone-0.1 | dynamic seen |
|---|---|---|---|---|---|---|
| 0 | 352 | −2.042 | −3.200 | −2.047 | −2.083 | −1.420 |
| 6 | 2452 | −2.712 | −3.200 | −2.713 | −2.719 | −2.090 |

Unseen-pair drift day0→day6: dynamic **−0.6702**, fixed-10k **+0.0000**,
Lidstone-0.5 −0.6661, Lidstone-0.1 −0.6352. Drift is structural to additive
smoothing (α barely moves it). Recommendation: **fixed V = 10k** when personal
grows; do not tune α.

## 09#3 — NE alias cap-vs-collision (8000 headwords)

| cap | buckets | >4 colliders | >5 colliders | % heads in >4 |
|---|---|---|---|---|
| 0 | 7637 | 1 | 0 | 0.1 |
| 1 | 7917 | 2 | 0 | 0.1 |
| 2–3 | 7932 | 2 | 0 | 0.1 |

Cap 0–3 barely moves collisions. Cap is not the collision lever; pick it on
UX grounds, not gate math. (Caveat: cap admits pack `alt` order as proxy, not
generated IAST→casual rules — owner should confirm under real alias
generation.)

## Incidental correctness fix (`4a8271b`)

The harness parity asserts exposed a real bug: the indexed cold path re-merged
one row per matching seq, summing its own frequency (phantom +0.3 for
multi-alias words, e.g. `बस्नेत` 2510→5020) and disagreeing with cache-hit
replays depending on query history. Fixed via per-row best-keyfit merge-once
(scan-path semantics) + regression test. Gate numbers barely moved (doubling
rarely decided top-3).

## Recommendation

1. **Default neighbor OFF with per-tab override** (`SuggestOpts` is the
   vehicle, in tree): +12.5pts scope, wins every tab. Gate neighbor-ON behind
   per-tab opt-in only if a mis-press corpus clears 09#6's ≥30% bar.
2. **Hard tab filter for code tabs** (+0.59 js / +0.55 medical, EN up too).
3. **Do not spend on**: cat-boost, alias caps, Lidstone-α. Prefer fixed V=10k.
4. Even OFF (0.469) + perfect top-20 overrides (→0.594) + tab filter stays far
   from 85%: closing needs weight retuning on real data (09#1) and the
   learning arm, not more cheap levers.

## Remaining gaps

- Typo-tolerance cost of default-OFF unmeasured (needs mis-press corpus, 09#6).
- KSPW arm still runs neighbor-ON (0.852); re-run with the OFF policy for the
  blessed number.
- 09#3 proxy caveat above.

## Ranker improvements applied (train-tuned, held-out reported)

Commits `f0795fe`, `8fd4147`, `bd4afa3`, `0a77a7f`, `913c9bd`, `1fd150b`
(core-rust only; gate thresholds frozen). Full suite green except the frozen
gate itself (still NO-GO on accuracy, as expected).

| change | held-out scope ON | held-out scope OFF/policy | KSPW | lift |
|---|---|---|---|---|
| baseline (NO-GO report) | 0.34 | 0.469 | 0.852 PASS | 100.0pts PASS |
| + fixed V=10k | 0.34 | 0.47 | 0.852 PASS | 100.0pts PASS |
| + per-tab OFF policy, code-tab filter, w_cat 0.3→1.5 | **0.37** | **0.4845** | **0.845 PASS** | 100.0pts PASS |
| + hard per-tab isolation default-ON every path | **0.37** | **0.49** | **0.844 PASS** | 99.9pts PASS |

Isolation note: the filter is now default-ON on every suggest path
(pre-merge scoping; `with_tab_filter(false)` is measurement-only), so the
gate's own-tab queries run filtered. Movement is small because `w_cat=1.5`
already paid most of the cross-tab cost: scope ON 0.37 flat, OFF
0.4845→0.49, KSPW 0.845→0.844, lift 100.0→99.9. Threshold sides unchanged
(still NO-GO on accuracy as frozen; KSPW + lift PASS). Fresh exp1c deltas
(filter vs explicit opt-out, ON arm): words +0.004, NE +0.001, js +0.037,
medical +0.021 — the filter never hurts any tab.

Per-tab OFF after: words 0.88→0.97, js 0.65→0.99, medical 0.67→1.00, NE
0.40 flat (within-tab ranking, not cross-tab noise). Per-tab ON after:
words 0.93, NE 0.27, js 0.95, medical 0.98.

- 09#1 coordinate sweep (train split only, transfer confirmed held-out):
  most elastic weight was `w_keyfit` but HARMFUL (−0.025 train/−0.019 held
  at 2.0 — do not raise); `w_base`/`w_reject`/`hide_threshold` ~zero
  elasticity (fixed, do not tune); only `w_cat` earned a change (monotone
  0.0/0.6/1.0/1.5 curve both splits, knee 1.5, plateau 1.5–3.0 flat).
- Overfit safeguards: train-only tuning; no per-word overrides added;
  structural-first (policy + filter + fixed-V); new `ranker_guard` test
  fails below the 0.469 OFF floor (negative-controlled: ON arm 0.373 fails).
- Remaining gap to 85%: 85 − 37 = **48pts** (ON) / 85 − 48.5 = **36.5pts**
  (policy). NE (0.40) is now the binding constraint — needs the learning
  arm / real-data retuning, not more cheap levers.

## Verb-form + connection prototypes (user feedback, train/held protocol)

Harness: `core-rust/tests/morph_conn_diag.rs` (3 diagnosis tests) +
`core-rust/tests/inflect_proto.rs` (tune-discount / held-report /
flag-off guard). Reproduce: `cargo test -p kbcore --test morph_conn_diag
-- --nocapture`, `cargo test -p kbcore --test inflect_proto -- --nocapture`.
Gate thresholds frozen; nothing below changes them or any default path
(both prototypes are explicit opt-in calls).

### Diagnosis numbers (frozen harness)

- **D1 coverage — verb forms are vocab-OOV, not ranking misses.** 51/51
  probed regular EN inflections (`runs/running/walked/…`) absent from the
  5150-word EN vocab (**100% missing**); 21/43 NE verb-form probes missing
  (**49%**). Typing `running` has no in-vocab target at all.
- **D2 miss link — gate misses are NOT inflectional.** Among policy-arm EN
  misses, targets that are strict inflections of a TRAIN stem hold 0.8% of
  miss weight (weight 120 of 14769; EN policy already 0.971). Perfect-fix
  upper bound: EN policy 0.971 → 0.972 (+0.001). A stem+inflection
  mechanism cannot move gate misses — it addresses OOV typing, which the
  gate never measures (all gate words are in-vocab by construction).
- **D3 connections — cold ctx is structurally a no-op; learned pairs
  rescue 88%.** With empty personal history, `ctx="the"` vs `ctx=""`
  agree byte-identically on all 2318 scope prefixes (backoff-0 ⇒ bigram
  term identically 0 — fresh users get zero connection signal). Simulating
  ONE learned pair (gate learning-arm protocol: `cpw=1, cp=1`) promotes
  the target past 3rd place for **88.1% of miss weight** (policy 0.486 →
  **0.939 ceiling**, assumes the learned prev is the actual ctx). The
  engine's bigram promotion works when data exists; the gap is cold-start
  data + no zero-keystroke next-word surface.

### P1 — inflection sidecar (EN rule table, load-time, flag-gated)

`core-rust/src/inflect.rs` + `DictionaryStack::expand_inflections(cfg)`:
regular EN suffix rules (s/es/ies, ed/ied/d, ing/ying/drop-e, CVC
doubling with doubled-only exclusivity — `*runing`/`*runed` never mint;
irregulars `made/went` never generate, they are vocab-coverage rows).
Closed-class gate is a frequency threshold (`max_stem_freq`, default
50k — `*thes`/`*anding` blocked with no word list). Derived freq =
`stem_freq / 8`. `DictEntry.synthetic` suppresses second-order minting
(`*runneds`); NE rows never expand under the EN table (guard-asserted).

OOV probe set: 12,010 regular derivations of TRAIN-split stems absent
from the full vocab (frozen FNV split, no leakage), 6074 tune / 5936
held. Unexpanded baseline scores 0.000 by construction.

| discount_div | tune op4 | tune full-exact | held op4 | held full-exact |
|---|---|---|---|---|
| 2 | 0.222 | 0.862 | — | — |
| 4 | 0.208 | 0.862 | — | — |
| **8 (frozen)** | 0.205 | 0.862 | **0.202** | **0.857** |
| 16 / 32 | 0.205 | 0.862 | — | — |

Curve is flat (8 chosen as the conservative knee; tune weakly prefers 2
by +0.017 op4 — not worth 4× hotter distractors). Transfer holds:
tune 0.205/0.862 → held 0.202/0.857.

Costs (+17,871 static rows, +127% rows; fixed-V untouched — static rows
never move bigram mass): gate-held collateral words 0.9714 → 0.9689
(**−0.0025**), NE 0.3976 → 0.3976 (+0.0000); mean union 4.2 → 18.3
(+330%, still far below the 200 truncate). KSPW arm not re-run with
expansion (neighbor-ON arm; policy-arm collateral only — stated caveat).

**Recommendation: SHIP behind the explicit call (default OFF).**
Held-OOV full-exact 0 → 0.857 with −0.0025 gate collateral and zero
default-path behavior change (flag-off guard: 4636 prefix queries
byte-identical across two default stacks). Do NOT enable by default
until KSPW-arm collateral is measured; NE needs its own rule table
(needs-data, not this ship).

### P2 — next-word surface (`suggest_next_at`, personal bigrams)

New method only: zero-digit query ranks learned followers of ctx's last
token (count desc, word asc; tab-isolated incl. the ★personal aggregate;
blocked excluded; bigram-only followers inherit their static pack tab).
Cold returns `[]` (honest silence, unit-locked). Gate-neutral by
construction (gate never queries empty digits; no existing path
touched). Unit tests: cold-empty, count ranking + limit, blocks + tabs,
`top_followers` ordering.

**Recommendation: SHIP as new surface (default code paths untouched),
value NEEDS-USAGE-DATA** — no harness sequence data exists to measure
next-word hit rate; validate with the Phase-0 corpus/session replay
(09#1) before any UI commitment. Static (non-personal) bigrams would
need a corpus that does not exist — explicitly not built.

### Remaining gaps (after these prototypes)

- NE verb morphology: 49% probe gap, no rule table (needs-data).
- Irregular inflections (`is/am/went/made/took`): vocab-coverage rows,
  not generatable — needs vocab expansion (packs/ data, out of scope).
- Missing base words (`do/make/talk/study/is/am` absent entirely):
  coverage, not morphology.
- Cold-start connections: no static bigram model (needs corpus).
- KSPW-arm collateral for P1 unmeasured (policy-arm only: −0.0025).
