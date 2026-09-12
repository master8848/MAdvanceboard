//! Determinism harness (`plan/04-engine-determinism.md` #5):
//! golden vectors per layout (`digits,ctx,tab -> top-5 words+scores`) +
//! seeded fuzz asserting byte-identical repeats.
//!
//! Invariant: same `(dict set, ctx, digits, tab, layout, personal
//! snapshot, now-hour)` => byte-identical `Vec<Suggestion>`. Scores are
//! compared by `to_bits` (exact f64 identity, NaN payloads included).
//! Any future HashMap-ordering or wall-clock regression turns this red.
//!
//! To refresh goldens after an intentional scoring change, read the
//! `GOLDEN MISMATCH` panic output and paste the printed literals.

use kbcore::{DictionaryStack, KeyMapping, LayoutRegistry, Suggestion};

/// Frozen scoring instant (already a 1h bucket).
const NOW: i64 = 1_726_000_000 - (1_726_000_000 % 3600);

fn corpus() -> DictionaryStack {
    let mut stack = DictionaryStack::new(
        DictionaryStack::load_base_json(
            r#"[{"w":"hello","freq":900,"cat":"EN","lang":"en"},
                {"w":"hell","freq":100,"cat":"EN","lang":"en"},
                {"w":"help","freq":200,"cat":"EN","lang":"en"},
                {"w":"fun","freq":150,"cat":"EN","lang":"en"},
                {"w":"run","freq":150,"cat":"EN","lang":"en"},
                {"w":"wxyz","freq":80,"cat":"EN","lang":"en"},
                {"w":"a@b","freq":50,"cat":"EN","lang":"en"},
                {"w":"कमल","freq":300,"cat":"NE","lang":"ne"}]"#,
        )
        .expect("golden corpus must parse"),
    );
    // Fixed personal snapshot (learn timestamps clamp to recency 1.0
    // against the frozen NOW, so this is deterministic).
    stack.personal.learn("hell", "EN");
    stack.personal.learn("hell", "EN");
    stack.personal.learn("help", "EN");
    stack.personal.record_bigram("the", "hello");
    stack
}

