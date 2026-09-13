//! Tier-0 follow-up experiments (plan/00 cheapest-first remedies + plan/09
//! desk spikes #2/#3). Frozen harness constants mirror `tests/gate.rs`
//! (same seed, split, `FROZEN_NOW`, op point, limit) so every delta is
//! measured against the reported NO-GO baseline
//! (top-3@4 ON=0.34 OFF=0.47 KSPW=0.851 lift=98.3).
//!
//! These tests assert structural invariants only — NEVER gate thresholds —
//! and print the measured numbers (`-- --nocapture`). No production default
//! is changed here; each remedy runs behind an explicit `SuggestOpts` /
//! `RankWeights` value.
//!
//! Run: `cargo test -p kbcore --test gate_exp -- --nocapture` (from
//! `core-rust/` drop the `-p` flag).

use kbcore::mapping::encode_word;
use kbcore::pack::load_pack_str;
use kbcore::personal::quantize_ts;
use kbcore::personal::PersonalDict;
use kbcore::rank::{bigram_term, RankWeights};
use kbcore::{DictionaryStack, SuggestOpts};

/// Frozen split seed (identical to `tests/gate.rs`; changing this
/// invalidates every number).
const GATE_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
/// Frozen wall-clock (quantized to its 1h bucket for scoring).
const FROZEN_NOW_RAW: i64 = 1_726_000_000;
/// Operating point: 4 digits.
const OP_K: usize = 4;
/// Suggest limit (expand-all sheet size).
const LIMIT: usize = 30;

fn frozen_now() -> i64 {
    quantize_ts(FROZEN_NOW_RAW).expect("frozen gate timestamp must quantize")
}

fn fnv1a(word: &str, lang: &str) -> u64 {
    let mut h: u64 = GATE_SEED;
    for b in word
        .bytes()
        .chain(std::iter::once(b'\x1f'))
        .chain(lang.bytes())
    {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01B3);
    }
    h
}

fn is_held_out(word: &str, lang: &str) -> bool {
    fnv1a(word, lang) % 5 == 4
}

struct CorpusWord {
    word: String,
    seq: String,
    freq: f64,
    cat: String,
    /// Roman transliteration source when the pack provides one (NE):
    /// the t9-16 arm re-encodes it under the 16-key pad, mirroring what a
    /// user of that layout types. Additive field; the frozen split, seqs,
    /// and every existing arm are untouched.
    tr: Option<String>,
}

fn load_corpus() -> (DictionaryStack, Vec<CorpusWord>) {
    let packs: &[(&str, i32)] = &[
        (include_str!("../../packs/words_en.json"), 0),
        (include_str!("../../packs/nepali.json"), 10),
        (include_str!("../../packs/code_js.json"), 20),
        (include_str!("../../packs/medical.json"), 30),
    ];
    let mut stack: Option<DictionaryStack> = None;
    let mut held = Vec::new();
    for (src, prio) in packs {
        let pack =
            load_pack_str(src).unwrap_or_else(|e| panic!("gate_exp: pack parse failed: {e}"));
        if pack.words.is_empty() {
            panic!("gate_exp: pack '{}' has zero words", pack.id);
        }
        let entries = pack.to_entries(*prio);
        if entries.is_empty() {
            panic!(
                "gate_exp: pack '{}' yields zero encodable entries",
                pack.id
            );
        }
        match &mut stack {
            None => stack = Some(DictionaryStack::new(entries)),
            Some(s) => s.add_entries(entries),
        }
        for w in &pack.words {
            let seq = w.seq.clone().unwrap_or_else(|| {
                w.tr.as_ref()
                    .map(|tr| encode_word(tr))
                    .unwrap_or_else(|| encode_word(&w.w))
            });
            if seq.is_empty() {
                continue;
            }
            let lang = w.lang.clone().unwrap_or_else(|| "en".to_string());
            if is_held_out(&w.w, &lang) {
                held.push(CorpusWord {
                    word: w.w.clone(),
                    seq,
                    freq: w.freq.max(1) as f64,
                    cat: w.cat.clone().unwrap_or_else(|| pack.id.clone()),
                    tr: w.tr.clone(),
                });
            }
        }
    }
    if held.is_empty() {
        panic!("gate_exp: frozen split produced an empty held-out set");
    }
    (stack.expect("gate_exp: no corpus packs loaded"), held)
}

