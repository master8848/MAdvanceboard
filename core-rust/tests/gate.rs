//! 00-gates harness (`plan/00-gates.md`): frozen corpus split + seed,
//! KSPW / top-3@4 / learning lift, neighbor ON vs OFF, per-tab EN/NE/js,
//! operating point 4 digits, T9 parity `hello->43556`, `कमल->267`.
//!
//! Frozen inputs (do not tune to pass):
//! - `GATE_SEED = 0x9E3779B97F4A7C15`, FNV-1a `(seed, word, lang) % 5 == 4`
//!   => 20% held-out (never trains personal).
//! - `FROZEN_NOW` = quantized `1726000000` (1h bucket, deterministic).
//! - `RankWeights::default()`, limit 30, `ctx=""` (learning arm `"the"`).
//! - Corpus: `packs/words_en.json` (EN base) + `packs/nepali.json` (NE,
//!   Devanagari words) + `packs/code_js.json` + `packs/medical.json`
//!   (distractors), embedded via `include_str!` so a missing corpus is a
//!   compile-time error; an empty pack is an explicit test failure.
//!
//! Thresholds (frozen before measuring):
//! - `KSPW <= 1.5` on the EN+NE freq-weighted set, neighbor ON
//!   (type-until-in-top-3 + 1 select tap, taps per char).
//! - `top-3 @ 4-digit prefix >= 85%` exact-seq, neighbor ON (EN+NE).
//! - `learning lift >= 10pts` top-3@4 on the repeat-word subset
//!   (held-out words the frozen base ranks >3 at 4 digits), paired A/B
//!   frozen-base vs +personal replay (2 accepts + 1 bigram context).
//!
//! Run with `cargo test -p kbcore --test gate -- --nocapture` to see the
//! full PASS/FAIL report on stdout.

use kbcore::mapping::encode_word;
use kbcore::pack::load_pack_str;
use kbcore::personal::quantize_ts;
use kbcore::rank::RankWeights;
use kbcore::DictionaryStack;

/// Frozen split seed. Changing this invalidates every gate number.
const GATE_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
/// Frozen wall-clock (quantized to its 1h bucket for scoring).
const FROZEN_NOW_RAW: i64 = 1_726_000_000;
/// Operating point: 4 digits (3-digit buckets are hopeless by theory).
const OP_K: usize = 4;
/// Suggest limit (expand-all sheet size).
const LIMIT: usize = 30;
/// Bigram context word for the learning arm (common prev, no base history).
const LEARN_CTX: &str = "the";

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

fn load_corpus() -> (DictionaryStack, Vec<CorpusWord>, Vec<CorpusWord>) {
    let packs: &[(&str, i32)] = &[
        (include_str!("../../packs/words_en.json"), 0),
        (include_str!("../../packs/nepali.json"), 10),
        (include_str!("../../packs/code_js.json"), 20),
        (include_str!("../../packs/medical.json"), 30),
    ];
    let mut stack: Option<DictionaryStack> = None;
    let mut train = Vec::new();
    let mut held = Vec::new();
    for (src, prio) in packs {
        let pack = load_pack_str(src)
            .unwrap_or_else(|e| panic!("gate: corpus pack failed to parse: {e}"));
        if pack.words.is_empty() {
            panic!("gate: corpus pack '{}' has zero words", pack.id);
        }
        let entries = pack.to_entries(*prio);
        if entries.is_empty() {
            panic!(
                "gate: corpus pack '{}' yields zero encodable entries",
                pack.id
            );
        }
        match &mut stack {
            None => stack = Some(DictionaryStack::new(entries)),
            Some(s) => s.add_entries(entries),
        }
        for w in &pack.words {
            let seq = w
                .seq
                .clone()
                .unwrap_or_else(|| encode_word(&w.w));
            if seq.is_empty() {
                continue;
            }
            let cw = CorpusWord {
                word: w.w.clone(),
                seq,
                freq: w.freq.max(1) as f64,
                cat: w.cat.clone().unwrap_or_else(|| pack.id.clone()),
                lang: w.lang.clone().unwrap_or_else(|| "en".to_string()),
            };
            if is_held_out(&cw.word, &cw.lang) {
                held.push(cw);
            } else {
                train.push(cw);
            }
        }
    }
    let stack = stack.expect("gate: no corpus packs loaded");
    if held.is_empty() {
        panic!("gate: frozen split produced an empty held-out set (seed {GATE_SEED:#x})");
    }
    (stack, train, held)
}

