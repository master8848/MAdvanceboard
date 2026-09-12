//! Morphology + connection diagnosis (user feedback: verb forms and
//! next-word connections "are not there").
//!
//! Frozen harness constants mirror `tests/gate.rs` (same seed, split,
//! `FROZEN_NOW`, op point, limit). Structural asserts only — NEVER gate
//! thresholds — plus printed numbers (`-- --nocapture`). No production
//! default is changed here.
//!
//! Run: `cargo test -p kbcore --test morph_conn_diag -- --nocapture`.

use kbcore::mapping::encode_word;
use kbcore::pack::load_pack_str;
use kbcore::personal::quantize_ts;
use kbcore::rank::bigram_term;
use kbcore::{DictionaryStack, SuggestOpts};

/// Frozen split seed (identical to `tests/gate.rs`).
const GATE_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
/// Frozen wall-clock (quantized to its 1h bucket for scoring).
const FROZEN_NOW_RAW: i64 = 1_726_000_000;
/// Operating point: 4 digits.
const OP_K: usize = 4;
/// Suggest limit (expand-all sheet size).
const LIMIT: usize = 30;
/// Bigram context word for the connection arm (mirrors the gate learning
/// arm: common prev, no base history).
const CONN_CTX: &str = "the";

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
    lang: String,
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
            load_pack_str(src).unwrap_or_else(|e| panic!("diag: pack parse failed: {e}"));
        if pack.words.is_empty() {
            panic!("diag: pack '{}' has zero words", pack.id);
        }
        let entries = pack.to_entries(*prio);
        if entries.is_empty() {
            panic!(
                "diag: pack '{}' yields zero encodable entries",
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
                    lang,
                });
            }
        }
    }
    if held.is_empty() {
        panic!("diag: frozen split produced an empty held-out set");
    }
    (stack.expect("diag: no corpus packs loaded"), held)
}

fn rank_of(results: &[kbcore::Suggestion], target: &str) -> Option<usize> {
    results
        .iter()
        .position(|s| s.word == target)
        .map(|i| i + 1)
}

fn op_prefix(seq: &str) -> Option<String> {
    if seq.chars().count() >= OP_K {
        Some(seq.chars().take(OP_K).collect())
    } else {
        None
    }
}

/// Strict English inflection check: is `word` a regular plural / 3sg / past
/// / progressive of a TRAIN-split stem? Returns the stem + rule name.
/// (Irregulars — ran, went, ate — are vocab-coverage rows, not morphology.)
fn strict_inflection_of(word: &str, train: &std::collections::HashSet<String>) -> Option<(String, &'static str)> {
    let wl = word.to_lowercase();
    // -ies -> y (tries -> try)
    if wl.ends_with("ies") && wl.len() > 4 {
        let st = format!("{}y", &wl[..wl.len() - 3]);
        if train.contains(&st) {
            return Some((st, "ies->y"));
        }
    }
    // -es (watches -> watch, fixes -> fix)
    if wl.ends_with("es") && wl.len() > 4 {
        for st in [wl[..wl.len() - 2].to_string(), wl[..wl.len() - 1].to_string()] {
            if train.contains(&st) {
                return Some((st, "-es"));
            }
        }
    }
    // -s (runs -> run), never -ss
    if wl.ends_with('s') && !wl.ends_with("ss") && wl.len() > 3 {
        let st = wl[..wl.len() - 1].to_string();
        if train.contains(&st) {
            return Some((st, "-s"));
        }
    }
    // -ied -> y (tried -> try)
    if wl.ends_with("ied") && wl.len() > 4 {
        let st = format!("{}y", &wl[..wl.len() - 3]);
        if train.contains(&st) {
            return Some((st, "ied->y"));
        }
    }
    // -ed (walked -> walk, made <- make e-drop, stopped <- stop dbl)
    if wl.ends_with("ed") && wl.len() > 4 {
        let st = wl[..wl.len() - 2].to_string();
        if train.contains(&st) {
            return Some((st.clone(), "-ed"));
        }
        let eform = format!("{st}e");
        if train.contains(&eform) {
            return Some((eform, "-ed e-drop"));
        }
        if st.len() >= 3 {
            let bytes = st.as_bytes();
            if bytes[st.len() - 1] == bytes[st.len() - 2] {
                let dbl = st[..st.len() - 1].to_string();
                if train.contains(&dbl) {
                    return Some((dbl, "-ed dbl"));
                }
            }
        }
    }
    // -ying -> y (trying -> try)
    if wl.ends_with("ying") && wl.len() > 5 {
        let st = format!("{}y", &wl[..wl.len() - 4]);
        if train.contains(&st) {
            return Some((st, "ying->y"));
        }
    }
    // -ing (walking -> walk, making <- make e-drop, running <- run dbl)
    if wl.ends_with("ing") && wl.len() > 5 {
        let st = wl[..wl.len() - 3].to_string();
        if train.contains(&st) {
            return Some((st.clone(), "-ing"));
        }
        let eform = format!("{st}e");
        if train.contains(&eform) {
            return Some((eform, "-ing e-drop"));
        }
        if st.len() >= 3 {
            let bytes = st.as_bytes();
            if bytes[st.len() - 1] == bytes[st.len() - 2] {
                let dbl = st[..st.len() - 1].to_string();
                if train.contains(&dbl) {
                    return Some((dbl, "-ing dbl"));
                }
            }
        }
    }
    None
}