fn in_scope(cat: &str) -> bool {
    cat == "words" || cat == "NE"
}

fn op_prefix(seq: &str) -> Option<String> {
    if seq.chars().count() >= OP_K {
        Some(seq.chars().take(OP_K).collect())
    } else {
        None
    }
}

fn rank_of(results: &[kbcore::Suggestion], target: &str) -> Option<usize> {
    results
        .iter()
        .position(|s| s.word == target)
        .map(|i| i + 1)
}

/// Freq-weighted top-3@4 for one tab-arm. Returns
/// `(hit_weight, total_weight)`.
fn arm_top3(
    stack: &DictionaryStack,
    held: &[CorpusWord],
    tab: Option<&str>,
    opts: &SuggestOpts,
    now: i64,
) -> (f64, f64) {
    let mut hit = 0.0;
    let mut tot = 0.0;
    for w in held {
        if !in_scope(&w.cat) {
            continue;
        }
        if let Some(t) = tab {
            if w.cat != t {
                continue;
            }
        }
        let Some(prefix) = op_prefix(&w.seq) else {
            continue;
        };
        tot += w.freq;
        let res = stack.suggest_with_opts_at("", &prefix, &w.cat, LIMIT, now, opts);
        if rank_of(&res, &w.word).map(|r| r <= 3).unwrap_or(false) {
            hit += w.freq;
        }
    }
    assert!(tot > 0.0, "gate_exp: empty scope for tab {tab:?}");
    (hit, tot)
}

/// Step 1 — instrument: for every freq-weighted scope miss at the operating
/// point (neighbor ON, frozen defaults), classify where the target was lost:
/// outside the union / cut by the 200-truncate / scored-but-out / hidden /
/// blocked. Prints the answer to "is the truncate the problem?".
#[test]
fn exp0_instrument_misses() {
    let now = frozen_now();
    let (base, held) = load_corpus();
    let opts = SuggestOpts::default();
    // (class, freq-weight) accumulators, EN+NE scope only.
    let (mut w_outside, mut w_cut, mut w_scored_out, mut w_hidden, mut w_blocked) =
        (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut w_miss, mut w_tot) = (0.0, 0.0);
    let (mut union_sum, mut union_max) = (0usize, 0usize);
    let mut n = 0usize;
    for w in &held {
        if !in_scope(&w.cat) {
            continue;
        }
        let Some(prefix) = op_prefix(&w.seq) else {
            continue;
        };
        w_tot += w.freq;
        let res = base.suggest_with_opts_at("", &prefix, &w.cat, LIMIT, now, &opts);
        if rank_of(&res, &w.word).map(|r| r <= 3).unwrap_or(false) {
            continue;
        }
        w_miss += w.freq;
        let p = base.probe_target_at("", &prefix, &w.cat, &w.word, LIMIT, now, &opts);
        // Probe/search agreement is load-bearing: top_rank must reproduce
        // the direct suggest rank (None = absent in both).
        assert_eq!(
            p.top_rank,
            rank_of(&res, &w.word),
            "probe disagrees with suggest for {:?}",
            w.word
        );
        union_sum += p.union_size;
        union_max = union_max.max(p.union_size);
        n += 1;
        if p.blocked {
            w_blocked += w.freq;
        } else if !p.in_union {
            w_outside += w.freq;
        } else if !p.in_truncate200 {
            w_cut += w.freq;
        } else if p.hidden_by_threshold {
            w_hidden += w.freq;
        } else {
            w_scored_out += w.freq;
        }
    }
    println!("== exp0 miss instrumentation (ON, EN+NE, freq-weighted) ==");
    println!("scope weight={w_tot:.0} miss weight={w_miss:.0} ({:.1}% of scope)", 100.0 * w_miss / w_tot);
    println!("outside-union : {:>8.1}% of MISS ({:.1}% of scope)", 100.0 * w_outside / w_miss, 100.0 * w_outside / w_tot);
    println!("cut-by-200    : {:>8.1}% of MISS ({:.1}% of scope)", 100.0 * w_cut / w_miss, 100.0 * w_cut / w_tot);
    println!("scored-but-out: {:>8.1}% of MISS ({:.1}% of scope)", 100.0 * w_scored_out / w_miss, 100.0 * w_scored_out / w_tot);
    println!("hidden-by-thr : {:>8.1}% of MISS ({:.1}% of scope)", 100.0 * w_hidden / w_miss, 100.0 * w_hidden / w_tot);
    println!("blocked       : {:>8.1}% of MISS ({:.1}% of scope)", 100.0 * w_blocked / w_miss, 100.0 * w_blocked / w_tot);
    println!("miss union size: mean={:.1} max={union_max} n={n}", union_sum as f64 / n.max(1) as f64);
}