fn canonical(sugs: &[Suggestion]) -> String {
    sugs
        .iter()
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

fn check(layout: &str, ctx: &str, digits: &str, tab: &str, golden: &str) {
    let stack = corpus();
    let reg = LayoutRegistry::with_builtins();
    let mapping = reg.get_or_default(layout).clone();
    assert_eq!(
        mapping.layout_id(),
        layout,
        "unknown golden layout {layout}"
    );
    let a = stack.suggest_for_layout_at(ctx, digits, &mapping, tab, 5, NOW);
    for s in &a {
        assert!(
            !s.score.is_nan(),
            "golden: NaN score for {:?} ({layout} {ctx:?} {digits:?})",
            s.word
        );
    }
    let actual = canonical(&a);
    // Byte-identical repeat (same inputs, second call).
    let b = stack.suggest_for_layout_at(ctx, digits, &mapping, tab, 5, NOW);
    assert_eq!(
        canonical(&b),
        actual,
        "repeat call diverged ({layout} {ctx:?} {digits:?} {tab})"
    );
    assert_eq!(
        actual, golden,
        "GOLDEN MISMATCH ({layout} {ctx:?} {digits:?} {tab}); paste this literal:\n{actual}"
    );
}

#[test]
fn golden_t9_9() {
    check("t9-9", "", "43556", "EN", GOLDEN_T9_9_A);
    check("t9-9", "", "386", "EN", GOLDEN_T9_9_B);
    check("t9-9", "the", "43556", "EN", GOLDEN_T9_9_C);
    check("t9-9", "", "9999", "EN", GOLDEN_T9_9_D);
    check("t9-9", "", "267", "NE", GOLDEN_T9_9_E);
}

#[test]
fn golden_t9_12() {
    check("t9-12", "", "43556", "EN", GOLDEN_T9_12_A);
    check("t9-12", "", "2*2", "EN", GOLDEN_T9_12_B);
    check("t9-12", "", "267", "NE", GOLDEN_T9_12_C);
}

#[test]
fn golden_t9_16() {
    check("t9-16", "", "43556", "EN", GOLDEN_T9_16_A);
    check("t9-16", "", "396", "EN", GOLDEN_T9_16_B);
    check("t9-16", "", "00AA", "EN", GOLDEN_T9_16_C);
    check("t9-16", "", "267", "NE", GOLDEN_T9_16_D);
}

const GOLDEN_T9_9_A: &str = "hello|400ba3d70a3d70a4|43556|EN|t9-9";
const GOLDEN_T9_9_B: &str = "fun|40056e978d4fdf3b|386|EN|t9-9";
const GOLDEN_T9_9_C: &str = "hello|400a835503456771|43556|EN|t9-9";
const GOLDEN_T9_9_D: &str = "wxyz|40034395810624dd|9999|EN|t9-9";
const GOLDEN_T9_9_E: &str = "कमल|4007d4fdf3b645a2|267|NE|t9-9";
const GOLDEN_T9_12_A: &str = "hello|400ba3d70a3d70a4|43556|EN|t9-12";
const GOLDEN_T9_12_B: &str = "a@b|4001a9fbe76c8b44|2*2|EN|t9-12";
const GOLDEN_T9_12_C: &str = "कमल|4007d4fdf3b645a2|267|NE|t9-12";
const GOLDEN_T9_16_A: &str = "hello|400ba3d70a3d70a4|43556|EN|t9-16";
const GOLDEN_T9_16_B: &str = "fun|40056e978d4fdf3b|396|EN|t9-16\nrun|4004cac083126e97|896|EN|t9-16";
const GOLDEN_T9_16_C: &str = "wxyz|40034395810624dd|00AA|EN|t9-16";
const GOLDEN_T9_16_D: &str = "कमल|4007d4fdf3b645a2|267|NE|t9-16";

/// Seeded fuzz: 2000 pseudo-random `(layout, ctx, digits)` queries, each
/// executed twice; outputs must be byte-identical, scores never NaN, and
/// the legacy `suggest_at` path must agree exactly with
/// `suggest_for_layout_at(t9-9)` on digit strings.
#[test]
fn seeded_fuzz_byte_identical_repeats() {
    let stack = corpus();
    let reg = LayoutRegistry::with_builtins();
    let layouts = ["t9-9", "t9-12", "t9-16"];
    let codes: Vec<Vec<char>> = layouts
        .iter()
        .map(|id| {
            reg.get_or_default(id)
                .keys
                .iter()
                .map(|k| k.code)
                .collect()
        })
        .collect();
    let ctxs = ["", "the", "a b", "hello world"];
    let mut rng: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    for i in 0..2000 {
        let li = (next() % 3) as usize;
        let layout = layouts[li];
        let mapping = reg.get_or_default(layout).clone();
        let ctx = ctxs[(next() % ctxs.len() as u64) as usize];
        let len = 1 + (next() % 8) as usize;
        let digits: String = (0..len)
            .map(|_| {
                let c = &codes[li];
                c[(next() % c.len() as u64) as usize]
            })
            .collect();
        let tab = if next() % 2 == 0 { "EN" } else { "NE" };
        let a = stack.suggest_for_layout_at(ctx, &digits, &mapping, tab, 5, NOW);
        let b = stack.suggest_for_layout_at(ctx, &digits, &mapping, tab, 5, NOW);
        assert_eq!(
            canonical(&a),
            canonical(&b),
            "fuzz iter {i} diverged ({layout} {ctx:?} {digits:?} {tab})"
        );
        for s in &a {
            assert!(
                !s.score.is_nan(),
                "fuzz iter {i}: NaN score for {:?}",
                s.word
            );
        }
        // Legacy parity under fuzz for digit-only t9-9 queries.
        if layout == "t9-9" && digits.chars().all(|c| c.is_ascii_digit()) {
            let legacy = stack.suggest_at(ctx, &digits, tab, 5, NOW);
            assert_eq!(
                canonical(&legacy),
                canonical(&a),
                "fuzz iter {i}: legacy path diverged ({ctx:?} {digits:?} {tab})"
            );
        }
    }
}
