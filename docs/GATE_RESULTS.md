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