/// Step 2a — prefix toggle: exact-only (no prefix, no neighbor) vs frozen
/// defaults, neighbor-ON arm. Measures what the 0.9-prefix mass is worth.
#[test]
fn exp1a_prefix_toggle() {
    let now = frozen_now();
    let (base, held) = load_corpus();
    let on = SuggestOpts::default();
    let exact = SuggestOpts::exact_only();
    println!("== exp1a prefix toggle (top-3@4, freq-weighted) ==");
    for tab in [None, Some("words"), Some("NE")] {
        let (h_on, t) = arm_top3(&base, &held, tab, &on, now);
        let (h_ex, _) = arm_top3(&base, &held, tab, &exact, now);
        println!(
            "tab {:<6}: default(ON)={:.3} exact-only={:.3} delta={:+.3}",
            tab.unwrap_or("EN+NE"),
            h_on / t,
            h_ex / t,
            h_ex / t - h_on / t
        );
    }
}

/// Step 2b — category boost 0.3 -> 0.6 under neighbor ON. Implemented purely
/// as a `RankWeights` value (no engine change): rebuilds the stack with the
/// same entries but the boosted weight.
#[test]
fn exp1b_cat_boost() {
    let now = frozen_now();
    let (base, held) = load_corpus();
    let on = SuggestOpts::default();
    let (h_on, t) = arm_top3(&base, &held, None, &on, now);
    // Rebuild an identical stack with w_cat=0.6: clone entries via a fresh
    // load so row order (and hence every tie rule) is unchanged.
    let packs: &[(&str, i32)] = &[
        (include_str!("../../packs/words_en.json"), 0),
        (include_str!("../../packs/nepali.json"), 10),
        (include_str!("../../packs/code_js.json"), 20),
        (include_str!("../../packs/medical.json"), 30),
    ];
    let mut boosted: Option<DictionaryStack> = None;
    for (src, prio) in packs {
        let pack = load_pack_str(src).expect("pack must parse");
        let entries = pack.to_entries(*prio);
        match &mut boosted {
            None => {
                let mut w = RankWeights::default();
                w.w_cat = 0.6;
                boosted = Some(DictionaryStack::with_weights(entries, w));
            }
            Some(s) => s.add_entries(entries),
        }
    }
    let boosted = boosted.expect("packs must load");
    let (h_b, _) = arm_top3(&boosted, &held, None, &on, now);
    println!("== exp1b cat-boost 0.3->0.6 (ON, EN+NE) ==");
    println!(
        "default w_cat=0.3: {:.3}  boosted w_cat=0.6: {:.3}  delta={:+.3}",
        h_on / t,
        h_b / t,
        h_b / t - h_on / t
    );
    for tab in [Some("words"), Some("NE")] {
        let (a, tt) = arm_top3(&base, &held, tab, &on, now);
        let (b, _) = arm_top3(&boosted, &held, tab, &on, now);
        println!(
            "tab {:<6}: 0.3={:.3} 0.6={:.3} delta={:+.3}",
            tab.unwrap(),
            a / tt,
            b / tt,
            b / tt - a / tt
        );
    }
}

