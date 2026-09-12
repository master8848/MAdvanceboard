//! Ranker anti-overfit guard: frozen held-out scope floor for the shipped
//! production policy (neighbor OFF, per-tab scoping).
//!
//! Split/seed/`FROZEN_NOW`/op-point/limit mirror `tests/gate.rs` exactly so
//! this measures the same held-out set — but unlike `gate.rs` it asserts no
//! 85% threshold: it fails iff the EN+NE scope top-3@4 through
//! `suggest_policy_at` regresses below the pre-tuning OFF baseline of 0.469
//! (Tier-0 measured OFF = 0.469; w_cat=1.5 tuning lifted it to ~0.485).
//! Any future scoring change that drops below the floor fails here, even if
//! it looks better on the train split.

use kbcore::mapping::encode_word;
use kbcore::pack::load_pack_str;
use kbcore::personal::quantize_ts;
use kbcore::DictionaryStack;

/// Frozen split seed (identical to `tests/gate.rs`; changing this
/// invalidates every number).
const GATE_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
/// Frozen wall-clock (quantized to its 1h bucket for scoring).
const FROZEN_NOW_RAW: i64 = 1_726_000_000;
/// Operating point: 4 digits.
const OP_K: usize = 4;
/// Suggest limit (expand-all sheet size).
const LIMIT: usize = 30;
/// Pre-tuning OFF baseline floor (scope EN+NE, freq-weighted). Never lower
/// this to make a change pass — retune on the train split instead.
const SCOPE_OFF_FLOOR: f64 = 0.469;

fn frozen_now() -> i64 {
    quantize_ts(FROZEN_NOW_RAW).expect("frozen guard timestamp must quantize")
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

fn load_held_scope() -> (DictionaryStack, Vec<CorpusWord>) {
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
            load_pack_str(src).unwrap_or_else(|e| panic!("guard: pack parse failed: {e}"));
        let entries = pack.to_entries(*prio);
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
                });
            }
        }
    }
    (stack.expect("guard: no corpus packs loaded"), held)
}

#[test]
fn policy_scope_top3_at4_holds_off_baseline() {
    let now = frozen_now();
    let (stack, held) = load_held_scope();
    let (mut hit, mut tot) = (0.0, 0.0);
    for w in &held {
        if w.cat != "words" && w.cat != "NE" {
            continue;
        }
        if w.seq.chars().count() < OP_K {
            continue;
        }
        let prefix: String = w.seq.chars().take(OP_K).collect();
        tot += w.freq;
        let res = stack.suggest_policy_at("", &prefix, &w.cat, LIMIT, now);
        if res.iter().take(3).any(|s| s.word == w.word) {
            hit += w.freq;
        }
    }
    assert!(tot > 0.0, "guard: held-out scope is empty");
    let rate = hit / tot;
    println!("guard: policy scope top-3@4 = {rate:.4} (floor {SCOPE_OFF_FLOOR})");
    assert!(
        rate >= SCOPE_OFF_FLOOR,
        "guard FAIL: policy scope top-3@4 {rate:.4} < floor {SCOPE_OFF_FLOOR} — \
         scoring change regressed held-out rank; retune on the train split"
    );
}
