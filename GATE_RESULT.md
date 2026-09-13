# Phase-0 Gate Result (row #1, `plan/00-gates.md`)

Date: 2026-09-13. Harness: `core-rust/tests/gate.rs` (frozen) +
`core-rust/tests/gate_matrix.rs` (report-only full matrix, new) +
`core-rust/tests/gate_exp.rs` (remedy arms) + `core-rust/tests/per_tab.rs`.

Frozen inputs: `GATE_SEED = 0x9E3779B97F4A7C15`, FNV-1a `(seed, word, lang) % 5 == 4`
-> 20% held-out (2808 words), `FROZEN_NOW = 1726000000` (1h bucket),
`RankWeights::default()`, limit 30, `ctx=""` (learning arm `"the"`).
Corpus: `packs/words_en.json` (5150) + `packs/nepali.json` (8005, Roman `tr`
seqs) + `packs/code_js.json` + `packs/medical.json` (distractors).
`just validate-packs` green (10 files).

## Determinism (HashMap tie-order)

Already fixed in-tree before this run; verified, not re-changed:

- `core-rust/src/stack.rs` pre-truncate sort is total
  (`freq desc, word asc, lang asc`) before `truncate(200)`, in both
  `suggest_inner` and `probe_target_at`.
- Final rank is total via `cmp_scored`
  (score desc, len, word, priority, lang); `select_nth_unstable` input is the
  totally-ordered vec, so the partition is repeat-stable.
- `merge_row` is insertion-order independent (freq sums; display = max
  priority, ties min row index; seq = min row index; keyfit = max), so
  `merge_hits` HashMap iteration order cannot leak into output.
- Personal paths sorted (`live_entries`, `to_jsonl`, `bigrams_jsonl`,
  flush keys); LFU victim `min_by` has a key tiebreak; `ids()` sorts.
- Locks: `truncate_is_total_order_repeat_stable` (50 repeats),
  `determinism.rs` goldens + seeded fuzz, OFF-arm parity in `exp1d`.

## Gate numbers (frozen `gate.rs`)

`cargo test -p kbcore --test gate -- --nocapture` → **FAIL (NO-GO)**, as designed:

```text
== 00-gates report (seed 0x9e3779b97f4a7c15, op 4 digits, limit 30) ==
held-out: 2808 words (train excluded from personal)
tab            n    n@4  top3@4 ON top3@4 OFF     KSPW
NE          1561   1550       0.27       0.40    0.863
js            83     80       0.99       0.99    0.575
medical      100     94       1.00       1.00    0.550
words       1064    768       0.93       0.97    0.676
EN+NE scope: n=2625 n@4=2318 top3@4 ON=0.37 OFF=0.49 KSPW=0.844
learning: repeat-subset n=1487 base=0.00 +personal=1.00 lift=99.9pts
KSPW<=1.5:        0.844  PASS
top-3@4>=85% ON:  0.37  FAIL
lift>=10pts:      99.9  PASS
```

Two of three bars pass. The failure is isolated to NE @ fixed k=4.

## Full matrix (new `gate_matrix.rs`, report-only)

`cargo test -p kbcore --test gate_matrix -- --nocapture` → ok (73.7s).
Freq-weighted top-1/top-3 at k=3, k=4, full length; arms ON (default opts),
OFF (`neighbor_off`), POLICY (`suggest_policy_at`); KSPW per (tab, arm).

```text
tab      k      arm         top1      top3         n         w    KSPW
words    @3     ON         0.762     0.812      1063    567514   0.676
words    @3     OFF        0.772     0.824      1063    567514   0.673
words    @3     POLICY     0.772     0.824      1063    567514   0.673
words    @4     ON         0.799     0.932       768    515983   0.676
words    @4     OFF        0.890     0.971       768    515983   0.673
words    @4     POLICY     0.890     0.971       768    515983   0.673
words    @full  ON         0.785     0.905      1064    567634   0.676
words    @full  OFF        0.872     0.942      1064    567634   0.673
NE       @3     ON         0.036     0.079      1560   2863902   0.863
NE       @3     OFF        0.052     0.122      1560   2863902   0.825
NE       @3     POLICY     0.052     0.122      1560   2863902   0.825
NE       @4     ON         0.104     0.273      1550   2842436   0.863
NE       @4     OFF        0.187     0.397      1550   2842436   0.825
NE       @4     POLICY     0.187     0.397      1550   2842436   0.825
NE       @full  ON         0.803     0.953      1561   2865496   0.863
NE       @full  OFF        0.906     0.981      1561   2865496   0.825
NE       @full  POLICY     0.906     0.981      1561   2865496   0.825
js       @3     ON         0.481     0.796        81     54000   0.575
js       @3     OFF        0.519     0.824        81     54000   0.567
js       @3     POLICY     0.519     0.824        81     54000   0.567
js       @4     ON         0.794     0.991        80     53500   0.575
js       @4     OFF        0.794     0.991        80     53500   0.567
js       @4     POLICY     0.794     0.991        80     53500   0.567
js       @full  ON         0.931     1.000        83     65500   0.575
js       @full  OFF        0.939     1.000        83     65500   0.567
js       @full  POLICY     0.939     1.000        83     65500   0.567
medical  @3     ON         0.665     0.898       100     25097   0.550
medical  @3     OFF        0.665     0.898       100     25097   0.550
medical  @3     POLICY     0.665     0.898       100     25097   0.550
medical  @full  ON         0.956     1.000       100     25097   0.550
medical  @full  OFF        1.000     1.000       100     25097   0.550
medical  @full  POLICY     1.000     1.000       100     25097   0.550
EN+NE    @4     ON         0.211     0.375             3358419       -
EN+NE    @4     OFF        0.295     0.485             3358419       -
EN+NE    @4     POLICY     0.295     0.485             3358419       -
```