/// Step 2c — hard tab filter (scope candidates to the active tab before the
/// truncate) under neighbor ON, all four tabs. Code tabs measure whether the
/// filter removes cross-tab noise or destroys EN/NE recall via merged rows.
/// (The filter is default-ON since per-tab isolation, so the no-filter arm
/// is the explicit opt-out — the delta is the same measurement, arm names
/// swapped.)
#[test]
fn exp1c_hard_tab_filter() {
    let now = frozen_now();
    let (base, held) = load_corpus();
    let filt = SuggestOpts::default();
    assert!(filt.hard_tab_filter, "filter must be default-ON");
    let on = SuggestOpts {
        hard_tab_filter: false,
        ..SuggestOpts::default()
    };
    // Tab-aware arm (no EN+NE scope restriction here: code tabs included).
    let arm_all = |tab: &str, opts: &SuggestOpts| -> (f64, f64) {
        let mut hit = 0.0;
        let mut tot = 0.0;
        for w in &held {
            if w.cat != tab {
                continue;
            }
            let Some(prefix) = op_prefix(&w.seq) else {
                continue;
            };
            tot += w.freq;
            let res = base.suggest_with_opts_at("", &prefix, &w.cat, LIMIT, now, opts);
            if rank_of(&res, &w.word).map(|r| r <= 3).unwrap_or(false) {
                hit += w.freq;
            }
        }
        (hit, tot)
    };
    println!("== exp1c hard tab filter (ON, top-3@4) ==");
    for tab in ["words", "NE", "js", "medical"] {
        let (a, t) = arm_all(tab, &on);
        let (b, _) = arm_all(tab, &filt);
        println!(
            "tab {:<7}: nofilter={:.3} hardfilter={:.3} delta={:+.3} (w={t:.0})",
            tab,
            a / t,
            b / t,
            b / t - a / t
        );
    }
}

/// Step 2d/4 — neighbor policy per tab: ON vs OFF under identical frozen
/// conditions (validates `suggest_no_neighbor_at` parity with the OFF opts
/// arm), plus the default-OFF tradeoff statement.
#[test]
fn exp1d_neighbor_policy() {
    let now = frozen_now();
    let (base, held) = load_corpus();
    let on = SuggestOpts::default();
    let off = SuggestOpts::neighbor_off();
    println!("== exp1d neighbor ON vs OFF (top-3@4, freq-weighted) ==");
    // Parity: the dedicated OFF entry point must agree exactly with the
    // opts arm on every scope word (same union, same scores, same order).
    let mut checked = 0;
    for w in &held {
        if !in_scope(&w.cat) {
            continue;
        }
        let Some(prefix) = op_prefix(&w.seq) else {
            continue;
        };
        let a = base.suggest_no_neighbor_at("", &prefix, &w.cat, LIMIT, now);
        let b = base.suggest_with_opts_at("", &prefix, &w.cat, LIMIT, now, &off);
        assert_eq!(
            a.iter().map(|s| (&s.word, s.score.to_bits())).collect::<Vec<_>>(),
            b.iter().map(|s| (&s.word, s.score.to_bits())).collect::<Vec<_>>(),
            "OFF-arm parity broken for {:?}",
            w.word
        );
        checked += 1;
    }
    println!("OFF parity: {checked} scope prefixes byte-identical");
    for tab in [None, Some("words"), Some("NE")] {
        let (h_on, t) = arm_top3(&base, &held, tab, &on, now);
        let (h_off, _) = arm_top3(&base, &held, tab, &off, now);
        println!(
            "tab {:<6}: ON={:.3} OFF={:.3} delta(OFF-ON)={:+.3}",
            tab.unwrap_or("EN+NE"),
            h_on / t,
            h_off / t,
            h_off / t - h_on / t
        );
    }
}

