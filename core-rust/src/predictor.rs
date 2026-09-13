//! UniFFI-exported predictor: `new`, `suggest`, `learn`, `forget`,
//! `reject`, `export_session` (proc-macro export, uniffi 0.32).

use std::sync::{Arc, Mutex};

use crate::layout::{KeyMapping, LayoutRegistry, DEFAULT_LAYOUT_ID};
use crate::pack::{load_pack_str, PackFile};
use crate::personal::now_quantized;
use crate::session::SessionLogger;
use crate::stack::{DictionaryStack, Suggestion};
use crate::store::{FlushStats, Store};

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
    /// Single durable handle (plan `06-persistence-power.md`): one
    /// `rusqlite::Connection` behind this `Mutex`, pragmas
    /// `WAL/NORMAL/MEMORY`, dirty-set + 2 s coalesced flush. `None` until
    /// [`Predictor::open_persist`] runs — suggest/learn work fully in RAM
    /// without it, so a missing DB never breaks typing.
    persist: Option<Store>,
    /// Last persistence failure, if any. Every flush/checkpoint I/O error
    /// lands here AND on stderr; UniFFI callers read it via
    /// [`Predictor::take_last_persist_error`] — failures are explicit, never
    /// a silent empty save.
    last_persist_error: Option<String>,
}

impl Inner {
    fn note_layout_error(&mut self, err: Option<String>) {
        if let Some(e) = err {
            eprintln!("kbcore layout: {e}");
            self.last_layout_error = Some(e);
        }
    }

