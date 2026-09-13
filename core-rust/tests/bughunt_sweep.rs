//! Bug-hunt sweep: one hard-assert file across the task scope
//! (core-rust engine, SPEC section 2 ranking, packs shape).
//!
//! Unlike the report-only harnesses (`gate_matrix`, `per_tab`), every test
//! here asserts. Unlike the frozen gates (`gate`, `ranker_guard`),
//! thresholds are structural (ordering / isolation / byte-identity), never
//! accuracy floors — so this file stays green across tuning.
//!
//! Sections mirror the sweep brief:
//! 1. determinism (personal-hour + snippet-hour, total order, HashMap ties,
//!    fixed-count fuzz)  2. suggest arms + per-tab + hard filter + KSPW /
//!    top1-top3 @3/@4/full  3. layout resolve  4. tr/alt/matra/Roman
//! 5. single-press + QWERTY  6. learn/reject/personal + gates + snippet
//! 7. persistence (single handle, WAL, 2s flush, pure-RAM hot path).

use kbcore::gates::{
    may_capture_clipboard, mic_visible, snippet_now, temporal_now, GateInput, learn_now,
};
use kbcore::mapping::{char_to_digit, encode_word};
use kbcore::pack::load_pack_str;
use kbcore::personal::{quantize_ts, PersonalDict};
use kbcore::snippet::{SnippetBuffer, MAX_ENTRIES, SNIPPET_TTL_MS};
use kbcore::store::{Store, FLUSH_COALESCE};
use kbcore::{DictionaryStack, KeyMapping, LayoutRegistry, Predictor, SuggestOpts, Suggestion};

/// Frozen scoring instant (already a 1h bucket, mirrors determinism.rs).
const NOW: i64 = 1_726_000_000 - (1_726_000_000 % 3600);
const LIMIT: usize = 30;

