//! Suggest-time inflection sidecar (prototype, flag-gated).
//!
//! User feedback: verb forms "are not there" — diagnosis
//! (`tests/morph_conn_diag.rs::diag1`) shows 51/51 probed regular English
//! inflections absent from the 5150-word EN vocab, while gate misses are
//! only 0.8%-inflectional (EN is 0.971 under the policy arm; the gate never
//! types OOV words, so it cannot see this gap).
//!
//! Mechanism: a declarative per-language suffix rule table (NOT per-word
//! overrides) expands in-vocab stems into derived [`DictEntry`] rows at
//! load time, with a discounted frequency (`stem_freq / discount_div`) so
//! derived forms never outrank their own stem on equal keyfit. Closed-class
//! stems (function words) are excluded by a frequency gate
//! (`max_stem_freq`), not a word list — structural, language-agnostic
//! plumbing with one English rule table today.
//!
//! Gating: NOTHING in the default load path calls this. Callers opt in
//! explicitly via [`DictionaryStack::expand_inflections`] (flag = the call
//! + [`InflectConfig`]); default stacks are byte-identical (locked by
//! `inflect_proto::flag_off_is_byte_identical`).
//!
//! Fixed-V interaction (plan/09#2): expansion adds STATIC rows only —
//! [`BIGRAM_FIXED_VOCAB`](crate::personal::BIGRAM_FIXED_VOCAB) is constant,
//! so unseen-pair mass never moves. The only growth is index/union size
//! (reported per prototype: rows +X%, mean union +Y).

use crate::stack::DictEntry;

/// Load-time inflection-expansion configuration (the prototype flag body).
#[derive(Clone, Debug)]
pub struct InflectConfig {
    /// Derived `freq = max(1, stem_freq / discount_div)`. Tuned on the
    /// TRAIN half of the OOV-inflection probe set only (see
    /// `tests/inflect_proto.rs`); default 8 is the measured knee.
    pub discount_div: u64,
    /// Stems with `freq >= max_stem_freq` never inflect: function words
    /// (`the` 1e6, `and` 650k, …) must not sprout `*thes`/`*anding`.
    /// A frequency gate, not a word list.
    pub max_stem_freq: u64,
    /// Minimum stem chars (kills 1–2-letter noise like `*as`, `*is`-from-`i`).
    pub min_stem_len: usize,
}

impl Default for InflectConfig {
    fn default() -> Self {
        Self {
            discount_div: 8,
            max_stem_freq: 50_000,
            min_stem_len: 3,
        }
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

fn is_consonant(c: char) -> bool {
    c.is_ascii_lowercase() && !is_vowel(c)
}

/// CVC-double test on the stem tail (`run`→`nn`, `stop`→`pp`):
/// ends consonant-vowel-consonant with the final outside `wxy`.
fn doubles_final(stem: &str) -> bool {
    let b = stem.as_bytes();
    if b.len() < 3 {
        return false;
    }
    let (c1, c2, c3) = (
        b[b.len() - 3] as char,
        b[b.len() - 2] as char,
        b[b.len() - 1] as char,
    );
    is_consonant(c1) && is_vowel(c2) && is_consonant(c3) && !matches!(c3, 'w' | 'x' | 'y')
}

fn ends_consonant_y(stem: &str) -> bool {
    let b = stem.as_bytes();
    b.len() >= 2 && b[b.len() - 1] == b'y' && is_consonant(b[b.len() - 2] as char)
}

fn ends_sibilant(stem: &str) -> bool {
    stem.ends_with('s')
        || stem.ends_with('x')
        || stem.ends_with('z')
        || stem.ends_with("ch")
        || stem.ends_with("sh")
}

/// Regular English suffix rules for one lowercase alpha stem.
/// Returns `(rule_id, derived_form)` pairs, deduped, stem itself excluded.
/// Irregulars (`run→ran`, `go→went`) are vocab-coverage rows, never
/// generated here.
pub fn inflect_stem_en(stem: &str) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let mut push = |rule: &'static str, form: String| {
        if form != stem && !out.iter().any(|(_, f)| *f == form) {
            out.push((rule, form));
        }
    };
    // Plural / 3sg present.
    if ends_consonant_y(stem) {
        push("ies", format!("{}ies", &stem[..stem.len() - 1]));
    } else if ends_sibilant(stem) {
        push("es", format!("{stem}es"));
    } else {
        push("s", format!("{stem}s"));
    }
    // Past: doubling stems take ONLY the doubled form (*runed never
    // occurs); all others take the plain suffix.
    if ends_consonant_y(stem) {
        push("ied", format!("{}ied", &stem[..stem.len() - 1]));
    } else if stem.ends_with('e') {
        push("d", format!("{stem}d"));
    } else if doubles_final(stem) {
        let last = stem.chars().last().unwrap_or('?');
        push("dbl-ed", format!("{stem}{last}ed"));
    } else {
        push("ed", format!("{stem}ed"));
    }
    // Progressive: same doubling exclusivity (*runing never occurs).
    if stem.ends_with("ie") {
        push("ying", format!("{}ying", &stem[..stem.len() - 2]));
    } else if stem.ends_with("ee") {
        push("ing", format!("{stem}ing"));
    } else if stem.ends_with('e') {
        push("drop-e-ing", format!("{}ing", &stem[..stem.len() - 1]));
    } else if doubles_final(stem) {
        let last = stem.chars().last().unwrap_or('?');
        push("dbl-ing", format!("{stem}{last}ing"));
    } else {
        push("ing", format!("{stem}ing"));
    }
    out
}