    fn note_persist_error(&mut self, err: String) {
        eprintln!("kbcore persist: {err}");
        self.last_persist_error = Some(err);
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
    /// Types through the production per-tab policy
    /// ([`SuggestOpts::policy_for_tab`](crate::stack::SuggestOpts::policy_for_tab)):
    /// neighbor matching OFF, every tab hard-scoped to its own words.
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
                    .suggest_for_layout_policy_at(&ctx, &digits, &mapping, tab, limit as usize, now)
            })
            .unwrap_or_else(|e| panic!("Predictor::suggest_with_layout: lock poisoned: {e}"))
    }

    /// Suggest for a category tab, resolving the layout per keystroke:
    /// per-cat override -> global default -> `t9-9` (see
    /// `LayoutRegistry::resolve`). Fallbacks are loud (see
    /// [`Self::take_last_layout_error`]). Types through the production
    /// per-tab policy (neighbor OFF, every tab hard-scoped).
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
                    .suggest_for_layout_policy_at(&ctx, &digits, &mapping, tab, limit as usize, now)
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

    /// Enable/disable a category tab at runtime (user-pack toggle,
    /// plan/08): a disabled cat suggests nothing anywhere until
    /// re-enabled (see [`DictionaryStack::set_cat_enabled`]). Unknown
    /// cats are accepted (a pack may install later); the toggle is
    /// explicit state, never derived silently.
    pub fn set_cat_enabled(&self, cat: String, enabled: bool) {
        self.inner
            .lock()
            .map(|mut i| i.stack.set_cat_enabled(&cat, enabled))
            .unwrap_or_else(|e| panic!("Predictor::set_cat_enabled: lock poisoned: {e}"));
    }

    /// True when `cat` contributes to suggest (installed and not disabled).
    pub fn is_cat_enabled(&self, cat: String) -> bool {
        self.inner
            .lock()
            .map(|i| i.stack.is_cat_enabled(&cat))
            .unwrap_or_else(|e| panic!("Predictor::is_cat_enabled: lock poisoned: {e}"))
    }

    /// Dictionary placement info for a word (`pack cat • freq • accepts`,
    /// see [`DictionaryStack::placement`]): which pack owns `word`, its
    /// static frequency, and the personal accept count. Returns `""` when
    /// no pack holds the word (UniFFI-safe `String`; empty is the explicit
    /// no-record signal, never a fabricated pack).
    pub fn placement(&self, word: String) -> String {
        self.inner
            .lock()
            .map(|i| i.stack.placement(&word).unwrap_or_default())
            .unwrap_or_else(|e| panic!("Predictor::placement: lock poisoned: {e}"))
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

    /// Register a custom layout from a `layouts/<id>.json` document.
    /// (Spelled to avoid a literal `*` after `/`: that two-character
    /// sequence would terminate the Kotlin KDoc block in the
    /// `uniffi-bindgen` output and break `:core-bridge` compilation.)
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
    /// and log an accepted session event. The session-log seq encodes
    /// under the category's resolved layout (per-cat override -> global
    /// default -> `t9-9`). The `shown` list is empty here — callers that
    /// hold the last suggest result must use [`Self::learn_with_shown`]
    /// instead so `learn` stays an O(1) HashMap op and never runs a full
    /// `suggest` inside the lock (plan/05 #5: the old path doubled
    /// keystroke latency and held the `Mutex` across scoring).
    /// Durability is coalesced: the mutation dirties RAM only; when the
    /// 2 s window has elapsed the flush runs inline (failures recorded to
    /// `take_last_persist_error`, never fatal to the keystroke).
    pub fn learn(&self, word: String, category: String) {
        self.learn_with_shown(word, category, Vec::new());
    }

    /// Learn with the caller-observed `shown` candidates (the words from
    /// the last `suggest` call that produced this commit). O(1) personal
    /// HashMap op + session log + coalesced flush — no suggest-in-`Mutex`.
    pub fn learn_with_shown(&self, word: String, category: String, shown: Vec<String>) {
        self.inner.lock().map(|mut i| {
            let cat = if category.is_empty() { "EN" } else { &category };
            let (mapping, err) = i.layouts.resolve_report(cat);
            let mapping = mapping.clone();
            i.note_layout_error(err);
            let seq = mapping.encode_word(&word);
            i.stack.personal.learn(&word, cat);
            i.session.log_accepted(&seq, "", &word, &shown);
            Self::flush_if_due_inner(&mut i);
        }).unwrap_or_else(|e| panic!("Predictor::learn_with_shown: lock poisoned: {e}"));
    }

    /// Block a word (personal tombstone).
    pub fn forget(&self, word: String) {
        self.inner.lock().map(|mut i| {
            i.stack.personal.forget(&word);
            Self::flush_if_due_inner(&mut i);
        }).unwrap_or_else(|e| panic!("Predictor::forget: lock poisoned: {e}"));
    }

    /// Record a rejection (reject penalty + rejected session event).
    /// Session-log seq encodes under the `EN` resolved layout. The `shown`
    /// list is empty here — callers holding the last suggest result must
    /// use [`Self::reject_with_shown`]; `reject` stays an O(1) HashMap op
    /// and never runs a full `suggest` inside the lock (plan/05 #5).
    pub fn reject(&self, word: String) {
        self.reject_with_shown(word, Vec::new());
    }

    /// Reject with the caller-observed `shown` candidates. O(1) personal
    /// HashMap op + session log + coalesced flush — no suggest-in-`Mutex`.
    pub fn reject_with_shown(&self, word: String, shown: Vec<String>) {
        self.inner.lock().map(|mut i| {
            let (mapping, err) = i.layouts.resolve_report("EN");
            let mapping = mapping.clone();
            i.note_layout_error(err);
            let seq = mapping.encode_word(&word);
            i.stack.personal.record_reject(&word);
            i.session.log_rejected(&seq, "", &word, &shown);
            Self::flush_if_due_inner(&mut i);
        }).unwrap_or_else(|e| panic!("Predictor::reject_with_shown: lock poisoned: {e}"));
    }

    /// Export the session log as JSONL.
    pub fn export_session(&self) -> String {
        self.inner
            .lock()
            .map(|i| i.session.export_jsonl())
            .unwrap_or_else(|e| panic!("Predictor::export_session: lock poisoned: {e}"))
    }

    // ---- Persistence (plan 06): single handle, coalesced flush ----

    /// Open (or create) the durable store at `path` and attach the sync
    /// folder for `wal.log` + hourly snapshots. Returns `""` on success or
    /// an explicit error string (also retrievable via
    /// [`Self::take_last_persist_error`]). Typing works fully without this;
    /// durability is strictly additive.
    pub fn open_persist(&self, path: String, sync_dir: String) -> String {
        self.inner
            .lock()
            .map(|mut i| match Store::open(std::path::Path::new(&path)) {
                Ok(mut store) => {
                    if !sync_dir.is_empty() {
                        store.set_sync_dir(Some(std::path::PathBuf::from(&sync_dir)));
                    }
                    i.persist = Some(store);
                    String::new()
                }
                Err(e) => {
                    i.note_persist_error(e.clone());
                    e
                }
            })
            .unwrap_or_else(|e| panic!("Predictor::open_persist: lock poisoned: {e}"))
    }

    /// Force-flush dirty rows + pending session events now (ignores the 2 s
    /// coalesce window). Returns `""` on success or an explicit error
    /// string. Without an open store the error says so explicitly.
    pub fn flush_persist(&self) -> String {
        self.inner
            .lock()
            .map(|mut i| match Self::flush_now_inner(&mut i) {
                Ok(stats) => {
                    if stats.session_rows > 0 && i.session.dropped_unflushed() > 0 {
                        let w = format!(
                            "session: {} unflushed event(s) dropped by pending-queue overflow",
                            i.session.dropped_unflushed()
                        );
                        i.note_persist_error(w.clone());
                        return w;
                    }
                    String::new()
                }
                Err(e) => {
                    i.note_persist_error(e.clone());
                    e
                }
            })
            .unwrap_or_else(|e| panic!("Predictor::flush_persist: lock poisoned: {e}"))
    }

    /// Hourly sync-folder compact: rewrite `personal.jsonl`/`bigrams.jsonl`,
    /// write `snapshot-<ts>.gz`, truncate `wal.log`, checkpoint WAL.
    /// Returns the snapshot path on success or an explicit error string.
    pub fn compact_persist(&self) -> String {
        self.inner
            .lock()
            .map(|mut i| {
                if i.persist.is_none() {
                    return "persist not opened: call open_persist first".to_string();
                }
                // Disjoint-in-time borrows: render JSONL first (short immutable
                // borrow), then compact under the store borrow. `MutexGuard`
                // deref is borrow-conservative, so the two may not overlap.
                let personal_jsonl = i.stack.personal.to_jsonl();
                let bigrams_jsonl = i.stack.personal.bigrams_jsonl();
                let res = match i.persist.as_mut() {
                    None => return "persist not opened: call open_persist first".to_string(),
                    Some(store) => store.compact_sync_folder(&personal_jsonl, &bigrams_jsonl),
                };
                match res {
                    Ok(s) => s.map(|p| p.display().to_string()).unwrap_or_default(),
                    Err(e) => {
                        i.note_persist_error(e.clone());
                        e
                    }
                }
            })
            .unwrap_or_else(|e| panic!("Predictor::compact_persist: lock poisoned: {e}"))
    }

    /// Take (and clear) the last persistence error. `""` means no recorded
    /// failure since the last call.
    pub fn take_last_persist_error(&self) -> String {
        self.inner
            .lock()
            .map(|mut i| i.last_persist_error.take().unwrap_or_default())
            .unwrap_or_else(|e| panic!("Predictor::take_last_persist_error: lock poisoned: {e}"))
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
                persist: None,
                last_persist_error: None,
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

    /// Register a user custom-category pack at runtime (names flow):
    /// `id` names the new category tab, `wordlist` is pasted/imported
    /// `word [freq]` text (see [`crate::pack::parse_user_wordlist`]),
    /// `lang` stamps every row, `priority` must sit in `10-90` (plan/08).
    /// Behaves like a first-class category tab from here on: isolated
    /// ranking under its tab (custom-tab policy), personal-learn overlay
    /// via `learn(word, id)`, enable toggle via [`Self::set_cat_enabled`].
    /// Loud failure — a half-loaded category never installs.
    ///
    /// Plain Rust (not UniFFI-exported): mobile callers build the same
    /// envelope with their local validator and install via the existing
    /// `add_pack_json` FFI until the bindings regen exposes this.
    pub fn try_add_user_pack(
        &self,
        id: &str,
        title: &str,
        wordlist: &str,
        lang: &str,
        priority: i32,
    ) -> Result<Vec<String>, String> {
        crate::pack::check_user_priority(priority).map_err(|e| {
            format!("Predictor::try_add_user_pack: {e}")
        })?;
        let pack = crate::pack::build_user_pack(id, title, wordlist, lang).map_err(|e| {
            format!("Predictor::try_add_user_pack: {e}")
        })?;
        self.try_add_pack(&pack, priority)
    }

    // ---- Persistence: fallible Rust APIs (UniFFI-safe wrappers above) ----

    /// Flush dirty rows + pending events through the open store.
    pub fn try_flush(&self) -> Result<FlushStats, String> {
        self.inner
            .lock()
            .map(|mut i| Self::flush_now_inner(&mut i))
            .unwrap_or_else(|e| panic!("Predictor::try_flush: lock poisoned: {e}"))
    }

    /// Flush only when the 2 s coalesce window has elapsed (hot-path hook).
    pub fn try_flush_if_due(&self) -> Result<Option<FlushStats>, String> {
        self.inner
            .lock()
            .map(|mut i| {
                let due = i.persist.as_ref().map(|s| s.flush_due()).unwrap_or(false);
                let dirty = i.stack.personal.is_flush_dirty() || i.session.pending_len() > 0;
                if due && dirty {
                    Self::flush_now_inner(&mut i).map(Some)
                } else {
                    Ok(None)
                }
            })
            .unwrap_or_else(|e| panic!("Predictor::try_flush_if_due: lock poisoned: {e}"))
    }

    fn flush_now_inner(i: &mut Inner) -> Result<FlushStats, String> {
        match i.persist.as_mut() {
            None => Err("persist not opened: call open_persist first".to_string()),
            Some(store) => store.flush(&mut i.stack.personal, &mut i.session),
        }
    }

    fn flush_if_due_inner(i: &mut Inner) {
        let due = i.persist.as_ref().map(|s| s.flush_due()).unwrap_or(false);
        let dirty = i.stack.personal.is_flush_dirty() || i.session.pending_len() > 0;
        if due && dirty {
            if let Err(e) = Self::flush_now_inner(i) {
                i.note_persist_error(e);
            }
        }
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
    fn suggest_types_through_off_policy() {
        // Production policy (Tier-0 outcome): exact + prefix match, but a
        // 1-edit neighbor miss no longer fuzzy-matches by default.
        let p = Predictor::new(
            r#"[{"w":"hello","freq":900,"cat":"EN"}]"#.to_string(),
        );
        let exact = p.suggest("".to_string(), "43556".to_string(), "EN".to_string(), 5);
        assert_eq!(exact[0].word, "hello");
        // "43555" differs from hello's 43556 in the last digit (6 vs 5,
        // adjacent keys): the frozen ON arm finds it, the shipped policy
        // must not.
        let fuzzy = p.suggest("".to_string(), "43555".to_string(), "EN".to_string(), 5);
        assert!(
            fuzzy.iter().all(|s| s.word != "hello"),
            "policy defaults neighbor OFF, got {:?}",
            fuzzy.iter().map(|s| &s.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn learn_reject_with_shown_log_caller_candidates_without_suggest() {
        // Plan/05 #5: learn/reject take `shown` from the caller instead of
        // running a full suggest inside the lock. The session log must
        // carry exactly what the caller passed.
        let p = Predictor::new(
            r#"[{"w":"hello","freq":900,"cat":"EN"}]"#.to_string(),
        );
        let shown: Vec<String> = p
            .suggest("".to_string(), "43556".to_string(), "EN".to_string(), 3)
            .into_iter()
            .map(|s| s.word)
            .collect();
        assert!(!shown.is_empty());
        p.learn_with_shown("hello".to_string(), "EN".to_string(), shown.clone());
        p.reject_with_shown("hell".to_string(), shown.clone());
        let exported = p.export_session();
        for w in &shown {
            assert!(exported.contains(w), "session log must carry shown {w:?}");
        }
        // Learning still promotes, rejection still penalizes.
        let s = p.suggest("".to_string(), "43556".to_string(), "EN".to_string(), 5);
        assert_eq!(s[0].word, "hello");
    }

    #[test]
    fn layout02_default_and_per_cat_switching() {
        let p = Predictor::new(
            // `fun` exists under both tabs: per-tab isolation scopes every
            // suggest to the active tab, so layout-resolution assertions
            // need same-tab content (cross-tab rows are hidden by design).
            r#"[{"w":"hello","freq":900,"cat":"EN"},
                {"w":"fun","freq":150,"cat":"EN"},
                {"w":"fun","freq":150,"cat":"NE"}]"#
                .to_string(),
        );
        // Compiled defaults: global t9-9, NE follows it (plan/02 + MASTER
        // rule 3; the tr-model matches Roman input under t9-9, so t9-16
        // is opt-in only).
        assert_eq!(p.default_layout(), "t9-9");
        assert_eq!(p.cat_layout("EN".to_string()), "t9-9");
        assert_eq!(p.cat_layout("NE".to_string()), "t9-9");
        assert_eq!(p.layout_diagnostic("EN".to_string()), "");
        // Per-cat suggest resolves the layout by itself (t9-9 digits).
        let s = p.suggest_for_cat("".to_string(), "386".to_string(), "NE".to_string(), 5);
        assert!(s.iter().any(|c| c.word == "fun"));
        assert_eq!(s.iter().find(|c| c.word == "fun").unwrap().layout_id, "t9-9");
        let s9 = p.suggest_for_cat("".to_string(), "386".to_string(), "EN".to_string(), 5);
        assert_eq!(s9.iter().find(|c| c.word == "fun").unwrap().layout_id, "t9-9");
        // Opting NE into t9-16 changes its digits deterministically
        // (fun: 386 -> 396); EN stays on the global.
        p.set_cat_layout("NE".to_string(), "t9-16".to_string());
        assert_eq!(p.encode("fun".to_string(), "t9-16".to_string()), "396");
        let s = p.suggest_for_cat("".to_string(), "396".to_string(), "NE".to_string(), 5);
        assert!(s.iter().any(|c| c.word == "fun"));
        // Switching the global moves tabs without overrides.
        p.set_default_layout("t9-16".to_string());
        let s = p.suggest_for_cat("".to_string(), "396".to_string(), "EN".to_string(), 5);
        assert!(s.iter().any(|c| c.word == "fun"));
        // Per-cat override wins over the global.
        p.set_cat_layout("EN".to_string(), "t9-9".to_string());
        let s = p.suggest_for_cat("".to_string(), "386".to_string(), "EN".to_string(), 5);
        assert!(s.iter().any(|c| c.word == "fun"));
        p.clear_cat_layout("EN".to_string());
        assert_eq!(p.cat_layout("EN".to_string()), "t9-16");
        // Clearing the NE opt-in returns it to the global.
        p.clear_cat_layout("NE".to_string());
        assert_eq!(p.cat_layout("NE".to_string()), "t9-16");
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
    fn persist_open_flush_compact_roundtrip() {
        let dir = std::env::temp_dir().join("kbcore_predictor_persist");
        let _ = std::fs::remove_dir_all(&dir);
        let db = dir.join("kb.sqlite").display().to_string();
        let sync = dir.join("sync").display().to_string();
        let p = Predictor::new("[]".to_string());
        // Explicit error before open (returned AND recorded for later take).
        assert!(p.flush_persist().contains("not opened"));
        assert!(p.take_last_persist_error().contains("not opened"));
        assert_eq!(p.open_persist(db, sync), "");
        p.learn("hello".to_string(), "EN".to_string());
        // Coalesced hook may or may not have flushed; forced flush is exact.
        let stats = p.try_flush().unwrap();
        assert!(stats.personal_rows <= 1, "stats: {stats:?}");
        let stats2 = p.try_flush().unwrap();
        assert_eq!(stats2.personal_rows, 0, "clean flush writes nothing");
        assert_eq!(stats2.session_rows, 0);
        let snap = p.compact_persist();
        assert!(snap.contains("snapshot-"), "snap: {snap}");
        assert_eq!(p.take_last_persist_error(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_open_failure_is_explicit() {
        let p = Predictor::new("[]".to_string());
        // A path through a regular file cannot hold a DB parent dir.
        let file = std::env::temp_dir().join("kbcore_persist_blocker");
        std::fs::write(&file, b"x").unwrap();
        let bad = file.join("kb.sqlite").display().to_string();
        let err = p.open_persist(bad, "".to_string());
        assert!(!err.is_empty(), "open failure must surface, not swallow");
        assert!(!p.take_last_persist_error().is_empty());
        assert_eq!(p.take_last_persist_error(), "", "take clears");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn layout02_cat_map_and_pack_affinity() {
        let p = Predictor::new("[]".to_string());
        p.load_cat_map(
            r#"{"default":"t9-9","cats":{"NE":"t9-16","ne":"t9-16"}}"#.to_string(),
        );
        assert_eq!(p.cat_layout("NE".to_string()), "t9-16");
        // Pack affinity seeds cat defaults where no override exists
        // ("math" has no compiled default).
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

    #[test]
    fn user_pack_registers_isolated_tab_with_learn_overlay() {
        // End-to-end names flow: register -> isolated suggest under the
        // new tab -> personal learn overlay -> disable hides the tab.
        let p = Predictor::new(
            r#"[{"w":"bob","freq":900,"cat":"EN"}]"#.to_string(),
        );
        assert!(p.is_cat_enabled("names".to_string()));
        p.try_add_user_pack("names", "Names", "coa 100\n", "en", 50).unwrap();
        // Isolated: the names tab shows its own word, not EN "bob" (262).
        let s = p.suggest("".to_string(), "262".to_string(), "names".to_string(), 5);
        assert!(!s.is_empty());
        assert!(s.iter().all(|c| c.cat == "names"), "got {:?}", s.iter().map(|c| &c.word).collect::<Vec<_>>());
        assert_eq!(s[0].word, "coa");
        // Personal-learn overlay under the custom tab.
        p.learn("cob".to_string(), "names".to_string());
        p.learn("cob".to_string(), "names".to_string());
        let s2 = p.suggest("".to_string(), "262".to_string(), "names".to_string(), 5);
        assert!(s2.iter().any(|c| c.word == "cob"), "learned word must suggest, got {:?}", s2.iter().map(|c| &c.word).collect::<Vec<_>>());
        // Disable hides the tab; re-enable restores it.
        p.set_cat_enabled("names".to_string(), false);
        assert!(!p.is_cat_enabled("names".to_string()));
        assert!(p.suggest("".to_string(), "262".to_string(), "names".to_string(), 5).is_empty());
        p.set_cat_enabled("names".to_string(), true);
        assert!(!p.suggest("".to_string(), "262".to_string(), "names".to_string(), 5).is_empty());
    }

    #[test]
    fn user_pack_bad_imports_fail_loudly() {
        let p = Predictor::new("[]".to_string());
        // Bad priority (outside 10-90).
        assert!(p.try_add_user_pack("names", "Names", "ava 100\n", "en", 5).is_err());
        assert!(p.try_add_user_pack("names", "Names", "ava 100\n", "en", 100).is_err());
        // Reserved id.
        let err = p.try_add_user_pack("words", "Taken", "ava 100\n", "en", 50).unwrap_err();
        assert!(err.contains("reserved"), "unexpected: {err}");
        // Bad rows: exact per-row diagnostics, nothing installed.
        let err = p.try_add_user_pack("names", "Names", "ava 100\nava 200\nbob 0\n", "en", 50).unwrap_err();
        assert!(err.contains("duplicate"), "unexpected: {err}");
        assert!(err.contains("out of range"), "unexpected: {err}");
        assert!(p.suggest("".to_string(), "282".to_string(), "names".to_string(), 5).is_empty());
        // Empty title / empty wordlist.
        assert!(p.try_add_user_pack("names", "", "ava 100\n", "en", 50).is_err());
        assert!(p.try_add_user_pack("names", "Names", "", "en", 50).is_err());
    }
}
