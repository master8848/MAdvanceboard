//! Flexible multi-layout engine: `LayoutSpec` registry + `KeyMapping` trait.
//!
//! Single source of truth is `layouts/*.json` (checked in at the repo root
//! and embedded at compile time via `include_str!`). Built-ins:
//!
//! - `t9-9` — classic 9-key T9, frozen reference behavior identical to
//!   `mapping.rs` (verified by `t9_parity_with_mapping`).
//! - `t9-12` — same 3x4 geometry; bottom row is first-class `Sym` (`*`
//!   emits for extra symbol chars) / `Space` (`0`) / `Delete` (`#`, control).
//! - `t9-16` — 4x4 grid with finer splits (`pq`/`rs`, `wx`+space/`yz`);
//!   extra codes `A` (yz) and `B` (symbols) are layout-local.
//!
//! Encoding rules (all layouts): ASCII digits pass through verbatim and own
//! their codes, so no key may claim a digit in `symbols`. All other chars
//! map via the first key (in file order) whose `symbols` contain the char,
//! falling back to its lowercase form. Control-role keys carry empty
//! `symbols` and therefore never emit (e.g. `Delete`).
//!
//! `mapping.rs` stays as the frozen `t9-9` reference implementation and the
//! emergency fallback (`t9_fallback_spec`) if the embedded JSON ever fails
//! to parse.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Default layout; all back-compat paths (`suggest`, `learn`, …) use it.
pub const DEFAULT_LAYOUT_ID: &str = "t9-9";

/// One physical key: the `code` it emits, its display `label`, the
/// characters that encode to it, its UI `role`
/// (`text` | `space` | `sym` | `delete` | `control`) and its grid position.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutKey {
    pub code: char,
    pub label: String,
    pub symbols: String,
    pub role: String,
    pub row: i32,
    pub col: i32,
}

/// Full key map + adjacency (fat-finger neighbor) graph for one pad.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutSpec {
    pub id: String,
    pub title: String,
    pub cols: i32,
    pub rows: i32,
    pub keys: Vec<LayoutKey>,
    /// Neighbor codes per code (symmetric, never containing the code itself).
    pub adjacency: HashMap<char, Vec<char>>,
    /// Precomputed char -> code map (first key wins). Rebuilt on load.
    #[serde(skip, default = "HashMap::new")]
    char_to_code: HashMap<char, char>,
}

// Raw file shapes use single-char strings so malformed JSON yields a clear
// validation error instead of a serde type error.
#[derive(Deserialize)]
struct RawGrid {
    cols: i32,
    rows: i32,
}

#[derive(Deserialize)]
struct RawKey {
    code: String,
    label: String,
    #[serde(default)]
    symbols: String,
    #[serde(default)]
    role: String,
    row: i32,
    col: i32,
}

#[derive(Deserialize)]
struct RawSpec {
    id: String,
    #[serde(default)]
    title: String,
    grid: RawGrid,
    keys: Vec<RawKey>,
    adjacency: HashMap<String, Vec<String>>,
}

/// Per-layout encoder/neighbor behavior. Implemented by `LayoutSpec`
/// (data-driven, from JSON) and by `mapping::T9Mapping` (frozen reference).
pub trait KeyMapping {
    fn layout_id(&self) -> &str;
    /// Word -> sequence under this layout (unmapped chars skipped).
    fn encode_word(&self, word: &str) -> String;
    /// True if two codes are distinct neighboring keys.
    fn neighbors(&self, a: char, b: char) -> bool;
    fn key_label(&self, code: char) -> &str;
    /// True if `c` is a code this layout can emit or match on.
    fn contains_code(&self, c: char) -> bool;
    /// Exactly-one substitution to an adjacent key (fat-finger fuzzy).
    fn is_one_edit_neighbor(&self, seq: &str, candidate_seq: &str) -> bool {
        if seq.len() != candidate_seq.len() {
            return false;
        }
        let mut edits = 0;
        for (a, b) in seq.chars().zip(candidate_seq.chars()) {
            if a == b {
                continue;
            }
            if !self.neighbors(a, b) {
                return false;
            }
            edits += 1;
            if edits > 1 {
                return false;
            }
        }
        edits == 1
    }
}