/// 1-based rank of `target` in `results` by exact word match.
fn rank_of(results: &[kbcore::Suggestion], target: &str) -> Option<usize> {
    results
        .iter()
        .position(|s| s.word == target)
        .map(|i| i + 1)
}

struct TabStats {
    n: usize,
    n_op: usize,
    w_op: f64,
    top3_on: f64,
    top3_off: f64,
    taps: f64,
    chars: f64,
}

impl TabStats {
    fn new() -> Self {
        Self {
            n: 0,
            n_op: 0,
            w_op: 0.0,
            top3_on: 0.0,
            top3_off: 0.0,
            taps: 0.0,
            chars: 0.0,
        }
    }
}

#[test]
fn gate_thresholds() {
    // T9 parity anchors (Python port checked by scripts/gate_parity.py).
    assert_eq!(
        encode_word("hello"),
        "43556",
        "gate: T9 parity broken for hello"
    );
    assert_eq!(
        encode_word("कमल"),
        "267",
        "gate: T9 parity broken for कमल"
    );

    let now = frozen_now();
    let weights = RankWeights::default();
    let _ = &weights;
    let (base, _train, held) = load_corpus();

    // Per-tab accumulators: "words" (EN), "NE", "js", "medical".
    let mut tabs: std::collections::HashMap<String, TabStats> =
        std::collections::HashMap::new();
    // Repeat-word subset for the learning arm: held-out words the frozen
    // base ranks >3 (or misses) at the operating point.
    let mut repeat_idx = Vec::new();

    for (i, w) in held.iter().enumerate() {
        let tab = tabs.entry(w.cat.clone()).or_insert_with(TabStats::new);
        tab.n += 1;
        let ch = w.word.chars().count().max(1) as f64;

        // KSPW arm (neighbor ON): type until in top-3, +1 select tap.
        let mut taps = w.seq.len() as f64 + 1.0; // fallback: full + select
        for k in 1..=w.seq.len() {
            let prefix: String = w.seq.chars().take(k).collect();
            let res = base.suggest_at("", &prefix, &w.cat, LIMIT, now);
            if rank_of(&res, &w.word).map(|r| r <= 3).unwrap_or(false) {
                taps = k as f64 + 1.0;
                break;
            }
        }
        tab.taps += w.freq * taps;
        tab.chars += w.freq * ch;

        // Operating-point arm (len >= 4 only).
        if w.seq.chars().count() >= OP_K {
            let prefix: String = w.seq.chars().take(OP_K).collect();
            let on = base.suggest_at("", &prefix, &w.cat, LIMIT, now);
            let off = base.suggest_no_neighbor_at("", &prefix, &w.cat, LIMIT, now);
            let hit_on = rank_of(&on, &w.word).map(|r| r <= 3).unwrap_or(false);
            let hit_off = rank_of(&off, &w.word).map(|r| r <= 3).unwrap_or(false);
            tab.n_op += 1;
            tab.w_op += w.freq;
            if hit_on {
                tab.top3_on += w.freq;
            }
            if hit_off {
                tab.top3_off += w.freq;
            }
            if !hit_on {
                repeat_idx.push(i);
            }
        }
    }

    // Learning arm: paired A/B per repeat word (fresh clone each time so
    // words never cross-talk through a shared personal dict).
    let mut lift_base = 0.0;
    let mut lift_pers = 0.0;
    let mut lift_w = 0.0;
    for &i in &repeat_idx {
        let w = &held[i];
        let prefix: String = w.seq.chars().take(OP_K).collect();
        let base_hit = rank_of(
            &base.suggest_at(LEARN_CTX, &prefix, &w.cat, LIMIT, now),
            &w.word,
        )
        .map(|r| r <= 3)
        .unwrap_or(false);
        let mut personal = base.clone();
        personal.personal.learn(&w.word, &w.cat);
        personal.personal.learn(&w.word, &w.cat);
        personal.personal.record_bigram(LEARN_CTX, &w.word);
        let pers_hit = rank_of(
            &personal.suggest_at(LEARN_CTX, &prefix, &w.cat, LIMIT, now),
            &w.word,
        )
        .map(|r| r <= 3)
        .unwrap_or(false);
        lift_w += w.freq;
        if base_hit {
            lift_base += w.freq;
        }
        if pers_hit {
            lift_pers += w.freq;
        }
    }
    if repeat_idx.is_empty() {
        panic!("gate: repeat-word subset is empty (base hits everything); lift undefined");
    }

    // Report.
    println!("== 00-gates report (seed {GATE_SEED:#x}, op {OP_K} digits, limit {LIMIT}) ==");
    println!("held-out: {} words (train excluded from personal)", held.len());
    println!(
        "{:<9} {:>6} {:>6} {:>10} {:>10} {:>8}",
        "tab", "n", "n@4", "top3@4 ON", "top3@4 OFF", "KSPW"
    );
    let mut order: Vec<&String> = tabs.keys().collect();
    order.sort();
    // EN+NE primary scope (threshold scope).
    let mut scope = TabStats::new();
    for t in &order {
        let s = &tabs[*t];
        let top_on = if s.w_op > 0.0 { s.top3_on / s.w_op } else { 0.0 };
        let top_off = if s.w_op > 0.0 { s.top3_off / s.w_op } else { 0.0 };
        let kspw = if s.chars > 0.0 { s.taps / s.chars } else { 0.0 };
        println!(
            "{:<9} {:>6} {:>6} {:>10.2} {:>10.2} {:>8.3}",
            t, s.n, s.n_op, top_on, top_off, kspw
        );
        if t.as_str() == "words" || t.as_str() == "NE" {
            scope.n += s.n;
            scope.n_op += s.n_op;
            scope.w_op += s.w_op;
            scope.top3_on += s.top3_on;
            scope.top3_off += s.top3_off;
            scope.taps += s.taps;
            scope.chars += s.chars;
        }
    }
    let scope_top_on = scope.top3_on / scope.w_op;
    let scope_top_off = scope.top3_off / scope.w_op;
    let scope_kspw = scope.taps / scope.chars;
    let lift_pts = 100.0 * (lift_pers - lift_base) / lift_w;
    println!(
        "EN+NE scope: n={} n@4={} top3@4 ON={:.2} OFF={:.2} KSPW={:.3}",
        scope.n, scope.n_op, scope_top_on, scope_top_off, scope_kspw
    );
    println!(
        "learning: repeat-subset n={} base={:.2} +personal={:.2} lift={:.1}pts",
        repeat_idx.len(),
        lift_base / lift_w,
        lift_pers / lift_w,
        lift_pts
    );

    let kspw_pass = scope_kspw <= 1.5;
    let top3_pass = scope_top_on >= 0.85;
    let lift_pass = lift_pts >= 10.0;
    println!(
        "KSPW<=1.5:        {:.3}  {}",
        scope_kspw,
        if kspw_pass { "PASS" } else { "FAIL" }
    );
    println!(
        "top-3@4>=85% ON:  {:.2}  {}",
        scope_top_on,
        if top3_pass { "PASS" } else { "FAIL" }
    );
    println!(
        "lift>=10pts:      {lift_pts:.1}  {}",
        if lift_pass { "PASS" } else { "FAIL" }
    );

    assert!(
        kspw_pass,
        "gate NO-GO: KSPW {scope_kspw:.3} > 1.5 (ranking/prefix broken?)"
    );
    assert!(
        top3_pass,
        "gate NO-GO: top-3@4 {scope_top_on:.2} < 85% neighbor ON"
    );
    assert!(
        lift_pass,
        "gate NO-GO: learning lift {lift_pts:.1}pts < 10pts on repeat subset"
    );
}
