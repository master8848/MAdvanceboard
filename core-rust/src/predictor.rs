//! UniFFI-exported predictor: `new`, `suggest`, `learn`, `forget`,
//! `reject`, `export_session` (proc-macro export, uniffi 0.32).

use std::sync::{Arc, Mutex};

use crate::layout::{KeyMapping, LayoutRegistry, DEFAULT_LAYOUT_ID};
use crate::mapping::encode_word;
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
    /// Create a predictor from a base JSON wordlist
    /// (array of `{w, freq?, cat?, lang?}` or plain strings).
    #[uniffi::constructor]
    pub fn new(base_json: String) -> Arc<Self> {
        let base = DictionaryStack::load_base_json(&base_json).unwrap_or_default();
        Arc::new(Self {
            inner: Mutex::new(Inner {
                stack: DictionaryStack::new(base),
                session: SessionLogger::new(),
                layouts: LayoutRegistry::with_builtins(),
            }),
        })
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
        self.inner
            .lock()
            .map(|i| {
                let mapping = i.layouts.get_or_default(&layout_id).clone();
                i.stack
                    .suggest_for_layout(&ctx, &digits, &mapping, tab, limit as usize)
            })
            .unwrap_or_default()
    }

    /// Encode a word under a layout (default `t9-9`).
    pub fn encode(&self, word: String, layout_id: String) -> String {
        self.inner
            .lock()
            .map(|i| i.layouts.get_or_default(&layout_id).encode_word(&word))
            .unwrap_or_default()
    }

    /// Registered layout ids (built-ins `t9-9`, `t9-12`, `t9-16` + custom).
    pub fn layouts(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|i| i.layouts.ids())
            .unwrap_or_default()
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
            .unwrap_or_default()
    }

    /// Learn a word into the personal overlay (no-op in password mode)
    /// and log an accepted session event.
    pub fn learn(&self, word: String, category: String) {
        if let Ok(mut i) = self.inner.lock() {
            let seq = encode_word(&word);
            let shown: Vec<String> = i
                .stack
                .suggest("", &seq, &category, 3)
                .into_iter()
                .map(|s| s.word)
                .collect();
            i.stack.personal.learn(&word, &category);
            i.session.log_accepted(&seq, "", &word, &shown);
        }
    }

    /// Block a word (personal tombstone).
    pub fn forget(&self, word: String) {
        if let Ok(mut i) = self.inner.lock() {
            i.stack.personal.forget(&word);
        }
    }

    /// Record a rejection (reject penalty + rejected session event).
    pub fn reject(&self, word: String) {
        if let Ok(mut i) = self.inner.lock() {
            let seq = encode_word(&word);
            let shown: Vec<String> = i
                .stack
                .suggest("", &seq, "EN", 3)
                .into_iter()
                .map(|s| s.word)
                .collect();
            i.stack.personal.record_reject(&word);
            i.session.log_rejected(&seq, "", &word, &shown);
        }
    }

    /// Export the session log as JSONL.
    pub fn export_session(&self) -> String {
        self.inner
            .lock()
            .map(|i| i.session.export_jsonl())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