/// Step 2 (ceiling) — explicit-seq override headroom: share of the OFF-arm
/// miss mass concentrated in the top-20 4-digit prefixes. A perfect fix of
/// those 20 buckets (zero collateral) is the UPPER BOUND of what seq
/// overrides for the top-20 colliders can buy; data lives in packs (untouched
/// here — this test only measures the ceiling).
#[test]
fn exp2_seq_override_ceiling() {
    let now = frozen_now();
    let (base, held) = load_corpus();
    let off = SuggestOpts::neighbor_off();
    let mut miss_by_prefix: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let (mut tot_w, mut miss_w) = (0.0, 0.0);
    for w in &held {
        if !in_scope(&w.cat) {
            continue;
        }
        let Some(prefix) = op_prefix(&w.seq) else {
            continue;
        };
        tot_w += w.freq;
        let res = base.suggest_with_opts_at("", &prefix, &w.cat, LIMIT, now, &off);
        if !rank_of(&res, &w.word).map(|r| r <= 3).unwrap_or(false) {
            miss_w += w.freq;
            *miss_by_prefix.entry(prefix).or_insert(0.0) += w.freq;
        }
    }
    let (h_off, _) = arm_top3(&base, &held, None, &off, now);
    let base_rate = h_off / tot_w;
    let mut v: Vec<(&String, &f64)> = miss_by_prefix.iter().collect();
    v.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    let top20: f64 = v.iter().take(20).map(|(_, w)| **w).sum();
    println!("== exp2 seq-override ceiling (OFF arm, EN+NE) ==");
    println!("OFF top3@4={base_rate:.3} scope weight={tot_w:.0} miss weight={miss_w:.0}");
    println!("top-5 miss prefixes:");
    for (p, w) in v.iter().take(5) {
        println!("  {p}: miss weight {w:.0} ({:.1}% of miss)", 100.0 * *w / miss_w);
    }
    println!(
        "top-20 prefix share: {:.1}% of MISS, {:.1}% of TOTAL",
        100.0 * top20 / miss_w,
        100.0 * top20 / tot_w
    );
    println!(
        "perfect top-20 fix upper bound: OFF {:.3} -> {:.3} (assumes zero collateral)",
        base_rate,
        base_rate + top20 / tot_w
    );
}

