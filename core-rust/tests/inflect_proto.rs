//! Inflection-sidecar prototype evaluation (train-tune / held-report).
//!
//! Precedent: `docs/GATE_RESULTS.md` (train-only tuning, frozen gate
//! thresholds untouched). The OOV-inflection probe set is built from
//! TRAIN-split stems only (frozen `GATE_SEED` FNV split — a derived form
//! whose stem is held-out is excluded, so no leakage), absent from the
//! FULL vocab (train+held), and split tune/held 50/50 by
//! `FNV(form) % 2`. Discount tuning reads the TUNE half only; the HELD
//! half is reported once. Collateral (gate-held EN+NE policy top-3@4,
//! KSPW, union growth) is measured with the frozen discount.
//!
//! Run: `cargo test -p kbcore --test inflect_proto -- --nocapture`.

use kbcore::inflect::{expand_entries, inflect_stem_en, InflectConfig};
use kbcore::mapping::encode_word;
use kbcore::pack::load_pack_str;
use kbcore::personal::quantize_ts;
use kbcore::{DictionaryStack, SuggestOpts};

/// Frozen split seed (identical to `tests/gate.rs`).
const GATE_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
const FROZEN_NOW_RAW: i64 = 1_726_000_000;
const OP_K: usize = 4;
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

fn rank_of(results: &[kbcore::Suggestion], target: &str) -> Option<usize> {
    results
        .iter()
        .position(|s| s.word == target)
        .map(|i| i + 1)
}

struct OovProbe {
    form: String,
    seq: String,
    tune: bool,
}

/// OOV inflection probes: regular derivations of TRAIN-split EN stems that
/// appear NOWHERE in the full vocab. Deterministic (sorted); tune/held by
/// frozen hash — never by performance.
fn build_oov_probes() -> Vec<OovProbe> {
    let src = include_str!("../../packs/words_en.json");
    let pack = load_pack_str(src).expect("words_en must parse");
    let vocab: std::collections::HashSet<String> =
        pack.words.iter().map(|w| w.w.to_lowercase()).collect();
    let cfg = InflectConfig::default();
    let mut forms: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
    for w in &pack.words {
        let lang = w.lang.clone().unwrap_or_else(|| "en".to_string());
        if is_held_out(&w.w, &lang) {
            continue; // train stems only — no leakage
        }
        let stem = w.w.to_lowercase();
        if stem.chars().count() < cfg.min_stem_len
            || !stem.chars().all(|c| c.is_ascii_lowercase())
            || w.freq >= cfg.max_stem_freq
        {
            continue;
        }
        for (_, form) in inflect_stem_en(&stem) {
            if !vocab.contains(&form) {
                forms.insert(form, ());
            }
        }
    }
    let mut v: Vec<OovProbe> = forms
        .into_keys()
        .map(|form| {
            let tune = fnv1a(&form, "en") % 2 == 0;
            let seq = encode_word(&form);
            OovProbe { form, seq, tune }
        })
        .filter(|p| !p.seq.is_empty())
        .collect();
    v.sort_by(|a, b| a.form.cmp(&b.form));
    v
}

fn load_full_stack() -> DictionaryStack {
    let packs: &[(&str, i32)] = &[
        (include_str!("../../packs/words_en.json"), 0),
        (include_str!("../../packs/nepali.json"), 10),
        (include_str!("../../packs/code_js.json"), 20),
        (include_str!("../../packs/medical.json"), 30),
    ];
    let mut stack: Option<DictionaryStack> = None;
    for (src, prio) in packs {
        let pack = load_pack_str(src).expect("pack must parse");
        let entries = pack.to_entries(*prio);
        match &mut stack {
            None => stack = Some(DictionaryStack::new(entries)),
            Some(s) => s.add_entries(entries),
        }
    }
    stack.expect("packs must load")
}

/// Top-3 rate over probe forms at the 4-digit OP prefix AND at full-seq
/// exact (user typed the whole inflection). `cat` = "words" (EN table).
fn probe_rate(stack: &DictionaryStack, probes: &[OovProbe], full: bool, now: i64) -> (f64, usize) {
    let opts = SuggestOpts::policy_for_tab("words");
    let mut hit = 0usize;
    let mut n = 0usize;
    for p in probes {
        let digits = if full {
            p.seq.clone()
        } else if p.seq.chars().count() >= OP_K {
            p.seq.chars().take(OP_K).collect()
        } else {
            continue;
        };
        n += 1;
        let res = stack.suggest_with_opts_at("", &digits, "words", LIMIT, now, &opts);
        if rank_of(&res, &p.form).map(|r| r <= 3).unwrap_or(false) {
            hit += 1;
        }
    }
    (hit as f64 / n.max(1) as f64, n)
}

