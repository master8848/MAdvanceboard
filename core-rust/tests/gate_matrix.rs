//! Phase-0 gate full accuracy matrix (`plan/00-gates.md` row #1).
//!
//! `tests/gate.rs` owns the frozen GO/NO-GO thresholds (top-3@4, KSPW, lift)
//! and must stay untouched by tuning. This file is the REPORT-ONLY
//! complement it was missing: top-1/top-3 at k=3, k=4, and full length, for
//! neighbor ON / OFF / shipped-policy arms, per tab (words/NE/js/medical +
//! EN+NE scope), plus KSPW per arm. No thresholds here — failing numbers
//! fail in `gate.rs`, never here.
//!
//! Frozen inputs mirror `gate.rs` exactly (same seed, split, `FROZEN_NOW`,
//! limit 30, corpus packs + priorities, `ctx=""`), so every cell is directly
//! comparable to the gate report.
//!
//! Run with `cargo test -p kbcore --test gate_matrix -- --nocapture`.

use kbcore::mapping::encode_word;
use kbcore::pack::load_pack_str;
use kbcore::personal::quantize_ts;
use kbcore::DictionaryStack;

/// Frozen split seed (identical to `tests/gate.rs`; changing this
/// invalidates every number).
const GATE_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
/// Frozen wall-clock (quantized to its 1h bucket for scoring).
const FROZEN_NOW_RAW: i64 = 1_726_000_000;
/// Suggest limit (expand-all sheet size).
const LIMIT: usize = 30;

fn frozen_now() -> i64 {
    quantize_ts(FROZEN_NOW_RAW).expect("frozen matrix timestamp must quantize")
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
}

fn load_held() -> (DictionaryStack, Vec<CorpusWord>) {
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
            load_pack_str(src).unwrap_or_else(|e| panic!("gate_matrix: pack parse failed: {e}"));
        if pack.words.is_empty() {
            panic!("gate_matrix: pack '{}' has zero words", pack.id);
        }
        let entries = pack.to_entries(*prio);
        if entries.is_empty() {
            panic!(
                "gate_matrix: pack '{}' yields zero encodable entries",
                pack.id
            );
        }
        match &mut stack {
            None => stack = Some(DictionaryStack::new(entries)),
            Some(s) => s.add_entries(entries),
        }
        for w in &pack.words {
            // Same seq precedence as the loader and `gate.rs`: seq >
            // encode(tr) > encode(w).
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
                });
            }
        }
    }
    if held.is_empty() {
        panic!("gate_matrix: frozen split produced an empty held-out set");
    }
    (stack.expect("gate_matrix: no corpus packs loaded"), held)
}

fn rank_of(results: &[kbcore::Suggestion], target: &str) -> Option<usize> {
    results
        .iter()
        .position(|s| s.word == target)
        .map(|i| i + 1)
}