fn one_char(s: &str, what: &str) -> Result<char, String> {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Ok(c),
        _ => Err(format!("{what} must be a single character, got {s:?}")),
    }
}

impl LayoutSpec {
    /// Validate raw parts and build the encode map.
    pub fn assemble(
        id: String,
        title: String,
        cols: i32,
        rows: i32,
        keys: Vec<LayoutKey>,
        adjacency: HashMap<char, Vec<char>>,
    ) -> Result<Self, String> {
        if id.is_empty() {
            return Err("layout id must not be empty".to_string());
        }
        if keys.is_empty() {
            return Err(format!("layout {id}: no keys"));
        }
        if cols <= 0 || rows <= 0 {
            return Err(format!("layout {id}: bad grid {cols}x{rows}"));
        }
        let mut seen_code = std::collections::HashSet::new();
        let mut seen_pos = std::collections::HashSet::new();
        for k in &keys {
            if !seen_code.insert(k.code) {
                return Err(format!("layout {id}: duplicate code {:?}", k.code));
            }
            if !(0 <= k.row && k.row < rows && 0 <= k.col && k.col < cols) {
                return Err(format!(
                    "layout {id}: key {:?} out of grid at ({},{})",
                    k.code, k.row, k.col
                ));
            }
            if !seen_pos.insert((k.row, k.col)) {
                return Err(format!(
                    "layout {id}: duplicate position ({},{})",
                    k.row, k.col
                ));
            }
            for ch in k.symbols.chars() {
                if ch.is_ascii_digit() {
                    return Err(format!(
                        "layout {id}: key {:?} must not claim digit {ch:?} \
                         (digits pass through)",
                        k.code
                    ));
                }
            }
        }
        for (a, ns) in &adjacency {
            if !seen_code.contains(a) {
                return Err(format!("layout {id}: adjacency for unknown code {a:?}"));
            }
            for b in ns {
                if b == a {
                    return Err(format!("layout {id}: self-adjacency {a:?}"));
                }
                if !seen_code.contains(b) {
                    return Err(format!("layout {id}: adjacency {a:?} -> unknown {b:?}"));
                }
                if !adjacency.get(b).map(|v| v.contains(a)).unwrap_or(false) {
                    return Err(format!("layout {id}: asymmetric adjacency {a:?} -> {b:?}"));
                }
            }
        }
        for code in seen_code.iter() {
            if !adjacency.contains_key(code) {
                return Err(format!("layout {id}: missing adjacency for {code:?}"));
            }
        }
        let mut char_to_code = HashMap::new();
        for k in &keys {
            for ch in k.symbols.chars() {
                char_to_code.entry(ch).or_insert(k.code);
            }
        }
        Ok(Self {
            id,
            title,
            cols,
            rows,
            keys,
            adjacency,
            char_to_code,
        })
    }

    /// Parse a `layouts/*.json` document.
    pub fn from_json(s: &str) -> Result<Self, String> {
        let raw: RawSpec = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let mut keys = Vec::with_capacity(raw.keys.len());
        for k in raw.keys {
            keys.push(LayoutKey {
                code: one_char(&k.code, "key code")?,
                label: k.label,
                symbols: k.symbols,
                role: k.role,
                row: k.row,
                col: k.col,
            });
        }
        let mut adjacency = HashMap::with_capacity(raw.adjacency.len());
        for (a, ns) in raw.adjacency {
            let a = one_char(&a, "adjacency key")?;
            let mut v = Vec::with_capacity(ns.len());
            for b in ns {
                v.push(one_char(&b, "adjacency neighbor")?);
            }
            adjacency.insert(a, v);
        }
        Self::assemble(raw.id, raw.title, raw.grid.cols, raw.grid.rows, keys, adjacency)
    }

    fn map_char(&self, c: char) -> Option<char> {
        if c.is_ascii_digit() {
            return Some(c);
        }
        if let Some(&d) = self.char_to_code.get(&c) {
            return Some(d);
        }
        let lower = c.to_lowercase().next().unwrap_or(c);
        if lower != c {
            if let Some(&d) = self.char_to_code.get(&lower) {
                return Some(d);
            }
        }
        None
    }
}

