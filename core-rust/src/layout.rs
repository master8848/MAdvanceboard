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
    /// Interned code set for O(1) `contains_code` (rebuilt on load; the
    /// old linear `keys.iter().any()` scan ran per input char).
    #[serde(skip, default = "std::collections::HashSet::new")]
    code_set: std::collections::HashSet<char>,
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
    /// Allocation-free encode into a caller-owned buffer (overridden by
    /// `LayoutSpec`; the default forwards through [`Self::encode_word`]).
    fn encode_word_into(&self, word: &str, buf: &mut String) {
        buf.clear();
        buf.push_str(&self.encode_word(word));
    }
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
    /// Neighbor codes for `code` (distinct adjacent keys; empty when the
    /// code is unknown). Backs generative 1-edit expansion (plan/05 #2):
    /// `len × ~8` exact posting lookups instead of an O(N·L) per-entry
    /// `is_one_edit_neighbor` scan. Order is layout-local and carries no
    /// semantics — matches merge by (row, best keyfit), so output never
    /// depends on it.
    fn neighbor_codes(&self, code: char) -> Vec<char>;
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
        let code_set = keys.iter().map(|k| k.code).collect();
        Ok(Self {
            id,
            title,
            cols,
            rows,
            keys,
            adjacency,
            char_to_code,
            code_set,
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

    /// Encode `word` appending into `buf` without an intermediate `String`
    /// allocation. Callers reuse the buffer across entries/keystrokes.
    pub fn encode_word_into(&self, word: &str, buf: &mut String) {
        buf.clear();
        buf.reserve(word.len());
        for c in word.chars() {
            if let Some(d) = self.map_char(c) {
                buf.push(d);
            }
        }
    }

}

impl KeyMapping for LayoutSpec {
    fn layout_id(&self) -> &str {
        &self.id
    }

    fn encode_word(&self, word: &str) -> String {
        word.chars().filter_map(|c| self.map_char(c)).collect()
    }

    fn encode_word_into(&self, word: &str, buf: &mut String) {
        LayoutSpec::encode_word_into(self, word, buf);
    }

    fn neighbors(&self, a: char, b: char) -> bool {
        if a == b {
            return false;
        }
        self.adjacency.get(&a).map(|v| v.contains(&b)).unwrap_or(false)
    }

    fn neighbor_codes(&self, code: char) -> Vec<char> {
        self.adjacency.get(&code).cloned().unwrap_or_default()
    }

    fn key_label(&self, code: char) -> &str {
        self.keys
            .iter()
            .find(|k| k.code == code)
            .map(|k| k.label.as_str())
            .unwrap_or("?")
    }

    fn contains_code(&self, c: char) -> bool {
        self.code_set.contains(&c)
    }
}

/// Runtime registry of layout specs plus the global default and the
/// per-category override map (`plan/02-layout-global-percat.md`).
///
/// Resolution order per keystroke: `cat_map[cat]` -> `default_id` ->
/// [`DEFAULT_LAYOUT_ID`] (`t9-9`). Fallbacks still resolve to `t9-9` but
/// the `_report` variants surface an explicit diagnostic string instead
/// of failing silently; the plain `resolve`/`get_or_default` wrappers are
/// kept for back-compat call sites that cannot surface errors.
#[derive(Clone, Debug)]
pub struct LayoutRegistry {
    specs: HashMap<String, LayoutSpec>,
    default_id: String,
    cat_map: HashMap<String, String>,
}

impl Default for LayoutRegistry {
    fn default() -> Self {
        Self {
            specs: HashMap::new(),
            default_id: DEFAULT_LAYOUT_ID.to_string(),
            cat_map: HashMap::new(),
        }
    }
}

/// Compiled-in category defaults (plan/02 + MASTER global rule 3:
/// NE defaults to `t9-9`).
///
/// The `tr`-model (plan/03) is landed: Nepali packs carry Roman `tr` seqs
/// (8005-row `w+tr` pack, v2.0.1), so Roman keystrokes match under the
/// shared Latin `t9-9` code set and the finer `t9-16` Latin splits buy no
/// accuracy — only a second pad to maintain. `t9-16` stays registered and
/// opt-in (user override or pack affinity), but out of the box every tab
/// — including NE/`ne` — resolves through the global default (`t9-9`).
/// Mirrored in `layouts/cat_map.json` (empty `cats`: no pinned override).
const BUILTIN_CAT_DEFAULTS: &[(&str, &str)] = &[];

impl LayoutRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process-wide cached built-ins for precompute paths (insert-time
    /// encoding under all three pads without re-parsing JSON per word).
    pub fn builtins_cached() -> &'static LayoutRegistry {
        static CELL: std::sync::OnceLock<LayoutRegistry> = std::sync::OnceLock::new();
        CELL.get_or_init(LayoutRegistry::with_builtins)
    }

    /// Built-ins embedded from `layouts/*.json` at compile time.
    /// `mapping.rs` stays as the fallback if a document fails to parse.
    /// No pinned per-tab default out of the box: every tab (including
    /// NE/`ne`) resolves through the global default (`t9-9`, MASTER rule
    /// 3). Additional compiled defaults belong in
    /// [`BUILTIN_CAT_DEFAULTS`] (currently empty).
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
        for (cat, layout) in BUILTIN_CAT_DEFAULTS {
            // Only known-good pairs are seeded; a typo here must panic at
            // startup rather than ship a silently-ignored default.
            r.set_cat_layout(cat, layout).unwrap_or_else(|e| {
                panic!("LayoutRegistry::with_builtins: built-in cat default invalid: {e}")
            });
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

    /// Resolve `id`, falling back to the global default then `t9-9`
    /// (empty/unknown ids included). Back-compat wrapper: prefer
    /// [`Self::get_or_default_report`] when the caller can surface the
    /// diagnostic.
    pub fn get_or_default(&self, id: &str) -> &LayoutSpec {
        self.get_or_default_report(id).0
    }

    /// Resolve `id` with an explicit diagnostic: `None` on a clean hit,
    /// `Some(context)` whenever an empty/unknown id (or a broken global
    /// default) forced a fallback to `t9-9`.
    pub fn get_or_default_report(&self, id: &str) -> (&LayoutSpec, Option<String>) {
        if id.is_empty() {
            let fb = self.default_spec();
            return (
                fb,
                Some(format!(
                    "LayoutRegistry::resolve: empty layout id; fell back to {:?}",
                    fb.id
                )),
            );
        }
        if let Some(s) = self.specs.get(id) {
            return (s, None);
        }
        let fb = self.default_spec();
        (
            fb,
            Some(format!(
                "LayoutRegistry::resolve: unknown layout id {id:?} (known: {:?}); fell back to {:?}",
                self.ids(),
                fb.id
            )),
        )
    }

    /// The global default spec (falls back to `t9-9` when `default_id`
    /// itself is unset; `with_builtins` always holds `t9-9`).
    fn default_spec(&self) -> &LayoutSpec {
        self.specs
            .get(&self.default_id)
            .or_else(|| self.specs.get(DEFAULT_LAYOUT_ID))
            .expect("registry always holds the default layout")
    }

    /// Current global default layout id.
    pub fn default_id(&self) -> &str {
        &self.default_id
    }

    /// Layout id selected for `cat` before spec resolution
    /// (`cat_map[cat]` or the global default).
    pub fn cat_layout_id(&self, cat: &str) -> &str {
        self.cat_map.get(cat).map(String::as_str).unwrap_or(&self.default_id)
    }

    /// Per-category override map (clone for inspection/tests).
    pub fn cat_map(&self) -> HashMap<String, String> {
        self.cat_map.clone()
    }

    /// Set the global default. Explicit error on empty/unknown ids;
    /// the previous default is kept unchanged on failure.
    pub fn set_default_layout(&mut self, id: &str) -> Result<(), String> {
        if id.is_empty() {
            return Err("LayoutRegistry::set_default_layout: layout id must not be empty".to_string());
        }
        if !self.specs.contains_key(id) {
            return Err(format!(
                "LayoutRegistry::set_default_layout: unknown layout id {id:?} (known: {:?})",
                self.ids()
            ));
        }
        self.default_id = id.to_string();
        Ok(())
    }

    /// Set a per-category override. Explicit error on empty cat or
    /// empty/unknown layout id; the previous mapping is kept on failure.
    pub fn set_cat_layout(&mut self, cat: &str, layout_id: &str) -> Result<(), String> {
        if cat.is_empty() {
            return Err("LayoutRegistry::set_cat_layout: cat must not be empty".to_string());
        }
        if layout_id.is_empty() {
            return Err(format!(
                "LayoutRegistry::set_cat_layout: layout id for cat {cat:?} must not be empty"
            ));
        }
        if !self.specs.contains_key(layout_id) {
            return Err(format!(
                "LayoutRegistry::set_cat_layout: unknown layout id {layout_id:?} for cat {cat:?} (known: {:?})",
                self.ids()
            ));
        }
        self.cat_map.insert(cat.to_string(), layout_id.to_string());
        Ok(())
    }

    /// Drop a per-category override (falls back to the global default).
    pub fn clear_cat_layout(&mut self, cat: &str) {
        self.cat_map.remove(cat);
    }

    /// Resolve the spec for `cat`: `cat_map[cat]` -> global default ->
    /// `t9-9`. Back-compat wrapper: prefer [`Self::resolve_report`]
    /// when the caller can surface the diagnostic.
    pub fn resolve(&self, cat: &str) -> &LayoutSpec {
        self.resolve_report(cat).0
    }

    /// Resolve the spec for `cat` with an explicit diagnostic (`None` on
    /// a clean hit, `Some(context)` on any fallback).
    pub fn resolve_report(&self, cat: &str) -> (&LayoutSpec, Option<String>) {
        match self.cat_map.get(cat) {
            None => {
                let (spec, err) = self.get_or_default_report(&self.default_id.clone());
                // A broken global default is itself a reportable condition.
                let diag = err.map(|e| {
                    format!("LayoutRegistry::resolve: cat {cat:?} has no override; {e}")
                });
                (spec, diag)
            }
            Some(mapped) => {
                let (spec, err) = self.get_or_default_report(mapped);
                let diag = err.map(|e| {
                    format!("LayoutRegistry::resolve: cat {cat:?} maps to invalid id; {e}")
                });
                (spec, diag)
            }
        }
    }

    /// Load a `layouts/cat_map.json` document, replacing the global
    /// default (when `"default"` is present) and the whole override map.
    /// Accepted shape:
    /// `{"default": "t9-9", "cats": {"NE": "t9-16"}}`
    /// or the flat shorthand `{"NE": "t9-16"}` (all values are cat ids).
    /// Unknown/empty ids are a hard error (nothing is applied); an empty
    /// document clears the overrides back to the global default.
    pub fn load_cat_map_json(&mut self, s: &str) -> Result<(), String> {
        let v: serde_json::Value =
            serde_json::from_str(s).map_err(|e| format!("LayoutRegistry::load_cat_map_json: invalid JSON: {e}"))?;
        let obj = v
            .as_object()
            .ok_or_else(|| "LayoutRegistry::load_cat_map_json: document must be a JSON object".to_string())?;
        let (default, cats) = if obj.contains_key("cats") || obj.contains_key("default") {
            let default = obj
                .get("default")
                .map(|d| {
                    d.as_str().ok_or_else(|| {
                        "LayoutRegistry::load_cat_map_json: \"default\" must be a string".to_string()
                    })
                })
                .transpose()?;
            let cats = obj
                .get("cats")
                .map(|c| {
                    c.as_object().ok_or_else(|| {
                        "LayoutRegistry::load_cat_map_json: \"cats\" must be an object".to_string()
                    })
                })
                .transpose()?
                .map(|m| m.iter().map(|(k, vv)| (k.clone(), vv.clone())).collect::<Vec<_>>())
                .unwrap_or_default();
            (default, cats)
        } else {
            (None, obj.iter().map(|(k, vv)| (k.clone(), vv.clone())).collect::<Vec<_>>())
        };
        if let Some(d) = default {
            if d.is_empty() || !self.specs.contains_key(d) {
                return Err(format!(
                    "LayoutRegistry::load_cat_map_json: unknown default layout id {d:?} (known: {:?}); nothing applied",
                    self.ids()
                ));
            }
        }
        let mut parsed = Vec::with_capacity(cats.len());
        for (cat, vv) in &cats {
            let id = vv.as_str().ok_or_else(|| {
                format!("LayoutRegistry::load_cat_map_json: layout id for cat {cat:?} must be a string")
            })?;
            if cat.is_empty() {
                return Err(
                    "LayoutRegistry::load_cat_map_json: cat must not be empty; nothing applied".to_string(),
                );
            }
            if id.is_empty() || !self.specs.contains_key(id) {
                return Err(format!(
                    "LayoutRegistry::load_cat_map_json: unknown layout id {id:?} for cat {cat:?} (known: {:?}); nothing applied",
                    self.ids()
                ));
            }
            parsed.push((cat.clone(), id.to_string()));
        }
        if let Some(d) = default {
            self.default_id = d.to_string();
        }
        self.cat_map = parsed.into_iter().collect();
        Ok(())
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
            "नमस्ते", "कि", "काठमाडौं",
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
    fn layout02_builtin_cat_defaults_ne_t9_9() {
        // NE decision (plan/02 + MASTER rule 3, with plan/03 landed): the
        // `tr`-model matches Roman input under the shared Latin t9-9 code
        // set, so NE follows the global default (t9-9). t9-16 stays
        // registered + opt-in, never the out-of-box NE pad.
        let r = registry();
        assert_eq!(r.default_id(), "t9-9");
        assert_eq!(r.cat_layout_id("NE"), "t9-9");
        assert_eq!(r.cat_layout_id("ne"), "t9-9");
        assert_eq!(r.resolve("NE").layout_id(), "t9-9");
        assert_eq!(r.resolve("ne").layout_id(), "t9-9");
        assert_eq!(r.resolve("EN").layout_id(), "t9-9");
        assert_eq!(r.resolve("").layout_id(), "t9-9");
        // Clean resolves carry no diagnostic.
        assert!(r.resolve_report("NE").1.is_none());
        assert!(r.resolve_report("EN").1.is_none());
    }

    #[test]
    fn layout02_resolve_report_is_loud() {
        let mut r = registry();
        // A cat override pointing at a spec that later disappears must
        // fall back loudly, never silently.
        let id = r
            .register_json(
                r##"{"id":"tmp-4","title":"Tmp","grid":{"cols":2,"rows":2},
                    "keys":[{"code":"1","label":"ab","symbols":"ab","role":"text","row":0,"col":0},
                            {"code":"2","label":"cd","symbols":"cd","role":"text","row":0,"col":1},
                            {"code":"*","label":"sym","symbols":"","role":"control","row":1,"col":0},
                            {"code":"#","label":"del","symbols":"","role":"delete","row":1,"col":1}],
                    "adjacency":{"1":["2","*","#"],"2":["1","*","#"],
                                 "*":["1","2","#"],"#":["1","2","*"]}}"##,
            )
            .unwrap();
        r.set_cat_layout("tmp", &id).unwrap();
        assert!(r.resolve_report("tmp").1.is_none());
        // Simulate the spec disappearing (unregister path): drop it and
        // the resolve must fall back loudly, never silently.
        r.specs.remove(&id);
        let (spec, err) = r.resolve_report("tmp");
        assert_eq!(spec.layout_id(), "t9-9");
        let err = err.expect("fallback must carry a diagnostic");
        assert!(err.contains("tmp"), "unexpected diagnostic: {err}");
        // Unknown/empty direct ids are loud too.
        assert!(r.get_or_default_report("nope").1.is_some());
        assert!(r.get_or_default_report("").1.is_some());
        assert!(r.get_or_default_report("t9-9").1.is_none());
    }

    #[test]
    fn layout02_setters_reject_loudly() {
        let mut r = registry();
        assert!(r.set_default_layout("").is_err());
        assert!(r.set_default_layout("t9-99").is_err());
        assert!(r.set_cat_layout("", "t9-9").is_err());
        assert!(r.set_cat_layout("EN", "").is_err());
        assert!(r.set_cat_layout("EN", "t9-99").is_err());
        // Failed setters change nothing.
        assert_eq!(r.default_id(), "t9-9");
        assert_eq!(r.cat_layout_id("EN"), "t9-9");
        // Valid setters apply.
        r.set_default_layout("t9-16").unwrap();
        assert_eq!(r.default_id(), "t9-16");
        assert_eq!(r.resolve("EN").layout_id(), "t9-16");
        // NE follows the new global (no pinned override out of the box).
        assert_eq!(r.resolve("NE").layout_id(), "t9-16");
        r.set_cat_layout("EN", "t9-12").unwrap();
        assert_eq!(r.resolve("EN").layout_id(), "t9-12");
        r.clear_cat_layout("EN");
        assert_eq!(r.resolve("EN").layout_id(), "t9-16");
    }

    #[test]
    fn layout02_cat_map_json_roundtrip() {
        let mut r = registry();
        // Structured form (mirrors layouts/cat_map.json).
        r.load_cat_map_json(
            r#"{"default":"t9-12","cats":{"EN":"t9-9","NE":"t9-16"}}"#,
        )
        .unwrap();
        assert_eq!(r.default_id(), "t9-12");
        assert_eq!(r.resolve("EN").layout_id(), "t9-9");
        assert_eq!(r.resolve("NE").layout_id(), "t9-16");
        assert_eq!(r.resolve("js").layout_id(), "t9-12");
        // Flat shorthand replaces the whole map.
        r.load_cat_map_json(r#"{"NE":"t9-9"}"#).unwrap();
        assert_eq!(r.default_id(), "t9-12");
        assert_eq!(r.resolve("NE").layout_id(), "t9-9");
        assert_eq!(r.resolve("EN").layout_id(), "t9-12");
        // The checked-in file loads and matches the compiled defaults:
        // global t9-9, no pinned per-cat override (NE follows global).
        let src = include_str!("../../layouts/cat_map.json");
        let mut r2 = registry();
        r2.load_cat_map_json(src).unwrap();
        assert_eq!(r2.default_id(), "t9-9");
        assert_eq!(r2.resolve("NE").layout_id(), "t9-9");
        assert_eq!(r2.resolve("ne").layout_id(), "t9-9");
    }

    #[test]
    fn layout02_cat_map_json_rejects_loudly() {
        let mut r = registry();
        let before_default = r.default_id().to_string();
        let before_map = r.cat_map();
        for bad in [
            "not json",
            "[1,2]",
            r#"{"default":"t9-99","cats":{}}"#,
            r#"{"default":"","cats":{}}"#,
            r#"{"default":42,"cats":{}}"#,
            r#"{"cats":{"EN":"t9-99"}}"#,
            r#"{"cats":{"EN":""}}"#,
            r#"{"cats":{"": "t9-9"}}"#,
            r#"{"cats":{"EN":42}}"#,
            r#"{"EN":"t9-99"}"#,
        ] {
            assert!(
                r.load_cat_map_json(bad).is_err(),
                "must reject {bad}"
            );
        }
        // Nothing applied on any failure.
        assert_eq!(r.default_id(), before_default);
        assert_eq!(r.cat_map(), before_map);
    }

    #[test]
    fn layout02_contains_code_and_reuse_buffer() {
        let r = registry();
        let t9 = r.get_or_default("t9-9");
        assert!(t9.contains_code('2'));
        assert!(!t9.contains_code('A'));
        let t16 = r.get_or_default("t9-16");
        assert!(t16.contains_code('A'));
        assert!(t16.contains_code('B'));
        let mut buf = String::from("junk");
        t16.encode_word_into("fun", &mut buf);
        assert_eq!(buf, "396");
        t9.encode_word_into("fun", &mut buf);
        assert_eq!(buf, "386");
    }

    #[test]
    fn matras_are_non_emitting_under_every_builtin() {
        // plan/03: matras/virama/signs can never stand alone, so no
        // built-in spec may map them (their seqs would be spurious `9`s;
        // Roman `tr` seqs are primary, the Devanagari consonant skeleton
        // fallback only). Locked for all three pads, not just the frozen
        // t9-9 reference.
        let r = registry();
        for id in ["t9-9", "t9-12", "t9-16"] {
            let spec = r.get_or_default(id);
            assert_eq!(spec.encode_word("कि"), "2", "matra must vanish under {id}");
            assert_eq!(spec.encode_word("नमस्ते"), "5685", "skeleton under {id}");
            assert_eq!(
                spec.encode_word("कमल"),
                "267",
                "consonants unaffected under {id}"
            );
        }
    }

    #[test]
    fn custom_layout_registers_from_json() {        let mut r = registry();
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
