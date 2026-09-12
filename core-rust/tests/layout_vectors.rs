//! Cross-layout vectors: JSON source of truth -> registry -> engine.
//! T9-parity anchors: legacy `suggest` and `suggest_for_layout(t9-9)` agree
//! exactly; non-default layouts only extend coverage.

use kbcore::{DictionaryStack, KeyMapping, LayoutRegistry, LayoutSpec, Predictor};

fn spec(id: &str) -> LayoutSpec {
    let src = match id {
        "t9-9" => include_str!("../../layouts/t9-9.json"),
        "t9-12" => include_str!("../../layouts/t9-12.json"),
        "t9-16" => include_str!("../../layouts/t9-16.json"),
        _ => panic!("unknown fixture layout"),
    };
    let s = LayoutSpec::from_json(src).unwrap();
    assert_eq!(s.id, id);
    s
}

#[test]
fn json_vectors_hello_fun() {
    let r = LayoutRegistry::with_builtins();
    assert_eq!(r.ids().len(), 3);
    let t9 = r.get_or_default("t9-9");
    assert_eq!(t9.encode_word("hello"), "43556");
    assert_eq!(t9.encode_word("fun"), "386");
    let t12 = r.get_or_default("t9-12");
    assert_eq!(t12.encode_word("hello"), "43556");
    assert_eq!(t12.encode_word("a@b"), "2*2");
    let t16 = r.get_or_default("t9-16");
    assert_eq!(t16.encode_word("pqrs"), "7788");
    assert_eq!(t16.encode_word("wxyz"), "00AA");
    assert_eq!(t16.encode_word("fun"), "396");
    // Fixture files parse to the same specs the registry holds.
    assert_eq!(spec("t9-9").encode_word("hello"), "43556");
    assert_eq!(spec("t9-16").encode_word("wxyz"), "00AA");
}

#[test]
fn suggest_default_layout_matches_legacy_exactly() {
    let stack = DictionaryStack::new(
        DictionaryStack::load_base_json(
            r#"[{"w":"hello","freq":900,"cat":"EN"},
                {"w":"hell","freq":100,"cat":"EN"},
                {"w":"help","freq":200,"cat":"EN"}]"#,
        )
        .unwrap(),
    );
    let r = LayoutRegistry::with_builtins();
    let t9 = r.get_or_default("t9-9");
    for seq in ["43556", "43555", "4355", "43"] {
        let a = stack.suggest("", seq, "EN", 5);
        let b = stack.suggest_for_layout("", seq, t9, "EN", 5);
        assert_eq!(a.len(), b.len(), "seq {seq}");
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.word, y.word);
            assert_eq!(x.seq, y.seq);
            assert_eq!(x.cat, y.cat);
            assert_eq!(x.score, y.score);
            assert_eq!(y.layout_id, "t9-9");
        }
    }
}

#[test]
fn layout_only_codes_match_only_there() {
    let stack = DictionaryStack::new(
        DictionaryStack::load_base_json(r#"[{"w":"wxyz","freq":900,"cat":"EN"}]"#).unwrap(),
    );
    let r = LayoutRegistry::with_builtins();
    // t9-16 split seq finds the word; legacy digit-only path rejects 'A'.
    let s = stack.suggest_for_layout("", "00AA", r.get_or_default("t9-16"), "EN", 5);
    assert!(s.iter().any(|c| c.word == "wxyz"));
    assert_eq!(s[0].layout_id, "t9-16");
    assert!(stack.suggest("", "00AA", "EN", 5).is_empty());
    // t9-12 Sym seq likewise.
    let stack2 = DictionaryStack::new(
        DictionaryStack::load_base_json(r#"[{"w":"a@b","freq":900,"cat":"EN"}]"#).unwrap(),
    );
    let s2 = stack2.suggest_for_layout("", "2*2", r.get_or_default("t9-12"), "EN", 5);
    assert!(s2.iter().any(|c| c.word == "a@b"));
}

#[test]
fn explicit_seq_overrides_stay_verbatim_across_layouts() {
    let mut stack = DictionaryStack::new(DictionaryStack::load_base_json("[]").unwrap());
    let pack = kbcore::pack::load_pack_str(
        r#"{"id":"emoji","title":"Emoji","version":"1.0.0",
            "words":[{"w":"❤️","seq":"432","freq":900,"cat":"emoji"}]}"#,
    )
    .unwrap();
    stack.add_pack(&pack, 10);
    let r = LayoutRegistry::with_builtins();
    for id in ["t9-9", "t9-12", "t9-16"] {
        let s = stack.suggest_for_layout("", "432", r.get_or_default(id), "emoji", 5);
        assert!(s.iter().any(|c| c.word == "❤️"), "layout {id}");
    }
}

#[test]
fn predictor_layout_api() {
    let p = Predictor::new(r#"[{"w":"hello","freq":900,"cat":"EN"}]"#.to_string());
    assert_eq!(p.layouts(), vec!["t9-12", "t9-16", "t9-9"]);
    assert_eq!(p.encode("hello".to_string(), "t9-9".to_string()), "43556");
    assert_eq!(p.encode("pqrs".to_string(), "t9-16".to_string()), "7788");
    // Back-compat suggest == explicit default layout.
    let a = p.suggest("".to_string(), "43556".to_string(), "EN".to_string(), 5);
    let b = p.suggest_with_layout(
        "".to_string(),
        "43556".to_string(),
        "t9-9".to_string(),
        "EN".to_string(),
        5,
    );
    assert_eq!(a[0].word, "hello");
    assert_eq!(b[0].word, "hello");
    assert_eq!(b[0].layout_id, "t9-9");
    // Unknown/empty layout ids fall back to the default.
    let c = p.suggest_with_layout(
        "".to_string(),
        "43556".to_string(),
        "nope".to_string(),
        "EN".to_string(),
        5,
    );
    assert_eq!(c[0].word, "hello");
    // Per-layout decode honors the code set.
    let d = p.decode("4355".to_string(), "t9-9".to_string(), 10);
    assert!(d.contains(&"43556".to_string()));
    assert!(p.decode("ZZZ".to_string(), "t9-9".to_string(), 10).is_empty());
}