/// Expand EN static rows into synthetic inflection rows (load-time sidecar).
/// `vocab` is the full-stack norm-key set (`lower(word)\x1f lang`): forms
/// already present are never duplicated. Rows failing the config gates
/// (non-EN, non-alpha, too short, closed-class freq) yield nothing.
/// Derived rows carry `freq = max(1, stem_freq / discount_div)` with the
/// stem's own `cat`/`lang`/`priority`, encoded as ordinary non-explicit
/// rows (T9 of the surface form — exactly what a user typing the
/// inflection produces).
pub fn expand_entries(
    entries: &[DictEntry],
    vocab: &std::collections::HashSet<String>,
    cfg: &InflectConfig,
) -> Vec<DictEntry> {
    let div = cfg.discount_div.max(1);
    let mut out = Vec::new();
    // Forms minted in THIS batch must also suppress duplicates (two stems
    // can generate the same surface, e.g. `jogging` class cases).
    let mut minted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in entries {
        if e.lang != "en" || e.synthetic {
            continue;
        }
        let stem = e.word.to_lowercase();
        if stem.chars().count() < cfg.min_stem_len
            || !stem.chars().all(|c| c.is_ascii_lowercase())
            || e.freq >= cfg.max_stem_freq
        {
            continue;
        }
        let freq = (e.freq / div).max(1);
        for (_, form) in inflect_stem_en(&stem) {
            let key = format!("{form}\x1fen");
            if vocab.contains(&key) || !minted.insert(key) {
                continue;
            }
            if let Some(mut row) =
                DictEntry::precomputed(form, freq, e.cat.clone(), e.lang.clone(), e.priority)
            {
                // Synthetic marker: this row may be matched and scored, but
                // never serves as an expansion stem (no `*runneds`).
                row.synthetic = true;
                out.push(row);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_paradigms() {
        // walk: plain s/ed/ing.
        let f: Vec<String> = inflect_stem_en("walk").into_iter().map(|(_, f)| f).collect();
        for want in ["walks", "walked", "walking"] {
            assert!(f.contains(&want.to_string()), "walk -> {want:?} missing in {f:?}");
        }
        // run: plain s + doubled ed/ing.
        let f: Vec<String> = inflect_stem_en("run").into_iter().map(|(_, f)| f).collect();
        for want in ["runs", "running"] {
            assert!(f.contains(&want.to_string()), "run -> {want:?} missing in {f:?}");
        }
        assert!(!f.contains(&"runing".to_string()), "undoubled *runing must not generate: {f:?}");
        // make: silent-e verbs take +d/+s/drop-e-ing by the REGULAR rule
        // (`maked` is the regularized form — `made` is an irregular and a
        // vocab-coverage row, never generated here).
        let f: Vec<String> = inflect_stem_en("make").into_iter().map(|(_, f)| f).collect();
        for want in ["makes", "maked", "making"] {
            assert!(f.contains(&want.to_string()), "make -> {want:?} missing in {f:?}");
        }
        assert!(!f.contains(&"made".to_string()), "irregular made must not generate: {f:?}");
        // try: y -> ies/ied + plain ing.
        let f: Vec<String> = inflect_stem_en("try").into_iter().map(|(_, f)| f).collect();
        for want in ["tries", "tried", "trying"] {
            assert!(f.contains(&want.to_string()), "try -> {want:?} missing in {f:?}");
        }
        // fix/watch/box: -es.
        let f: Vec<String> = inflect_stem_en("watch").into_iter().map(|(_, f)| f).collect();
        assert!(f.contains(&"watches".to_string()), "{f:?}");
        assert!(!f.contains(&"watchs".to_string()), "{f:?}");
        // play (vowel+y): plain suffixes, never *playying.
        let f: Vec<String> = inflect_stem_en("play").into_iter().map(|(_, f)| f).collect();
        for want in ["plays", "played", "playing"] {
            assert!(f.contains(&want.to_string()), "play -> {want:?} missing in {f:?}");
        }
    }

    #[test]
    fn stem_never_self_generates_and_dedupes() {
        // `sing` yields `sing` via no rule (all rules lengthen), and no
        // surface appears twice even with dbl+plain pairs.
        for stem in ["sing", "run", "stop", "rain"] {
            let forms = inflect_stem_en(stem);
            assert!(!forms.iter().any(|(_, f)| f == stem), "{stem} regenerated itself");
            let mut seen = std::collections::HashSet::new();
            for (_, f) in &forms {
                assert!(seen.insert(f), "{stem} duplicated {f}");
            }
        }
    }

    #[test]
    fn closed_class_gate_is_frequency_not_wordlist() {
        // `the` (freq 1e6) is alpha and long enough — only the freq gate
        // stops `*thes`. No word list involved: a low-freq `the` WOULD
        // expand (assert the mechanism, not the instance).
        let cfg = InflectConfig::default();
        let vocab = std::collections::HashSet::new();
        let hi = DictEntry::precomputed("the".into(), 1_000_000, "words".into(), "en".into(), 0).unwrap();
        assert!(expand_entries(std::slice::from_ref(&hi), &vocab, &cfg).is_empty());
        let lo = DictEntry::precomputed("the".into(), 100, "words".into(), "en".into(), 0).unwrap();
        assert!(!expand_entries(std::slice::from_ref(&lo), &vocab, &cfg).is_empty());
    }

    #[test]
    fn discount_and_vocab_suppression() {
        let cfg = InflectConfig {
            discount_div: 8,
            ..InflectConfig::default()
        };
        // `run` 800 -> derived freq 100.
        let run = DictEntry::precomputed("run".into(), 800, "words".into(), "en".into(), 0).unwrap();
        let mut vocab = std::collections::HashSet::new();
        vocab.insert("run\x1fen".to_string());
        vocab.insert("runs\x1fen".to_string()); // already covered: never duped
        let rows = expand_entries(std::slice::from_ref(&run), &vocab, &cfg);
        let words: Vec<&str> = rows.iter().map(|r| r.word.as_str()).collect();
        assert!(!words.contains(&"run"), "{words:?}");
        assert!(!words.contains(&"runs"), "{words:?}");
        assert!(words.contains(&"running"), "{words:?}");
        assert!(rows.iter().all(|r| r.freq == 100), "{rows:?}");
        assert!(rows.iter().all(|r| r.cat == "words" && r.lang == "en"));
        // Non-EN rows never expand (NE morphology needs its own table).
        let ne = DictEntry::from_parts(
            "गर्नु".into(),
            None,
            Some("garnu".into()),
            vec![],
            800,
            "NE".into(),
            "ne".into(),
            10,
        )
        .unwrap();
        assert!(expand_entries(std::slice::from_ref(&ne), &vocab, &cfg).is_empty());
    }
}