/// Step 5 (last resort) — per-category 16-key layout arm (plan/00 remedy
/// 5, "weak lever — do last"): `t9-16` splits pq/rs + wx/yz, so crowded
/// t9-9 buckets sharing a 7/9 code (e.g. NE `7262` = sam* + pan*) diverge.
/// Both digits AND matching move to the 16-key pad (fair layout comparison:
/// the user types on that pad — digits re-encoded from `tr`/word under the
/// t9-16 mapping, mirroring the loader's `seq_t916` precedence). Neighbor
/// OFF (shipped policy; the ON-vs-OFF delta is owned by `exp1d`).
/// Report-only; the production default stays `t9-9` (MASTER §2 rule 3).
#[test]
fn exp3_t916_layout_arm() {
    use kbcore::{KeyMapping, LayoutRegistry};
    let now = frozen_now();
    let (base, held) = load_corpus();
    let reg = LayoutRegistry::with_builtins();
    let m16 = reg.get_or_default("t9-16").clone();
    assert_eq!(m16.layout_id(), "t9-16", "t9-16 built-in must exist");
    println!("== exp3 t9-16 layout arm (OFF, top-3@4, freq-weighted) ==");
    for scope in ["words", "NE"] {
        let (mut b_hit, mut b_tot) = (0.0, 0.0);
        let (mut s_hit, mut s_tot) = (0.0, 0.0);
        let (mut n, mut n16, mut nempty) = (0usize, 0usize, 0usize);
        for w in &held {
            if w.cat != scope {
                continue;
            }
            let Some(prefix) = op_prefix(&w.seq) else {
                continue;
            };
            n += 1;
            b_tot += w.freq;
            let res = base.suggest_no_neighbor_at("", &prefix, &w.cat, LIMIT, now);
            if rank_of(&res, &w.word).map(|r| r <= 3).unwrap_or(false) {
                b_hit += w.freq;
            }
            // t9-16 digits from the same source the loader precomputes
            // `seq_t916` from (`tr` when present, else the word).
            let src = w.tr.as_deref().unwrap_or(&w.word);
            let seq16 = m16.encode_word(src);
            if seq16.chars().count() < OP_K {
                nempty += 1;
                continue;
            }
            n16 += 1;
            s_tot += w.freq;
            let p16: String = seq16.chars().take(OP_K).collect();
            let res16 =
                base.suggest_for_layout_no_neighbor_at("", &p16, &m16, &w.cat, LIMIT, now);
            if rank_of(&res16, &w.word).map(|r| r <= 3).unwrap_or(false) {
                s_hit += w.freq;
            }
        }
        assert!(n > 0, "gate_exp exp3: empty scope for tab {scope:?}");
        println!(
            "tab {:<6}: t9-9 OFF={:.3} (n={n})  t9-16 OFF={:.3} (n16={n16} skipped-short={nempty})  delta={:+.3}",
            scope,
            b_hit / b_tot,
            if s_tot > 0.0 { s_hit / s_tot } else { f64::NAN },
            if s_tot > 0.0 { s_hit / s_tot } else { f64::NAN } - b_hit / b_tot
        );
    }
}

/// 09#2 — bigram-V stability desk spike: 7-day replay with a growing dict
/// (350 learns/day + a stable 5-count ("the","quick") history from day 0).
/// Compares dynamic-V (current `personal.rs:172`), fixed-V=10k, and Lidstone
/// alpha 0.5/0.1. Scores are weight-applied (`w_bigram=0.8`) so drift reads
/// directly in suggest-score points. Falsifiable outcome: dynamic-V unseen
/// drift over the week vs fixed (0 by construction) vs Lidstone.
#[test]
fn spike_bigram_v() {
    fn gen(cpw: u64, cp: u64, v: u64, alpha: f64) -> f64 {
        if cp == 0 {
            return 0.0;
        }
        (cpw as f64 + alpha).log10() - (cp as f64 + alpha * v as f64).log10()
    }
    // Sanity: the alpha=1.0/dynamic-V formula must reproduce bigram_term.
    assert_eq!(bigram_term(0, 0, 7), 0.0);
    let probe = bigram_term(0, 5, 352);
    assert!(
        (probe - gen(0, 5, 352, 1.0)).abs() < 1e-12,
        "gen must mirror bigram_term at alpha=1, got {probe}"
    );
    let pool: Vec<String> = (0..3000).map(|i| format!("w{i:04}")).collect();
    let mut d = PersonalDict::new();
    const W_B: f64 = 0.8;
    println!("== 09#2 bigram-V stability (weight-applied, w=0.8) ==");
    println!("{:>3} {:>7} {:>9} {:>9} {:>9} {:>9} {:>9}", "day", "V", "dynUnseen", "fixUnseen", "lid05Uns", "lid01Uns", "dynSeen");
    for day in 0..7 {
        for i in 0..350 {
            d.learn(&pool[(day * 350 + i) % 3000], "EN");
        }
        if day == 0 {
            for _ in 0..5 {
                d.record_bigram("the", "quick");
            }
            d.learn("the", "EN");
            d.learn("quick", "EN");
        }
        let (cpw_seen, cp, v_dyn) = d.bigram_counts("the", "quick");
        let (cpw_un, _, _) = d.bigram_counts("the", "zorblax");
        assert_eq!(cpw_un, 0, "zorblax must stay unseen");
        println!(
            "{:>3} {:>7} {:>9.3} {:>9.3} {:>9.3} {:>9.3} {:>9.3}",
            day,
            v_dyn,
            W_B * gen(cpw_un, cp, v_dyn, 1.0),
            W_B * gen(cpw_un, cp, 10_000, 1.0),
            W_B * gen(cpw_un, cp, v_dyn, 0.5),
            W_B * gen(cpw_un, cp, v_dyn, 0.1),
            W_B * gen(cpw_seen, cp, v_dyn, 1.0),
        );
    }
    let (cpw_seen, cp, _) = d.bigram_counts("the", "quick");
    assert_eq!((cpw_seen, cp), (5, 5));
    // Day-0 V (352: 350 pool + the/quick) vs day-6 V summary drifts.
    for (name, alpha, fixed) in [("dynamic", 1.0, None), ("fixed10k", 1.0, Some(10_000u64)), ("lidstone0.5", 0.5, None), ("lidstone0.1", 0.1, None)] {
        let v0 = 352u64;
        let v7 = d.bigram_counts("the", "quick").2;
        let drift = W_B
            * (gen(0, 5, fixed.unwrap_or(v7), alpha) - gen(0, 5, fixed.unwrap_or(v0), alpha));
        println!("{name:<12} unseen-pair drift day0->day6: {drift:+.4} score pts");
    }
}

