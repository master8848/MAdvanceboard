//! UniFFI-exported predictor: `new`, `suggest`, `learn`, `forget`,
//! `reject`, `export_session` (proc-macro export, uniffi 0.32).

use std::sync::{Arc, Mutex};

use crate::layout::{KeyMapping, LayoutRegistry, DEFAULT_LAYOUT_ID};
use crate::mapping::encode_word;
use crate::personal::now_quantized;
use crate::session::SessionLogger;
use crate::stack::{DictionaryStack, Suggestion};

struct Inner {
    stack: DictionaryStack,
    session: SessionLogger,
    layouts: LayoutRegistry,
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

    /// Suggest under an explicit layout (`"t9-9"` default for empty or
    /// unknown ids, so no call-site breakage).
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
            .map(|i| {
                let mapping = i.layouts.get_or_default(&layout_id).clone();
                i.stack
                    .suggest_for_layout_at(&ctx, &digits, &mapping, tab, limit as usize, now)
            })
            .unwrap_or_else(|e| panic!("Predictor::suggest_with_layout: lock poisoned: {e}"))
    }

    /// Encode a word under a layout (default `t9-9`).
    pub fn encode(&self, word: String, layout_id: String) -> String {
        self.inner
            .lock()
            .map(|i| i.layouts.get_or_default(&layout_id).encode_word(&word))
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
    pub fn decode(&self, prefix: String, layout_id: String, limit: u32) -> Vec<String> {
        self.inner
            .lock()
            .map(|i| {
                let mapping = i.layouts.get_or_default(&layout_id).clone();
                i.stack
                    .decode_prefix_for_layout(&prefix, &mapping, limit as usize)
            })
            .unwrap_or_else(|e| panic!("Predictor::decode: lock poisoned: {e}"))
    }

    /// Learn a word into the personal overlay (no-op in password mode)
    /// and log an accepted session event.
    pub fn learn(&self, word: String, category: String) {
        self.inner.lock().map(|mut i| {
            let seq = encode_word(&word);
            let shown: Vec<String> = i
                .stack
                .suggest("", &seq, &category, 3)
                .into_iter()
                .map(|s| s.word)
                .collect();
            i.stack.personal.learn(&word, &category);
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
    pub fn reject(&self, word: String) {
        self.inner.lock().map(|mut i| {
            let seq = encode_word(&word);
            let shown: Vec<String> = i
                .stack
                .suggest("", &seq, "EN", 3)
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
            }),
        }))
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
}
