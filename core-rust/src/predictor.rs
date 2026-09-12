//! UniFFI-exported predictor: `new`, `suggest`, `learn`, `forget`,
//! `reject`, `export_session` (proc-macro export, uniffi 0.32).

use std::sync::{Arc, Mutex};

use crate::layout::{KeyMapping, LayoutRegistry, DEFAULT_LAYOUT_ID};
use crate::pack::{load_pack_str, PackFile};
use crate::personal::now_quantized;
use crate::session::SessionLogger;
use crate::stack::{DictionaryStack, Suggestion};

struct Inner {
    stack: DictionaryStack,
    session: SessionLogger,
    layouts: LayoutRegistry,
    /// Last layout-resolution diagnostic (fallback notices from
    /// `suggest_with_layout`/`encode`/`decode`/`suggest_for_cat`).
    /// `None` means the last call resolved cleanly. Surfaced via
    /// [`Predictor::take_last_layout_error`] so UniFFI callers that only
    /// receive `Vec<Suggestion>` still see the failure explicitly
    /// instead of a silent `t9-9` fallback. Also mirrored to stderr
    /// for host logs.
    last_layout_error: Option<String>,
}

impl Inner {
    fn note_layout_error(&mut self, err: Option<String>) {
        if let Some(e) = err {
            eprintln!("kbcore layout: {e}");
            self.last_layout_error = Some(e);
        }
    }
}

/// UniFFI-exported predictor (see `kbcore.udl` for the IDL view).
#[derive(uniffi::Object)]
pub struct Predictor {
    inner: Mutex<Inner>,
}

#[uniffi::export]
impl Predictor {
    /// Fallible constructor: returns `Err` with context when the base
    /// wordlist cannot be parsed, instead of silently starting empty.
    /// (Plain impl block below: UniFFI export blocks only support
    /// UniFFI-compatible signatures.)
    #[uniffi::constructor]
    pub fn new(base_json: String) -> Arc<Self> {
        Self::try_new(&base_json)
            .unwrap_or_else(|e| panic!("Predictor::new: invalid base wordlist: {e}"))
    }

    /// Suggest up to `limit` candidates for `digits` with context `ctx`.
    /// `active_tab` selects the category boost (e.g. `"EN"`, `"js"`).
    /// Back-compat path: always the `t9-9` default layout.
    pub fn suggest(
        &self,
        ctx: String,
        digits: String,
        active_tab: String,
        limit: u32,
    ) -> Vec<Suggestion> {
        self.suggest_with_layout(ctx, digits, DEFAULT_LAYOUT_ID.to_string(), active_tab, limit)
    }

    /// Suggest under an explicit layout. Unknown/empty ids still resolve
    /// to `t9-9` (no call-site breakage) but the fallback is LOUD: the
    /// diagnostic is logged and retrievable via
    /// [`Self::take_last_layout_error`] instead of failing silently.
    pub fn suggest_with_layout(
        &self,
        ctx: String,
        digits: String,
        layout_id: String,
        active_tab: String,
        limit: u32,
    ) -> Vec<Suggestion> {
        let tab = if active_tab.is_empty() { "EN" } else { &active_tab };
        // Quantize wall-clock once here so scoring is stable within the
        // hour (see `personal::quantize_ts`). A poisoned lock is an explicit
        // panic, never a silent empty suggestion list.
        let now = now_quantized();
        self.inner
            .lock()
            .map(|mut i| {
                let (mapping, err) = i.layouts.get_or_default_report(&layout_id);
                let mapping = mapping.clone();
                i.note_layout_error(err);
                i.stack
                    .suggest_for_layout_at(&ctx, &digits, &mapping, tab, limit as usize, now)
            })
            .unwrap_or_else(|e| panic!("Predictor::suggest_with_layout: lock poisoned: {e}"))
    }

