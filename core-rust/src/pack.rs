//! Extension pack format: `packs/<id>.json`
//! `{id, title, version, words[]}` + loader.

use serde::{Deserialize, Serialize};

use crate::mapping::encode_word;
use crate::stack::DictEntry;

/// One word in a pack file. `freq` defaults to 100, `cat` defaults to
/// the pack id, `lang` defaults to `"en"`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackWord {
    pub w: String,
    #[serde(default = "default_freq")]
    pub freq: u64,
    #[serde(default)]
    pub cat: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    /// Optional explicit digit sequence. When present it is used as-is
    /// (needed for packs whose display word is not encodable latin text,
    /// e.g. emoji `{"w":"❤️","seq":"432","key":"hea"}` or math
    /// `{"w":"\\alpha","seq":"257","key":"alp"}` per SPEC section 1).
    #[serde(default)]
    pub seq: Option<String>,
    /// Roman transliteration hint (Nepali pack, plan/03): the primary
    /// search seq is `encode(tr)` while display stays Devanagari `w`,
    /// so Roman keystrokes (`k`->`5`-style T9) surface Devanagari words.
    /// Precedence: `seq` > `encode(tr)` > `encode(w)`.
    #[serde(default)]
    pub tr: Option<String>,
    /// Extra Roman spelling variants (e.g. `["paani"]` alongside
    /// `tr: "pani"`): each encodes to an additional match-any seq.
    #[serde(default)]
    pub alt: Vec<String>,
}

fn default_freq() -> u64 {
    100
}

/// Extension pack file: `packs/<id>.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackFile {
    pub id: String,
    pub title: String,
    pub version: String,
    pub words: Vec<PackWord>,
    /// Pack-declared layout affinity (e.g. a Nepali pack pinning
    /// `"t9-16"`). Stamped onto every [`DictEntry::layout_id`], which
    /// seeds the category default when the user has no explicit
    /// override (plan/02). `Some("")` is invalid and reported by
    /// [`Self::validate`]; unknown ids are reported by the
    /// registry-aware `Predictor::try_add_pack` path.
    #[serde(default)]
    pub layout: Option<String>,
}

impl PackFile {
    /// Declared affinity normalized (`Some("")` -> `None`; the empty
    /// case is a [`Self::validate`] error, never a silent override).
    pub fn affinity(&self) -> Option<&str> {
        self.layout.as_deref().filter(|l| !l.is_empty())
    }

    /// Convert pack words to dictionary entries at the given priority.
    /// Primary seq precedence: explicit `seq` verbatim, else `encode(tr)`,
    /// else `encode(w)`. Words with an empty primary seq are skipped;
    /// use [`Self::validate`] for explicit per-word diagnostics.
    /// `alt` variants and (when a `tr` primary exists) the distinct
    /// `encode(w)` skeleton become match-any `aliases`.
    /// The pack-level `layout` affinity is stamped onto every entry's
    /// `layout_id` (empty affinity normalizes to `None` here; loud
    /// failure lives in [`Self::validate`] + `Predictor::try_add_pack`).
    pub fn to_entries(&self, priority: i32) -> Vec<DictEntry> {
        let affinity = self.affinity().map(str::to_string);
        self.words
            .iter()
            .filter_map(|w| {
                let mut e = DictEntry::from_parts(
                    w.w.clone(),
                    w.seq.clone(),
                    w.tr.clone(),
                    w.alt.clone(),
                    w.freq,
                    w.cat.clone().unwrap_or_else(|| self.id.clone()),
                    w.lang.clone().unwrap_or_else(|| "en".to_string()),
                    priority,
                )?;
                e.layout_id = affinity.clone();
                Some(e)
            })
            .collect()
    }