impl KeyMapping for LayoutSpec {
    fn layout_id(&self) -> &str {
        &self.id
    }

    fn encode_word(&self, word: &str) -> String {
        word.chars().filter_map(|c| self.map_char(c)).collect()
    }

    fn neighbors(&self, a: char, b: char) -> bool {
        if a == b {
            return false;
        }
        self.adjacency.get(&a).map(|v| v.contains(&b)).unwrap_or(false)
    }

    fn key_label(&self, code: char) -> &str {
        self.keys
            .iter()
            .find(|k| k.code == code)
            .map(|k| k.label.as_str())
            .unwrap_or("?")
    }

    fn contains_code(&self, c: char) -> bool {
        self.keys.iter().any(|k| k.code == c)
    }
}

/// Runtime registry of layout specs. Custom layouts register from a
/// `layouts/*.json` document; unknown ids resolve to the `t9-9` default.
#[derive(Clone, Debug, Default)]
pub struct LayoutRegistry {
    specs: HashMap<String, LayoutSpec>,
}

impl LayoutRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Built-ins embedded from `layouts/*.json` at compile time.
    /// `mapping.rs` stays as the fallback if a document fails to parse.
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        for src in [
            include_str!("../../layouts/t9-9.json"),
            include_str!("../../layouts/t9-12.json"),
            include_str!("../../layouts/t9-16.json"),
        ] {
            if let Ok(spec) = LayoutSpec::from_json(src) {
                r.register(spec);
            }
        }
        if !r.specs.contains_key(DEFAULT_LAYOUT_ID) {
            r.register(crate::mapping::t9_fallback_spec());
        }
        r
    }

    pub fn register(&mut self, spec: LayoutSpec) {
        self.specs.insert(spec.id.clone(), spec);
    }

    /// Parse and register a `layouts/*.json` document at runtime.
    pub fn register_json(&mut self, s: &str) -> Result<String, String> {
        let spec = LayoutSpec::from_json(s)?;
        let id = spec.id.clone();
        self.register(spec);
        Ok(id)
    }

    pub fn get(&self, id: &str) -> Option<&LayoutSpec> {
        self.specs.get(id)
    }

    /// Resolve `id`, falling back to `t9-9` (empty/unknown ids included).
    pub fn get_or_default(&self, id: &str) -> &LayoutSpec {
        self.specs
            .get(id)
            .or_else(|| self.specs.get(DEFAULT_LAYOUT_ID))
            .expect("registry always holds the default layout")
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.specs.keys().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::{self, T9Mapping};

    fn registry() -> LayoutRegistry {
        LayoutRegistry::with_builtins()
    }

    #[test]
    fn builtin_ids_present() {
        let ids = registry().ids();
        assert!(ids.contains(&"t9-9".to_string()));
        assert!(ids.contains(&"t9-12".to_string()));
        assert!(ids.contains(&"t9-16".to_string()));
    }

    #[test]
    fn t9_builtin_encode_hello_fun() {
        let r = registry();
        let t9 = r.get_or_default("t9-9");
        assert_eq!(t9.encode_word("hello"), "43556");
        assert_eq!(t9.encode_word("fun"), "386");
        assert_eq!(t9.encode_word("Hello"), t9.encode_word("hello"));
        assert_eq!(t9.encode_word("a b"), "202");
        assert_eq!(t9.encode_word("hi!"), "441");
        assert_eq!(t9.encode_word("कमल"), "267");
    }

    #[test]
    fn t9_parity_with_mapping() {
        let t9 = registry().get_or_default("t9-9").clone();
        let m = T9Mapping;
        for w in [
            "hello", "fun", "Hello", "a b", "hi!", "a1", "a*c", "कमल",
            "serendipity", "user@host", "QoS", "C++", "e-mail", "don't",
        ] {
            assert_eq!(t9.encode_word(w), m.encode_word(w), "word {w:?}");
        }
        for (a, b) in [('2', '3'), ('2', '5'), ('5', '9'), ('2', '2'), ('2', '9'), ('1', '9')] {
            assert_eq!(t9.neighbors(a, b), mapping::is_neighbor(a, b), "{a}{b}");
        }
        for c in ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '*', '#'] {
            assert_eq!(t9.key_label(c), mapping::key_label(c));
        }
    }

    #[test]
    fn t12_sym_space_delete_semantics() {
        let r = registry();
        let t12 = r.get_or_default("t9-12");
        assert_eq!(t12.encode_word("hello"), "43556");
        assert_eq!(t12.encode_word("fun"), "386");
        // Extra symbol coverage emits the Sym key code.
        assert_eq!(t12.encode_word("a@b"), "2*2");
        // Space and punct match the frozen reference.
        assert_eq!(t12.encode_word("a b"), "202");
        assert_eq!(t12.encode_word("hi!"), "441");
        // Delete is a control: skipped, never emitted.
        assert_eq!(t12.encode_word("#"), "");
        assert_eq!(t12.key_label('*'), "sym");
        assert!(t12.neighbors('*', '0'));
        assert!(t12.contains_code('#'));
    }

    #[test]
    fn t16_finer_splits() {
        let r = registry();
        let t16 = r.get_or_default("t9-16");
        // Unsplit keys agree with the reference …
        assert_eq!(t16.encode_word("hello"), "43556");
        // … split keys diverge: pq|rs, wx|yz.
        assert_eq!(t16.encode_word("pqrs"), "7788");
        assert_eq!(t16.encode_word("wxyz"), "00AA");
        assert_eq!(t16.encode_word("fun"), "396");
        assert!(t16.contains_code('A'));
        assert!(t16.contains_code('B'));
        // 4x4 geometry: 2 and 7 are adjacent here, not on the 3x4 pads.
        assert!(t16.neighbors('2', '7'));
        assert!(!r.get_or_default("t9-9").neighbors('2', '7'));
    }

    #[test]
    fn unknown_layout_resolves_to_default() {
        let r = registry();
        assert_eq!(r.get_or_default("nope").layout_id(), "t9-9");
        assert_eq!(r.get_or_default("").layout_id(), "t9-9");
    }

    #[test]
    fn custom_layout_registers_from_json() {
        let mut r = registry();
        let id = r
            .register_json(
                r##"{"id":"mini-4","title":"Mini","grid":{"cols":2,"rows":2},
                    "keys":[{"code":"1","label":"ab","symbols":"abAB","role":"text","row":0,"col":0},
                            {"code":"2","label":"cd","symbols":"cdCD","role":"text","row":0,"col":1},
                            {"code":"*","label":"sym","symbols":"","role":"control","row":1,"col":0},
                            {"code":"#","label":"del","symbols":"","role":"delete","row":1,"col":1}],
                    "adjacency":{"1":["2","*","#"],"2":["1","*","#"],
                                 "*":["1","2","#"],"#":["1","2","*"]}}"##,
            )
            .unwrap();
        assert_eq!(id, "mini-4");
        assert!(r.ids().contains(&"mini-4".to_string()));
        assert_eq!(r.get_or_default("mini-4").encode_word("bad"), "112");
    }

    #[test]
    fn rejects_bad_documents() {
        assert!(LayoutSpec::from_json("{}").is_err());
        // Asymmetric adjacency.
        assert!(LayoutSpec::from_json(
            r#"{"id":"x","title":"X","grid":{"cols":1,"rows":2},
                "keys":[{"code":"1","label":"a","symbols":"a","role":"text","row":0,"col":0},
                        {"code":"2","label":"b","symbols":"b","role":"text","row":1,"col":0}],
                "adjacency":{"1":["2"],"2":[]}}"#,
        )
        .is_err());
        // Digits are reserved for passthrough.
        assert!(LayoutSpec::from_json(
            r#"{"id":"x","title":"X","grid":{"cols":1,"rows":1},
                "keys":[{"code":"1","label":"a","symbols":"a1","role":"text","row":0,"col":0}],
                "adjacency":{"1":[]}}"#,
        )
        .is_err());
    }
}