    /// Suggest for a category tab, resolving the layout per keystroke:
    /// per-cat override -> global default -> `t9-9` (see
    /// `LayoutRegistry::resolve`). Fallbacks are loud (see
    /// [`Self::take_last_layout_error`]).
    pub fn suggest_for_cat(
        &self,
        ctx: String,
        digits: String,
        active_tab: String,
        limit: u32,
    ) -> Vec<Suggestion> {
        let tab = if active_tab.is_empty() { "EN" } else { &active_tab };
        let now = now_quantized();
        self.inner
            .lock()
            .map(|mut i| {
                let (mapping, err) = i.layouts.resolve_report(tab);
                let mapping = mapping.clone();
                i.note_layout_error(err);
                i.stack
                    .suggest_for_layout_at(&ctx, &digits, &mapping, tab, limit as usize, now)
            })
            .unwrap_or_else(|e| panic!("Predictor::suggest_for_cat: lock poisoned: {e}"))
    }

    /// Encode a word under a layout (default `t9-9`; fallbacks are loud,
    /// see [`Self::take_last_layout_error`]).
    pub fn encode(&self, word: String, layout_id: String) -> String {
        self.inner
            .lock()
            .map(|mut i| {
                let (mapping, err) = i.layouts.get_or_default_report(&layout_id);
                let out = mapping.encode_word(&word);
                i.note_layout_error(err);
                out
            })
            .unwrap_or_else(|e| panic!("Predictor::encode: lock poisoned: {e}"))
    }