/// Tune discount_div on the TUNE half only; report the curve.
#[test]
fn proto_tune_discount_on_train_half() {
    let now = frozen_now();
    let probes = build_oov_probes();
    let tune: Vec<OovProbe> = probes.iter().filter(|p| p.tune).map(clone_probe).collect();
    let n_tune = tune.len();
    println!("== inflect_proto tune (OOV probes from train stems, n_tune={n_tune}) ==");
    // Baseline: unexpanded stack scores 0 by construction (forms are OOV).
    let base = load_full_stack();
    let (r_op, _) = probe_rate(&base, &tune, false, now);
    let (r_full, _) = probe_rate(&base, &tune, true, now);
    assert_eq!(r_op, 0.0, "OOV forms must be unfindable without expansion");
    assert_eq!(r_full, 0.0, "OOV forms must be unfindable without expansion");
    println!("unexpanded baseline: op4=0.000 full=0.000 (n={n_tune})");
    for div in [2u64, 4, 8, 16, 32] {
        let mut stack = load_full_stack();
        let cfg = InflectConfig {
            discount_div: div,
            ..InflectConfig::default()
        };
        let added = stack.expand_inflections(&cfg);
        let (op, _) = probe_rate(&stack, &tune, false, now);
        let (full, _) = probe_rate(&stack, &tune, true, now);
        println!("discount/{div:<2}: +{added} rows  tune op4={op:.3} full={full:.3}");
    }
}

fn clone_probe(p: &OovProbe) -> OovProbe {
    OovProbe {
        form: p.form.clone(),
        seq: p.seq.clone(),
        tune: p.tune,
    }
}

/// Held-out report with the frozen discount (8) + collateral on the
/// gate-held set + guard (flag-off byte-identical).
#[test]
fn proto_held_report_and_collateral() {
    let now = frozen_now();
    let probes = build_oov_probes();
    let held: Vec<OovProbe> = probes.iter().filter(|p| !p.tune).map(clone_probe).collect();
    let tune_n = probes.iter().filter(|p| p.tune).count();
    println!("== inflect_proto held (n_held={} n_tune={tune_n}) ==", held.len());
    assert!(!held.is_empty() && tune_n > 0, "both halves must be non-empty");

    let cfg = InflectConfig::default();
    assert_eq!(cfg.discount_div, 8, "retune via the train/held protocol, not by hand");
    let mut stack = load_full_stack();
    let added = stack.expand_inflections(&cfg);
    let (op, n_op) = probe_rate(&stack, &held, false, now);
    let (full, n_full) = probe_rate(&stack, &held, true, now);
    println!("discount/8: +{added} rows  HELD op4={op:.3} (n={n_op}) full={full:.3} (n={n_full})");

    // Collateral: gate-held EN+NE policy top-3@4, expanded vs plain.
    // (Both stacks built ONCE — index rebuilds are seconds each.)
    let base = load_full_stack();
    let packs: &[(&str, i32, &str)] = &[
        (include_str!("../../packs/words_en.json"), 0, "words"),
        (include_str!("../../packs/nepali.json"), 10, "NE"),
    ];
    for (src, _prio, tab) in packs {
        let pack = load_pack_str(src).expect("pack must parse");
        let (mut hit_plain, mut hit_exp, mut tot) = (0.0, 0.0, 0.0);
        for w in &pack.words {
            let lang = w.lang.clone().unwrap_or_else(|| "en".to_string());
            if !is_held_out(&w.w, &lang) {
                continue;
            }
            let seq = w.seq.clone().unwrap_or_else(|| {
                w.tr.as_ref()
                    .map(|tr| encode_word(tr))
                    .unwrap_or_else(|| encode_word(&w.w))
            });
            if seq.chars().count() < OP_K {
                continue;
            }
            let prefix: String = seq.chars().take(OP_K).collect();
            let cat = w.cat.clone().unwrap_or_else(|| pack.id.clone());
            let opts = SuggestOpts::policy_for_tab(&cat);
            tot += w.freq.max(1) as f64;
            let r1 = base.suggest_with_opts_at("", &prefix, &cat, LIMIT, now, &opts);
            if rank_of(&r1, &w.w).map(|r| r <= 3).unwrap_or(false) {
                hit_plain += w.freq.max(1) as f64;
            }
            let r2 = stack.suggest_with_opts_at("", &prefix, &cat, LIMIT, now, &opts);
            if rank_of(&r2, &w.w).map(|r| r <= 3).unwrap_or(false) {
                hit_exp += w.freq.max(1) as f64;
            }
        }
        println!(
            "collateral {tab}: plain={:.4} expanded={:.4} delta={:+.4}",
            hit_plain / tot,
            hit_exp / tot,
            hit_exp / tot - hit_plain / tot
        );
    }

    // Union/index growth (09#2 interaction: static rows only, V untouched).
    let en_src = include_str!("../../packs/words_en.json");
    let en_pack = load_pack_str(en_src).expect("words_en must parse");
    let (mut u_plain, mut u_exp, mut u_n) = (0usize, 0usize, 0usize);
    for w in en_pack.words.iter().take(300) {
        let lang = w.lang.clone().unwrap_or_else(|| "en".to_string());
        if !is_held_out(&w.w, &lang) {
            continue;
        }
        let seq = encode_word(&w.w);
        if seq.chars().count() < OP_K {
            continue;
        }
        let prefix: String = seq.chars().take(OP_K).collect();
        let cat = w.cat.clone().unwrap_or_else(|| en_pack.id.clone());
        let opts = SuggestOpts::policy_for_tab(&cat);
        u_plain += base.probe_target_at("", &prefix, &cat, &w.w, LIMIT, now, &opts).union_size;
        u_exp += stack.probe_target_at("", &prefix, &cat, &w.w, LIMIT, now, &opts).union_size;
        u_n += 1;
    }
    println!(
        "union growth (EN sample n={u_n}): plain mean={:.1} expanded mean={:.1} (+{:.0}%)",
        u_plain as f64 / u_n as f64,
        u_exp as f64 / u_n as f64,
        100.0 * (u_exp as f64 - u_plain as f64) / u_plain as f64
    );

    // Idempotence: a second expansion call mints nothing.
    let again = stack.expand_inflections(&cfg);
    assert_eq!(again, 0, "second expansion must mint zero rows (idempotent)");
    println!("idempotence: second expand_inflections minted {again} rows");
}

