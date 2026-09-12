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
}