/// D1 — inflection coverage gap: fixed probe list of common regular
/// inflections, how many are absent from the EN vocab entirely.
#[test]
fn diag1_inflection_coverage() {
    let src = include_str!("../../packs/words_en.json");
    let pack = load_pack_str(src).expect("words_en must parse");
    let vocab: std::collections::HashSet<String> =
        pack.words.iter().map(|w| w.w.to_lowercase()).collect();
    // Regular inflections of common verbs/nouns (stem + s/es/ed/ing).
    let probes = [
        "runs", "running", "walks", "walked", "walking", "plays", "played", "playing",
        "works", "worked", "working", "talks", "talked", "talking", "likes", "liked",
        "liking", "gets", "getting", "comes", "coming", "sees", "seeing", "knows",
        "knowing", "gives", "giving", "thinks", "thinking", "feels", "feeling",
        "tries", "trying", "studies", "studied", "studying", "cities", "babies",
        "boxes", "watches", "watched", "watching", "fixes", "fixed", "fixing",
        "buzzes", "buzzed", "buzzing", "eating", "writing", "written",
    ];
    let missing: Vec<&&str> = probes.iter().filter(|p| !vocab.contains(&p.to_string())).collect();
    println!("== diag1 inflection coverage (EN vocab n={}) ==", vocab.len());
    println!(
        "probe regular inflections: {}  missing: {} ({:.1}%)",
        probes.len(),
        missing.len(),
        100.0 * missing.len() as f64 / probes.len() as f64
    );
    println!("missing: {missing:?}");
    // Structural lock: the gap is real and large (majority of the probe
    // list absent). If a future vocab expansion fixes this, this assert
    // must be updated deliberately — not silently.
    assert!(
        missing.len() * 2 > probes.len(),
        "diag1: expected most probe inflections to be missing from vocab"
    );
}