Reading:

- OFF beats ON in every cell (neighbor fuzz never helps held-out accuracy);
  POLICY == OFF everywhere (no tab opts back into neighbors) — the shipped
  defaults are the accuracy-optimal arms. KSPW ≤ 0.863 on every tab (≤1.5 PASS).
- EN clears 85% top-3@4 on both arms (ON 0.932, OFF 0.971); js/medical ≈ 1.00.
- NE collapses at fixed k (0.273/0.397 @4) but clears the bar at full length
  (ON 0.953, OFF 0.981 top-3; OFF top-1 0.906). NE words average 8.2 seq
  digits vs EN 4.1 (median 8 vs 4), so a fixed-4 cut measures bucket luck,
  not ranking quality. NE KSPW 0.825 confirms users find words by typing on.
- Diagnosis: NE pack freqs are placeholder-flat (top-10% words hold 16% of
  weight vs 92% for EN's true Zipf; NE median 1742 / mean 1834), so
  freq-weighting spreads evenly over crowded transliteration buckets
  (4-prefix avg 5.3 words, max 109: `7262` sam*/pan* inflections). Per global
  rule 4, tail freqs are low-confidence — do not tune against them.

## Cheapest-first remedies tried (`00-gates.md` order)

1. prefix 0.9 toggle (`exp1a`): exact-only (no prefix, no neighbor) LOSES
   −0.215 scope top-3@4 (EN+NE 0.375→0.160; EN 0.932→0.883; NE 0.273→0.028).
   Prefix matching is load-bearing — rejected, prefix stays ON.
2. category boost / hard tab filter (`exp1b`, `exp1c`): already production
   defaults and confirmed. `w_cat` is 1.5 (train-tuned, +0.015 scope; the
   `exp1b` 0.3→0.6 labels are stale — delta +0.000 against the 1.5 default).
   Hard filter default-ON: js +0.037 (0.953→0.991), medical +0.021
   (0.979→1.000), EN/NE neutral (+0.005/+0.001). Kept ON everywhere.
3. personal+bigram: gate learning arm passes — repeat-subset (n=1487, words
   the frozen base ranks >3 @4) goes base 0.00 → +personal 1.00,
   lift +99.9pts (≥10 PASS). No change needed.
4. explicit `seq` overrides top-20 (`exp2` ceiling): miss mass is diffuse —
   top-20 4-prefixes hold 24.3% of OFF-arm miss weight (top prefix `7262`
   just 2.0%). A perfect zero-collateral fix of all 20 lifts OFF only
   0.485 → 0.611 upper bound — still 24pts short of 85%. Not implemented
   (ceiling insufficient; real overrides would do less).
5. per-category 16-key (`exp3`, new): t9-16 (pq/rs + wx/yz splits) OFF-arm
   top-3@4: EN 0.971→0.977 (+0.006), NE 0.397→0.457 (+0.060). Weak lever as
   predicted, and it contradicts the NE→t9-9 default (MASTER §2 rule 3).
   Not shipped; measured and documented.

Miss instrumentation (`exp0`, ON, EN+NE): 100% of miss weight is
scored-but-out (union mean 40.2, max 144 — the 200-truncate never bites;
0% outside-union / cut / hidden / blocked). It is a ranking-vs-crowding
problem, and remedies 1–5 cannot close a 37pt gap: list exhausted.

## GO / NO-GO

**NO-GO on the frozen bar** — scope top-3@4 neighbor-ON = 0.375 < 0.85
(KSPW 0.844 ≤ 1.5 PASS; lift +99.9pts ≥ 10 PASS). Remedies 1–5 exhausted:
none moves the scope number more than +0.13, and the two structural fixes
(real Zipf freqs, morphology-aware ranking) belong to Phase-1 row #3, not
Phase 0. Do not tune ranking against the flat NE tail (global rule 4).

Qualified read (what the numbers actually say):

- **EN is GO-level**: top-3@4 0.932 ON / 0.971 OFF, KSPW 0.673. js/medical ≈ 1.00.
- **NE fails only the fixed-k operating point**: top-3@full 0.953 ON /
  0.981 OFF (top-1 OFF 0.906) with KSPW 0.825 — the engine ranks NE well;
  k=4 on 8.2-digit words measures bucket collision, not quality.
- Shipped policy (neighbor-OFF + hard tab filter on every tab) is the
  accuracy-optimal arm in all 36 matrix cells — already the default, no
  change.
- Unblocks: everything that does not assume NE@4 numbers (Tier-1 rows
  2, 4–8). Re-run this gate after row #3 lands real NE freqs (HF 2.4M +
  Leipzig intersect → 5k+ `w+tr` pack); the frozen seed/split/harness is
  untouched and ready. Auto-detect multilingual union stays gated until
  NE per-tab clears (MASTER §5.4).