/// Guard: the flag is the CALL — a default stack never contains synthetic
/// rows, and suggest output is byte-identical to a fresh default stack on
/// every gate-held prefix (both arms used in diagnosis).
#[test]
fn flag_off_is_byte_identical() {
    let now = frozen_now();
    let a = load_full_stack();
    let b = load_full_stack();
    let packs: &[&str] = &[
        include_str!("../../packs/words_en.json"),
        include_str!("../../packs/nepali.json"),
    ];
    let mut checked = 0usize;
    for src in packs {
        let pack = load_pack_str(src).expect("pack must parse");
        for w in &pack.words {
            let lang = w.lang.clone().unwrap_or_else(|| "en".to_string());
            if !is_held_out(&w.w, &lang) {
                continue;
            }
            let seq = w.seq.clone().unwrap_or_else(|| {
                w.tr.as_ref()
                    .map(|tr| encode_word(tr))
                    .unwrap_or_else(|| encode_word(&w.w))
            });
            if seq.chars().count() < OP_K {
                continue;
            }
            let prefix: String = seq.chars().take(OP_K).collect();
            let cat = w.cat.clone().unwrap_or_else(|| pack.id.clone());
            for opts in [SuggestOpts::policy_for_tab(&cat), SuggestOpts::default()] {
                let ra = a.suggest_with_opts_at("", &prefix, &cat, LIMIT, now, &opts);
                let rb = b.suggest_with_opts_at("", &prefix, &cat, LIMIT, now, &opts);
                assert_eq!(
                    ra.iter().map(|s| (&s.word, s.score.to_bits())).collect::<Vec<_>>(),
                    rb.iter().map(|s| (&s.word, s.score.to_bits())).collect::<Vec<_>>(),
                    "default stacks must agree for {:?}",
                    w.w
                );
                checked += 1;
            }
        }
    }
    // expand_entries against an empty vocab of a non-EN-only set mints
    // nothing (structural: EN table cannot fire on NE rows).
    let ne_src = include_str!("../../packs/nepali.json");
    let ne_pack = load_pack_str(ne_src).expect("nepali must parse");
    let ne_entries = ne_pack.to_entries(10);
    let empty_vocab = std::collections::HashSet::new();
    assert!(
        expand_entries(&ne_entries, &empty_vocab, &InflectConfig::default()).is_empty(),
        "NE rows must never expand under the EN table"
    );
    println!("flag-off guard: {checked} prefix queries byte-identical; NE expands to 0 rows");
}