#[test]
fn gate_accuracy_matrix() {
    let now = frozen_now();
    let (base, held) = load_held();

    let arms = ["ON", "OFF", "POLICY"];
    // NOTE: the POLICY arm calls `suggest_policy_at`, which resolves the
    // per-tab policy internally (neighbor-OFF + hard filter on every tab
    // today, so POLICY == OFF unless a tab opts back in — none has).

    // (label, k): k=0 means full-length seq.
    let ks: &[(&str, usize)] = &[("@3", 3), ("@4", 4), ("@full", 0)];
    let tabs: &[&str] = &["words", "NE", "js", "medical"];

    println!("== gate accuracy matrix (seed {GATE_SEED:#x}, limit {LIMIT}, held n={}) ==", held.len());
    println!(
        "{:<8} {:<6} {:<6} {:>9} {:>9} {:>9} {:>9} {:>7}",
        "tab", "k", "arm", "top1", "top3", "n", "w", "KSPW"
    );
    for tab in tabs {
        // KSPW per (tab, arm), computed once: type until in top-3 under the
        // arm's own opts, +1 select tap. Independent of the @k cut, so it
        // must not sit inside the k-loop (it dominated runtime).
        let mut kspw: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
        for arm_name in arms {
            let mut taps = 0.0;
            let mut chars = 0.0;
            for w in &held {
                if w.cat != *tab {
                    continue;
                }
                let seqlen = w.seq.chars().count();
                let ch = w.word.chars().count().max(1) as f64;
                let mut t = seqlen as f64 + 1.0;
                for c in 1..=seqlen {
                    let p: String = w.seq.chars().take(c).collect();
                    let r = match arm_name {
                        "ON" => base.suggest_at("", &p, &w.cat, LIMIT, now),
                        "OFF" => base.suggest_no_neighbor_at("", &p, &w.cat, LIMIT, now),
                        _ => base.suggest_policy_at("", &p, &w.cat, LIMIT, now),
                    };
                    if rank_of(&r, &w.word).map(|rr| rr <= 3).unwrap_or(false) {
                        t = c as f64 + 1.0;
                        break;
                    }
                }
                taps += w.freq * t;
                chars += w.freq * ch;
            }
            kspw.insert(arm_name, taps / chars);
        }
        for (klabel, k) in ks {
            for arm_name in arms {
                let mut top1 = 0.0;
                let mut top3 = 0.0;
                let mut tot = 0.0;
                let mut n = 0usize;
                for w in &held {
                    if w.cat != *tab {
                        continue;
                    }
                    let seqlen = w.seq.chars().count();
                    // @k arms keep gate.rs comparability (len >= k only);
                    // @full takes every word at its full seq.
                    let kk = if *k == 0 { seqlen } else { *k };
                    if seqlen < kk {
                        continue;
                    }
                    let prefix: String = w.seq.chars().take(kk).collect();
                    let res = match arm_name {
                        "ON" => base.suggest_at("", &prefix, &w.cat, LIMIT, now),
                        "OFF" => base.suggest_no_neighbor_at("", &prefix, &w.cat, LIMIT, now),
                        _ => base.suggest_policy_at("", &prefix, &w.cat, LIMIT, now),
                    };
                    let rank = rank_of(&res, &w.word);
                    tot += w.freq;
                    n += 1;
                    if rank.map(|r| r == 1).unwrap_or(false) {
                        top1 += w.freq;
                    }
                    if rank.map(|r| r <= 3).unwrap_or(false) {
                        top3 += w.freq;
                    }
                }
                if n == 0 {
                    continue;
                }
                println!(
                    "{:<8} {:<6} {:<6} {:>9.3} {:>9.3} {:>9} {:>9.0} {:>7.3}",
                    tab,
                    klabel,
                    arm_name,
                    top1 / tot,
                    top3 / tot,
                    n,
                    tot,
                    kspw[arm_name]
                );
            }
        }
    }
    // EN+NE scope roll-up at the operating point (gate comparability).
    for klabel in ["@4"] {
        for arm_name in arms {
            let mut top1 = 0.0;
            let mut top3 = 0.0;
            let mut tot = 0.0;
            for w in &held {
                if w.cat != "words" && w.cat != "NE" {
                    continue;
                }
                if w.seq.chars().count() < 4 {
                    continue;
                }
                let prefix: String = w.seq.chars().take(4).collect();
                let res = match arm_name {
                    "ON" => base.suggest_at("", &prefix, &w.cat, LIMIT, now),
                    "OFF" => base.suggest_no_neighbor_at("", &prefix, &w.cat, LIMIT, now),
                    _ => base.suggest_policy_at("", &prefix, &w.cat, LIMIT, now),
                };
                let rank = rank_of(&res, &w.word);
                tot += w.freq;
                if rank.map(|r| r == 1).unwrap_or(false) {
                    top1 += w.freq;
                }
                if rank.map(|r| r <= 3).unwrap_or(false) {
                    top3 += w.freq;
                }
            }
            println!(
                "{:<8} {:<6} {:<6} {:>9.3} {:>9.3} {:>9} {:>9.0} {:>7}",
                "EN+NE", klabel, arm_name, top1 / tot, top3 / tot, "", tot, "-"
            );
        }
    }
}