    /// Strict loader-compat diagnostics: one error string per problem
    /// row (empty primary seq, empty `alt` variant encoding). Empty =
    /// every row loads. Callers that need loud failure (pack pipeline,
    /// gate harness) must surface these instead of silently shipping
    /// the skipped rows of [`Self::to_entries`].
    pub fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();
        if self.id.is_empty() {
            errs.push("pack id must not be empty".to_string());
        }
        if let Some(l) = &self.layout {
            if l.is_empty() {
                errs.push(format!(
                    "pack {:?}: layout affinity must not be empty (omit the field for no affinity)",
                    self.id
                ));
            }
        }
        for (i, w) in self.words.iter().enumerate() {
            let primary = match (&w.seq, &w.tr) {
                (Some(s), _) => s.clone(),
                (None, Some(tr)) => encode_word(tr),
                (None, None) => encode_word(&w.w),
            };
            if primary.is_empty() {
                errs.push(format!(
                    "word[{i}] {:?}: empty primary seq (w unencodable, no usable seq/tr)",
                    w.w
                ));
            }
            for a in &w.alt {
                if encode_word(a).is_empty() {
                    errs.push(format!(
                        "word[{i}] {:?}: alt variant {a:?} encodes to empty seq",
                        w.w
                    ));
                }
            }
            if let Some(tr) = &w.tr {
                if w.seq.is_none() && encode_word(tr).is_empty() {
                    errs.push(format!(
                        "word[{i}] {:?}: tr {tr:?} encodes to empty seq",
                        w.w
                    ));
                }
            }
        }
        errs
    }
}

/// Load a pack from a JSON string.
pub fn load_pack_str(s: &str) -> Result<PackFile, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

/// Load a pack from `packs/<id>.json`-style file path.
pub fn load_pack(path: &std::path::Path) -> Result<PackFile, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    load_pack_str(&s)
}

// ---- User custom categories (names-style packs, plan/08) ----

/// User-pack priority window (plan/08): base `0` fixed bottom, personal
/// `100` fixed top, extensions float `10-90` user-ordered.
pub const USER_PRIORITY_MIN: i32 = 10;
/// See [`USER_PRIORITY_MIN`].
pub const USER_PRIORITY_MAX: i32 = 90;

/// Category ids a user pack must not claim (built-in pack cats +
/// the personal overlay).
pub const RESERVED_CATS: &[&str] = &[
    "words", "ne", "numbers", "js", "rust", "html", "emoji", "math", "medical", "personal",
];

/// User wordlist frequency window (matches `docs/VOCAB_SDK.md` F3).
pub const USER_FREQ_MAX: u64 = 1_000_000;

/// Check an extension priority sits in the user window `10-90`.
/// Loud `Err`, never a silent clamp (plan/08 reorder contract).
pub fn check_user_priority(priority: i32) -> Result<(), String> {
    if (USER_PRIORITY_MIN..=USER_PRIORITY_MAX).contains(&priority) {
        Ok(())
    } else {
        Err(format!(
            "user pack priority {priority} out of range {USER_PRIORITY_MIN}-{USER_PRIORITY_MAX} \
             (base 0 and personal 100 are fixed)"
        ))
    }
}

/// Validate a user-chosen category id: `^[a-z][a-z0-9_]{1,23}$` and not
/// reserved. Loud `Err`, never a silent rename.
pub fn validate_user_id(id: &str) -> Result<(), String> {
    let mut chars = id.chars();
    let ok = id.len() >= 2
        && id.len() <= 24
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !ok {
        return Err(format!(
            "user category id {id:?} invalid: use 2-24 chars of a-z, 0-9, _ starting with a letter"
        ));
    }
    if RESERVED_CATS.contains(&id) {
        return Err(format!(
            "user category id {id:?} invalid: reserved by a built-in pack (pick another id)"
        ));
    }
    Ok(())
}