    /// Registered layout ids (built-ins `t9-9`, `t9-12`, `t9-16` + custom).
    pub fn layouts(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|i| i.layouts.ids())
            .unwrap_or_else(|e| panic!("Predictor::layouts: lock poisoned: {e}"))
    }

    /// Known sequences starting with `prefix` under a layout's code set.
    /// Unknown/empty ids resolve to `t9-9` loudly (see
    /// [`Self::take_last_layout_error`]).
    pub fn decode(&self, prefix: String, layout_id: String, limit: u32) -> Vec<String> {
        self.inner
            .lock()
            .map(|mut i| {
                let (mapping, err) = i.layouts.get_or_default_report(&layout_id);
                let mapping = mapping.clone();
                i.note_layout_error(err);
                i.stack
                    .decode_prefix_for_layout(&prefix, &mapping, limit as usize)
            })
            .unwrap_or_else(|e| panic!("Predictor::decode: lock poisoned: {e}"))
    }

    /// Set the global default layout. Invalid ids are an explicit panic
    /// (context lists known ids); the previous default is unchanged.
    /// Rust callers wanting `Result` use [`Self::try_set_default_layout`].
    pub fn set_default_layout(&self, layout_id: String) {
        if let Err(e) = self.try_set_default_layout(&layout_id) {
            panic!("{e}");
        }
    }

    /// Set a per-category layout override. Invalid input is an explicit
    /// panic; Rust callers wanting `Result` use
    /// [`Self::try_set_cat_layout`].
    pub fn set_cat_layout(&self, cat: String, layout_id: String) {
        if let Err(e) = self.try_set_cat_layout(&cat, &layout_id) {
            panic!("{e}");
        }
    }

    /// Drop a per-category override (falls back to the global default).
    pub fn clear_cat_layout(&self, cat: String) {
        self.inner
            .lock()
            .map(|mut i| i.layouts.clear_cat_layout(&cat))
            .unwrap_or_else(|e| panic!("Predictor::clear_cat_layout: lock poisoned: {e}"));
    }

    /// Current global default layout id.
    pub fn default_layout(&self) -> String {
        self.inner
            .lock()
            .map(|i| i.layouts.default_id().to_string())
            .unwrap_or_else(|e| panic!("Predictor::default_layout: lock poisoned: {e}"))
    }

    /// Layout id resolved for `cat` (override -> global default).
    pub fn cat_layout(&self, cat: String) -> String {
        self.inner
            .lock()
            .map(|i| i.layouts.cat_layout_id(&cat).to_string())
            .unwrap_or_else(|e| panic!("Predictor::cat_layout: lock poisoned: {e}"))
    }

    /// Resolution diagnostic for `cat`: `""` on a clean hit, otherwise
    /// the explicit fallback context (same string the suggest path logs).
    pub fn layout_diagnostic(&self, cat: String) -> String {
        self.inner
            .lock()
            .map(|i| i.layouts.resolve_report(&cat).1.unwrap_or_default())
            .unwrap_or_else(|e| panic!("Predictor::layout_diagnostic: lock poisoned: {e}"))
    }

    /// Take (and clear) the last layout-resolution diagnostic. `""`
    /// means every recent resolve was clean. This is how UniFFI callers
    /// observe the explicit error behind a `t9-9` fallback.
    pub fn take_last_layout_error(&self) -> String {
        self.inner
            .lock()
            .map(|mut i| i.last_layout_error.take().unwrap_or_default())
            .unwrap_or_else(|e| panic!("Predictor::take_last_layout_error: lock poisoned: {e}"))
    }

    /// Register a custom layout from a `layouts/*.json` document.
    /// Returns the new id; malformed documents are an explicit panic.
    /// Rust callers wanting `Result` use [`Self::try_register_layout`].
    pub fn register_layout(&self, json: String) -> String {
        match self.try_register_layout(&json) {
            Ok(id) => id,
            Err(e) => panic!("{e}"),
        }
    }

    /// Replace the global default + per-cat overrides from a
    /// `layouts/cat_map.json` document. Invalid documents are an
    /// explicit panic; Rust callers wanting `Result` use
    /// [`Self::try_load_cat_map`].
    pub fn load_cat_map(&self, json: String) {
        if let Err(e) = self.try_load_cat_map(&json) {
            panic!("{e}");
        }
    }

    /// Install an extension pack from its JSON (`packs/<id>.json`
    /// shape) at `priority`, seeding category defaults from the pack's
    /// `layout` affinity where the user has no explicit override.
    /// Returns the seeds applied (`"NE->t9-16"`, possibly empty when
    /// overrides already exist — clean, not an error). Invalid packs
    /// (bad JSON, skipped rows, empty/unknown affinity) are an explicit
    /// panic; Rust callers wanting `Result` use [`Self::try_add_pack`].
    pub fn add_pack_json(&self, pack_json: String, priority: i32) -> String {
        match self.try_add_pack_json(&pack_json, priority) {
            Ok(seeds) => seeds.join(","),
            Err(e) => panic!("{e}"),
        }
    }

    /// Learn a word into the personal overlay (no-op in password mode)
    /// and log an accepted session event. The shown-candidates seq and
    /// the session-log seq encode under the category's resolved layout
    /// (per-cat override -> global default -> `t9-9`).
    pub fn learn(&self, word: String, category: String) {
        self.inner.lock().map(|mut i| {
            let cat = if category.is_empty() { "EN" } else { &category };
            let (mapping, err) = i.layouts.resolve_report(cat);
            let mapping = mapping.clone();
            i.note_layout_error(err);
            let seq = mapping.encode_word(&word);
            let shown: Vec<String> = i
                .stack
                .suggest_for_layout("", &seq, &mapping, cat, 3)
                .into_iter()
                .map(|s| s.word)
                .collect();
            i.stack.personal.learn(&word, cat);
            i.session.log_accepted(&seq, "", &word, &shown);
        }).unwrap_or_else(|e| panic!("Predictor::learn: lock poisoned: {e}"));
    }

    /// Block a word (personal tombstone).
    pub fn forget(&self, word: String) {
        self.inner.lock().map(|mut i| {
            i.stack.personal.forget(&word);
        }).unwrap_or_else(|e| panic!("Predictor::forget: lock poisoned: {e}"));
    }

    /// Record a rejection (reject penalty + rejected session event).
    /// Session-log seq encodes under the `EN` resolved layout.
    pub fn reject(&self, word: String) {
        self.inner.lock().map(|mut i| {
            let (mapping, err) = i.layouts.resolve_report("EN");
            let mapping = mapping.clone();
            i.note_layout_error(err);
            let seq = mapping.encode_word(&word);
            let shown: Vec<String> = i
                .stack
                .suggest_for_layout("", &seq, &mapping, "EN", 3)
                .into_iter()
                .map(|s| s.word)
                .collect();
            i.stack.personal.record_reject(&word);
            i.session.log_rejected(&seq, "", &word, &shown);
        }).unwrap_or_else(|e| panic!("Predictor::reject: lock poisoned: {e}"));
    }

    /// Export the session log as JSONL.
    pub fn export_session(&self) -> String {
        self.inner
            .lock()
            .map(|i| i.session.export_jsonl())
            .unwrap_or_else(|e| panic!("Predictor::export_session: lock poisoned: {e}"))
    }
}

