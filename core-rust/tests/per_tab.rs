//! Per-tab production-vocab report (`packs/*.json`, all 9 categories).
//!
//! Loads every pack through [`DictionaryStack`] (a missing corpus is a
//! compile-time error via `include_str!`; an empty or under-minimum pack
//! is an explicit test failure — never a silent near-empty category) and
//! reports freq-weighted top-3@4-digit-prefix, neighbor ON vs OFF, per
//! tab, plus load-and-suggest spot checks for every category, the
//! numbers digit-commit rule (SPEC §1), and the ★personal overlay
//! promotion path (SPEC §4).
//!
//! This is a REPORT, not a gate: no 85% thresholds here (the frozen
//! `gate.rs` owns GO/NO-GO; dense tabs like NE/js are known crowded).
//! Run with `cargo test -p kbcore --test per_tab -- --nocapture`.

use kbcore::mapping::encode_word;
use kbcore::pack::load_pack_str;
use kbcore::personal::quantize_ts;
use kbcore::DictionaryStack;

/// Deterministic clock (same frozen bucket as `gate.rs`).
const NOW_RAW: i64 = 1_726_000_000;
/// Operating point: 4 digits. Suggest limit (expand-all sheet size).
const OP_K: usize = 4;
const LIMIT: usize = 30;

/// Production minimum per pack id (mirrors `MIN_WORDS` in
/// `scripts/build_pack.py`; duplicated here so Rust-side loading fails
/// loudly too if a near-empty pack ever ships).
const MIN_WORDS: &[(&str, usize)] = &[
    ("words", 5000),
    ("ne", 5000),
    ("numbers", 10),
    ("js", 200),
    ("rust", 200),
    ("html", 200),
    ("emoji", 150),
    ("math", 100),
    ("medical", 500),
];

/// (pack file, stack priority). Base 0, NE 10, extension 20-60.
const CORPUS: &[(&str, i32)] = &[
    (include_str!("../../packs/words_en.json"), 0),
    (include_str!("../../packs/nepali.json"), 10),
    (include_str!("../../packs/numbers.json"), 15),
    (include_str!("../../packs/code_js.json"), 20),
    (include_str!("../../packs/code_rust.json"), 25),
    (include_str!("../../packs/code_html.json"), 30),
    (include_str!("../../packs/emoji.json"), 40),
    (include_str!("../../packs/math.json"), 50),
    (include_str!("../../packs/medical.json"), 60),
];

fn rank_of(results: &[kbcore::Suggestion], target: &str) -> Option<usize> {
    results
        .iter()
        .position(|s| s.word == target)
        .map(|i| i + 1)
}

fn load_stack() -> DictionaryStack {
    let mut stack: Option<DictionaryStack> = None;
    for (src, prio) in CORPUS {
        let pack =
            load_pack_str(src).unwrap_or_else(|e| panic!("per_tab: pack failed to parse: {e}"));
        assert!(
            !pack.words.is_empty(),
            "per_tab: pack '{}' has zero words",
            pack.id
        );
        let min = MIN_WORDS
            .iter()
            .find(|(id, _)| *id == pack.id)
            .unwrap_or_else(|| panic!("per_tab: pack '{}' has no production minimum", pack.id))
            .1;
        assert!(
            pack.words.len() >= min,
            "per_tab: pack '{}' has {} words (< production minimum {}): refusing near-empty pack",
            pack.id,
            pack.words.len(),
            min
        );
        let entries = pack.to_entries(*prio);
        assert!(
            entries.len() == pack.words.len(),
            "per_tab: pack '{}' yields {}/{} encodable entries ({} silently dropped)",
            pack.id,
            entries.len(),
            pack.words.len(),
            pack.words.len() - entries.len()
        );
        match &mut stack {
            None => stack = Some(DictionaryStack::new(entries)),
            Some(s) => s.add_entries(entries),
        }
    }
    stack.expect("per_tab: no corpus packs loaded")
}