fn canonical(sugs: &[Suggestion]) -> String {
    sugs.iter()
        .map(|s| {
            format!(
                "{}|{:016x}|{}|{}|{}",
                s.word,
                s.score.to_bits(),
                s.seq,
                s.cat,
                s.layout_id
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fixture_base() -> DictionaryStack {
    DictionaryStack::new(
        DictionaryStack::load_base_json(
            r#"[{"w":"hello","freq":900,"cat":"EN","lang":"en"},
                {"w":"hell","freq":100,"cat":"EN","lang":"en"},
                {"w":"help","freq":200,"cat":"EN","lang":"en"},
                {"w":"fun","freq":150,"cat":"EN","lang":"en"},
                {"w":"run","freq":150,"cat":"EN","lang":"en"},
                {"w":"fever","freq":400,"cat":"medical","lang":"en"},
                {"w":"hellx","freq":50,"cat":"js","lang":"en"},
                {"w":"कमल","freq":300,"cat":"NE","lang":"ne"}]"#,
        )
        .expect("fixture must parse"),
    )
}

fn rank_of(results: &[Suggestion], target: &str) -> Option<usize> {
    results.iter().position(|s| s.word == target).map(|i| i + 1)
}

// ---- 1. determinism -------------------------------------------------------

#[test]
fn sweep_personal_hour_plus_snippet_hour_byte_identical() {
    // Same (dict, ctx, digits, tab, layout, personal-hour, snippet-hour)
    // => byte-identical suggest output AND snippet peek.
    let build = || {
        let mut stack = fixture_base();
        stack.personal.learn_at("hell", "EN", NOW);
        stack.personal.learn_at("hell", "EN", NOW);
        stack.personal.learn_at("help", "EN", NOW);
        stack.personal.record_bigram("the", "hello");
        let mut snip = SnippetBuffer::new();
        snip.offer("if", &["if (".to_string(), "{".to_string()], 0).unwrap();
        (stack, snip)
    };
    let reg = LayoutRegistry::with_builtins();
    let mapping = reg.get_or_default("t9-9").clone();
    let (a_stack, mut a_snip) = build();
    let (b_stack, mut b_snip) = build();
    for (ctx, digits, tab) in [("", "43556", "EN"), ("the", "43556", "EN"), ("", "267", "NE")] {
        let a = a_stack.suggest_for_layout_at(ctx, digits, &mapping, tab, 5, NOW);
        let b = b_stack.suggest_for_layout_at(ctx, digits, &mapping, tab, 5, NOW);
        assert_eq!(canonical(&a), canonical(&b), "suggest diverged at {ctx:?} {digits:?} {tab}");
        for s in &a {
            assert!(!s.score.is_nan(), "NaN score for {:?}", s.word);
        }
    }
    assert_eq!(a_snip.peek(0), b_snip.peek(0), "snippet peek diverged");
    assert_eq!(a_snip.peek(SNIPPET_TTL_MS - 1), b_snip.peek(SNIPPET_TTL_MS - 1));
    assert!(a_snip.peek(3 * 60 * 1000).is_empty());
}

#[test]
fn sweep_total_order_lexicographic_and_repeat_stable() {
    // 3^4 = 81 equal-freq words over {a,b,c}, all seq "2222": scores tie,
    // so the total order (shorter, lexicographic, priority, lang) must emit
    // lexicographic order, repeat-stable across fresh instances.
    let mut words = Vec::new();
    for a in ['a', 'b', 'c'] {
        for b in ['a', 'b', 'c'] {
            for c in ['a', 'b', 'c'] {
                for d in ['a', 'b', 'c'] {
                    words.push(format!("{a}{b}{c}{d}"));
                }
            }
        }
    }
    assert_eq!(words.len(), 81);
    let json = format!(
        "[{}]",
        words.iter().map(|w| format!(r#"{{"w":"{w}","freq":100,"cat":"EN"}}"#)).collect::<Vec<_>>().join(",")
    );
    let first = DictionaryStack::new(DictionaryStack::load_base_json(&json).unwrap());
    let out = first.suggest_at("", "2222", "EN", 81, NOW);
    assert_eq!(out.len(), 81);
    let mut lex = words.clone();
    lex.sort();
    let got: Vec<&str> = out.iter().map(|s| s.word.as_str()).collect();
    assert_eq!(got, lex, "ties must resolve lexicographically");
    // Cross-instance (fresh HashMap RandomState) byte-identity.
    let second = DictionaryStack::new(DictionaryStack::load_base_json(&json).unwrap());
    let out2 = second.suggest_at("", "2222", "EN", 81, NOW);
    assert_eq!(canonical(&out), canonical(&out2), "HashMap tie-order diverged across instances");
    // Truncate keeps the lexicographically-first 30.
    let top30 = first.suggest_at("", "2222", "EN", 30, NOW);
    assert_eq!(top30.len(), 30);
    assert_eq!(top30[0].word, lex[0]);
    assert_eq!(top30[29].word, lex[29]);
}

#[test]
fn sweep_fixed_count_fuzz_byte_identical() {
    // Fixed-count (256) seeded fuzz: every query twice => byte-identical,
    // no NaN. Fixed seed + fixed count => same work on every machine.
    let stack = fixture_base();
    let reg = LayoutRegistry::with_builtins();
    let layouts = ["t9-9", "t9-12", "t9-16"];
    let codes: Vec<Vec<char>> = layouts
        .iter()
        .map(|id| reg.get_or_default(id).keys.iter().map(|k| k.code).collect())
        .collect();
    let ctxs = ["", "the"];
    let mut rng: u64 = 0x1234_5678_9ABC_DEF0;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    const N: usize = 256;
    for i in 0..N {
        let li = (next() % 3) as usize;
        let mapping = reg.get_or_default(layouts[li]).clone();
        let ctx = ctxs[(next() % ctxs.len() as u64) as usize];
        let len = 1 + (next() % 6) as usize;
        let digits: String = (0..len)
            .map(|_| {
                let c = &codes[li];
                c[(next() % c.len() as u64) as usize]
            })
            .collect();
        let tab = if next() % 2 == 0 { "EN" } else { "NE" };
        let a = stack.suggest_for_layout_at(ctx, &digits, &mapping, tab, 5, NOW);
        let b = stack.suggest_for_layout_at(ctx, &digits, &mapping, tab, 5, NOW);
        assert_eq!(canonical(&a), canonical(&b), "fuzz {i} diverged ({digits:?} {tab})");
        for s in &a {
            assert!(!s.score.is_nan(), "fuzz {i}: NaN for {:?}", s.word);
        }
    }
}

// ---- 2. suggest arms, per-tab, hard filter, KSPW/top-k --------------------

#[test]
fn sweep_exact_prefix_neighbor_arms() {
    let stack = fixture_base();
    // Exact: full seq surfaces the word top-1.
    let exact = stack.suggest_at("", "43556", "EN", LIMIT, NOW);
    assert_eq!(exact[0].word, "hello");
    // Prefix: "435" surfaces all three hello/hell/help rows.
    let prefix = stack.suggest_at("", "435", "EN", LIMIT, NOW);
    for w in ["hello", "hell", "help"] {
        assert!(prefix.iter().any(|s| s.word == w), "prefix must contain {w}");
    }
    // Neighbor ON (default arm): 1-edit miss still matches.
    let fuzzy_on = stack.suggest_at("", "43555", "EN", LIMIT, NOW);
    assert!(fuzzy_on.iter().any(|s| s.word == "hello"), "ON arm must fuzzy-match hello");
    // Neighbor OFF: the same miss no longer matches; exact still does.
    let fuzzy_off = stack.suggest_no_neighbor_at("", "43555", "EN", LIMIT, NOW);
    assert!(fuzzy_off.iter().all(|s| s.word != "hello"), "OFF arm must not fuzzy-match");
    let exact_off = stack.suggest_no_neighbor_at("", "43556", "EN", LIMIT, NOW);
    assert_eq!(exact_off[0].word, "hello");
    // Exact-only: no prefix mass — "435" (len 3) matches nothing real.
    let exact_only = stack.suggest_with_opts_at("", "435", "EN", LIMIT, NOW, &SuggestOpts::exact_only());
    assert!(exact_only.is_empty(), "exact-only must not prefix-match, got {exact_only:?}");
    // Production policy == OFF arm on this single-cat slice.
    let policy = stack.suggest_policy_at("", "43555", "EN", LIMIT, NOW);
    assert_eq!(
        policy.iter().map(|s| &s.word).collect::<Vec<_>>(),
        fuzzy_off.iter().map(|s| &s.word).collect::<Vec<_>>(),
        "policy must equal OFF arm here"
    );
}

#[test]
fn sweep_per_tab_isolation_en_ne_js_medical() {
    let stack = fixture_base();
    // Each tab shows its own rows only (hard filter, policy path).
    let en = stack.suggest_policy_at("", "4355", "EN", LIMIT, NOW);
    assert!(!en.is_empty());
    assert!(en.iter().all(|s| SuggestOpts::tab_key(&s.cat) == "words"), "EN leak: {en:?}");
    let js = stack.suggest_policy_at("", "4355", "js", LIMIT, NOW);
    assert!(js.iter().any(|s| s.word == "hellx"), "js must keep its row, got {js:?}");
    assert!(js.iter().all(|s| s.cat == "js"), "js leak: {js:?}");
    let med = stack.suggest_policy_at("", &encode_word("fever"), "medical", LIMIT, NOW);
    assert!(med.iter().any(|s| s.word == "fever"), "medical must surface fever");
    assert!(med.iter().all(|s| s.cat == "medical"), "medical leak: {med:?}");
    let ne = stack.suggest_policy_at("", "267", "NE", LIMIT, NOW);
    assert!(ne.iter().any(|s| s.word == "कमल"), "NE must surface कमल");
    // Cross-tab negatives: EN word never under js, js word never under EN.
    assert!(js.iter().all(|s| s.word != "hello"));
    assert!(en.iter().all(|s| s.word != "hellx"));
    // Unfiltered measurement arm DOES mix (fixture validity for the above).
    let nofilter = SuggestOpts::neighbor_off().with_tab_filter(false);
    let mixed = stack.suggest_with_opts_at("", "4355", "js", LIMIT, NOW, &nofilter);
    assert!(mixed.iter().any(|s| s.cat == "EN"), "fixture needs cross-tab noise, got {mixed:?}");
}

#[test]
fn sweep_topk_and_kspw_shape_at_3_4_full() {
    // Structural (not threshold) locks over a tiny corpus at @3/@4/full:
    // rates in [0,1], top1 <= top3, full-seq self-rank is 1, KSPW >= 1.
    let stack = fixture_base();
    let corpus = [
        ("hello", encode_word("hello"), "EN"),
        ("hell", encode_word("hell"), "EN"),
        ("help", encode_word("help"), "EN"),
        ("fun", encode_word("fun"), "EN"),
        ("कमल", "267".to_string(), "NE"),
    ];
    for (klabel, k) in [("@3", 3usize), ("@4", 4), ("@full", 0)] {
        let (mut t1, mut t3, mut n) = (0usize, 0usize, 0usize);
        for (word, seq, tab) in &corpus {
            let kk = if k == 0 { seq.chars().count() } else { k };
            if seq.chars().count() < kk {
                continue;
            }
            let prefix: String = seq.chars().take(kk).collect();
            let res = stack.suggest_policy_at("", &prefix, tab, LIMIT, NOW);
            n += 1;
            if rank_of(&res, word).map(|r| r == 1).unwrap_or(false) {
                t1 += 1;
            }
            if rank_of(&res, word).map(|r| r <= 3).unwrap_or(false) {
                t3 += 1;
            }
        }
        assert!(n > 0, "{klabel}: empty slice");
        let (r1, r3) = (t1 as f64 / n as f64, t3 as f64 / n as f64);
        assert!((0.0..=1.0).contains(&r1) && (0.0..=1.0).contains(&r3), "{klabel}: rates out of range");
        assert!(t1 <= t3, "{klabel}: top1 ({t1}) must be <= top3 ({t3})");
    }
    // Full-seq self-rank is 1 for unique seqs.
    assert_eq!(rank_of(&stack.suggest_policy_at("", "43556", "EN", LIMIT, NOW), "hello"), Some(1));
    // KSPW (type-until-in-top-3 + 1 select tap, per char) is positive and
    // bounded: it CAN dip below 1.0 (one tap can select a multi-char word,
    // e.g. कमल at a 1-digit prefix), and each word costs at most
    // seqlen+1 taps, so the mean stays under maxlen+1 (= 6 here).
    let (mut taps, mut chars) = (0.0, 0.0);
    for (word, seq, tab) in &corpus {
        let mut t = seq.len() as f64 + 1.0;
        for c in 1..=seq.len() {
            let p: String = seq.chars().take(c).collect();
            if rank_of(&stack.suggest_policy_at("", &p, tab, LIMIT, NOW), word).map(|r| r <= 3).unwrap_or(false) {
                t = c as f64 + 1.0;
                break;
            }
        }
        taps += t;
        chars += word.chars().count().max(1) as f64;
    }
    let kspw = taps / chars;
    assert!(kspw > 0.0 && kspw <= 6.0, "KSPW out of structural bounds, got {kspw}");
}

// ---- 3. layout resolve ----------------------------------------------------

#[test]
fn sweep_layout_resolve_per_keystroke_ne_default_unknown_loud_precomputed() {
    let p = Predictor::new(
        r#"[{"w":"hello","freq":900,"cat":"EN"},
            {"w":"fun","freq":150,"cat":"EN"},
            {"w":"fun","freq":150,"cat":"NE"}]"#
            .to_string(),
    );
    // NE defaults to t9-9 (no pinned override out of the box).
    assert_eq!(p.default_layout(), "t9-9");
    assert_eq!(p.cat_layout("NE".to_string()), "t9-9");
    // Per-keystroke resolve: per-cat suggest finds fun under t9-9 digits.
    let s = p.suggest_for_cat("".to_string(), "386".to_string(), "NE".to_string(), 5);
    assert!(s.iter().any(|c| c.word == "fun"));
    assert_eq!(s.iter().find(|c| c.word == "fun").unwrap().layout_id, "t9-9");
    // Opt NE into t9-16: digits change deterministically (fun 386 -> 396).
    p.set_cat_layout("NE".to_string(), "t9-16".to_string());
    assert_eq!(p.encode("fun".to_string(), "t9-16".to_string()), "396");
    let s16 = p.suggest_for_cat("".to_string(), "396".to_string(), "NE".to_string(), 5);
    assert!(s16.iter().any(|c| c.word == "fun"));
    assert!(p.suggest_for_cat("".to_string(), "386".to_string(), "NE".to_string(), 5).iter().all(|c| c.word != "fun"));
    p.clear_cat_layout("NE".to_string());
    assert_eq!(p.cat_layout("NE".to_string()), "t9-9");
    // Unknown layout -> t9-9 LOUD (results + explicit diagnostic).
    let s = p.suggest_with_layout("".to_string(), "43556".to_string(), "nope".to_string(), "EN".to_string(), 5);
    assert_eq!(s[0].word, "hello");
    let err = p.take_last_layout_error();
    assert!(err.contains("nope"), "fallback must name the bad id, got {err:?}");
    let _ = p.suggest_with_layout("".to_string(), "43556".to_string(), "t9-9".to_string(), "EN".to_string(), 5);
    assert_eq!(p.take_last_layout_error(), "");
    // Precomputed seqs agree with live encoding; explicit overrides verbatim.
    let reg = LayoutRegistry::with_builtins();
    for id in ["t9-9", "t9-12", "t9-16"] {
        let m = reg.get_or_default(id);
        let e = kbcore::stack::DictEntry::precomputed("hello".to_string(), 900, "EN".to_string(), "en".to_string(), 0).unwrap();
        assert_eq!(e.seq_for_layout(id), m.encode_word("hello"), "precompute mismatch under {id}");
    }
    let x = kbcore::stack::DictEntry::explicit("❤️".to_string(), "432".to_string(), 900, "emoji".to_string(), "en".to_string(), 10).unwrap();
    for id in ["t9-9", "t9-12", "t9-16"] {
        assert_eq!(x.seq_for_layout(id), "432", "explicit must stay verbatim under {id}");
    }
}

// ---- 4. tr/alt/matra/Roman -------------------------------------------------

#[test]
fn sweep_tr_alt_matra_roman_to_devanagari() {
    // tr is primary, alt + skeleton are match-any aliases.
    let pack = load_pack_str(
        r#"{"id":"ne","title":"Nepali","version":"1.0.0",
            "words":[{"w":"पानी","tr":"pani","alt":["paani"],"freq":9000,"cat":"NE","lang":"ne"}]}"#,
    )
    .unwrap();
    assert!(pack.validate().is_empty());
    let entries = pack.to_entries(10);
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.word, "पानी");
    assert_eq!(e.seq, encode_word("pani"), "tr must be primary");
    assert!(e.aliases.contains(&encode_word("paani")), "alt must alias");
    assert!(e.aliases.contains(&encode_word("पानी")), "skeleton fallback must alias");
    assert!(!e.aliases.contains(&e.seq.clone()), "primary must not alias itself");
    // Matras/virama/signs are non-emitting everywhere.
    for m in ['ा', 'ि', 'ी', 'ु', 'ू', 'े', 'ै', 'ो', 'ौ', 'ं', 'ः', 'ँ', 'ॉ', '्'] {
        assert_eq!(char_to_digit(m), None, "matra {m:?} must not emit");
    }
    assert_eq!(encode_word("कि"), "2", "matra must vanish, not emit 9");
    assert_eq!(encode_word("नमस्ते"), "5685", "consonant skeleton");
    // End-to-end: Roman keystrokes surface Devanagari.
    let p = Predictor::new(
        r#"[{"w":"पानी","tr":"pani","alt":["paani"],"freq":9000,"cat":"NE","lang":"ne"}]"#.to_string(),
    );
    let digits = p.encode("pani".to_string(), "t9-9".to_string());
    assert!(!digits.is_empty());
    let got = p.suggest("".to_string(), digits, "NE".to_string(), 10);
    assert!(got.iter().any(|s| s.word == "पानी"), "roman 'pani' must suggest पानी, got {got:?}");
    let digits2 = p.encode("paani".to_string(), "t9-9".to_string());
    let got2 = p.suggest("".to_string(), digits2, "NE".to_string(), 10);
    assert!(got2.iter().any(|s| s.word == "पानी"), "alt 'paani' must suggest पानी");
}

// ---- 5. single-press + QWERTY ----------------------------------------------

#[test]
fn sweep_single_press_s_first_len3_untouched_qwerty_passthrough() {
    let stack = DictionaryStack::new(
        DictionaryStack::load_base_json(
            r#"[{"w":"some","freq":5000,"cat":"EN"},
                {"w":"school","freq":3000,"cat":"EN"},
                {"w":"sad","freq":1000,"cat":"EN"},
                {"w":"so","freq":200,"cat":"EN"},
                {"w":"an","freq":8000,"cat":"EN"}]"#,
        )
        .unwrap(),
    );
    // len<=2: single press leads with s (never empty).
    let s = stack.suggest_at("", "7", "words", 10, NOW);
    assert!(!s.is_empty());
    assert_eq!(s[0].word, "s", "got {:?}", s.iter().map(|c| &c.word).collect::<Vec<_>>());
    // len==2 exact first: so (freq 200) outranks some (freq 5000).
    let s2 = stack.suggest_at("", "76", "words", 10, NOW);
    assert_eq!(s2[0].word, "so");
    // len>=3 unchanged: no single-letter rows, prefix order decides.
    let stack3 = DictionaryStack::new(
        DictionaryStack::load_base_json(
            r#"[{"w":"son","freq":200,"cat":"EN"},{"w":"song","freq":5000,"cat":"EN"}]"#,
        )
        .unwrap(),
    );
    let s3 = stack3.suggest_at("", "766", "words", 10, NOW);
    assert_eq!(s3[0].word, "song");
    assert!(s3.iter().all(|c| c.word.chars().count() > 1), "len>=3 injects nothing, got {s3:?}");
    assert!(stack3.suggest_at("", "1", "words", 10, NOW).is_empty());
    assert!(stack3.suggest_at("", "0", "words", 10, NOW).is_empty());
    // QWERTY unaffected: digits pass through, QWERTY flow (encode raw latin
    // under the tab layout, then suggest) recovers the word; the fallback
    // never alters len>=3 ordering (default opts == legacy arm there).
    assert_eq!(encode_word("a1"), "21");
    let p = Predictor::new(r#"[{"w":"hello","freq":900,"cat":"EN"}]"#.to_string());
    let digits = p.encode("hello".to_string(), "t9-9".to_string());
    assert_eq!(digits, "43556");
    let got = p.suggest_with_layout("".to_string(), digits, "t9-9".to_string(), "EN".to_string(), 5);
    assert_eq!(got[0].word, "hello");
    let words = |v: Vec<Suggestion>| v.into_iter().map(|x| (x.word, x.score.to_bits())).collect::<Vec<_>>();
    assert_eq!(
        words(stack3.suggest_at("", "766", "words", 10, NOW)),
        words(stack3.suggest_with_opts_at("", "766", "words", 10, NOW, &SuggestOpts::default())),
        "len>=3 must match the frozen default arm exactly"
    );
}

// ---- 6. learn/reject/personal + gates + snippet ----------------------------

#[test]
fn sweep_learn_reject_personal_oov_and_password_bypass() {
    let mut stack = fixture_base();
    let oov = "zxqyphen";
    let seq = encode_word(oov);
    assert!(rank_of(&stack.suggest_at("", &seq, "words", LIMIT, NOW), oov).is_none(), "OOV must start absent");
    stack.personal.learn_at(oov, "words", NOW);
    stack.personal.learn_at(oov, "words", NOW);
    stack.personal.record_bigram("the", oov);
    let rank = rank_of(&stack.suggest_at("the", &seq, "words", LIMIT, NOW), oov);
    assert!(rank.map(|r| r <= 3).unwrap_or(false), "2 accepts must promote OOV to top-3, got {rank:?}");
    // Reject penalty demotes: 9 rejects score below clean.
    let mut clean = fixture_base();
    clean.personal.learn_at(oov, "words", NOW);
    let mut rej = fixture_base();
    rej.personal.learn_at(oov, "words", NOW);
    for _ in 0..9 {
        rej.personal.record_reject(oov);
    }
    let score = |st: &DictionaryStack| {
        st.suggest_at("", &seq, "words", LIMIT, NOW).iter().find(|s| s.word == oov).map(|s| s.score)
    };
    assert!(score(&rej) < score(&clean), "rejects must demote");
    // Block tombstone hides everywhere.
    rej.personal.forget(oov);
    assert!(rej.personal.is_blocked(oov));
    assert!(rank_of(&rej.suggest_at("", &seq, "words", LIMIT, NOW), oov).is_none());
    // Re-learn unblocks.
    rej.personal.learn(oov, "words");
    assert!(!rej.personal.is_blocked(oov));
    // Password mode: learn is a no-op, nothing stored.
    let mut pw = PersonalDict::new();
    pw.password_mode = true;
    assert!(!pw.learn("secret", "EN"));
    assert!(pw.get("secret").is_none());
    // Predictor forget path hides too.
    let p = Predictor::new(r#"[{"w":"hello","freq":900,"cat":"EN"}]"#.to_string());
    p.forget("hello".to_string());
    assert!(p.suggest("".to_string(), "43556".to_string(), "EN".to_string(), 5).iter().all(|s| s.word != "hello"));
}

#[test]
fn sweep_snippet_learn_gate_truth_table() {
    // Frozen truth table (mirrors gates.rs docs): every row pins both gates
    // plus the allied read/write gates where they apply.
    let g = |password: bool, incognito: bool, text_class: bool, email_or_uri: bool, tab: &str, learn_for_tab: bool| GateInput {
        password, incognito, text_class, email_or_uri, tab: tab.to_string(), learn_for_tab,
    };
    // normal EN/NE/js: both fire.
    for tab in ["words", "ne", "js"] {
        let gi = g(false, false, true, false, tab, true);
        assert!(learn_now(&gi), "{tab}: learn");
        assert!(snippet_now(&gi), "{tab}: snippet");
        assert_eq!(temporal_now(&gi), learn_now(&gi), "{tab}: temporal == learn");
    }
    // numbers tab: neither.
    let gi = g(false, false, true, false, "numbers", false);
    assert!(!learn_now(&gi));
    assert!(!snippet_now(&gi));
    // math tab: snippet exempt from no-learn.
    let gi = g(false, false, true, false, "math", false);
    assert!(!learn_now(&gi));
    assert!(snippet_now(&gi));
    assert!(!temporal_now(&gi));
    // password field: neither + no clipboard + mic hidden.
    let gi = g(true, false, true, false, "js", true);
    assert!(!learn_now(&gi));
    assert!(!snippet_now(&gi));
    assert!(!temporal_now(&gi));
    assert!(!may_capture_clipboard(true, false));
    assert!(!mic_visible(true, false));
    // incognito: neither; mic stays visible (inserts learn-free).
    let gi = g(false, true, true, false, "js", true);
    assert!(!learn_now(&gi));
    assert!(!snippet_now(&gi));
    assert!(!may_capture_clipboard(false, true));
    assert!(mic_visible(false, false));
    // email/URI: snippet but never learn.
    let gi = g(false, false, true, true, "words", true);
    assert!(!learn_now(&gi));
    assert!(snippet_now(&gi));
    // NUMBER/PHONE/DATETIME: neither.
    let gi = g(false, false, false, false, "js", true);
    assert!(!learn_now(&gi));
    assert!(!snippet_now(&gi));
    // Snippet buffer mechanics: TTL, cap, consume-wipes, owner clear.
    let mut b = SnippetBuffer::new();
    b.offer("if", &["if (".to_string()], 0).unwrap();
    assert!(!b.peek(0).is_empty());
    assert!(!b.peek(SNIPPET_TTL_MS - 1).is_empty());
    assert!(b.peek(3 * 60 * 1000).is_empty(), "TTL must expire");
    let live = {
        let mut b2 = SnippetBuffer::new();
        b2.offer("if", &["if (".to_string()], 0).unwrap();
        b2.consume(1_000)
    };
    assert_eq!(live, vec!["if (".to_string()]);
    let mut b3 = SnippetBuffer::new();
    for t in ["a", "b", "c", "d"] {
        b3.offer(t, &[t.to_string()], 0).unwrap();
    }
    assert_eq!(b3.size(0), MAX_ENTRIES, "cap is {MAX_ENTRIES}");
    assert!(quantize_ts(NOW).is_ok());
}

// ---- 7. persistence ---------------------------------------------------------

#[test]
fn sweep_persistence_single_handle_wal_coalesce_pure_ram() {
    // Power contract constants: single WAL handle, 2s coalesce window.
    assert_eq!(FLUSH_COALESCE.as_secs(), 2, "flush window must be 2s");
    let dir = std::env::temp_dir().join("kbcore_sweep_persist");
    let _ = std::fs::remove_dir_all(&dir);
    let db = dir.join("kb.sqlite").display().to_string();
    let sync = dir.join("sync").display().to_string();
    let p = Predictor::new(
        r#"[{"w":"hello","freq":900,"cat":"EN"},{"w":"hell","freq":100,"cat":"EN"}]"#.to_string(),
    );
    // Pure-RAM hot path: typing works fully with no store open.
    // NOTE: "hellp" is h4-e3-l5-l5-p7 = "43557": it joins the "4355"
    // prefix union, not the exact "43556" row — so the probe must cover
    // both (mirrors the predictor hot-path test).
    let run = || {
        let mut out = Vec::new();
        for digits in ["43556", "4355"] {
            for s in p.suggest("".to_string(), digits.to_string(), "EN".to_string(), 5) {
                out.push((s.word, s.score.to_bits(), s.seq));
            }
        }
        out
    };
    let cold = run();
    assert!(!cold.is_empty());
    p.learn("hellp".to_string(), "EN".to_string());
    p.learn("hellp".to_string(), "EN".to_string());
    let ram_only = run();
    assert_ne!(cold, ram_only, "learn must promote in RAM with no DB");
    assert!(p.try_flush().unwrap_err().contains("not opened"));
    // Single handle: open once, WAL mode, flush writes dirty-only.
    assert_eq!(p.open_persist(db, sync), "");
    let mode: String = Store::open(std::path::Path::new(&dir.join("kb.sqlite")))
        .map(|_| "wal".to_string())
        .unwrap_or_else(|e| panic!("reopen must work: {e}"));
    assert_eq!(mode, "wal");
    let stats = p.try_flush().unwrap();
    assert!(stats.personal_rows <= 1, "one dirty word, got {stats:?}");
    let clean = p.try_flush().unwrap();
    assert_eq!(clean.personal_rows, 0, "clean flush writes nothing");
    assert_eq!(clean.session_rows, 0);
    // DB attach must not move the read path.
    assert_eq!(run(), ram_only, "flush must not move the read path");
    // WAL got the delta (sync dir configured).
    let wal = std::fs::read_to_string(dir.join("sync").join("wal.log")).unwrap();
    assert!(wal.contains("hellp"), "wal.log must carry the delta");
    let snap = p.compact_persist();
    assert!(snap.contains("snapshot-"), "compact must snapshot, got {snap:?}");
    assert_eq!(p.take_last_persist_error(), "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sweep_packs_parse_and_validate_loudly() {
    // Packs are data: valid fixtures load, invalid ones fail loudly —
    // never a silent half-loaded dictionary.
    let good = load_pack_str(
        r#"{"id":"x","title":"X","version":"1.0.0","words":[{"w":"hello","freq":900}]}"#,
    )
    .unwrap();
    assert!(good.validate().is_empty());
    assert_eq!(good.to_entries(0).len(), 1);
    assert!(load_pack_str(r#"{"nope":true}"#).is_err());
    let bad = load_pack_str(
        r#"{"id":"x","title":"X","version":"1.0.0","words":[{"w":"*"},{"w":"ok","alt":["*"]}]}"#,
    )
    .unwrap();
    assert_eq!(bad.validate().len(), 2, "one error per bad row");
    // Production minimums: every shipped pack id carries a real vocab
    // (mirrors per_tab MIN_WORDS so a near-empty pack fails loudly here too).
    for (src, id, min) in [
        (include_str!("../../packs/words_en.json"), "words", 5000),
        (include_str!("../../packs/nepali.json"), "ne", 5000),
        (include_str!("../../packs/code_js.json"), "js", 200),
        (include_str!("../../packs/medical.json"), "medical", 500),
    ] {
        let pack = load_pack_str(src).unwrap_or_else(|e| panic!("pack {id} must parse: {e}"));
        assert_eq!(pack.id, id);
        assert!(pack.words.len() >= min, "pack {id} has {} words (< {min})", pack.words.len());
        assert!(pack.validate().is_empty(), "pack {id} must validate clean");
    }
}