/// Parse pasted/imported wordlist text: one `word [freq]` per line.
/// Blank lines and `#` comment lines are skipped. Returns the rows on
/// success, or the exact per-row errors on failure — never a partial
/// silent drop. Every row is stamped `cat`/`lang` for the new category.
pub fn parse_user_wordlist(
    text: &str,
    cat: &str,
    lang: &str,
) -> Result<Vec<PackWord>, Vec<String>> {
    let mut words = Vec::new();
    let mut errs = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let w = parts.next().unwrap_or("");
        if w.is_empty() {
            errs.push(format!("line {line_no}: missing word"));
            continue;
        }
        let freq = match parts.next() {
            None => default_freq(),
            Some(f) => match f.parse::<u64>() {
                Ok(n) if n >= 1 && n <= USER_FREQ_MAX => n,
                _ => {
                    errs.push(format!(
                        "line {line_no} {w:?}: freq {f:?} out of range 1-{USER_FREQ_MAX}"
                    ));
                    continue;
                }
            },
        };
        if parts.next().is_some() {
            errs.push(format!(
                "line {line_no} {w:?}: expected `word [freq]`, trailing text rejected"
            ));
            continue;
        }
        let key = format!("{}\x1f{}", w.to_lowercase(), lang);
        if !seen.insert(key) {
            errs.push(format!("line {line_no} {w:?}: duplicate word"));
            continue;
        }
        if encode_word(w).is_empty() {
            errs.push(format!(
                "line {line_no} {w:?}: encodes to empty seq (latin letters/digits only in wordlists)"
            ));
            continue;
        }
        words.push(PackWord {
            w: w.to_string(),
            freq,
            cat: Some(cat.to_string()),
            lang: Some(lang.to_string()),
            seq: None,
            tr: None,
            alt: Vec::new(),
        });
    }
    if words.is_empty() && errs.is_empty() {
        errs.push("wordlist is empty: add one `word [freq]` per line".to_string());
    }
    if errs.is_empty() { Ok(words) } else { Err(errs) }
}