/// Non-UniFFI fallible APIs (kept out of the `#[uniffi::export]` block,
///
/// whose signatures must be UniFFI-compatible).
impl Predictor {
    /// Fallible constructor: returns `Err` with context when the base
    /// wordlist cannot be parsed, instead of silently starting empty.
    pub fn try_new(base_json: &str) -> Result<Arc<Self>, String> {
        let base = DictionaryStack::load_base_json(base_json).map_err(|e| {
            format!("Predictor::try_new: base wordlist must be a JSON array of strings or {{w, freq?, cat?, lang?}} objects: {e}")
        })?;
        Ok(Arc::new(Self {
            inner: Mutex::new(Inner {
                stack: DictionaryStack::new(base),
                session: SessionLogger::new(),
                layouts: LayoutRegistry::with_builtins(),
                last_layout_error: None,
            }),
        }))
    }

    /// Fallible global-default setter (see [`Self::set_default_layout`]).
    pub fn try_set_default_layout(&self, layout_id: &str) -> Result<(), String> {
        self.inner
            .lock()
            .map(|mut i| i.layouts.set_default_layout(layout_id))
            .unwrap_or_else(|e| panic!("Predictor::try_set_default_layout: lock poisoned: {e}"))
    }

    /// Fallible per-category override setter (see [`Self::set_cat_layout`]).
    pub fn try_set_cat_layout(&self, cat: &str, layout_id: &str) -> Result<(), String> {
        self.inner
            .lock()
            .map(|mut i| i.layouts.set_cat_layout(cat, layout_id))
            .unwrap_or_else(|e| panic!("Predictor::try_set_cat_layout: lock poisoned: {e}"))
    }

    /// Fallible custom-layout registration (see [`Self::register_layout`]).
    pub fn try_register_layout(&self, json: &str) -> Result<String, String> {
        self.inner
            .lock()
            .map(|mut i| {
                i.layouts.register_json(json).map_err(|e| {
                    format!("Predictor::try_register_layout: invalid layout document: {e}")
                })
            })
            .unwrap_or_else(|e| panic!("Predictor::try_register_layout: lock poisoned: {e}"))
    }

    /// Fallible cat-map load (see [`Self::load_cat_map`]).
    pub fn try_load_cat_map(&self, json: &str) -> Result<(), String> {
        self.inner
            .lock()
            .map(|mut i| i.layouts.load_cat_map_json(json))
            .unwrap_or_else(|e| panic!("Predictor::try_load_cat_map: lock poisoned: {e}"))
    }