/// D2 — gate-miss inflection link: among policy-arm misses at the operating
/// point, how many targets are strict inflections of a TRAIN stem, and what
/// is the perfect-fix upper bound (freq-weighted)?
#[test]
fn diag2_miss_inflection_share() {
    let now = frozen_now();
    let (base, held) = load_corpus();
    // TRAIN stems per tab (EN scope only for the morphology rules).
    let train_en: std::collections::HashSet<String> = {
        let packs: &[&str] = &[include_str!("../../packs/words_en.json")];
        let mut t = std::collections::HashSet::new();
        for src in packs {
            let pack = load_pack_str(src).expect("pack must parse");
            for w in &pack.words {
                let lang = w.lang.clone().unwrap_or_else(|| "en".to_string());
                if !is_held_out(&w.w, &lang) {
                    t.insert(w.w.to_lowercase());
                }
            }
        }
        t
    };
    let policy_of = |tab: &str| SuggestOpts::policy_for_tab(tab);
    let (mut tot_w, mut miss_w, mut infl_w) = (0.0, 0.0, 0.0);
    let (mut n_tot, n_miss, n_infl) = (0usize, 0usize, 0usize);
    let mut infl_examples: Vec<(String, String, &'static str)> = Vec::new();
    for w in &held {
        if w.cat != "words" {
            continue;
        }
        let Some(prefix) = op_prefix(&w.seq) else {
            continue;
        };
        tot_w += w.freq;
        n_tot += 1;
        let opts = policy_of(&w.cat);
        let res = base.suggest_with_opts_at("", &prefix, &w.cat, LIMIT, now, &opts);
        if rank_of(&res, &w.word).map(|r| r <= 3).unwrap_or(false) {
            continue;
        }
        miss_w += w.freq;
        let _ = n_miss;
        if let Some((stem, rule)) = strict_inflection_of(&w.word, &train_en) {
            infl_w += w.freq;
            let _ = n_infl;
            if infl_examples.len() < 10 {
                infl_examples.push((w.word.clone(), stem, rule));
            }
        }
    }
    let _ = (n_miss, n_infl);
    println!("== diag2 policy-arm EN misses vs inflection (op=4 digits) ==");
    println!("EN scope: n={n_tot} weight={tot_w:.0}");
    println!(
        "miss weight={miss_w:.0} ({:.1}% of scope)",
        100.0 * miss_w / tot_w
    );
    println!(
        "misses that ARE strict inflections of a train stem: weight={infl_w:.0} \
         ({:.1}% of MISS, {:.1}% of scope)",
        100.0 * infl_w / miss_w,
        100.0 * infl_w / tot_w
    );
    println!("examples (target -> stem via rule): {infl_examples:?}");
    let base_rate = (tot_w - miss_w) / tot_w;
    println!(
        "perfect inflection-fix upper bound: EN policy {:.3} -> {:.3} (assumes zero collateral)",
        base_rate,
        base_rate + infl_w / tot_w
    );
}

/// D3 — connection gap: (a) cold-start structural fact — with no bigram
/// history, setting ctx changes nothing (backoff-0 => term identically 0);
/// (b) rescue ceiling — among policy-arm misses, how many would ONE learned
/// bigram pair (the gate learning-arm protocol: 1x record_bigram) promote
/// into the top-3 via the observed-vs-unseen swing?
#[test]
fn diag3_bigram_connection() {
    let now = frozen_now();
    let (base, held) = load_corpus();
    // (a) Cold: ctx="" vs ctx="the" with empty personal must agree exactly.
    let mut checked = 0usize;
    for w in &held {
        if w.cat != "words" && w.cat != "NE" {
            continue;
        }
        let Some(prefix) = op_prefix(&w.seq) else {
            continue;
        };
        let opts = SuggestOpts::policy_for_tab(&w.cat);
        let cold_empty = base.suggest_with_opts_at("", &prefix, &w.cat, LIMIT, now, &opts);
        let cold_ctx = base.suggest_with_opts_at(CONN_CTX, &prefix, &w.cat, LIMIT, now, &opts);
        assert_eq!(
            cold_empty.iter().map(|s| (&s.word, s.score.to_bits())).collect::<Vec<_>>(),
            cold_ctx.iter().map(|s| (&s.word, s.score.to_bits())).collect::<Vec<_>>(),
            "cold ctx must be a no-op for {:?}",
            w.word
        );
        checked += 1;
    }
    println!("== diag3 connections (ctx={CONN_CTX:?}) ==");
    println!("(a) cold-start: ctx is a no-op on {checked} scope prefixes (backoff-0, structural)");

    // (b) Rescue: per miss, simulate the learned pair (cpw=1, cp=1 — the
    // gate learning-arm protocol) scored against the REAL current top-3
    // (which would eat the unseen penalty once `prev` has history).
    let w_bigram = kbcore::rank::RankWeights::default().w_bigram;
    let (mut tot_w, mut miss_w, mut rescue_w) = (0.0, 0.0, 0.0);
    for w in &held {
        if w.cat != "words" && w.cat != "NE" {
            continue;
        }
        let Some(prefix) = op_prefix(&w.seq) else {
            continue;
        };
        tot_w += w.freq;
        let opts = SuggestOpts::policy_for_tab(&w.cat);
        let res = base.suggest_with_opts_at("", &prefix, &w.cat, LIMIT, now, &opts);
        if rank_of(&res, &w.word).map(|r| r <= 3).unwrap_or(false) {
            continue;
        }
        miss_w += w.freq;
        // Score swing for the target (seen pair) relative to a distractor
        // (unseen pair) once prev has count 1: both move from 0.0 (backoff)
        // to their smoothed terms.
        let target_gain = w_bigram * bigram_term(1, 1, 10_000);
        let distr_gain = w_bigram * bigram_term(0, 1, 10_000);
        let swing = target_gain - distr_gain;
        assert!(swing > 0.0, "seen pair must outrank unseen pair");
        // Target needs to pass the 3rd-place score; distractors ahead of it
        // all eat `distr_gain`, the target eats `target_gain`.
        if res.len() >= 3 {
            let third = res[2].score;
            let target_score = res
                .iter()
                .find(|s| s.word == w.word)
                .map(|s| s.score);
            match target_score {
                Some(ts) if ts + swing >= third + distr_gain => rescue_w += w.freq,
                None => {
                    // Target absent from top-30: needs to beat the 30th too;
                    // count only if the swing clears the 3rd (upper bound —
                    // assumes the target is otherwise rank ~4-30, else the
                    // probe below over-counts; reported as ceiling).
                    let probe = base.probe_target_at("", &prefix, &w.cat, &w.word, LIMIT, now, &opts);
                    if probe.in_union {
                        rescue_w += w.freq * 0.0; // absent from list: not rescued at limit 30
                    }
                }
                _ => {}
            }
        } else {
            rescue_w += w.freq;
        }
    }
    println!(
        "(b) one-learned-pair rescue ceiling: miss weight={miss_w:.0} rescued={rescue_w:.0} \
         ({:.1}% of MISS, {:.1}% of scope)",
        100.0 * rescue_w / miss_w,
        100.0 * rescue_w / tot_w
    );
    let base_rate = (tot_w - miss_w) / tot_w;
    println!(
        "perfect single-pair-bigram upper bound: policy {:.3} -> {:.3} (assumes the learned prev is the actual ctx)",
        base_rate,
        base_rate + rescue_w / tot_w
    );
}