/// Build a first-class user pack from an id, title, and wordlist text.
/// Fails loudly (joined per-row diagnostics) so a half-loaded category
/// never ships silently.
pub fn build_user_pack(id: &str, title: &str, text: &str, lang: &str) -> Result<PackFile, String> {
    validate_user_id(id)?;
    if title.trim().is_empty() {
        return Err("user pack title must not be empty".to_string());
    }
    let words = parse_user_wordlist(text, id, lang)
        .map_err(|errs| format!("user pack {id:?} invalid ({} problem(s)): {}", errs.len(), errs.join("; ")))?;
    let pack = PackFile {
        id: id.to_string(),
        title: title.to_string(),
        version: "1.0.0".to_string(),
        words,
        layout: None,
    };
    let problems = pack.validate();
    if !problems.is_empty() {
        return Err(format!(
            "user pack {id:?} invalid ({} problem(s)): {}",
            problems.len(),
            problems.join("; ")
        ));
    }
    Ok(pack)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_encodes() {
        let pack = load_pack_str(
            r#"{"id":"words","title":"English","version":"1.0.0",
                "words":[{"w":"hello","freq":900},{"w":"hi"}]}"#,
        )
        .unwrap();
        let entries = pack.to_entries(0);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq, "43556");
        assert_eq!(entries[1].cat, "words");
    }

    #[test]
    fn seq_override_for_emoji_math() {
        let pack = load_pack_str(
            r#"{"id":"emoji","title":"Emoji","version":"1.0.0",
                "words":[{"w":"❤️","seq":"432","freq":900,"cat":"emoji"}]}"#,
        )
        .unwrap();
        let entries = pack.to_entries(10);
        assert_eq!(entries[0].seq, "432");
    }

    #[test]
    fn tr_is_primary_seq_display_stays_devanagari() {
        // plan/03: `seq || encode(tr) || encode(w)`.
        let pack = load_pack_str(
            r#"{"id":"ne","title":"Nepali","version":"1.0.0",
                "words":[{"w":"नमस्ते","tr":"namaste","freq":9000,"cat":"NE","lang":"ne"}]}"#,
        )
        .unwrap();
        let entries = pack.to_entries(0);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.word, "नमस्ते");
        assert_eq!(e.seq, encode_word("namaste"));
        assert_ne!(e.seq, encode_word("नमस्ते"));
        assert_eq!(e.tr.as_deref(), Some("namaste"));
        // Skeleton fallback alias keeps direct-Devanagari digits working.
        assert!(e.aliases.contains(&encode_word("नमस्ते")));
    }

    #[test]
    fn explicit_seq_wins_over_tr() {
        let pack = load_pack_str(
            r#"{"id":"x","title":"X","version":"1.0.0",
                "words":[{"w":"नमस्ते","seq":"123","tr":"namaste"}]}"#,
        )
        .unwrap();
        let entries = pack.to_entries(0);
        assert_eq!(entries[0].seq, "123");
        assert!(entries[0].explicit);
    }

    #[test]
    fn alt_variants_become_aliases() {
        let pack = load_pack_str(
            r#"{"id":"ne","title":"Nepali","version":"1.0.0",
                "words":[{"w":"पानी","tr":"pani","alt":["paani"],"lang":"ne"}]}"#,
        )
        .unwrap();
        let entries = pack.to_entries(0);
        assert_eq!(entries[0].seq, encode_word("pani"));
        assert!(entries[0].aliases.contains(&encode_word("paani")));
        assert!(!entries[0].aliases.contains(&entries[0].seq.clone()));
    }

    #[test]
    fn validate_reports_bad_rows() {
        // Unencodable word with no seq/tr, plus an empty alt variant.
        let pack = load_pack_str(
            r#"{"id":"x","title":"X","version":"1.0.0",
                "words":[{"w":"*"},{"w":"ok","alt":["*"]}]}"#,
        )
        .unwrap();
        let errs = pack.validate();
        assert_eq!(errs.len(), 2, "want one error per bad row, got {errs:?}");
        assert!(errs[0].contains("\"*\""));
        // Clean packs validate silently.
        let good = load_pack_str(
            r#"{"id":"ne","title":"N","version":"1.0.0",
                "words":[{"w":"नमस्ते","tr":"namaste","lang":"ne"}]}"#,
        )
        .unwrap();
        assert!(good.validate().is_empty());
        assert_eq!(good.to_entries(0).len(), 1);
    }

    #[test]
    fn rejects_invalid() {
        assert!(load_pack_str(r#"{"nope":true}"#).is_err());
    }

    #[test]
    fn user_id_rules_are_loud() {
        assert!(validate_user_id("names").is_ok());
        assert!(validate_user_id("my_names2").is_ok());
        for bad in ["", "N", "names!", "1names", "a", "names with space", "UPPER"] {
            assert!(validate_user_id(bad).is_err(), "id {bad:?} must fail");
        }
        for reserved in ["words", "ne", "personal", "medical"] {
            let err = validate_user_id(reserved).unwrap_err();
            assert!(err.contains("reserved"), "unexpected: {err}");
        }
    }

    #[test]
    fn user_priority_window_10_to_90() {
        assert!(check_user_priority(10).is_ok());
        assert!(check_user_priority(50).is_ok());
        assert!(check_user_priority(90).is_ok());
        for bad in [0, 9, 91, 100, -1] {
            assert!(check_user_priority(bad).is_err(), "priority {bad} must fail");
        }
    }

    #[test]
    fn user_wordlist_parses_word_freq_lines() {
        let words = parse_user_wordlist("# contacts\nava 9000\nbob\n", "names", "en").unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].w, "ava");
        assert_eq!(words[0].freq, 9000);
        assert_eq!(words[0].cat.as_deref(), Some("names"));
        assert_eq!(words[1].freq, 100);
    }

    #[test]
    fn user_wordlist_bad_rows_fail_loudly_never_silently() {
        let err = parse_user_wordlist("ava 9000\nava 100\nbob 0\n*\nbob 100 extra\n", "names", "en")
            .unwrap_err();
        assert_eq!(err.len(), 4, "one error per bad row, got {err:?}");
        assert!(err.iter().any(|e| e.contains("duplicate")), "{err:?}");
        assert!(err.iter().any(|e| e.contains("out of range")), "{err:?}");
        assert!(err.iter().any(|e| e.contains("empty seq")), "{err:?}");
        assert!(err.iter().any(|e| e.contains("trailing text")), "{err:?}");
        assert!(parse_user_wordlist("", "names", "en").unwrap_err()[0].contains("empty"));
    }

    #[test]
    fn build_user_pack_end_to_end() {
        let pack = build_user_pack("names", "Names", "ava 9000\nbob 8000\n", "en").unwrap();
        assert_eq!(pack.id, "names");
        assert!(pack.validate().is_empty());
        assert_eq!(pack.to_entries(50).len(), 2);
        assert!(build_user_pack("words", "Taken", "ava\n", "en").is_err());
        assert!(build_user_pack("names", "", "ava\n", "en").is_err());
        assert!(build_user_pack("names", "Names", "ava 0\n", "en").is_err());
    }
}