/// 09#3 — schwa/alias cap-vs-collision curve (NE desk spike): for cap=0..3
/// (first-`cap` alts per headword admitted as match-any seqs, primary always
/// present), count 9-key buckets with >4 / >5 distinct-headword colliders
/// and the share of headwords sitting in such buckets. Owner picks the cap
/// from this curve — the test only measures it.
#[test]
fn spike_alias_cap_curve() {
    let src = include_str!("../../packs/nepali.json");
    let pack = load_pack_str(src).expect("nepali pack must parse");
    // (headword, primary seq, alt seqs in pack order)
    let mut heads: Vec<(String, String, Vec<String>)> = Vec::new();
    for w in &pack.words {
        let primary = w.seq.clone().unwrap_or_else(|| {
            w.tr.as_ref()
                .map(|tr| encode_word(tr))
                .unwrap_or_else(|| encode_word(&w.w))
        });
        if primary.is_empty() {
            continue;
        }
        let mut alts = Vec::new();
        for a in &w.alt {
            let s = encode_word(a);
            if !s.is_empty() && s != primary && !alts.contains(&s) {
                alts.push(s);
            }
        }
        heads.push((w.w.clone(), primary, alts));
    }
    assert!(!heads.is_empty(), "nepali pack must yield headwords");
    println!("== 09#3 alias cap-vs-collision (NE, {} headwords) ==", heads.len());
    println!("{:>3} {:>9} {:>9} {:>11} {:>11}", "cap", "seqs", "buckets", ">4coll", ">5coll");
    for cap in 0..=3usize {
        let mut buckets: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for (word, primary, alts) in &heads {
            buckets.entry(primary.clone()).or_default().push(word.clone());
            for a in alts.iter().take(cap) {
                buckets.entry(a.clone()).or_default().push(word.clone());
            }
        }
        // Dedupe headwords per bucket (primary==alt overlap across rows of
        // the same word must not count as a collision with itself).
        let mut over4 = 0usize;
        let mut over5 = 0usize;
        let mut words_in_over4: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for members in buckets.values() {
            let mut uniq: Vec<&str> = members.iter().map(String::as_str).collect();
            uniq.sort();
            uniq.dedup();
            if uniq.len() > 4 {
                over4 += 1;
                words_in_over4.extend(uniq.iter().copied());
            }
            if uniq.len() > 5 {
                over5 += 1;
            }
        }
        let seqs: usize = buckets.len();
        println!(
            "{cap:>3} {seqs:>9} {:>9} {over4:>11} {over5:>11}  ({:.1}% heads in >4 buckets)",
            buckets.len(),
            100.0 * words_in_over4.len() as f64 / heads.len() as f64
        );
    }
}
