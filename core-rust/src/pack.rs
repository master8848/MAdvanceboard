//! Extension pack format: `packs/<id>.json`
//! `{id, title, version, words[]}` + loader.

use serde::{Deserialize, Serialize};

use crate::mapping::encode_word;
use crate::stack::DictEntry;

/// One word in a pack file. `freq` defaults to 100, `cat` defaults to
/// the pack id, `lang` defaults to `"en"`. Unknown fields (e.g. `tr`
/// romanization hints in the Nepali pack) are ignored.
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
}

impl PackFile {
    /// Convert pack words to dictionary entries at the given priority.
    /// Words that encode to an empty sequence are skipped. An explicit
    /// `seq` override (emoji/math packs) is used verbatim.
    pub fn to_entries(&self, priority: i32) -> Vec<DictEntry> {
        self.words
            .iter()
            .filter_map(|w| {
                let explicit = w.seq.is_some();
                let seq = w.seq.clone().unwrap_or_else(|| encode_word(&w.w));
                if seq.is_empty() {
                    return None;
                }
                Some(DictEntry {
                    word: w.w.clone(),
                    seq,
                    freq: w.freq,
                    cat: w.cat.clone().unwrap_or_else(|| self.id.clone()),
                    lang: w.lang.clone().unwrap_or_else(|| "en".to_string()),
                    priority,
                    explicit,
                })
            })
            .collect()
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
    fn rejects_invalid() {
        assert!(load_pack_str(r#"{"nope":true}"#).is_err());
    }
}