    /// Fallible pack install (see [`Self::add_pack_json`]): strict and
    /// loud — skipped rows (`PackFile::validate`) or an empty/unknown
    /// `layout` affinity fail the whole install with context, so a
    /// half-loaded pack never ships silently. Returns the category
    /// seeds applied from the pack affinity (`"NE->t9-16"`); empty when
    /// the user already holds explicit overrides (clean, not an error).
    pub fn try_add_pack(&self, pack: &PackFile, priority: i32) -> Result<Vec<String>, String> {
        let problems = pack.validate();
        if !problems.is_empty() {
            return Err(format!(
                "Predictor::try_add_pack: pack {:?} invalid ({} problem(s)): {}",
                pack.id,
                problems.len(),
                problems.join("; ")
            ));
        }
        if let Some(affinity) = pack.affinity() {
            let known = self.inner.lock().map(|i| i.layouts.ids()).unwrap_or_else(|e| {
                panic!("Predictor::try_add_pack: lock poisoned: {e}")
            });
            if !known.contains(&affinity.to_string()) {
                return Err(format!(
                    "Predictor::try_add_pack: pack {:?} declares unknown layout affinity {affinity:?} (known: {known:?}); nothing installed",
                    pack.id
                ));
            }
        }
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|e| panic!("Predictor::try_add_pack: lock poisoned: {e}"));
        inner.stack.add_pack(pack, priority);
        // Seed category defaults from the affinity wherever the user
        // holds no explicit override. Deterministic: pack cats sorted.
        let mut seeds = Vec::new();
        if let Some(affinity) = pack.affinity() {
            let mut cats: Vec<String> = pack
                .to_entries(priority)
                .iter()
                .map(|e| e.cat.clone())
                .collect();
            cats.sort();
            cats.dedup();
            for cat in cats {
                if inner.layouts.cat_map().contains_key(&cat) {
                    continue;
                }
                // Affinity was validated known above; a failure here
                // names the pack + cat explicitly.
                inner
                    .layouts
                    .set_cat_layout(&cat, affinity)
                    .map_err(|e| {
                        format!(
                            "Predictor::try_add_pack: pack {:?} could not seed cat {cat:?}: {e}",
                            pack.id
                        )
                    })?;
                seeds.push(format!("{cat}->{affinity}"));
            }
        }
        Ok(seeds)
    }

    /// Fallible JSON pack install (see [`Self::add_pack_json`]).
    pub fn try_add_pack_json(&self, pack_json: &str, priority: i32) -> Result<Vec<String>, String> {
        let pack = load_pack_str(pack_json).map_err(|e| {
            format!("Predictor::try_add_pack_json: invalid pack JSON: {e}")
        })?;
        self.try_add_pack(&pack, priority)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_new_rejects_invalid_wordlist_explicitly() {
        // Invalid JSON must surface an explicit error, never a silent
        // empty dictionary.
        let err = match Predictor::try_new("not json") {
            Ok(_) => panic!("try_new must fail on invalid JSON"),
            Err(e) => e,
        };
        assert!(err.contains("base wordlist"), "unexpected error: {err}");
        let err2 = match Predictor::try_new(r#"{"nope":true}"#) {
            Ok(_) => panic!("try_new must fail on non-array JSON"),
            Err(e) => e,
        };
        assert!(err2.contains("base wordlist"), "unexpected error: {err2}");
        assert!(Predictor::try_new("[]").is_ok());
    }

    #[test]
    fn predictor_roundtrip() {
        let p = Predictor::new(
            r#"[{"w":"hello","freq":900,"cat":"EN"}]"#.to_string(),
        );
        let s = p.suggest("".to_string(), "43556".to_string(), "EN".to_string(), 5);
        assert_eq!(s[0].word, "hello");
        p.learn("hello".to_string(), "EN".to_string());
        let exported = p.export_session();
        assert!(exported.contains("hello"));
    }

    #[test]
    fn layout02_default_and_per_cat_switching() {
        let p = Predictor::new(
            r#"[{"w":"hello","freq":900,"cat":"EN"},
                {"w":"fun","freq":150,"cat":"EN"}]"#
                .to_string(),
        );
        // Compiled defaults: global t9-9, NE override t9-16.
        assert_eq!(p.default_layout(), "t9-9");
        assert_eq!(p.cat_layout("EN".to_string()), "t9-9");
        assert_eq!(p.cat_layout("NE".to_string()), "t9-16");
        assert_eq!(p.layout_diagnostic("EN".to_string()), "");
        // Per-cat suggest resolves the layout by itself.
        let s = p.suggest_for_cat("".to_string(), "396".to_string(), "NE".to_string(), 5);
        assert!(s.iter().any(|c| c.word == "fun"));
        assert_eq!(s.iter().find(|c| c.word == "fun").unwrap().layout_id, "t9-16");
        let s9 = p.suggest_for_cat("".to_string(), "386".to_string(), "EN".to_string(), 5);
        assert_eq!(s9.iter().find(|c| c.word == "fun").unwrap().layout_id, "t9-9");
        // Switching the global changes labels + results deterministically.
        p.set_default_layout("t9-16".to_string());
        assert_eq!(p.encode("fun".to_string(), "t9-16".to_string()), "396");
        let s = p.suggest_for_cat("".to_string(), "396".to_string(), "EN".to_string(), 5);
        assert!(s.iter().any(|c| c.word == "fun"));
        // Per-cat override wins over the global.
        p.set_cat_layout("EN".to_string(), "t9-9".to_string());
        let s = p.suggest_for_cat("".to_string(), "386".to_string(), "EN".to_string(), 5);
        assert!(s.iter().any(|c| c.word == "fun"));
        p.clear_cat_layout("EN".to_string());
        assert_eq!(p.cat_layout("EN".to_string()), "t9-16");
    }

    #[test]
    fn layout02_invalid_ids_are_loud_not_silent() {
        let p = Predictor::new("[]".to_string());
        assert!(p.try_set_default_layout("").is_err());
        assert!(p.try_set_default_layout("t9-99").is_err());
        assert!(p.try_set_cat_layout("".to_string().as_str(), "t9-9").is_err());
        assert!(p.try_set_cat_layout("EN", "").is_err());
        assert!(p.try_set_cat_layout("EN", "t9-99").is_err());
        assert!(p.try_register_layout("not json").is_err());
        assert!(p.try_load_cat_map(r#"{"cats":{"EN":"t9-99"}}"#).is_err());
        // Unknown layout in suggest still resolves to t9-9 (compat) but
        // records an explicit diagnostic instead of failing silently.
        let s = p.suggest_with_layout(
            "".to_string(),
            "43556".to_string(),
            "nope".to_string(),
            "EN".to_string(),
            5,
        );
        let _ = s;
        let err = p.take_last_layout_error();
        assert!(err.contains("nope"), "unexpected diagnostic: {err}");
        // Clean calls leave no error behind.
        let _ = p.suggest_with_layout(
            "".to_string(),
            "43556".to_string(),
            "t9-9".to_string(),
            "EN".to_string(),
            5,
        );
        assert_eq!(p.take_last_layout_error(), "");
    }

    #[test]
    fn layout02_cat_map_and_pack_affinity() {
        let p = Predictor::new("[]".to_string());
        p.load_cat_map(
            r#"{"default":"t9-9","cats":{"NE":"t9-16","ne":"t9-16"}}"#.to_string(),
        );
        assert_eq!(p.cat_layout("NE".to_string()), "t9-16");
        // Pack affinity seeds cat defaults where no override exists
        // ("math" has no compiled default, unlike NE).
        let seeds = p
            .try_add_pack_json(
                r#"{"id":"math","title":"Math","version":"1.0.0","layout":"t9-12",
                    "words":[{"w":"\\alpha","seq":"257","freq":500,"cat":"math"}]}"#,
                10,
            )
            .unwrap();
        assert!(seeds.iter().any(|s| s == "math->t9-12"), "seeds: {seeds:?}");
        assert_eq!(p.cat_layout("math".to_string()), "t9-12");
        // Explicit user override wins over later pack seeds.
        p.set_cat_layout("math".to_string(), "t9-9".to_string());
        let seeds2 = p
            .try_add_pack_json(
                r#"{"id":"math2","title":"Math 2","version":"1.0.0","layout":"t9-12",
                    "words":[{"w":"\\beta","seq":"2382","freq":400,"cat":"math"}]}"#,
                10,
            )
            .unwrap();
        assert!(seeds2.is_empty(), "override must win: {seeds2:?}");
        assert_eq!(p.cat_layout("math".to_string()), "t9-9");
        // Unknown affinity fails the install loudly; nothing installed.
        let err = p
            .try_add_pack_json(
                r#"{"id":"xx","title":"X","version":"1.0.0","layout":"t9-99",
                    "words":[{"w":"hello","freq":10}]}"#,
                10,
            )
            .unwrap_err();
        assert!(err.contains("t9-99"), "unexpected error: {err}");
    }
}
