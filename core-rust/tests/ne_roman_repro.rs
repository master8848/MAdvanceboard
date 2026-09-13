//! On-device bug repro: romanized Nepali gets no suggestions.
//!
//! Path fidelity with `KbInputMethodService.refreshQwertySuggestions`:
//! raw Latin -> `Predictor::encode(raw, layoutId)` -> `suggest_with_layout`
//! with the tab's resolved layout (`ne` -> `t9-9` global default, MASTER
//! rule 3 + plan/02) and the canonical asset id (`ne`) as the active
//! tab. Base wordlist merges `packs/words_en.json` + `packs/nepali.json`
//! exactly like `loadBaseWordlist` (single array into `Predictor::new`).

use kbcore::pack::load_pack_str;
use kbcore::Predictor;

/// IME-equivalent base: merged `words[]` of words_en + nepali.
fn ime_base_json() -> String {
    let en = load_pack_str(include_str!("../../packs/words_en.json"))
        .expect("words_en must parse");
    let ne =
        load_pack_str(include_str!("../../packs/nepali.json")).expect("nepali must parse");
    let mut out = String::from("[");
    let mut first = true;
    for w in en.words.iter().chain(ne.words.iter()) {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(
            &serde_json::to_string(w).expect("pack word must serialize"),
        );
    }
    out.push(']');
    out
}

fn ime_predictor() -> std::sync::Arc<Predictor> {
    Predictor::new(ime_base_json())
}

/// The exact two-step IME QWERTY query: encode Latin under the tab's
/// layout, then suggest under the same layout + tab.
fn qwerty_suggest(
    p: &Predictor,
    raw: &str,
    tab: &str,
) -> (String, String, Vec<String>) {
    let layout = p.cat_layout(tab.to_string());
    let digits = p.encode(raw.to_string(), layout.clone());
    let got: Vec<String> = p
        .suggest_with_layout(
            "".to_string(),
            digits.clone(),
            layout.clone(),
            tab.to_string(),
            30,
        )
        .into_iter()
        .map(|s| s.word)
        .collect();
    (layout, digits, got)
}

#[test]
fn roman_meri_suggests_meri_class() {
    let p = ime_predictor();
    let (layout, digits, got) = qwerty_suggest(&p, "meri", "ne");
    assert_eq!(layout, "t9-9", "ne tab must resolve to the global default");
    assert!(!digits.is_empty(), "latin 'meri' must encode under {layout}");
    assert!(
        got.iter().any(|w| w == "मेरी"),
        "typing 'meri' (layout={layout} digits={digits}) must suggest मेरी, got {got:?}"
    );
}

#[test]
fn roman_mailea_suggests_maile_class() {
    let p = ime_predictor();
    let (layout, digits, got) = qwerty_suggest(&p, "mailea", "ne");
    assert!(!digits.is_empty(), "latin 'mailea' must encode under {layout}");
    assert!(
        got.iter().any(|w| w == "मैले"),
        "typing 'mailea' (layout={layout} digits={digits}) must suggest मैले, got {got:?}"
    );
}

/// Control: the tr-first pipeline itself resolves romanized input when
/// the rows exist — isolates the failure to pack data, not the engine.
#[test]
fn tr_first_pipeline_resolves_romanized_input_when_rows_exist() {
    let p = Predictor::new(
        r#"[{"w":"मेरी","tr":"meri","freq":9000,"cat":"NE","lang":"ne"},
            {"w":"मैले","tr":"maile","alt":["mailea"],"freq":9000,"cat":"NE","lang":"ne"}]"#
            .to_string(),
    );
    let (_, _, got_meri) = qwerty_suggest(&p, "meri", "ne");
    assert!(
        got_meri.iter().any(|w| w == "मेरी"),
        "control: synthetic मेरी/meri row must suggest, got {got_meri:?}"
    );
    let (_, _, got_mailea) = qwerty_suggest(&p, "mailea", "ne");
    assert!(
        got_mailea.iter().any(|w| w == "मैले"),
        "control: synthetic मैले/maile+alt row must suggest for 'mailea', got {got_mailea:?}"
    );
}

/// t9-16 stays opt-in: a per-cat override re-encodes the same Roman input
/// under the finer splits and still surfaces Devanagari (both layouts
/// share the tr pipeline; only the digit strings differ).
#[test]
fn roman_typing_surfaces_devanagari_under_t9_16_opt_in() {
    let p = ime_predictor();
    p.set_cat_layout("ne".to_string(), "t9-16".to_string());
    let (layout, digits, got) = qwerty_suggest(&p, "meri", "ne");
    assert_eq!(layout, "t9-16");
    assert!(!digits.is_empty(), "latin 'meri' must encode under t9-16");
    assert!(
        got.iter().any(|w| w == "मेरी"),
        "opt-in t9-16 typing 'meri' (digits={digits}) must suggest मेरी, got {got:?}"
    );
}