#[test]
fn per_tab_report() {
    let now = quantize_ts(NOW_RAW).expect("frozen timestamp must quantize");
    let stack = load_stack();

    // Per-tab freq-weighted top-3@4 (full vocab, own cat as active tab).
    let mut tabs: std::collections::BTreeMap<String, (f64, f64, f64, usize)> =
        std::collections::BTreeMap::new();
    for (src, _) in CORPUS {
        let pack = load_pack_str(src).expect("corpus must parse");
        for w in &pack.words {
            let seq = w.seq.clone().unwrap_or_else(|| {
                w.tr.as_ref()
                    .map(|tr| encode_word(tr))
                    .unwrap_or_else(|| encode_word(&w.w))
            });
            if seq.chars().count() < OP_K {
                continue;
            }
            let tab = w.cat.clone().unwrap_or_else(|| pack.id.clone());
            let prefix: String = seq.chars().take(OP_K).collect();
            let freq = w.freq.max(1) as f64;
            let on = stack.suggest_at("", &prefix, &tab, LIMIT, now);
            let off = stack.suggest_no_neighbor_at("", &prefix, &tab, LIMIT, now);
            let e = tabs.entry(tab).or_insert((0.0, 0.0, 0.0, 0));
            e.3 += 1;
            e.2 += freq;
            if rank_of(&on, &w.w).map(|r| r <= 3).unwrap_or(false) {
                e.0 += freq;
            }
            if rank_of(&off, &w.w).map(|r| r <= 3).unwrap_or(false) {
                e.1 += freq;
            }
        }
    }
    println!("== per-tab report (freq-weighted top-3@4, full vocab) ==");
    println!("{:<10} {:>6} {:>10} {:>10}", "tab", "n@4", "ON", "OFF");
    for (tab, (on, off, wsum, n)) in &tabs {
        println!(
            "{:<10} {:>6} {:>10.3} {:>10.3}",
            tab,
            n,
            on / wsum,
            off / wsum
        );
    }

    // Load-and-suggest spot checks: (word, seq, active tab). Full-seq
    // query must surface the word inside the top-30 sheet.
    let spots = [
        ("hello", encode_word("hello"), "words"),       // 43556
        ("function", encode_word("function"), "js"),    // 386...
        ("fn", encode_word("fn"), "rust"),              // 36
        ("div", encode_word("div"), "html"),            // 348
        ("❤️", "432".to_string(), "emoji"),              // hea
        ("\\alpha", "257".to_string(), "math"),          // alp
        ("fever", encode_word("fever"), "medical"),     // 33837
    ];
    println!("== spot checks (full-seq rank in top-30) ==");
    for (word, seq, tab) in spots {
        let res = stack.suggest_at("", &seq, tab, LIMIT, now);
        let rank = rank_of(&res, word);
        println!("{tab:<10} {word:<10} seq={seq:<8} rank={rank:?}");
        assert!(
            rank.map(|r| r <= LIMIT).unwrap_or(false),
            "per_tab: spot {word:?} (tab {tab}) missing from top-{LIMIT} for seq {seq}"
        );
    }

    // Nepali structural invariants (plan/03 pipeline contract): every row
    // carries `tr`, and no entry is a matra-only (all-`9`) seq.
    let ne = load_pack_str(CORPUS[1].0).expect("ne pack must parse");
    let mut no_tr = 0;
    for w in &ne.words {
        if w.tr.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
            no_tr += 1;
        }
    }
    assert_eq!(no_tr, 0, "per_tab: ne pack has {no_tr} rows without `tr`");
    let ne_entries = ne.to_entries(10);
    let all9 = ne_entries
        .iter()
        .filter(|e| !e.seq.is_empty() && e.seq.chars().all(|c| c == '9'))
        .count();
    assert_eq!(all9, 0, "per_tab: ne pack has {all9} matra-only (all-9) seqs");
    println!("ne invariants: tr present on all {} rows, 0 all-9 seqs", ne.words.len());

    // Numbers (SPEC §1): digits commit, no prediction. Encoder passes
    // digits through verbatim, and the digit suggests itself.
    for d in ["0", "5", "9"] {
        assert_eq!(encode_word(d), d, "per_tab: digit {d} must pass through");
        let res = stack.suggest_at("", d, "numbers", LIMIT, now);
        assert!(
            rank_of(&res, d).is_some(),
            "per_tab: digit {d} must suggest itself"
        );
    }
    println!("numbers: digit-commit passthrough verified (0/5/9)");

    // ★personal overlay (SPEC §4): an OOV word accepted twice promotes
    // into the top-3 on its next sighting (per-tab boost + personal).
    let mut personal = stack.clone();
    let oov = "zxqyphen";
    let oov_seq = encode_word(oov);
    assert!(
        rank_of(&stack.suggest_at("", &oov_seq, "words", LIMIT, now), oov).is_none(),
        "per_tab: OOV probe {oov:?} must start absent"
    );
    personal.personal.learn(oov, "words");
    personal.personal.learn(oov, "words");
    personal.personal.record_bigram("the", oov);
    let rank = rank_of(
        &personal.suggest_at("the", &oov_seq, "words", LIMIT, now),
        oov,
    );
    println!("personal: OOV {oov:?} rank after 2 accepts + bigram: {rank:?}");
    assert!(
        rank.map(|r| r <= 3).unwrap_or(false),
        "per_tab: OOV {oov:?} must promote to top-3 after 2 accepts"
    );
}
