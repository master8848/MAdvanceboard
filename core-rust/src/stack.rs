//! Trie/FST dictionary store + `DictionaryStack` (base < extensions < personal).
//!
//! Lookup unions candidates from the tab-allowed slice of the stack matching
//! `seq` (exact + 1-edit adjacent-key neighbor + prefix), scores them with
//! the ranking formula, and returns the top-N.
//!
//! Per-tab isolation rule (hard, default on every suggest path): a candidate
//! is visible under `active_tab` iff at least one contributing static row's
//! `cat` canonicalizes to the tab (see [`tab_key`]), or — for personal-OOV
//! rows — the entry was learned under that tab. Scoping happens BEFORE the
//! merge, so display word/`cat` and summed frequencies derive from
//! tab-allowed contributors only: no cross-tab display label can leak, and a
//! word shared with a higher-priority pack is never lost from its home tab.
//!
//! General exception (the ONLY one): the `★personal` tab is the aggregate of
//! the user's own words — static rows whose word is in the live personal
//! dict (any learned category) plus every live personal-OOV entry. Static
//! rows the user never touched never appear there. There is deliberately NO
//! shared set beyond this: numbers stay under `numbers`, emoji under
//! `emoji`, English under `words`/`EN` — nothing is visible everywhere.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::layout::{KeyMapping, LayoutRegistry, DEFAULT_LAYOUT_ID};
use crate::mapping::{encode_word, is_one_edit_neighbor};
use crate::pack::PackFile;
use crate::personal::{now_quantized, PersonalDict};
use crate::rank::{base_term, dequantize_base, quantize_base, score_candidate, score_candidate_with_base, RankInput, RankWeights};

/// Prefix-result cache entry: `(layout_id, digits, fallback_version)`
/// -> exact+prefix `(row, keyfit-tag)` sets (`1` = exact, `0` = prefix).
/// The version pins the single-press fallback generation (plan/17 Step 3):
/// bumping [`SINGLE_PRESS_FALLBACK_VERSION`] retires stale len-1/len-2
/// sets across pack updates instead of serving pre-fallback rows.
type PrefixCache = HashMap<(String, String, u32), Vec<(u32, u8)>>;

/// Single-press fallback generation (plan/17 Step 3). Bump when the
/// synthetic `key_label` injection rule or [`fallback_freq`] changes so
/// cached `(layout, digits)` sets from older generations never serve.
pub const SINGLE_PRESS_FALLBACK_VERSION: u32 = 1;

/// Targeted short-exact bonus (plan/17 Step 2): added to the final score
/// of exact-length matches (`word.len() == digits.len()`) when
/// `digits.len() <= 2`, so len-1 `s` outranks longer prefixes (`some`)
/// while freq still tie-breaks among exacts. Applies ONLY to len<=2 —
/// len>=3 ordering is byte-identical with or without it. Kept out of
/// `RankWeights` on purpose: raising global `w_keyfit` hurts both splits
/// (rank.rs docs), this bonus is scoped, not global.
pub const SHORT_EXACT_BOOST: f64 = 3.0;

/// Layout ids with precomputed sequences (all current built-ins).
pub const PRECOMPUTED_LAYOUTS: &[&str] = &["t9-9", "t9-12", "t9-16"];

/// Prefix-result cache capacity (plan/05 #6): whole-word keystroke chains
/// fit several times over; overflow clears (behavior-neutral).
const PREFIX_CACHE_CAP: usize = 16;

/// Static dictionary row (base or extension pack).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DictEntry {
    pub word: String,
    pub seq: String,
    pub freq: u64,
    pub cat: String,
    pub lang: String,
    pub priority: i32,
    /// True when `seq` came from an explicit pack override (emoji/math):
    /// per-layout matching keeps it verbatim instead of re-encoding.
    #[serde(default)]
    pub explicit: bool,
    /// Roman transliteration source (Nepali pack, plan/03): sequence
    /// computation uses `tr` instead of `word`, so Roman keystrokes
    /// match while display stays Devanagari. `None` for all other packs.
    #[serde(default)]
    pub tr: Option<String>,
    /// Roman spelling-variant sources (e.g. `"paani"` next to `tr:
    /// "pani"`): each encodes to an extra match-any seq.
    #[serde(default)]
    pub alt: Vec<String>,
    /// Variant seqs under the canonical `t9-9` code set, primary `seq`
    /// excluded. Serves `t9-9` and `t9-12` (their 1-9 symbols are
    /// identical, so latin/Devanagari encodings agree; locked by
    /// `t9_12_shares_t99_aliases`).
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Variant seqs under `t9-16` (finer pq/rs + wx/yz splits).
    #[serde(default)]
    pub aliases_t916: Vec<String>,
    /// Pack-declared layout affinity (`PackFile.layout`): seeds the
    /// category default via [`DictionaryStack::pack_layout_hint`]; never
    /// filters matches (words stay findable under every layout).
    #[serde(default)]
    pub layout_id: Option<String>,
    /// Precomputed `t9-12` sequence (insert-time; `seq` is canonical
    /// `t9-9`). Empty only for explicit entries (verbatim `seq`) or rows
    /// built before precompute landed (repaired by `ensure_precomputed`).
    #[serde(default)]
    pub seq_t912: String,
    /// Precomputed `t9-16` sequence (same contract as `seq_t912`).
    #[serde(default)]
    pub seq_t916: String,
    /// Precomputed dedupe key `lower(word) + "\x1f" + lang`: the suggest
    /// union clones this instead of `format!` + `to_lowercase` per match
    /// per keystroke (plan/05 #4). Filled by [`Self::ensure_cached`];
    /// legacy deserialized rows repair on insert.
    #[serde(default)]
    pub norm_key: String,
    /// Quantized static score `log10(freq+1)` x1000 (see
    /// [`crate::rank::quantize_base`]): single-contributor candidates skip
    /// the per-keystroke `log10` (plan/05 #7). Fixed at load, so repeats
    /// stay byte-identical.
    #[serde(default)]
    pub base_q: u16,
    /// True for load-time inflection-sidecar rows
    /// ([`crate::inflect::expand_entries`]): second-order expansion
    /// (`runned` → `*runneds`) is suppressed structurally — synthetic rows
    /// never serve as stems, so repeated `expand_inflections` calls mint
    /// nothing. Defaults `false` (every constructor, every legacy
    /// deserialized row); never part of scoring or merge identity.
    #[serde(default)]
    pub synthetic: bool,
}

impl DictEntry {
    /// Sequence source: the Roman transliteration when the pack
    /// provides one (plan/03 tr-model), else the display word itself.
    pub fn seq_source(&self) -> &str {
        self.tr.as_deref().unwrap_or(&self.word)
    }

    /// Precompute the allocation-killing caches from the immutable row
    /// identity (`word`, `lang`, `freq`): dedupe key + quantized static
    /// score. Called by every constructor; [`Self::ensure_cached`] repairs
    /// legacy rows.
    fn cached_parts(word: &str, lang: &str, freq: u64) -> (String, u16) {
        (format!("{}\x1f{}", word.to_lowercase(), lang), quantize_base(freq))
    }

    /// Fill empty `norm_key`/`base_q` on legacy rows (deserialized before
    /// the caches landed, or built by external struct literals). Recompute
    /// is keyed on `norm_key.is_empty()` — `base_q == 0` is a genuine value
    /// (`freq == 0`), never a missing marker.
    pub fn ensure_cached(&mut self) {
        if self.norm_key.is_empty() {
            let (norm, q) = Self::cached_parts(&self.word, &self.lang, self.freq);
            self.norm_key = norm;
            self.base_q = q;
        }
    }

    /// Build a row from pack parts. Primary seq precedence: explicit
    /// `seq` verbatim, else `encode(tr)`, else `encode(word)`
    /// (plan/03 `PackWord.tr` first-class). Returns `None` when the
    /// primary seq is empty (caller skips the row; see
    /// [`crate::pack::PackFile::validate`] for loud diagnostics).
    /// Aliases (match-any with the primary): encoded `alt` variants
    /// plus — when a `tr` primary exists — the distinct `encode(word)`
    /// consonant-skeleton fallback for direct-Devanagari digits.
    pub fn from_parts(
        word: String,
        seq: Option<String>,
        tr: Option<String>,
        alt: Vec<String>,
        freq: u64,
        cat: String,
        lang: String,
        priority: i32,
    ) -> Option<Self> {
        let reg = LayoutRegistry::builtins_cached();
        let t99 = reg.get_or_default("t9-9");
        if let Some(s) = seq {
            if s.is_empty() {
                return None;
            }
            let mut e = Self {
                word,
                seq: s,
                freq,
                cat,
                lang,
                priority,
                explicit: true,
                layout_id: None,
                tr,
                alt,
                seq_t912: String::new(),
                seq_t916: String::new(),
                norm_key: String::new(),
                base_q: 0,
                synthetic: false,
                aliases: Vec::new(),
                aliases_t916: Vec::new(),
            };
            e.ensure_cached();
            e.fill_aliases();
            return Some(e);
        }
        let source = tr.as_deref().unwrap_or(&word);
        let primary = t99.encode_word(source);
        if primary.is_empty() {
            return None;
        }
        let mut e = Self {
            word,
            seq: primary,
            freq,
            cat,
            lang,
            priority,
            explicit: false,
            layout_id: None,
            tr,
            alt,
            seq_t912: String::new(),
            seq_t916: String::new(),
            norm_key: String::new(),
            base_q: 0,
            synthetic: false,
            aliases: Vec::new(),
            aliases_t916: Vec::new(),
        };
        e.ensure_precomputed();
        e.fill_aliases();
        Some(e)
    }

    /// (Re)derive alias seqs from `alt` variants + the `encode(word)`
    /// skeleton fallback. Idempotent: clears and rebuilds, so legacy
    /// rows deserialized without aliases repair on insert.
    fn fill_aliases(&mut self) {
        self.aliases.clear();
        self.aliases_t916.clear();
        let reg = LayoutRegistry::builtins_cached();
        let t99 = reg.get_or_default("t9-9");
        let t916 = reg.get_or_default("t9-16");
        let mut push = |seq: String, seq16: String| {
            if !seq.is_empty() && seq != self.seq && !self.aliases.contains(&seq) {
                self.aliases.push(seq);
            }
            // t9-16 evaluated independently: a variant can coincide with
            // the primary under t9-9 yet split under t9-16 (pq/rs, wx/yz).
            if !seq16.is_empty() && seq16 != self.seq_t916 && !self.aliases_t916.contains(&seq16)
            {
                self.aliases_t916.push(seq16);
            }
        };
        for a in self.alt.clone() {
            push(t99.encode_word(&a), t916.encode_word(&a));
        }
        // Direct-Devanagari fallback: when Roman `tr` is primary, the
        // consonant skeleton stays findable too (plan/03, fallback only).
        if self.tr.is_some() {
            push(t99.encode_word(&self.word), t916.encode_word(&self.word));
        }
    }
    /// Build a row, precomputing all per-layout sequences up front so
    /// per-keystroke matching borrows instead of calling `encode_word`
    /// per entry (~15k `String` allocs per keystroke at 15k words).
    pub fn precomputed(
        word: String,
        freq: u64,
        cat: String,
        lang: String,
        priority: i32,
    ) -> Option<Self> {
        let reg = LayoutRegistry::builtins_cached();
        let seq = reg.get_or_default("t9-9").encode_word(&word);
        if seq.is_empty() {
            return None;
        }
        let mut e = Self {
            word,
            seq,
            freq,
            cat,
            lang,
            priority,
            explicit: false,
            layout_id: None,
            tr: None,
            alt: Vec::new(),
            seq_t912: String::new(),
            seq_t916: String::new(),
            norm_key: String::new(),
            base_q: 0,
            synthetic: false,
            aliases: Vec::new(),
            aliases_t916: Vec::new(),
        };
        e.ensure_precomputed();
        Some(e)
    }

    /// Build a row with an explicit verbatim sequence (emoji/math packs):
    /// the override holds under every layout.
    pub fn explicit(
        word: String,
        seq: String,
        freq: u64,
        cat: String,
        lang: String,
        priority: i32,
    ) -> Option<Self> {
        if seq.is_empty() {
            return None;
        }
        let mut e = Self {
            word,
            seq,
            freq,
            cat,
            lang,
            priority,
            explicit: true,
            layout_id: None,
            tr: None,
            alt: Vec::new(),
            seq_t912: String::new(),
            seq_t916: String::new(),
            norm_key: String::new(),
            base_q: 0,
            synthetic: false,
            aliases: Vec::new(),
            aliases_t916: Vec::new(),
        };
        e.ensure_cached();
        Some(e)
    }

    /// Fill empty precomputed seqs (legacy rows). Non-explicit rows
    /// re-encode the sequence source (`tr` when present, else `word`)
    /// under each pad with a reused buffer; rows that genuinely
    /// do not encode under a pad keep the canonical `seq` at match time
    /// (same fallback the old per-keystroke `encode_word` path used).
    pub fn ensure_precomputed(&mut self) {
        self.ensure_cached();
        if self.explicit {
            return;
        }
        let reg = LayoutRegistry::builtins_cached();
        let mut buf = String::new();
        if self.seq_t912.is_empty() {
            reg.get_or_default("t9-12")
                .encode_word_into(self.seq_source(), &mut buf);
            if !buf.is_empty() {
                self.seq_t912 = buf.clone();
            }
        }
        if self.seq_t916.is_empty() {
            reg.get_or_default("t9-16")
                .encode_word_into(self.seq_source(), &mut buf);
            if !buf.is_empty() {
                self.seq_t916 = buf.clone();
            }
        }
    }

    /// Fill the precomputed seq for one layout only, reusing the
    /// caller's buffer (OOV hot path: one buffer serves the whole loop).
    /// Unknown layouts and explicit entries are no-ops.
    pub fn precompute_for(&mut self, layout_id: &str, buf: &mut String) {
        if self.explicit {
            return;
        }
        let reg = LayoutRegistry::builtins_cached();
        match layout_id {
            "t9-12" if self.seq_t912.is_empty() => {
                reg.get_or_default("t9-12")
                    .encode_word_into(self.seq_source(), buf);
                if !buf.is_empty() {
                    self.seq_t912 = buf.clone();
                }
            }
            "t9-16" if self.seq_t916.is_empty() => {
                reg.get_or_default("t9-16")
                    .encode_word_into(self.seq_source(), buf);
                if !buf.is_empty() {
                    self.seq_t916 = buf.clone();
                }
            }
            _ => {}
        }
    }
    /// Borrowed match sequence for `layout_id`: explicit overrides stay
    /// verbatim; built-ins use the precomputed seq (zero alloc); unknown
    /// or genuinely-unencodable cases fall back to canonical `seq`.
    pub fn seq_for_layout(&self, layout_id: &str) -> &str {
        if self.explicit {
            return &self.seq;
        }
        match layout_id {
            "t9-12" if !self.seq_t912.is_empty() => &self.seq_t912,
            "t9-16" if !self.seq_t916.is_empty() => &self.seq_t916,
            _ => &self.seq,
        }
    }

    /// Borrowed variant seqs for `layout_id` (match-any with
    /// [`Self::seq_for_layout`]). `t9-9`/`t9-12` share `aliases`;
    /// `t9-16` has its own splits. Unknown layouts report none here —
    /// the on-the-fly custom path derives its own variants instead.
    pub fn aliases_for_layout(&self, layout_id: &str) -> &[String] {
        match layout_id {
            "t9-16" => &self.aliases_t916,
            "t9-9" | "t9-12" => &self.aliases,
            _ => &[],
        }
    }

    /// Canonical match seqs, primary first (frozen `t9-9` path).
    pub fn all_seqs(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.seq.as_str()).chain(self.aliases.iter().map(String::as_str))
    }

    /// On-the-fly `(primary, aliases)` under a custom (non-precomputed)
    /// mapping. Explicit seqs stay verbatim; otherwise the Roman sources
    /// (`tr`, `alt`) plus the `encode(word)` skeleton fallback are
    /// re-encoded under `mapping`, with the canonical seq as the empty
    /// fallback — the same rule the precomputed paths bake in at insert.
    pub fn custom_seqs(&self, mapping: &dyn KeyMapping) -> (String, Vec<String>) {
        if self.explicit {
            let aliases: Vec<String> = self
                .alt
                .iter()
                .map(|a| mapping.encode_word(a))
                .filter(|s| !s.is_empty() && *s != self.seq)
                .collect();
            return (self.seq.clone(), aliases);
        }
        let primary = mapping.encode_word(self.seq_source());
        let primary = if primary.is_empty() {
            self.seq.clone()
        } else {
            primary
        };
        let mut aliases = Vec::new();
        for a in &self.alt {
            let s = mapping.encode_word(a);
            if !s.is_empty() && s != primary && !aliases.contains(&s) {
                aliases.push(s);
            }
        }
        if self.tr.is_some() {
            let s = mapping.encode_word(&self.word);
            if !s.is_empty() && s != primary && !aliases.contains(&s) {
                aliases.push(s);
            }
        }
        (primary, aliases)
    }
}

/// One unioned suggest candidate (module scope so the merge helpers can
/// name it): deduped on the row's precomputed `norm_key`, frequencies
/// summed, display from the highest-priority row (ties: smallest row
/// index), best keyfit, match seq from the smallest contributing row
/// index. The index rules make the merge insertion-order independent, so
/// the indexed path (FST-stream order) and the scan path (row order)
/// produce byte-identical unions.
struct MergedRow {
    word: String,
    seq: String,
    freq_base: u64,
    cat: String,
    lang: String,
    priority: i32,
    keyfit: f64,
    /// Static rows merged into this candidate (0 = personal-only).
    contributors: u32,
    /// `base_q` of the sole contributor (valid iff `contributors == 1`):
    /// the quantized `log10` skip.
    single_q: u16,
    /// Row index behind the display word/cat (highest priority, ties to
    /// the smallest index). `u32::MAX` for personal-only rows.
    first_idx: u32,
    /// Smallest contributing row index: the match-`seq` source.
    /// `u32::MAX` for personal-only rows (seq stored directly).
    min_idx: u32,
}

/// Experimental tuning knobs for the suggest pipeline (plan/00 cheapest-first
/// remedies, plan/09 spikes). `Default` is the production behavior
/// (`include_prefix=true` at keyfit 0.9, neighbor on at 0.6, hard per-tab
/// filter ON); every experiment passes an explicit non-default value so
/// deltas are measured, never silently applied. Keyfit values are
/// contractually in `[0.0, 1.0]` (debug-asserted); out-of-range values are a
/// caller bug, not clamped silently.
#[derive(Clone, Debug)]
pub struct SuggestOpts {
    /// Match prefix candidates (`seq` starts with `digits`) at `prefix_keyfit`.
    pub include_prefix: bool,
    /// Keyfit score for prefix matches (frozen default 0.9).
    pub prefix_keyfit: f64,
    /// Match 1-edit adjacent-key neighbors at `neighbor_keyfit`.
    pub include_neighbor: bool,
    /// Keyfit score for neighbor matches (frozen default 0.6).
    pub neighbor_keyfit: f64,
    /// Scope candidates to the active tab BEFORE the merge (default ON):
    /// a static row contributes only when its `cat` canonicalizes to the
    /// tab ([`Self::tab_key`]), a personal-OOV row only when learned under
    /// it — except the `★personal` tab, which aggregates the user's own
    /// words across categories (see the module docs for the one exception).
    /// Scoping pre-merge means display word/`cat` and summed frequencies
    /// derive from tab-allowed contributors only. Set explicitly `false`
    /// for cross-tab measurement arms (see [`Self::with_tab_filter`]).
    pub hard_tab_filter: bool,
}

impl Default for SuggestOpts {
    fn default() -> Self {
        Self {
            include_prefix: true,
            prefix_keyfit: 0.9,
            include_neighbor: true,
            neighbor_keyfit: 0.6,
            hard_tab_filter: true,
        }
    }
}

impl SuggestOpts {
    /// The gate's precision arm: exact + prefix only, no fuzzy neighbors.
    pub fn neighbor_off() -> Self {
        Self {
            include_neighbor: false,
            ..Default::default()
        }
    }

    /// Exact-only arm (prefix-toggle experiment): no prefix, no neighbors.
    pub fn exact_only() -> Self {
        Self {
            include_prefix: false,
            include_neighbor: false,
            ..Default::default()
        }
    }

    /// Tabs whose vocabularies are code identifiers, not prose: cross-tab
    /// prose distractors dominate their unions (measured held-out ON-arm
    /// +0.59 js / +0.55 medical under the hard filter; EN/NE
    /// unaffected-to-positive), so the production policy scopes them hard.
    /// (Kept for measurement continuity; the filter itself is now default
    /// ON for every tab — see [`Self::policy_for_tab`].)
    pub fn is_code_tab(active_tab: &str) -> bool {
        matches!(active_tab, "js" | "medical")
    }

    /// Built-in tab ids (pack cats + display aliases + overlays). Any other
    /// non-empty active tab is a user custom category (e.g. `names`).
    pub const BUILTIN_TABS: &[&str] = &[
        "words", "EN", "ne", "NE", "numbers", "js", "rust", "html", "emoji", "math", "medical",
        "personal", "★personal",
    ];

    /// True for user custom categories: any non-empty tab outside
    /// [`Self::BUILTIN_TABS`]. Empty maps to `EN` upstream (`Predictor`
    /// defaults it), so it never counts as custom here.
    pub fn is_custom_tab(active_tab: &str) -> bool {
        !active_tab.is_empty() && !Self::BUILTIN_TABS.contains(&active_tab)
    }

    /// Tabs whose unions scope hard to the active tab: code identifiers
    /// plus every user custom category (a custom tab shows its own words
    /// — the names-category isolation rule).
    /// (Kept for measurement continuity; the filter itself is now default
    /// ON for every tab — see [`Self::policy_for_tab`].)
    pub fn is_isolated_tab(active_tab: &str) -> bool {
        Self::is_code_tab(active_tab) || Self::is_custom_tab(active_tab)
    }

    /// Canonical tab key for the isolation rule: lowercase with legacy
    /// aliases folded in — `EN`/`en` → `words` (English base), `NE`/`nepali`
    /// → `ne` (Nepali pack), `★personal` → `personal`. A static row is
    /// tab-allowed iff `tab_key(row.cat) == tab_key(active_tab)`; a
    /// personal-OOV row iff `tab_key(entry.category)` matches. Unknown
    /// (custom) cats canonicalize to themselves, so a custom tab shows
    /// exactly its own words. Empty canonicalizes to itself (matches
    /// nothing); the UniFFI boundary maps `""` → `EN` before it reaches
    /// here, so empty never filters a real query.
    pub fn tab_key(tab: &str) -> String {
        let lower = tab.to_lowercase();
        match lower.as_str() {
            "en" => "words".to_string(),
            "ne" | "nepali" => "ne".to_string(),
            "★personal" => "personal".to_string(),
            _ => lower,
        }
    }

    /// True when `active_tab` is the `★personal` aggregate view (either
    /// spelling). The personal tab is the ONE general exception to
    /// isolation: it shows the user's own words across categories, never
    /// untouched static rows.
    pub fn is_personal_tab(active_tab: &str) -> bool {
        Self::tab_key(active_tab) == "personal"
    }

    /// Production suggest policy per tab (Tier-0 NO-GO follow-up outcome):
    /// neighbor matching defaults OFF on every tab (+12.5pts scope top-3@4
    /// on held-out, wins all four tabs) and EVERY tab scopes its union hard
    /// to the active tab before the truncate (code-tab wins +0.59 js /
    /// +0.55 medical; EN/NE unaffected-to-positive; custom tabs isolate to
    /// their own words). Frozen entry points (`suggest_at`,
    /// `SuggestOpts::default()`) apply the same filter — this policy is the
    /// vehicle the keyboard actually types through, and the filter is not
    /// opt-in anywhere.
    ///
    /// Per-tab neighbor opt-in lives here, not in a global toggle: pass
    /// `.with_neighbor(true)` for a tab only after a mis-press corpus
    /// clears 09#6's ≥30% failed-commit-reduction bar. No tab has cleared
    /// it, so none opts in today.
    pub fn policy_for_tab(_active_tab: &str) -> Self {
        Self {
            include_neighbor: false,
            hard_tab_filter: true,
            ..Default::default()
        }
    }

    /// Per-tab neighbor override (see [`Self::policy_for_tab`]): the caller
    /// picks `include_neighbor` per tab and the choice stays explicit and
    /// measurable at the call site, never a silent global.
    pub fn with_neighbor(mut self, on: bool) -> Self {
        self.include_neighbor = on;
        self
    }

    /// Per-tab isolation override (see [`Self::policy_for_tab`]): pass
    /// `false` ONLY for cross-tab measurement arms (gate experiments that
    /// quantify the filter's delta). Production paths never opt out — a
    /// word from another category must never show in the active tab.
    pub fn with_tab_filter(mut self, on: bool) -> Self {
        self.hard_tab_filter = on;
        self
    }
}

/// Where a gate miss lost its target in the suggest pipeline (step-1
/// instrumentation): outside the union entirely (alias/encoding gap), cut by
/// the pre-truncate 200 cap (freq-order victim), or scored but ranked out of
/// the top-3 (weight problem). All ranks are 1-based; `None` means absent at
/// that stage.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetProbe {
    /// Target matched some row's (primary, aliases) seqs at all.
    pub in_union: bool,
    /// Union size before truncation.
    pub union_size: usize,
    /// Rank in the pre-truncate freq-desc total order.
    pub pre_truncate_rank: Option<usize>,
    /// Survives the 200-truncate (rank <= 200).
    pub in_truncate200: bool,
    /// Best keyfit across the target's matching seqs.
    pub keyfit: Option<f64>,
    /// Rank after full scoring with NO truncation (all union rows scored).
    pub scored_rank_all: Option<usize>,
    /// Rank in the real top-`limit` output (`None` = absent: truncated,
    /// threshold-hidden, or blocked).
    pub top_rank: Option<usize>,
    /// Present in the scored set but below `hide_threshold`.
    pub hidden_by_threshold: bool,
    /// Suppressed by a personal block tombstone.
    pub blocked: bool,
}

/// Fully-scored candidate before the top-N heap (module scope so
/// [`DictionaryStack::score_rows`] and the probe path can share it).
struct Scored {
    word: String,
    seq: String,
    cat: String,
    lang: String,
    score: f64,
    priority: i32,
}

/// Total-order rank comparator (score desc, shorter, lexicographic,
/// pack priority, lang): shared by the top-N heap and the probe's
/// full-list ranking so both agree on every tie.
fn cmp_scored(a: &Scored, b: &Scored) -> std::cmp::Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.word.len().cmp(&b.word.len()))
        .then_with(|| a.word.cmp(&b.word))
        .then_with(|| b.priority.cmp(&a.priority))
        .then_with(|| a.lang.cmp(&b.lang))
}

/// Fixed fallback frequency per letter (plan/17 Step 1): `a`/`i` at
/// word-parity (they ARE words — `a 600000` ships in the pack), `s` with
/// a head boost (most frequent consonant; makes `suggest("7")` lead with
/// `s`, the expected single-press result), everything else low. Ranking
/// among fallback letters still goes through [`SHORT_EXACT_BOOST`] +
/// freq tie-break, never insertion order.
fn fallback_freq(c: char) -> u64 {
    match c {
        'a' => 600_000,
        'i' => 500_000,
        's' => 1_000,
        _ => 100,
    }
}

/// Layout-derived single-press letters for `digits` (plan/17 Step 1):
/// the `key_label` chars of the LAST digit, in label order. Applies only
/// when `digits` is 1-2 codes long and the last code is a letter key
/// (`2`-`9`); `1` (punct), `0` (space), controls, and len>=3 yield none.
/// QWERTY raw text never reaches here (that path takes
/// `suggest_qwerty_at`, never `suggest_inner`), so QWERTY single letters
/// stay literal and never see `pqrs` chips.
fn single_press_fallback_letters(digits: &str, mapping: &dyn KeyMapping) -> Vec<char> {
    let n = digits.chars().count();
    if n == 0 || n > 2 {
        return Vec::new();
    }
    let Some(last) = digits.chars().last() else {
        return Vec::new();
    };
    if !('2'..='9').contains(&last) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for c in mapping.key_label(last).chars() {
        if c.is_ascii_lowercase() && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}
/// a personal-bigram follower of the committed previous word.
/// Next-word candidate returned by [`DictionaryStack::suggest_next_at`]:
/// a personal-bigram follower of the committed previous word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextSuggestion {
    pub word: String,
    pub count: u64,
    pub cat: String,
}

/// Default autocorrect confidence delta (plan/22 Step 1): `S(top) −
/// S(literal_as_OOV)` must exceed this for Space to take the correction.
/// Separate from `hide_threshold` (display) — never conflated. Settings
/// surface (plan/19): Aggressive 0.5 / Conservative 2.0 / Suggest-only
/// `INFINITY` (corrections shown, Space keeps the literal) / Off (limit 0,
/// literal only — no FFI change, the IME just stops asking).
pub const AUTOCORRECT_THRESHOLD_DEFAULT: f64 = 1.0;
/// See [`AUTOCORRECT_THRESHOLD_DEFAULT`].
pub const AUTOCORRECT_THRESHOLD_AGGRESSIVE: f64 = 0.5;
/// See [`AUTOCORRECT_THRESHOLD_DEFAULT`].
pub const AUTOCORRECT_THRESHOLD_CONSERVATIVE: f64 = 2.0;

/// Fixed-count variant budget (plan/22 Step 1): variant seqs are sorted +
/// truncated to this many, so long QWERTY buffers cost a bounded number of
/// posting lookups, never wall-clock work.
pub const QWERTY_VARIANT_CAP: usize = 128;

/// QWERTY key coordinates in doubled staggered units (`x = 2*col + row`,
/// `y = 2*row`): neighbors satisfy `dx²+dy² <= 8`. Matches the physical
/// board (`q/w`, `o/p`, `a/s` adjacent; `q/s`, `m/l` not).
fn qwerty_pos(c: char) -> Option<(i32, i32)> {
    let (row, col) = match c {
        'q' | 'w' | 'e' | 'r' | 't' | 'y' | 'u' | 'i' | 'o' | 'p' => {
            (0, "qwertyuiop".find(c).unwrap() as i32)
        }
        'a' | 's' | 'd' | 'f' | 'g' | 'h' | 'j' | 'k' | 'l' => {
            (1, "asdfghjkl".find(c).unwrap() as i32)
        }
        'z' | 'x' | 'c' | 'v' | 'b' | 'n' | 'm' => (2, "zxcvbnm".find(c).unwrap() as i32),
        _ => return None,
    };
    Some((2 * col + row, 2 * row))
}

/// Distinct QWERTY-adjacent letters of `c` (sorted, never contains `c`).
/// Empty for non-letters. Backs the typo model AND the
/// correction/completion classifier, so generation and classification can
/// never disagree on the graph.
fn qwerty_neighbors(c: char) -> Vec<char> {
    let Some((x, y)) = qwerty_pos(c) else {
        return Vec::new();
    };
    let mut out: Vec<char> = ('a'..='z')
        .filter(|&b| {
            b != c
                && qwerty_pos(b).is_some_and(|(bx, by)| {
                    let (dx, dy) = (x - bx, y - by);
                    dx * dx + dy * dy <= 8
                })
        })
        .collect();
    out.sort();
    out
}

/// True when the typo model may expand `raw`: alphabetic text (apostrophe
/// and hyphen ride along for `don't`/`e-mail`, fixed in place — never
/// substituted). Digits/punct/space take the literal + digit-seq union
/// only, so Sym-taps through the QWERTY buffer never summon letter
/// corrections.
fn qwerty_variant_gate(raw: &str) -> bool {
    !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '\'' || c == '-')
        && raw.chars().any(|c| c.is_ascii_alphabetic())
}

/// 1-edit letter variants of `raw` (lowercased): adjacent-key
/// substitutes, single deletes, adjacent transposes, and inserts of
/// letters adjacent to a neighboring char. Sorted + deduped + capped at
/// [`QWERTY_VARIANT_CAP`] (fixed-count budget). Used ONLY for encode
/// (pre-encode typo model, plan/22) — the commit form is always the
/// verbatim `raw`, never a variant.
fn qwerty_variants(raw: &str) -> Vec<String> {
    if !qwerty_variant_gate(raw) {
        return Vec::new();
    }
    let lower = raw.to_lowercase();
    let ch: Vec<char> = lower.chars().collect();
    let mut out: Vec<String> = Vec::new();
    // Substitutes (adjacent keys only).
    for (i, &c) in ch.iter().enumerate() {
        if !c.is_ascii_lowercase() {
            continue;
        }
        for n in qwerty_neighbors(c) {
            let mut v = ch.clone();
            v[i] = n;
            out.push(v.into_iter().collect());
        }
    }
    // Deletes.
    for i in 0..ch.len() {
        let mut v = ch.clone();
        v.remove(i);
        out.push(v.into_iter().collect());
    }
    // Adjacent transposes.
    for i in 1..ch.len() {
        let mut v = ch.clone();
        v.swap(i - 1, i);
        out.push(v.into_iter().collect());
    }
    // Inserts (letters adjacent to either flanking char, union).
    for i in 0..=ch.len() {
        let mut cands: Vec<char> = Vec::new();
        if i > 0 && ch[i - 1].is_ascii_lowercase() {
            cands.extend(qwerty_neighbors(ch[i - 1]));
        }
        if i < ch.len() && ch[i].is_ascii_lowercase() {
            cands.extend(qwerty_neighbors(ch[i]));
        }
        cands.sort();
        cands.dedup();
        for n in cands {
            let mut v = ch.clone();
            v.insert(i, n);
            out.push(v.into_iter().collect());
        }
    }
    out.sort();
    out.dedup();
    out.retain(|v| v != &lower);
    out.truncate(QWERTY_VARIANT_CAP);
    out
}

/// True when `word` is the raw text itself (case-insensitive) or one
/// letter-edit away (adjacent-substitute, delete, insert, transpose) —
/// i.e. a CORRECTION. Anything else the union surfaced (prefix
/// completions like `tehran` for `teh`) is a completion. Classification
/// mirrors generation: substitutes must be QWERTY-adjacent.
fn is_qwerty_correction(raw_lower: &str, word_lower: &str) -> bool {
    if raw_lower == word_lower {
        return true;
    }
    let a: Vec<char> = raw_lower.chars().collect();
    let b: Vec<char> = word_lower.chars().collect();
    // Substitute (same length, exactly one diff, adjacent keys).
    if a.len() == b.len() {
        let mut diffs = 0;
        let mut pair = (' ', ' ');
        for (&x, &y) in a.iter().zip(b.iter()) {
            if x != y {
                diffs += 1;
                pair = (x, y);
                if diffs > 1 {
                    break;
                }
            }
        }
        if diffs == 1 && qwerty_neighbors(pair.0).contains(&pair.1) {
            return true;
        }
        // Transpose (same length, one adjacent swap).
        if diffs == 2 {
            for i in 1..a.len() {
                let mut t = a.clone();
                t.swap(i - 1, i);
                if t == b {
                    return true;
                }
            }
        }
        return false;
    }
    // Delete/insert (lengths differ by exactly one).
    if a.len() + 1 == b.len() {
        // `b` is `a` plus one inserted char.
        for i in 0..=a.len() {
            if let Some(&bc) = b.get(i) {
                let mut w = a.clone();
                w.insert(i, bc);
                if w == b {
                    return true;
                }
            }
        }
        return false;
    }
    if a.len() == b.len() + 1 {
        for i in 0..a.len() {
            let mut v = a.clone();
            v.remove(i);
            if v == b {
                return true;
            }
        }
    }
    false
}

/// One QWERTY correction/completion (UniFFI record): `is_correction`
/// distinguishes typo fixes (center-bold slot) from prefix completions
/// (right slot). `score` runs the SAME weights as T9 (freq + personal +
/// bigram + recency + cat + keyfit − reject) — only the match set differs
/// (letter-graph pre-encode, never T9 digit-neighbors).
#[derive(Clone, Debug, uniffi::Record)]
pub struct QwertyCorrection {
    pub word: String,
    pub score: f64,
    pub seq: String,
    pub cat: String,
    pub is_correction: bool,
}

/// QWERTY suggest output (UniFFI record): `literal` is ALWAYS the verbatim
/// raw text (AOSP: never missing); `corrections` are scored dict words;
/// `confident` gates Space-takes-correction (`S(top) − S(literal) >
/// threshold`). QWERTY and T9 ranking never blend (MASTER rule 5): this
/// path never applies [`SHORT_EXACT_BOOST`], never reads the digit-neighbor
/// policy — the frozen T9 arms are untouched by construction (no shared
/// mutable state, separate entry point).
#[derive(Clone, Debug, uniffi::Record)]
pub struct QwertyResult {
    pub literal: String,
    pub corrections: Vec<QwertyCorrection>,
    pub confident: bool,
}
/// Scored candidate returned to the UI / UniFFI.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Suggestion {
    pub word: String,
    pub score: f64,
    pub seq: String,
    pub cat: String,
    /// Layout the match was computed under (`"t9-9"` default).
    pub layout_id: String,
}

#[derive(Clone)]
pub struct DictionaryStack {
    base: Vec<DictEntry>,
    extensions: Vec<DictEntry>,
    pub personal: PersonalDict,
    weights: RankWeights,
    /// Per-layout FST indexes over known sequences (exact + prefix
    /// search). Built from the precomputed per-layout seqs at insert
    /// time; every layout present in [`PRECOMPUTED_LAYOUTS`] gets one.
    fst_by_layout: HashMap<String, fst::Set<Vec<u8>>>,
    /// Per-layout `seq -> row-index` posting lists over the same key space
    /// as the FST (primary + match-any aliases). Rows are `u32` indexes
    /// into `base ++ extensions`, appended in row order so every posting
    /// list is ascending — merging in index order reproduces the legacy
    /// scan's first-wins display exactly. Rebuilt with the FST at insert;
    /// backs exact-HashMap, generative-neighbor, and automaton-prefix
    /// lookups (plan/05 #1-2).
    seq_postings: HashMap<String, HashMap<String, Vec<u32>>>,
    /// Prefix-result cache (plan/05 #6): `(layout_id, digits)` ->
    /// exact+prefix `(row, keyfit-tag)` sets (`1` = exact, `0` = prefix).
    /// Typing appends one digit, and exact+prefix matches only shrink, so
    /// the next keystroke filters the parent's set instead of re-running
    /// the FST page. Cap 16 (whole-word chains fit; overflow clears —
    /// behavior-neutral, the cache never affects output, only speed).
    /// Cleared on every index rebuild; personal rows never enter it.
    /// `RefCell` because scoring is single-threaded (plan/04) and
    /// `suggest` takes `&self`.
    prefix_cache: RefCell<PrefixCache>,
    /// FST build failures by layout (insert sets are sorted+deduped so
    /// this is only populated on genuine `fst` errors; surfaced via
    /// [`Self::fst_build_error`] instead of a silent missing index).
    fst_errors: HashMap<String, String>,
    /// Disabled category tabs (user-pack enable toggle, plan/08):
    /// disabled cats contribute no rows to the suggest union, so the tab
    /// hides completely. Empty by default (every installed cat suggests).
    disabled_cats: HashSet<String>,
}

impl DictionaryStack {
    pub fn new(base: Vec<DictEntry>) -> Self {
        let mut s = Self {
            base,
            extensions: Vec::new(),
            personal: PersonalDict::new(),
            weights: RankWeights::default(),
            fst_by_layout: HashMap::new(),
            seq_postings: HashMap::new(),
            prefix_cache: RefCell::new(HashMap::new()),
            fst_errors: HashMap::new(),
            disabled_cats: HashSet::new(),
        };
        s.ensure_precomputed_all();
        s.rebuild_index();
        s
    }

    pub fn with_weights(base: Vec<DictEntry>, weights: RankWeights) -> Self {
        let mut s = Self {
            base,
            extensions: Vec::new(),
            personal: PersonalDict::new(),
            weights,
            fst_by_layout: HashMap::new(),
            seq_postings: HashMap::new(),
            prefix_cache: RefCell::new(HashMap::new()),
            fst_errors: HashMap::new(),
            disabled_cats: HashSet::new(),
        };
        s.ensure_precomputed_all();
        s.rebuild_index();
        s
    }

    /// Repair legacy rows missing precomputed seqs (see
    /// [`DictEntry::ensure_precomputed`]).
    fn ensure_precomputed_all(&mut self) {
        for e in self.base.iter_mut().chain(self.extensions.iter_mut()) {
            e.ensure_precomputed();
        }
    }

    /// Push an extension pack's entries at the given priority
    /// (higher wins on display category).
    pub fn add_pack(&mut self, pack: &PackFile, priority: i32) {
        let mut entries = pack.to_entries(priority);
        for e in entries.iter_mut() {
            e.ensure_precomputed();
        }
        self.extensions.extend(entries);
        self.rebuild_index();
    }

    pub fn add_entries(&mut self, entries: Vec<DictEntry>) {
        let mut entries = entries;
        for e in entries.iter_mut() {
            e.ensure_precomputed();
        }
        self.extensions.extend(entries);
        self.rebuild_index();
    }

    /// Enable/disable a category tab at runtime (user-pack toggle,
    /// plan/08): a disabled cat contributes no rows to the suggest union
    /// (static rows and personal rows learned under it), so the tab hides
    /// completely. Unknown cats are accepted (a pack may install later);
    /// the toggle is explicit state, never derived silently. Re-enabling
    /// restores the rows with no rebuild (the index keeps every row).
    pub fn set_cat_enabled(&mut self, cat: &str, enabled: bool) {
        if enabled {
            self.disabled_cats.remove(cat);
        } else {
            self.disabled_cats.insert(cat.to_string());
        }
    }

    /// True when `cat` contributes to suggest (installed and not disabled).
    pub fn is_cat_enabled(&self, cat: &str) -> bool {
        !self.disabled_cats.contains(cat)
    }

    /// Pack-declared layout affinity for `cat`: the `layout_id` of the
    /// highest-priority entry carrying one (ties: smallest layout id,
    /// then smallest word — fully deterministic). Used to seed the
    /// category default when the user has no explicit override; never
    /// filters matches. `None` when no entry declares an affinity.
    pub fn pack_layout_hint(&self, cat: &str) -> Option<String> {
        // Normative total order: max priority, then smallest layout id,
        // then smallest word (HashMap-free iteration, Vec order is
        // insertion order, sort makes the pick deterministic).
        let mut hints: Vec<(&str, i32, &str)> = Vec::new();
        for e in self.base.iter().chain(self.extensions.iter()) {
            if let Some(hint) = e.layout_id.as_deref() {
                if e.cat == cat && !hint.is_empty() {
                    hints.push((hint, e.priority, e.word.as_str()));
                }
            }
        }
        hints.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.cmp(b.0))
                .then_with(|| a.2.cmp(b.2))
        });
        hints.first().map(|h| h.0.to_string())
    }

    /// Load base entries from a JSON wordlist. Accepts either an array
    /// of `{w|word, freq?, cat?, lang?, seq?, tr?, alt?}` objects or an
    /// array of plain strings (freq 100, cat `"EN"`, lang `"en"`).
    /// `seq` is verbatim, else `encode(tr)`, else `encode(w)`
    /// (see [`DictEntry::from_parts`]). Per-layout seqs are precomputed
    /// at insert. Objects whose primary seq is empty are skipped;
    /// malformed shapes are a hard `Err` (never silent).
    pub fn load_base_json(json: &str) -> Result<Vec<DictEntry>, String> {
        let v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| e.to_string())?;
        let arr = v.as_array().ok_or_else(|| "wordlist must be a JSON array".to_string())?;
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            if let Some(w) = item.as_str() {
                if let Some(e) =
                    DictEntry::precomputed(w.to_string(), 100, "EN".to_string(), "en".to_string(), 0)
                {
                    out.push(e);
                }
            } else if let Some(o) = item.as_object() {
                let w = o
                    .get("w")
                    .or_else(|| o.get("word"))
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| "word object missing \"w\"".to_string())?;
                let alt = match o.get("alt") {
                    None => Vec::new(),
                    Some(v) => v
                        .as_array()
                        .ok_or_else(|| {
                            format!("word object {w:?}: \"alt\" must be an array of strings")
                        })?
                        .iter()
                        .map(|x| {
                            x.as_str().map(str::to_string).ok_or_else(|| {
                                format!("word object {w:?}: \"alt\" entries must be strings")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                };
                if let Some(e) = DictEntry::from_parts(
                    w.to_string(),
                    o.get("seq").and_then(|x| x.as_str()).map(str::to_string),
                    o.get("tr").and_then(|x| x.as_str()).map(str::to_string),
                    alt,
                    o.get("freq").and_then(|x| x.as_u64()).unwrap_or(100),
                    o.get("cat")
                        .and_then(|x| x.as_str())
                        .unwrap_or("EN")
                        .to_string(),
                    o.get("lang")
                        .and_then(|x| x.as_str())
                        .unwrap_or("en")
                        .to_string(),
                    0,
                ) {
                    out.push(e);
                }
            } else {
                return Err("wordlist items must be strings or objects".to_string());
            }
        }
        Ok(out)
    }

    /// Row behind a posting-list index (`base ++ extensions` order).
    /// Panics with context on a corrupt index instead of returning a
    /// wrong row: postings are rebuilt from these same vecs, so a miss
    /// here is a bug, never user input.
    fn row(&self, idx: u32) -> &DictEntry {
        let nb = self.base.len() as u32;
        if idx < nb {
            self.base.get(idx as usize).unwrap_or_else(|| {
                panic!("DictionaryStack::row: base index {idx} out of range (base len {})", self.base.len())
            })
        } else {
            self.extensions.get((idx - nb) as usize).unwrap_or_else(|| {
                panic!(
                    "DictionaryStack::row: extension index {} out of range (extensions len {})",
                    idx - nb,
                    self.extensions.len()
                )
            })
        }
    }

    /// Rebuild the per-layout FST sequence indexes (keys inserted sorted).
    /// Indexes cover primary seqs plus match-any aliases, so variant
    /// spellings and the Devanagari skeleton fallback decode too.
    /// Posting lists cover the identical key space in row order.
    fn rebuild_index(&mut self) {
        self.fst_by_layout.clear();
        self.fst_errors.clear();
        self.seq_postings.clear();
        self.prefix_cache.borrow_mut().clear();
        for layout in PRECOMPUTED_LAYOUTS {
            let mut seqs: Vec<&str> = self
                .base
                .iter()
                .chain(self.extensions.iter())
                .flat_map(|e| {
                    std::iter::once(e.seq_for_layout(layout))
                        .chain(e.aliases_for_layout(layout).iter().map(String::as_str))
                })
                .filter(|s| !s.is_empty())
                .collect();
            seqs.sort();
            seqs.dedup();
            let mut builder = fst::SetBuilder::memory();
            let mut failed: Option<String> = None;
            for s in &seqs {
                if let Err(e) = builder.insert(s) {
                    failed = Some(format!("DictionaryStack::rebuild_index: fst insert for layout {layout:?} failed: {e}"));
                    break;
                }
            }
            if let Some(err) = failed {
                self.fst_errors.insert((*layout).to_string(), err);
                continue;
            }
            match builder.into_inner() {
                Ok(bytes) => match fst::Set::new(bytes) {
                    Ok(set) => {
                        self.fst_by_layout.insert((*layout).to_string(), set);
                    }
                    Err(e) => {
                        self.fst_errors.insert(
                            (*layout).to_string(),
                            format!("DictionaryStack::rebuild_index: fst load for layout {layout:?} failed: {e}"),
                        );
                        continue;
                    }
                },
                Err(e) => {
                    self.fst_errors.insert(
                        (*layout).to_string(),
                        format!("DictionaryStack::rebuild_index: fst finish for layout {layout:?} failed: {e}"),
                    );
                    continue;
                }
            }
            // Posting lists over the identical key space, in row order
            // (ascending indexes => legacy-scan-order merge). A row
            // contributes once per distinct seq even when primary and an
            // alias coincide (dedup within the row, never across rows).
            let mut map: HashMap<String, Vec<u32>> = HashMap::new();
            for (idx, e) in self.base.iter().chain(self.extensions.iter()).enumerate() {
                let idx = idx as u32;
                let mut seen_primary = false;
                let primary = e.seq_for_layout(layout);
                if !primary.is_empty() {
                    map.entry(primary.to_string()).or_default().push(idx);
                    seen_primary = true;
                }
                for a in e.aliases_for_layout(layout) {
                    if a.is_empty() || (seen_primary && a.as_str() == primary) {
                        continue;
                    }
                    map.entry(a.clone()).or_default().push(idx);
                }
            }
            self.seq_postings.insert((*layout).to_string(), map);
        }
    }

    /// FST-backed prefix search under the canonical `t9-9` index.
    pub fn decode_prefix(&self, prefix: &str, limit: usize) -> Vec<String> {
        self.decode_prefix_for_id(prefix, "t9-9", limit)
    }

    /// FST `StartsWith` automaton query (plan/05 #1): the automaton seeks
    /// to the prefix range instead of linearly scanning the whole set.
    /// Yields lexicographic order, identical to the old linear filter.
    fn decode_prefix_for_id(&self, prefix: &str, layout_id: &str, limit: usize) -> Vec<String> {
        let Some(set) = self.fst_by_layout.get(layout_id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let auto = fst::automaton::Str::new(prefix).starts_with();
        let mut stream = set.search(auto).into_stream();
        use fst::{Automaton, IntoStreamer, Streamer};
        while let Some(k) = stream.next() {
            if let Ok(s) = std::str::from_utf8(k) {
                out.push(s.to_string());
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }

    /// Number of sequences in the canonical `t9-9` index.
    pub fn fst_len(&self) -> usize {
        self.fst_len_for("t9-9")
    }

    /// Number of sequences in the index for `layout_id` (0 when the
    /// layout has no index).
    pub fn fst_len_for(&self, layout_id: &str) -> usize {
        self.fst_by_layout.get(layout_id).map(|s| s.len()).unwrap_or(0)
    }

    /// FST build failure for `layout_id`, if any (explicit surface for
    /// what used to be a silent missing index).
    pub fn fst_build_error(&self, layout_id: &str) -> Option<&str> {
        self.fst_errors.get(layout_id).map(String::as_str)
    }

    /// Per-layout decode: the prefix must consist of the layout's codes,
    /// then that layout's own sequence index is scanned.
    pub fn decode_prefix_for_layout(
        &self,
        prefix: &str,
        mapping: &dyn KeyMapping,
        limit: usize,
    ) -> Vec<String> {
        if !prefix.chars().all(|c| mapping.contains_code(c)) {
            return Vec::new();
        }
        // Custom (non-precomputed) layouts have no dedicated index; their
        // code set still gates the query, but there is no index to scan,
        // so the result is empty rather than a wrong-layout scan.
        self.decode_prefix_for_id(prefix, mapping.layout_id(), limit)
    }

    /// Category boost under isolation: 1.0 when the row's `cat`
    /// canonicalizes to the active tab ([`SuggestOpts::tab_key`], so legacy
    /// `EN` and pack `words` boost each other, likewise `NE`/`ne`), 0.5
    /// for the `★personal` aggregate view over the user's own words, else
    /// 0.0. Under the default hard filter every surviving row scores 1.0
    /// (or 0.5 on the personal tab) — the weight still separates the
    /// unfiltered measurement arms.
    fn cat_boost(&self, entry_cat: &str, in_personal: bool, active_tab: &str) -> f64 {
        if SuggestOpts::tab_key(entry_cat) == SuggestOpts::tab_key(active_tab) {
            1.0
        } else if in_personal && SuggestOpts::is_personal_tab(active_tab) {
            0.5
        } else {
            0.0
        }
    }

    /// Static-row side of the isolation rule: under the `★personal` tab a
    /// row is allowed iff its word is in the live personal dict (the ONE
    /// aggregate exception — the user's own words across categories);
    /// under every other tab iff its `cat` canonicalizes to the tab.
    fn static_row_allowed(&self, e: &DictEntry, tab: &str, personal_tab: bool) -> bool {
        if personal_tab {
            self.personal.get(&e.word).map(|p| !p.deleted).unwrap_or(false)
        } else {
            SuggestOpts::tab_key(&e.cat) == tab
        }
    }

    /// Suggest candidates for `digits` given previous-word `ctx`.
    /// Matches exact (KeyFit 1.0), prefix (0.9), and 1-edit adjacent-key
    /// neighbors (0.6); dedupes on `(norm(word), lang)` keeping the
    /// highest-priority pack's display category; sums static
    /// frequencies; applies the personal overlay; hides blocked words
    /// and sub-threshold scores; sorts by score (tie: shorter,
    /// lexicographic, pack priority).
    pub fn suggest(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        limit: usize,
    ) -> Vec<Suggestion> {
        self.suggest_at(ctx, digits, active_tab, limit, now_quantized())
    }

    /// Deterministic entry point: same `(dict set, ctx, digits, tab,
    /// layout, personal snapshot, now)` => byte-identical output.
    /// `now` should be a 1h-quantized timestamp (`personal::quantize_ts`).
    pub fn suggest_at(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        limit: usize,
        now: i64,
    ) -> Vec<Suggestion> {
        // Frozen t9-9 path: precomputed canonical seqs, digits only. The
        // mapping feeds neighbor generation + personal encoding; its
        // behavior is parity-locked with the frozen free functions
        // (`t9_parity_with_mapping`), so the indexed path matches the old
        // scan exactly.
        let mapping = LayoutRegistry::builtins_cached().get_or_default(DEFAULT_LAYOUT_ID);
        self.suggest_inner(
            ctx,
            digits,
            active_tab,
            limit,
            DEFAULT_LAYOUT_ID,
            &|e| {
                (
                    Cow::Borrowed(e.seq_for_layout(DEFAULT_LAYOUT_ID)),
                    Cow::Borrowed(e.aliases_for_layout(DEFAULT_LAYOUT_ID)),
                )
            },
            mapping,
            &SuggestOpts::default(),
            &is_one_edit_neighbor,
            &|c| c.is_ascii_digit(),
            now,
        )
    }

    /// Exact + prefix only (neighbor OFF): the gate's precision arm.
    /// Same quantization contract as [`Self::suggest_at`].
    pub fn suggest_no_neighbor(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        limit: usize,
    ) -> Vec<Suggestion> {
        self.suggest_no_neighbor_at(ctx, digits, active_tab, limit, now_quantized())
    }

    /// Neighbor-OFF deterministic entry point (see [`Self::suggest_at`]).
    pub fn suggest_no_neighbor_at(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        limit: usize,
        now: i64,
    ) -> Vec<Suggestion> {
        let mapping = LayoutRegistry::builtins_cached().get_or_default(DEFAULT_LAYOUT_ID);
        self.suggest_inner(
            ctx,
            digits,
            active_tab,
            limit,
            DEFAULT_LAYOUT_ID,
            &|e| {
                (
                    Cow::Borrowed(e.seq_for_layout(DEFAULT_LAYOUT_ID)),
                    Cow::Borrowed(e.aliases_for_layout(DEFAULT_LAYOUT_ID)),
                )
            },
            mapping,
            &SuggestOpts::neighbor_off(),
            &|_, _| false,
            &|c| c.is_ascii_digit(),
            now,
        )
    }

    /// Suggest with explicit tuning knobs (plan/00 cheapest-first remedies):
    /// frozen `t9-9` path, same quantization contract as
    /// [`Self::suggest_at`]. `SuggestOpts::default()` reproduces
    /// [`Self::suggest_at`] exactly; every experiment passes an explicit
    /// non-default value. This is also the per-tab override vehicle behind
    /// a default-neighbor-OFF policy (plan/00 item 4): the caller picks
    /// `include_neighbor` per tab and the choice is measured, not silent.
    pub fn suggest_with_opts_at(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        limit: usize,
        now: i64,
        opts: &SuggestOpts,
    ) -> Vec<Suggestion> {
        let mapping = LayoutRegistry::builtins_cached().get_or_default(DEFAULT_LAYOUT_ID);
        self.suggest_inner(
            ctx,
            digits,
            active_tab,
            limit,
            DEFAULT_LAYOUT_ID,
            &|e| {
                (
                    Cow::Borrowed(e.seq_for_layout(DEFAULT_LAYOUT_ID)),
                    Cow::Borrowed(e.aliases_for_layout(DEFAULT_LAYOUT_ID)),
                )
            },
            mapping,
            opts,
            &is_one_edit_neighbor,
            &|c| c.is_ascii_digit(),
            now,
        )
    }

    /// Production policy entry point (frozen `t9-9` path): applies
    /// [`SuggestOpts::policy_for_tab`] for `active_tab` (neighbor OFF;
    /// hard tab filter on code tabs). Same quantization contract as
    /// [`Self::suggest_at`]. This is what the keyboard types through;
    /// the frozen arms above stay byte-identical for gate measurement.
    pub fn suggest_policy(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        limit: usize,
    ) -> Vec<Suggestion> {
        self.suggest_policy_at(ctx, digits, active_tab, limit, now_quantized())
    }

    /// Deterministic production-policy entry point (see [`Self::suggest_at`]).
    pub fn suggest_policy_at(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        limit: usize,
        now: i64,
    ) -> Vec<Suggestion> {
        let opts = SuggestOpts::policy_for_tab(active_tab);
        self.suggest_with_opts_at(ctx, digits, active_tab, limit, now, &opts)
    }

    /// Step-1 miss instrumentation: where `target` is lost in the pipeline
    /// for (`ctx`, `digits`, tab) under `opts` (frozen `t9-9` path, same
    /// quantization contract as [`Self::suggest_at`]). The union is built by
    /// the same [`Self::build_union`] the real path uses, the pre-truncate
    /// rank uses the same total order, and the full-list rank scores every
    /// union row with no truncation — so `in_union=false` means an
    /// alias/encoding gap, `!in_truncate200` a freq-order victim, and
    /// `scored_rank_all>3` a weight problem. Never panics: invalid input
    /// yields an all-absent probe (mirroring the empty suggest output).
    pub fn probe_target_at(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        target: &str,
        limit: usize,
        now: i64,
        opts: &SuggestOpts,
    ) -> TargetProbe {
        let blocked = self.personal.is_blocked(target);
        let absent = |union_size: usize| TargetProbe {
            in_union: false,
            union_size,
            pre_truncate_rank: None,
            in_truncate200: false,
            keyfit: None,
            scored_rank_all: None,
            top_rank: None,
            hidden_by_threshold: false,
            blocked,
        };
        if digits.is_empty()
            || !digits.chars().all(|c| c.is_ascii_digit())
            || limit == 0
            || target.is_empty()
        {
            return absent(0);
        }
        let mapping = LayoutRegistry::builtins_cached().get_or_default(DEFAULT_LAYOUT_ID);
        // Same union builder as the real path (isolation scoped inside),
        // so `in_union=false` means an alias/encoding gap under the active
        // tab — never a cross-tab row the real path would have kept.
        let merged = self.build_union(
            digits,
            active_tab,
            DEFAULT_LAYOUT_ID,
            &|e| {
                (
                    Cow::Borrowed(e.seq_for_layout(DEFAULT_LAYOUT_ID)),
                    Cow::Borrowed(e.aliases_for_layout(DEFAULT_LAYOUT_ID)),
                )
            },
            mapping,
            opts,
            &is_one_edit_neighbor,
            &[],
        );
        let union_size = merged.len();
        let mut items: Vec<MergedRow> = merged.into_values().collect();
        items.sort_by(|a, b| {
            b.freq_base
                .cmp(&a.freq_base)
                .then_with(|| a.word.cmp(&b.word))
                .then_with(|| a.lang.cmp(&b.lang))
        });
        let hit = items.iter().find(|m| m.word == target);
        let Some(hit) = hit else {
            return absent(union_size);
        };
        let keyfit = hit.keyfit;
        let pre_rank = items.iter().position(|m| m.word == target).map(|i| i + 1);
        let in_truncate200 = pre_rank.map(|r| r <= 200).unwrap_or(false);
        let prev = ctx.split_whitespace().last().unwrap_or("").to_lowercase();
        let mut scored_all = self.score_rows(items, &prev, active_tab, now, true);
        scored_all.sort_by(cmp_scored);
        let scored_rank = scored_all
            .iter()
            .position(|s| s.word == target)
            .map(|i| i + 1);
        let top_rank = self
            .suggest_with_opts_at(ctx, digits, active_tab, limit, now, opts)
            .iter()
            .position(|s| s.word == target)
            .map(|i| i + 1);
        TargetProbe {
            in_union: true,
            union_size,
            pre_truncate_rank: pre_rank,
            in_truncate200,
            keyfit: Some(keyfit),
            scored_rank_all: scored_rank,
            top_rank,
            hidden_by_threshold: in_truncate200 && !blocked && scored_rank.is_none(),
            blocked,
        }
    }

    /// Suggest under an explicit layout: candidates match by the seq the
    /// word encodes to under `mapping` (explicit pack `seq` overrides stay
    /// verbatim), fuzzy matching uses the layout's neighbor graph, and
    /// input codes validate against the layout's code set.
    pub fn suggest_for_layout(
        &self,
        ctx: &str,
        digits: &str,
        mapping: &dyn KeyMapping,
        active_tab: &str,
        limit: usize,
    ) -> Vec<Suggestion> {
        self.suggest_for_layout_at(ctx, digits, mapping, active_tab, limit, now_quantized())
    }

    /// Deterministic per-layout entry point (see [`Self::suggest_at`]).
    pub fn suggest_for_layout_at(
        &self,
        ctx: &str,
        digits: &str,
        mapping: &dyn KeyMapping,
        active_tab: &str,
        limit: usize,
        now: i64,
    ) -> Vec<Suggestion> {
        // Built-in layouts borrow the insert-time precomputed seq (zero
        // alloc per entry); explicit pack overrides stay verbatim; custom
        // layouts encode on the fly with the same empty-falls-back-to-
        // canonical rule the old closure used.
        let layout_id = mapping.layout_id();
        if PRECOMPUTED_LAYOUTS.contains(&layout_id) {
            self.suggest_inner(
                ctx,
                digits,
                active_tab,
                limit,
                layout_id,
                &|e| {
                    (
                        Cow::Borrowed(e.seq_for_layout(layout_id)),
                        Cow::Borrowed(e.aliases_for_layout(layout_id)),
                    )
                },
                mapping,
                &SuggestOpts::default(),
                &|a, b| mapping.is_one_edit_neighbor(a, b),
                &|c| mapping.contains_code(c),
                now,
            )
        } else {
            self.suggest_inner(
                ctx,
                digits,
                active_tab,
                limit,
                layout_id,
                &|e| {
                    let (primary, aliases) = e.custom_seqs(mapping);
                    (Cow::Owned(primary), Cow::Owned(aliases))
                },
                mapping,
                &SuggestOpts::default(),
                &|a, b| mapping.is_one_edit_neighbor(a, b),
                &|c| mapping.contains_code(c),
                now,
            )
        }
    }

    /// Per-layout neighbor-OFF deterministic entry point.
    pub fn suggest_for_layout_no_neighbor_at(
        &self,
        ctx: &str,
        digits: &str,
        mapping: &dyn KeyMapping,
        active_tab: &str,
        limit: usize,
        now: i64,
    ) -> Vec<Suggestion> {
        let layout_id = mapping.layout_id();
        if PRECOMPUTED_LAYOUTS.contains(&layout_id) {
            self.suggest_inner(
                ctx,
                digits,
                active_tab,
                limit,
                layout_id,
                &|e| {
                    (
                        Cow::Borrowed(e.seq_for_layout(layout_id)),
                        Cow::Borrowed(e.aliases_for_layout(layout_id)),
                    )
                },
                mapping,
                &SuggestOpts::neighbor_off(),
                &|_, _| false,
                &|c| mapping.contains_code(c),
                now,
            )
        } else {
            self.suggest_inner(
                ctx,
                digits,
                active_tab,
                limit,
                layout_id,
                &|e| {
                    let (primary, aliases) = e.custom_seqs(mapping);
                    (Cow::Owned(primary), Cow::Owned(aliases))
                },
                mapping,
                &SuggestOpts::neighbor_off(),
                &|_, _| false,
                &|c| mapping.contains_code(c),
                now,
            )
        }
    }

    /// Production policy under an explicit layout: per-tab policy opts
    /// ([`SuggestOpts::policy_for_tab`]) with the layout's own neighbor
    /// graph when a tab opts back in (none has — see `policy_for_tab`).
    /// Deterministic entry point (see [`Self::suggest_at`]).
    pub fn suggest_for_layout_policy_at(
        &self,
        ctx: &str,
        digits: &str,
        mapping: &dyn KeyMapping,
        active_tab: &str,
        limit: usize,
        now: i64,
    ) -> Vec<Suggestion> {
        let opts = SuggestOpts::policy_for_tab(active_tab);
        let layout_id = mapping.layout_id();
        // Neighbor generation is skipped entirely while no tab opts in;
        // the opt-in arm passes the layout graph through so a future
        // per-tab opt-in measures the real layout-aware fuzzy set.
        let neighbor_on = opts.include_neighbor;
        if PRECOMPUTED_LAYOUTS.contains(&layout_id) {
            self.suggest_inner(
                ctx,
                digits,
                active_tab,
                limit,
                layout_id,
                &|e| {
                    (
                        Cow::Borrowed(e.seq_for_layout(layout_id)),
                        Cow::Borrowed(e.aliases_for_layout(layout_id)),
                    )
                },
                mapping,
                &opts,
                &|a, b| neighbor_on && mapping.is_one_edit_neighbor(a, b),
                &|c| mapping.contains_code(c),
                now,
            )
        } else {
            self.suggest_inner(
                ctx,
                digits,
                active_tab,
                limit,
                layout_id,
                &|e| {
                    let (primary, aliases) = e.custom_seqs(mapping);
                    (Cow::Owned(primary), Cow::Owned(aliases))
                },
                mapping,
                &opts,
                &|a, b| neighbor_on && mapping.is_one_edit_neighbor(a, b),
                &|c| mapping.contains_code(c),
                now,
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// QWERTY suggest (plan/22 Step 1): literal slot + letter-graph
    /// corrections + confidence. Additive alongside the frozen T9 arms —
    /// never a fork: same union builder (same isolation, same merge
    /// discipline), same weights, separate entry point with its own opts
    /// (prefix ON, digit-neighbors OFF — the typo model is the letter
    /// graph pre-encode, passed as `extra_exact`) and NO short-exact
    /// boost (MASTER rule 5: T9 and QWERTY ranking never blend).
    ///
    /// `literal` is always the verbatim `raw` (AOSP: never missing — even
    /// on misses, empty input, or limit 0). `corrections` are scored dict
    /// words minus the literal itself (case-insensitive); each carries
    /// `is_correction` (typo fix, center-bold slot) vs completion (right
    /// slot). `confident = S(top) − S(literal_as_OOV) >
    /// autocorrect_threshold`: Space takes the correction only then.
    /// Pass `f64::INFINITY` for Suggest-only (corrections shown, Space
    /// keeps the literal); pass `limit == 0` for Off (literal only).
    /// Same quantization contract as [`Self::suggest_at`].
    pub fn suggest_qwerty_at(
        &self,
        raw: &str,
        ctx: &str,
        active_tab: &str,
        limit: usize,
        mapping: &dyn KeyMapping,
        now: i64,
        autocorrect_threshold: f64,
    ) -> QwertyResult {
        let literal = raw.to_string();
        if limit == 0 {
            return QwertyResult {
                literal,
                corrections: Vec::new(),
                confident: false,
            };
        }
        let layout_id = mapping.layout_id();
        let raw_seq = mapping.encode_word(raw);
        let mut variant_seqs: Vec<String> = qwerty_variants(raw)
            .iter()
            .map(|v| mapping.encode_word(v))
            .filter(|s| !s.is_empty())
            .collect();
        variant_seqs.sort();
        variant_seqs.dedup();
        if raw_seq.is_empty() && variant_seqs.is_empty() {
            return QwertyResult {
                literal,
                corrections: Vec::new(),
                confident: false,
            };
        }
        // QWERTY policy: exact + prefix on the raw seq, letter-graph
        // variants exact-only, hard tab filter. Digit-neighbors stay OFF
        // (they would collapse `q/w`, `o/p` geometry post-encode).
        let opts = SuggestOpts {
            include_prefix: true,
            include_neighbor: false,
            hard_tab_filter: true,
            ..Default::default()
        };
        let build = |seqs_of: &dyn Fn(&DictEntry) -> (Cow<'_, str>, Cow<'_, [String]>)| {
            self.build_union(
                &raw_seq,
                active_tab,
                layout_id,
                seqs_of,
                mapping,
                &opts,
                &|_, _| false,
                &variant_seqs,
            )
        };
        let merged = if PRECOMPUTED_LAYOUTS.contains(&layout_id) {
            build(&|e| {
                (
                    Cow::Borrowed(e.seq_for_layout(layout_id)),
                    Cow::Borrowed(e.aliases_for_layout(layout_id)),
                )
            })
        } else {
            build(&|e| {
                let (primary, aliases) = e.custom_seqs(mapping);
                (Cow::Owned(primary), Cow::Owned(aliases))
            })
        };
        // Same SPEC 200-cap + total order as the T9 path.
        let mut items: Vec<MergedRow> = merged.into_values().collect();
        items.sort_by(|a, b| {
            b.freq_base
                .cmp(&a.freq_base)
                .then_with(|| a.word.cmp(&b.word))
                .then_with(|| a.lang.cmp(&b.lang))
        });
        items.truncate(200);
        let prev = ctx.split_whitespace().last().unwrap_or("").to_lowercase();
        // Unfiltered: the literal row must be observable for its score
        // even when it sits below the display threshold (a known word
        // typed verbatim scores WITH its frequency — delta ~0 against
        // itself, so Space keeps it). Corrections filter below.
        let mut scored = self.score_rows(items, &prev, active_tab, now, false);
        scored.sort_by(cmp_scored);
        let raw_lower = raw.to_lowercase();
        // Literal score: the union row when `raw` is a known word (full
        // freq + personal overlay — typing `the` must not "correct" to
        // `then`), else the SAME formula as an OOV (freq 0, keyfit 1.0,
        // same-tab boost). Never depends on threshold/block membership:
        // the slot is always present.
        let literal_score = scored
            .iter()
            .find(|s| s.word.to_lowercase() == raw_lower)
            .map(|s| s.score)
            .unwrap_or_else(|| {
                let p = self.personal.get(raw);
                let (freq_personal, in_personal, acc, rej, last_seen) = match p {
                    Some(e) if !e.deleted => (e.count, true, e.acc, e.rej, e.last_seen),
                    _ => (0, false, 0, 0, 0),
                };
                let (bpw, bp, bv) = if prev.is_empty() {
                    (0, 0, 1)
                } else {
                    self.personal.bigram_counts(&prev, raw)
                };
                score_candidate(
                    &RankInput {
                        freq_base: 0,
                        freq_personal,
                        in_personal,
                        bigram_pw: bpw,
                        bigram_prev: bp,
                        bigram_vocab: bv,
                        now_ts: now,
                        last_seen_ts: last_seen,
                        cat_boost: 1.0,
                        keyfit: 1.0,
                        accepts: acc,
                        rejects: rej,
                        temporal: 0.0,
                    },
                    &self.weights,
                )
            });
        let raw_lower = raw.to_lowercase();
        let corrections: Vec<QwertyCorrection> = scored
            .into_iter()
            .filter(|s| s.word.to_lowercase() != raw_lower)
            .filter(|s| s.score >= self.weights.hide_threshold)
            .take(limit)
            .map(|s| {
                let is_correction = is_qwerty_correction(&raw_lower, &s.word.to_lowercase());
                QwertyCorrection {
                    word: s.word,
                    score: s.score,
                    seq: s.seq,
                    cat: s.cat,
                    is_correction,
                }
            })
            .collect();
        let confident = corrections
            .first()
            .is_some_and(|c| c.score - literal_score > autocorrect_threshold);
        QwertyResult {
            literal,
            corrections,
            confident,
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Build the suggest union (static rows + personal OOV) without
    /// truncating or scoring: shared by [`Self::suggest_inner`] and the
    /// step-1 miss instrumentation ([`Self::probe_target_at`]).
    ///
    /// Isolation is enforced HERE, before the merge, when
    /// `opts.hard_tab_filter` (default ON): disallowed static rows never
    /// contribute, so display word/`cat` and summed frequencies derive from
    /// tab-allowed contributors only; disallowed personal-OOV rows never
    /// enter. The prefix cache stays tab-agnostic (it memoizes row-index
    /// sets; scoping applies at merge time), so mixed-tab keystroke
    /// sequences stay byte-identical to cold runs.
    fn build_union(
        &self,
        digits: &str,
        active_tab: &str,
        layout_id: &str,
        seqs_of: &dyn Fn(&DictEntry) -> (Cow<'_, str>, Cow<'_, [String]>),
        mapping: &dyn KeyMapping,
        opts: &SuggestOpts,
        is_neighbor_edit: &dyn Fn(&str, &str) -> bool,
        // Extra exact-only match seqs (plan/22 letter-graph variants,
        // pre-encoded under `mapping`): a row matching one scores keyfit
        // 1.0. Never cached (they derive from the raw text, not the
        // digits — two raws can share a seq), never prefix-expanded.
        // Empty on every T9 path (behavior there is byte-identical).
        extra_exact: &[String],
    ) -> HashMap<String, MergedRow> {
        // Union across base + extensions, keyed by (norm word, lang).
        // Each row matches match-any over its (primary, aliases) seqs;
        // keyfit is the best across them. Alias ROWS (same word, distinct
        // seqs in separate rows) merge into one candidate instead of
        // collapsing to the first seq: frequencies sum, display follows
        // the highest-priority row, keyfit takes the max.
        let mut merged: HashMap<String, MergedRow> = HashMap::new();
        // Isolation scope for this query (pre-merge): the canonical tab
        // key once here, then a row-membership check per match (never per
        // row — non-matching rows skip before it).
        let tab = SuggestOpts::tab_key(active_tab);
        let personal_tab = tab == "personal";
        let scope = opts.hard_tab_filter;
        let keyfit_of = |eseq: &str| -> Option<f64> {
            if eseq == digits {
                Some(1.0)
            } else if !digits.is_empty() && opts.include_prefix && eseq.starts_with(digits) {
                // `starts_with("")` is true for every row: the QWERTY path
                // may pass empty digits (raw unencodable, variants live),
                // so the prefix phase needs the non-empty guard.
                // `suggest_inner` never passes empty digits (early return),
                // so T9 output is unchanged.
                Some(opts.prefix_keyfit)
            } else if opts.include_neighbor && is_neighbor_edit(digits, eseq) {
                Some(opts.neighbor_keyfit)
            } else {
                None
            }
        };
        let keyfit_any = |primary: &str, aliases: &[String]| -> Option<f64> {
            let mut best = if primary.is_empty() {
                None
            } else {
                keyfit_of(primary)
            };
            for a in aliases {
                if let Some(k) = keyfit_of(a) {
                    best = Some(best.map_or(k, |m: f64| m.max(k)));
                }
            }
            // Plan/22 extra exacts (variant seqs): exact-only at 1.0.
            // Empty `extra_exact` short-circuits (no T9 cost).
            if best.is_none() && !extra_exact.is_empty() && !primary.is_empty() {
                if extra_exact.iter().any(|v| v == primary)
                    || aliases.iter().any(|a| extra_exact.contains(a))
                {
                    best = Some(1.0);
                }
            }
            best
        };
        if PRECOMPUTED_LAYOUTS.contains(&layout_id) && self.seq_postings.contains_key(layout_id) {
            // Indexed path (plan/05 #1-2): exact posting lookup + FST
            // `StartsWith` prefix page + generative neighbor expansion.
            // Same match-any semantics as the scan below (best keyfit
            // across primary + aliases), merged in ascending row order so
            // first-wins display is identical.
            self.merge_indexed(digits, active_tab, layout_id, mapping, opts, &mut merged, extra_exact);
        } else {
            // Legacy per-entry scan: custom layouts with no index, or a
            // layout whose FST failed to build (explicit error via
            // `fst_build_error`, never a silent wrong-layout scan).
            // Ascending row indexes feed `merge_row`, matching the indexed
            // path's tie rules exactly.
            for (idx, e) in self.base.iter().chain(self.extensions.iter()).enumerate() {
                if scope && !self.static_row_allowed(e, &tab, personal_tab) {
                    continue;
                }
                let (primary, aliases) = seqs_of(e);
                let Some(keyfit) = keyfit_any(&primary, &aliases) else {
                    continue;
                };
                Self::merge_row(&mut merged, idx as u32, e, primary.into_owned(), keyfit);
            }
        }

        // Personal-OOV index: learned words with no static entry (QWERTY /
        // OOV commits) participate through the same pipeline. Entries whose
        // (norm, lang) collides with a static candidate merge (keyfit max,
        // never a duplicate row); the personal overlay below still supplies
        // freq_personal / recency / bigram. Encoding goes straight through
        // `mapping` into one reused buffer — no per-entry tmp `DictEntry`,
        // no `seq_of` closure alloc. Personal rows carry no tr/alt, so the
        // match set is just (primary). The empty-falls-back-to-canonical
        // rule matches the old `custom_seqs`/precompute paths: a word
        // unencodable under the layout falls back to the frozen t9-9
        // skeleton, and only a word unencodable under both is skipped.
        let mut enc_buf = String::new();
        // Shortest extra seq (plan/22): the length gate below must admit
        // words matching a short variant even when the raw seq is longer
        // (e.g. raw `teh` deletes to `te`). `None` on T9 paths (no extras)
        // keeps the gate byte-identical there.
        let extra_floor = extra_exact
            .iter()
            .map(|v| v.chars().count())
            .min();
        for p in self.personal.live_entries() {
            // Length gate: a word emits at most one code per char, so a
            // word with fewer chars than the digit string can never be an
            // exact, prefix, or same-length neighbor match. Skips the
            // encode + map work. (Char counts on both sides: sound for
            // multi-byte custom codes too.)
            let floor = extra_floor.map_or(digits.chars().count(), |m| {
                m.min(digits.chars().count())
            });
            if p.word.chars().count() < floor {
                continue;
            }
            // Personal overlay carries its category: an OOV learned under
            // one tab never leaks into another. (The `★personal` aggregate
            // tab is the one exception — it shows the user's own words
            // across categories.)
            if scope {
                let pcat = if p.category.is_empty() {
                    "personal"
                } else {
                    p.category.as_str()
                };
                if !(personal_tab || SuggestOpts::tab_key(pcat) == tab) {
                    continue;
                }
            }
            mapping.encode_word_into(&p.word, &mut enc_buf);
            if enc_buf.is_empty() {
                enc_buf.push_str(&encode_word(&p.word));
                if enc_buf.is_empty() {
                    continue;
                }
            }
            let primary = enc_buf.as_str();
            let keyfit = if primary == digits {
                1.0
            } else if !digits.is_empty() && opts.include_prefix && primary.starts_with(digits) {
                opts.prefix_keyfit
            } else if opts.include_neighbor
                && primary.len() == digits.len()
                && mapping.is_one_edit_neighbor(digits, primary)
            {
                opts.neighbor_keyfit
            } else if !extra_exact.is_empty() && extra_exact.iter().any(|v| v == primary) {
                // Plan/22 variant-exact (personal-OOV side): the learned
                // word encodes to a typo-variant seq.
                1.0
            } else {
                continue;
            };
            let norm = format!("{}\x1f{}", p.word.to_lowercase(), p.lang);
            match merged.get_mut(&norm) {
                Some(m) => {
                    if keyfit > m.keyfit {
                        m.keyfit = keyfit;
                    }
                }
                None => {
                    merged.insert(
                        norm,
                        MergedRow {
                            word: p.word.clone(),
                            seq: primary.to_string(),
                            freq_base: 0,
                            cat: if p.category.is_empty() {
                                "personal".to_string()
                            } else {
                                p.category.clone()
                            },
                            lang: p.lang.clone(),
                            priority: 100,
                            keyfit,
                            contributors: 0,
                            single_q: 0,
                            first_idx: u32::MAX,
                            min_idx: u32::MAX,
                        },
                    );
                }
            }
        }
        // Enable toggle (plan/08): disabled cats leave the union entirely
        // (static + personal-under-that-cat), so the tab hides everywhere
        // including the probe path that shares this union.
        let disabled = &self.disabled_cats;
        if !disabled.is_empty() {
            merged.retain(|_, m| !disabled.contains(&m.cat));
        }
        merged
    }

    /// Score union rows (blocked words skipped, sub-threshold scores
    /// hidden unless `apply_threshold` is false), unsorted. Shared by
    /// [`Self::suggest_inner`] and [`Self::probe_target_at`] (both pass
    /// `true`) and the QWERTY path (passes `false`: corrections filter
    /// downstream, the literal slot is never threshold-hidden).
    fn score_rows(
        &self,
        items: Vec<MergedRow>,
        prev: &str,
        active_tab: &str,
        now: i64,
        apply_threshold: bool,
    ) -> Vec<Scored> {
        let mut scored: Vec<Scored> = Vec::new();
        for m in items {
            if self.personal.is_blocked(&m.word) {
                continue;
            }
            let p = self.personal.get(&m.word);
            let (freq_personal, in_personal, acc, rej, last_seen) = match p {
                Some(e) if !e.deleted => (e.count, true, e.acc, e.rej, e.last_seen),
                _ => (0, false, 0, 0, 0),
            };
            let (bpw, bp, bv) = if prev.is_empty() {
                (0, 0, 1)
            } else {
                self.personal.bigram_counts(&prev, &m.word)
            };
            let input = RankInput {
                freq_base: m.freq_base,
                freq_personal,
                in_personal,
                bigram_pw: bpw,
                bigram_prev: bp,
                bigram_vocab: bv,
                now_ts: now,
                last_seen_ts: last_seen,
                cat_boost: self.cat_boost(&m.cat, in_personal, active_tab),
                keyfit: m.keyfit,
                accepts: acc,
                rejects: rej,
                // Plan/13 P1 temporal input: not yet plumbed on this path
                // (context_counts wiring is separate work); 0.0 = no data
                // and the weight defaults dormant, so scoring is unchanged.
                temporal: 0.0,
            };
            // Quantized fast path (plan/05 #7): a single static row with no
            // personal overlay scores its load-time fixed-point base term
            // (no per-keystroke `log10`). Merges and personal-boosted rows
            // take the exact log. Both are load/personal-state functions,
            // so repeats stay byte-identical.
            let base = if m.contributors == 1 && freq_personal == 0 {
                dequantize_base(m.single_q)
            } else {
                base_term(m.freq_base, freq_personal)
            };
            let score = score_candidate_with_base(&input, &self.weights, base);
            debug_assert!(
                !score.is_nan(),
                "suggest: score must never be NaN for {:?} (inputs cannot NaN: log10(>=1), bounded exp)",
                m.word
            );
            if apply_threshold && score < self.weights.hide_threshold {
                continue;
            }
            scored.push(Scored {
                word: m.word,
                seq: m.seq,
                cat: m.cat,
                lang: m.lang,
                score,
                priority: m.priority,
            });
        }
        scored
    }

    #[allow(clippy::too_many_arguments)]
    fn suggest_inner(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        limit: usize,
        layout_id: &str,
        seqs_of: &dyn Fn(&DictEntry) -> (Cow<'_, str>, Cow<'_, [String]>),
        mapping: &dyn KeyMapping,
        opts: &SuggestOpts,
        is_neighbor_edit: &dyn Fn(&str, &str) -> bool,
        is_code: &dyn Fn(char) -> bool,
        now: i64,
    ) -> Vec<Suggestion> {
        if digits.is_empty() || !digits.chars().all(is_code) || limit == 0 {
            return Vec::new();
        }
        debug_assert!(
            (0.0..=1.0).contains(&opts.prefix_keyfit),
            "SuggestOpts::prefix_keyfit out of range: {}",
            opts.prefix_keyfit
        );
        debug_assert!(
            (0.0..=1.0).contains(&opts.neighbor_keyfit),
            "SuggestOpts::neighbor_keyfit out of range: {}",
            opts.neighbor_keyfit
        );
        let prev = ctx.split_whitespace().last().unwrap_or("").to_lowercase();

        // Union construction lives in `build_union` (shared with the probe);
        // the merged set is already match-any over (primary, aliases) AND
        // already scoped to the active tab when `opts.hard_tab_filter`
        // (default ON) — no post-merge retain: display word/`cat` and
        // frequencies derive from tab-allowed contributors only.
        let merged = self.build_union(
            digits,
            active_tab,
            layout_id,
            seqs_of,
            mapping,
            opts,
            is_neighbor_edit,
            &[],
        );

        // Cap matches at 200 before the top-N heap (SPEC). The pre-truncate
        // order must be a TOTAL order: `merged` is a HashMap (RandomState
        // iteration), so sorting by `freq_base` alone lets equal-freq ties
        // resolve to a different 200-set on every call. (freq desc, word
        // asc, lang asc) is total over the dedupe key space.
        let mut items: Vec<MergedRow> = merged.into_values().collect();
        items.sort_by(|a, b| {
            b.freq_base
                .cmp(&a.freq_base)
                .then_with(|| a.word.cmp(&b.word))
                .then_with(|| a.lang.cmp(&b.lang))
        });
        items.truncate(200);

        // Single-press fallback (plan/17 Step 1): layout-derived letters
        // append POST-truncate (truncate-exempt) so low-freq synthetics
        // survive inside crowded unions (644-row `7*`). Skipped when the
        // letter already has a row (real pack rows win — never both, so
        // `merge_row` can never double-sum a synthetic with a seed row).
        // Explicit `cat` = active tab (the ONE isolation exception:
        // fallback chips show under every tab) with lowest priority and
        // `contributors == 1` so the quantized fast path applies.
        // No-learn is enforced by the caller: chips commit via `commitRaw`
        // (Android), never the learn path.
        for c in single_press_fallback_letters(digits, mapping) {
            let w = c.to_string();
            if items.iter().any(|m| m.word == w) {
                continue;
            }
            let freq = fallback_freq(c);
            items.push(MergedRow {
                word: w,
                seq: digits.to_string(),
                freq_base: freq,
                cat: active_tab.to_string(),
                lang: "en".to_string(),
                priority: -1,
                keyfit: 1.0,
                contributors: 1,
                single_q: quantize_base(freq),
                first_idx: u32::MAX,
                min_idx: u32::MAX,
            });
        }

        let mut scored = self.score_rows(items, &prev, active_tab, now, true);

        // Short-exact boost (plan/17 Step 2): exact-length matches at
        // len<=2 outrank longer prefixes (`s` before `some`); freq still
        // tie-breaks among boosted exacts. Len>=3 untouched by
        // construction (`digits_len > 2` skips the loop body entirely).
        let digits_len = digits.chars().count();
        if digits_len <= 2 {
            for s in scored.iter_mut() {
                if s.word.chars().count() == digits_len {
                    s.score += SHORT_EXACT_BOOST;
                }
            }
        }

        // Real top-N heap (plan/05 #3): `select_nth_unstable` partitions the
        // top `limit` in O(n) instead of fully sorting, then only the
        // survivors sort. Total-order comparator (score desc, len,
        // lexicographic, priority, lang) keeps every tie deterministic;
        // the input order is already total (freq desc, word, lang), so the
        // partition itself is repeat-stable.
        let mut scored_cmp = |a: &Scored, b: &Scored| cmp_scored(a, b);
        if scored.len() > limit {
            scored.select_nth_unstable_by(limit, &mut scored_cmp);
            scored.truncate(limit);
        }
        scored.sort_by(scored_cmp);
        scored
            .into_iter()
            .map(|s| Suggestion {
                word: s.word,
                score: s.score,
                seq: s.seq,
                cat: s.cat,
                layout_id: layout_id.to_string(),
            })
            .collect()
    }

    /// Fold one static row into the suggest union keyed by the row's
    /// precomputed `norm_key` (no `format!`/`to_lowercase` per match).
    /// Frequencies sum ACROSS DISTINCT ROWS (alias rows encoding the same
    /// word in separate rows), display follows the highest-priority row
    /// (ties to the smallest row index — both scan and indexed paths pass
    /// true row indexes, so the outcome is identical regardless of visit
    /// order), keyfit takes the max, `seq` comes from the smallest
    /// contributing row (what the legacy ascending scan stored first).
    /// Callers merge each row index AT MOST ONCE per query with its best
    /// keyfit: re-merging one row for a second matching seq would sum its
    /// own frequency twice (phantom +0.3 score inflation observed on
    /// multi-alias words, and cache-hit/cold disagreement).
    fn merge_row(
        merged: &mut HashMap<String, MergedRow>,
        idx: u32,
        e: &DictEntry,
        seq: String,
        keyfit: f64,
    ) {
        match merged.get_mut(&e.norm_key) {
            Some(m) => {
                m.freq_base += e.freq;
                m.contributors += 1;
                if idx < m.min_idx {
                    m.min_idx = idx;
                    m.seq = seq;
                }
                if e.priority > m.priority
                    || (e.priority == m.priority && idx < m.first_idx)
                {
                    m.cat = e.cat.clone();
                    m.word = e.word.clone();
                    m.priority = e.priority;
                    m.first_idx = idx;
                }
                if keyfit > m.keyfit {
                    m.keyfit = keyfit;
                }
            }
            None => {
                merged.insert(
                    e.norm_key.clone(),
                    MergedRow {
                        word: e.word.clone(),
                        seq,
                        freq_base: e.freq,
                        cat: e.cat.clone(),
                        lang: e.lang.clone(),
                        priority: e.priority,
                        keyfit,
                        contributors: 1,
                        single_q: e.base_q,
                        first_idx: idx,
                        min_idx: idx,
                    },
                );
            }
        }
    }

    /// Indexed match (plan/05 #1-2 + cache #6): exact posting-list lookup,
    /// FST `StartsWith` prefix page, and generative 1-edit neighbor
    /// expansion through `neighbor_codes` (~50 lookups for 7-digit input).
    /// Each phase records its hits into a per-row best-keyfit table and
    /// every row index merges exactly once at the end ([`Self::merge_row`]
    /// sums frequencies, so a second visit would double-count one entry —
    /// the table keeps the max keyfit, matching the scan path's
    /// match-any/best-keyfit rule). Insertion order into the union does not
    /// matter: `merge_row` is order-independent, so the FST-stream order
    /// needs no reordering.
    /// `opts.include_neighbor == false` skips generation (the gate's
    /// precision arm); `opts.include_prefix == false` skips the FST prefix
    /// page and cached prefix rows (the prefix-toggle experiment).
    ///
    /// Prefix-result cache: exact+prefix row sets memoize per
    /// `(layout_id, digits)` (cap 16, behavior-neutral under a FIXED opts:
    /// tags are reinterpreted through the current `opts` at merge time, and
    /// prefix rows are skipped outright when `include_prefix` is false, so
    /// mixed-opts call sequences still return identical output to cold
    /// runs). An exact hit merges the set directly; a parent (`digits`
    /// minus one code) hit filters the parent set with borrow-only keyfit
    /// checks instead of re-running the FST page. Neighbors regenerate
    /// every keystroke (cheap: `len x ~8` lookups) on all three paths.
    ///
    /// `extra_exact` (plan/22 letter-graph variants): exact posting lookups
    /// merged into the best-keyfit table on ALL THREE paths (never cached —
    /// variants derive from the raw text, and two raws can share a seq).
    /// Empty on every T9 path.
    #[allow(clippy::too_many_arguments)]
    fn merge_indexed(
        &self,
        digits: &str,
        active_tab: &str,
        layout_id: &str,
        mapping: &dyn KeyMapping,
        opts: &SuggestOpts,
        merged: &mut HashMap<String, MergedRow>,
        extra_exact: &[String],
    ) {
        let postings = self.seq_postings.get(layout_id).unwrap_or_else(|| {
            panic!(
                "DictionaryStack::merge_indexed: no posting index for layout {layout_id:?} \
                 (guard with seq_postings.contains_key)"
            )
        });
        let key = (
            layout_id.to_string(),
            digits.to_string(),
            SINGLE_PRESS_FALLBACK_VERSION,
        );
        // Per-row best keyfit across exact/prefix/neighbor phases: each row
        // index merges exactly once below, so multi-seq rows (primary +
        // aliases) never sum their own frequency twice.
        let mut hits: HashMap<u32, f64> = HashMap::new();
        // Path 1: exact cache hit — merge the memoized exact+prefix set.
        {
            let cache = self.prefix_cache.borrow();
            if let Some(rows) = cache.get(&key) {
                for &(idx, tag) in rows.iter() {
                    if tag != 1 && !opts.include_prefix {
                        continue;
                    }
                    Self::note_hit(&mut hits, idx, if tag == 1 { 1.0 } else { opts.prefix_keyfit });
                }
                drop(cache);
                if opts.include_neighbor {
                    self.merge_neighbors(digits, mapping, postings, opts.neighbor_keyfit, &mut hits);
                }
                self.merge_extra_exact(postings, extra_exact, &mut hits);
                self.merge_hits(active_tab, layout_id, hits, merged, opts);
                return;
            }
            // Path 2: parent hit — exact+prefix matches only shrink as
            // digits grow, so filter the parent set with borrow-only
            // checks (no FST page, no encodes). Skipped for empty digits
            // (the QWERTY unencodable-raw shape): `starts_with("")` would
            // otherwise match every parent row.
            if !digits.is_empty() {
            if let Some(parent) = digits.get(..digits.len().saturating_sub(1)) {
                let pkey = (
                    layout_id.to_string(),
                    parent.to_string(),
                    SINGLE_PRESS_FALLBACK_VERSION,
                );
                if let Some(parent_rows) = cache.get(&pkey) {
                    let mut mine: Vec<(u32, u8)> = Vec::new();
                    for &(idx, _) in parent_rows.iter() {
                        let e = self.row(idx);
                        let mut tag: Option<u8> = None;
                        let primary = e.seq_for_layout(layout_id);
                        if primary == digits {
                            tag = Some(1);
                        } else if opts.include_prefix && primary.starts_with(digits) {
                            tag = Some(0);
                        }
                        if tag != Some(1) {
                            for a in e.aliases_for_layout(layout_id) {
                                if a.as_str() == digits {
                                    tag = Some(1);
                                    break;
                                } else if opts.include_prefix && a.starts_with(digits) {
                                    tag = tag.or(Some(0));
                                }
                            }
                        }
                        if let Some(t) = tag {
                            mine.push((idx, t));
                            Self::note_hit(&mut hits, idx, if t == 1 { 1.0 } else { opts.prefix_keyfit });
                        }
                    }
                    drop(cache);
                    mine.sort_unstable();
                    let mut cache = self.prefix_cache.borrow_mut();
                    if cache.len() >= PREFIX_CACHE_CAP {
                        cache.clear();
                    }
                    cache.insert(key, mine);
                    if opts.include_neighbor {
                        self.merge_neighbors(digits, mapping, postings, opts.neighbor_keyfit, &mut hits);
                    }
                    self.merge_extra_exact(postings, extra_exact, &mut hits);
                    self.merge_hits(active_tab, layout_id, hits, merged, opts);
                    return;
                }
            }
            } // end empty-digits guard (Path 2 needs non-empty digits)
        }
        // Path 3: cold — exact lookup + FST prefix page, recording the
        // exact+prefix set for the cache. Phases run exact-first so the
        // common top hit never waits on the prefix page; `merge_row`
        // takes the max keyfit, so phase order cannot change the result.
        // The prefix page is skipped outright when `include_prefix` is
        // false (exact-only experiment arm).
        let mut mine: Vec<(u32, u8)> = Vec::new();
        if !digits.is_empty() {
            if let Some(rows) = postings.get(digits) {
                for &r in rows {
                    Self::note_hit(&mut hits, r, 1.0);
                    mine.push((r, 1));
                }
            }
        }
        // Empty digits (QWERTY unencodable-raw shape) skip the prefix page:
        // `StartsWith("")` would otherwise page the whole index.
        if !digits.is_empty() && opts.include_prefix {
            if let Some(set) = self.fst_by_layout.get(layout_id) {
                let auto = fst::automaton::Str::new(digits).starts_with();
                let mut stream = set.search(auto).into_stream();
                use fst::{Automaton, IntoStreamer, Streamer};
                while let Some(k) = stream.next() {
                    if let Ok(s) = std::str::from_utf8(k) {
                        if s == digits {
                            continue;
                        }
                        if let Some(rows) = postings.get(s) {
                            for &r in rows {
                                // Best-keyfit table (not a direct merge): a
                                // row reachable via two prefix seqs (primary
                                // + alias) records one hit, so its frequency
                                // counts once. `mine` may hold (r,1)+(r,0);
                                // the exact-first dedup below keeps tag 1.
                                Self::note_hit(&mut hits, r, opts.prefix_keyfit);
                                mine.push((r, 0));
                            }
                        }
                    }
                }
            }
        }
        // One row can contribute an exact seq and a prefix seq (primary +
        // alias): sort exact-first per row, dedupe keeping the exact tag.
        mine.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
        mine.dedup_by(|a, b| a.0 == b.0);
        {
            let mut cache = self.prefix_cache.borrow_mut();
            if cache.len() >= PREFIX_CACHE_CAP {
                cache.clear();
            }
            cache.insert(key, mine);
        }
        if opts.include_neighbor {
            self.merge_neighbors(digits, mapping, postings, opts.neighbor_keyfit, &mut hits);
        }
        self.merge_extra_exact(postings, extra_exact, &mut hits);
        self.merge_hits(active_tab, layout_id, hits, merged, opts);
    }

    /// Plan/22 extra-exact phase: every variant seq becomes an exact
    /// posting-list lookup at keyfit 1.0, recorded into the best-keyfit
    /// table (never merged directly, never cached). A row reachable via
    /// two variants still counts its frequency once; a row the exact phase
    /// already hit keeps the max (1.0 either way).
    fn merge_extra_exact(
        &self,
        postings: &HashMap<String, Vec<u32>>,
        extra_exact: &[String],
        hits: &mut HashMap<u32, f64>,
    ) {
        for v in extra_exact {
            if let Some(rows) = postings.get(v.as_str()) {
                for &r in rows {
                    Self::note_hit(hits, r, 1.0);
                }
            }
        }
    }

    /// Record one phase hit: keep the best keyfit per row index so the
    /// row merges exactly once below.
    fn note_hit(hits: &mut HashMap<u32, f64>, idx: u32, keyfit: f64) {
        hits.entry(idx)
            .and_modify(|k| {
                if keyfit > *k {
                    *k = keyfit;
                }
            })
            .or_insert(keyfit);
    }

    /// Merge every hit row into the union exactly once with its best
    /// keyfit. Iteration order over `hits` is irrelevant: `merge_row` is
    /// insertion-order independent by construction.
    fn merge_hits(
        &self,
        active_tab: &str,
        layout_id: &str,
        hits: HashMap<u32, f64>,
        merged: &mut HashMap<String, MergedRow>,
        opts: &SuggestOpts,
    ) {
        // Pre-merge isolation scope (see `build_union`): disallowed rows
        // never merge, so display/freq derive from tab-allowed
        // contributors only. Tab-agnostic prefix-cache sets flow through
        // here too — scoping at merge keeps mixed-tab sequences identical
        // to cold runs.
        let tab = SuggestOpts::tab_key(active_tab);
        let personal_tab = tab == "personal";
        let scope = opts.hard_tab_filter;
        for (idx, keyfit) in hits {
            let e = self.row(idx);
            if scope && !self.static_row_allowed(e, &tab, personal_tab) {
                continue;
            }
            Self::merge_row(
                merged,
                idx,
                e,
                e.seq_for_layout(layout_id).to_string(),
                keyfit,
            );
        }
    }

    /// Generative 1-edit neighbor expansion: every single-code
    /// substitution to an adjacent key becomes an exact posting-list
    /// lookup (`len x ~8` lookups, zero per-entry allocs beyond the one
    /// reused variant buffer). Hits record into the best-keyfit table
    /// (never merged directly): exact/prefix phases already ran, and the
    /// table keeps the max, so earlier stronger matches always win while a
    /// row reachable via two neighbor variants still counts its frequency
    /// once.
    fn merge_neighbors(
        &self,
        digits: &str,
        mapping: &dyn KeyMapping,
        postings: &HashMap<String, Vec<u32>>,
        neighbor_keyfit: f64,
        hits: &mut HashMap<u32, f64>,
    ) {
        let mut variant = String::with_capacity(digits.len() + 1);
        for (i, c) in digits.char_indices() {
            for nc in mapping.neighbor_codes(c) {
                variant.clear();
                variant.push_str(digits);
                let mut nb = [0u8; 4];
                variant.replace_range(i..i + c.len_utf8(), nc.encode_utf8(&mut nb));
                if let Some(rows) = postings.get(variant.as_str()) {
                    for &r in rows {
                        Self::note_hit(hits, r, neighbor_keyfit);
                    }
                }
            }
        }
    }

    /// Load-time inflection sidecar (prototype, flag-gated): expand
    /// in-vocab EN stems into discounted synthetic rows
    /// ([`crate::inflect::expand_entries`]) and reindex. The call IS the
    /// flag — default-constructed stacks never contain synthetic rows
    /// (locked by `inflect_proto::flag_off_is_byte_identical`). Returns the
    /// number of synthetic rows appended. Idempotence guard: a second call
    /// with the same config adds nothing (minted forms now sit in the
    /// vocab set), so repeated calls are safe but wasteful — call once.
    pub fn expand_inflections(&mut self, cfg: &crate::inflect::InflectConfig) -> usize {
        let vocab: std::collections::HashSet<String> = self
            .base
            .iter()
            .chain(self.extensions.iter())
            .map(|e| e.norm_key.clone())
            .collect();
        let mut rows =
            crate::inflect::expand_entries(&self.base, &vocab, cfg);
        let vocab2: std::collections::HashSet<String> = self
            .extensions
            .iter()
            .map(|e| e.norm_key.clone())
            .collect();
        // Extensions expand against base+extensions vocab so cross-partition
        // duplicates never mint (base forms were covered by `vocab` above;
        // extension-vs-extension dupes merge via the union anyway, but
        // minting them would double-count freq — suppress here).
        let mut ext_vocab = vocab;
        ext_vocab.extend(vocab2);
        rows.extend(crate::inflect::expand_entries(&self.extensions, &ext_vocab, cfg));
        let n = rows.len();
        self.extensions.extend(rows);
        self.rebuild_index();
        n
    }

    /// Next-word connection surface (prototype): with NO digits typed yet,
    /// rank the personal bigram followers of `ctx`'s last token by learned
    /// count (desc, then word asc — deterministic). Empty ctx, unknown prev,
    /// or `limit == 0` yields `[]`. Blocked words and other-tab rows are
    /// excluded under the same isolation rule as suggest (plus the
    /// `★personal` aggregate exception). This is a NEW surface: no existing
    /// suggest path calls it, so it cannot move gate numbers by
    /// construction (gate never queries empty digits — `suggest_inner`
    /// returns `[]` there, unchanged).
    pub fn suggest_next_at(
        &self,
        ctx: &str,
        active_tab: &str,
        limit: usize,
        _now: i64,
    ) -> Vec<NextSuggestion> {
        if limit == 0 {
            return Vec::new();
        }
        let prev = ctx.split_whitespace().last().unwrap_or("").to_lowercase();
        if prev.is_empty() {
            return Vec::new();
        }
        let tab = SuggestOpts::tab_key(active_tab);
        let personal_tab = tab == "personal";
        let mut out: Vec<NextSuggestion> = Vec::new();
        for (word, count) in self.personal.top_followers(&prev) {
            if out.len() >= limit {
                break;
            }
            if self.personal.is_blocked(&word) {
                continue;
            }
            // Category for isolation: the personal overlay first (the user
            // may have re-learned a static word under another tab); else
            // the highest-priority static row (a bigram-only follower the
            // user typed but never unigram-learned keeps its pack tab);
            // else the personal aggregate (true OOV).
            let cat = match self.personal.get(&word) {
                Some(e) if !e.category.is_empty() => e.category.clone(),
                _ => self
                    .static_cat(&word)
                    .unwrap_or_else(|| "personal".to_string()),
            };
            if !(personal_tab || SuggestOpts::tab_key(&cat) == tab) {
                continue;
            }
            out.push(NextSuggestion {
                word,
                count,
                cat,
            });
        }
        out.truncate(limit);
        out
    }

    /// Highest-priority static `cat` for `word` (case-insensitive), or
    /// `None` when no static row carries it. Backs next-word tab scoping
    /// for bigram-only followers (typed in a pair, never unigram-learned).
    fn static_cat(&self, word: &str) -> Option<String> {
        let norm = word.to_lowercase();
        let mut best: Option<(&str, i32)> = None;
        for e in self.base.iter().chain(self.extensions.iter()) {
            if e.word.to_lowercase() == norm && best.map(|b| e.priority > b.1).unwrap_or(true) {
                best = Some((&e.cat, e.priority));
            }
        }
        best.map(|b| b.0.to_string())
    }

    /// Dictionary placement info for long-press popup:
    /// `pack cat • freq • accepts`.
    pub fn placement(&self, word: &str) -> Option<String> {
        let norm = word.to_lowercase();
        let mut best: Option<&DictEntry> = None;
        for e in self.base.iter().chain(self.extensions.iter()) {
            if e.word.to_lowercase() == norm && best.map(|b| e.priority > b.priority).unwrap_or(true)
            {
                best = Some(e);
            }
        }
        best.map(|e| {
            let acc = self.personal.get(word).map(|p| p.acc).unwrap_or(0);
            format!("{} • freq {} • accepts {}", e.cat, e.freq, acc)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::load_pack_str;

    fn base() -> Vec<DictEntry> {
        DictionaryStack::load_base_json(
            r#"[{"w":"hello","freq":900,"cat":"EN"},
                {"w":"hell","freq":100,"cat":"EN"},
                {"w":"help","freq":200,"cat":"EN"}]"#,
        )
        .unwrap()
    }

    #[test]
    fn personal_oov_participates_in_suggest() {
        let mut stack = DictionaryStack::new(base());
        // "qzx" -> 799 matches no static entry (hello 43556, hell 4355, ...).
        assert!(stack
            .suggest("", "799", "EN", 5)
            .iter()
            .all(|c| c.word != "qzx"));
        stack.personal.learn("qzx", "EN");
        stack.personal.learn("qzx", "EN");
        let s = stack.suggest("", "799", "EN", 5);
        assert!(
            s.iter().any(|c| c.word == "qzx"),
            "learned OOV must suggest, got {:?}",
            s.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn personal_alias_merges_never_duplicates() {
        let mut stack = DictionaryStack::new(base());
        stack.personal.learn("HELLO", "EN");
        stack.personal.learn("HELLO", "EN");
        let s = stack.suggest("", "43556", "EN", 5);
        assert_eq!(
            s.iter().filter(|c| c.word.to_lowercase() == "hello").count(),
            1,
            "alias must merge into one row"
        );
        assert_eq!(s[0].word.to_lowercase(), "hello");
    }

    #[test]
    fn truncate_is_total_order_repeat_stable() {
        // 3^5 = 243 words over {a,b,c}, all encoding to "22222" with equal
        // freq: the 200-truncate MUST resolve ties totally. `merged` is a
        // HashMap with per-instance RandomState, so any freq-only sort would
        // pick a different 200-set across calls.
        let mut words = Vec::new();
        for a in ['a', 'b', 'c'] {
            for b in ['a', 'b', 'c'] {
                for c in ['a', 'b', 'c'] {
                    for d in ['a', 'b', 'c'] {
                        for e in ['a', 'b', 'c'] {
                            words.push(format!("{a}{b}{c}{d}{e}"));
                        }
                    }
                }
            }
        }
        assert_eq!(words.len(), 243);
        let json = format!(
            "[{}]",
            words
                .iter()
                .map(|w| format!(r#"{{"w":"{w}","freq":100,"cat":"EN"}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let stack = DictionaryStack::new(DictionaryStack::load_base_json(&json).unwrap());
        let first = stack.suggest("", "22222", "EN", 30);
        assert_eq!(first.len(), 30);
        for _ in 0..50 {
            let again = stack.suggest("", "22222", "EN", 30);
            assert_eq!(
                first.iter().map(|s| &s.word).collect::<Vec<_>>(),
                again.iter().map(|s| &s.word).collect::<Vec<_>>(),
                "suggest must be repeat-stable under equal-freq ties"
            );
        }
        // Total order => the survivors are the lexicographically first 200.
        let all = stack.suggest("", "22222", "EN", 200);
        assert_eq!(all.len(), 200);
        let mut lex: Vec<String> = words.clone();
        lex.sort();
        assert_eq!(all[0].word, lex[0]);
        assert_eq!(all[199].word, lex[199]);
    }

    #[test]
    fn prefix_cache_typing_chain_matches_fresh_queries() {
        // Plan/05 #6: sequential keystrokes ("4".."43556") populate and
        // filter the prefix cache; every chained result must equal the
        // same query on a cold index, under both neighbor arms and every
        // built-in layout.
        let stack = DictionaryStack::new(base());
        let reg = LayoutRegistry::with_builtins();
        let now = crate::personal::now_quantized();
        for layout_id in ["t9-9", "t9-12", "t9-16"] {
            let mapping = reg.get_or_default(layout_id).clone();
            // Digits valid under each layout's own code set.
            let chain: &[&str] = match layout_id {
                "t9-16" => &["4", "43", "435", "4355", "43556"],
                _ => &["4", "43", "435", "4355", "43556"],
            };
            for digits in chain {
                let chained = stack.suggest_for_layout_at("", digits, &mapping, "EN", 5, now);
                let chained_off =
                    stack.suggest_for_layout_no_neighbor_at("", digits, &mapping, "EN", 5, now);
                // Fresh stack = cold cache: identical rows must come back.
                let fresh = DictionaryStack::new(base());
                let expect = fresh.suggest_for_layout_at("", digits, &mapping, "EN", 5, now);
                let expect_off =
                    fresh.suggest_for_layout_no_neighbor_at("", digits, &mapping, "EN", 5, now);
                let words = |s: Vec<Suggestion>| {
                    s.into_iter().map(|x| (x.word, x.score.to_bits())).collect::<Vec<_>>()
                };
                assert_eq!(words(chained), words(expect), "chain {layout_id} {digits}");
                assert_eq!(words(chained_off), words(expect_off), "chain-off {layout_id} {digits}");
            }
        }
    }

    #[test]
    fn opts_default_parity_with_legacy_arms() {
        // `SuggestOpts::default()` must reproduce `suggest` exactly, and
        // `neighbor_off()` must reproduce `suggest_no_neighbor_at`, across
        // exact / prefix / neighbor / miss shapes (frozen-behavior lock).
        let stack = DictionaryStack::new(base());
        let now = crate::personal::now_quantized();
        let words = |s: Vec<Suggestion>| {
            s.into_iter().map(|x| (x.word, x.score.to_bits())).collect::<Vec<_>>()
        };
        for digits in ["4", "43", "435", "4355", "43556", "43555", "999"] {
            assert_eq!(
                words(stack.suggest_at("", digits, "EN", 5, now)),
                words(stack.suggest_with_opts_at("", digits, "EN", 5, now, &SuggestOpts::default())),
                "default-opts parity at {digits}",
            );
            assert_eq!(
                words(stack.suggest_no_neighbor_at("", digits, "EN", 5, now)),
                words(stack.suggest_with_opts_at("", digits, "EN", 5, now, &SuggestOpts::neighbor_off())),
                "neighbor-off parity at {digits}",
            );
        }
    }

    #[test]
    fn policy_defaults_to_neighbor_off_everywhere() {
        // No tab has cleared 09#6's ≥30% mis-press bar, so every tab ships
        // neighbor-OFF until a per-tab opt-in is measured.
        for tab in ["EN", "words", "NE", "js", "medical", "personal", "emoji"] {
            let opts = SuggestOpts::policy_for_tab(tab);
            assert!(
                !opts.include_neighbor,
                "policy must default neighbor OFF for tab {tab:?}"
            );
        }
    }

    #[test]
    fn policy_hard_filters_every_tab() {
        // Isolation is default-ON everywhere, not opt-in per tab: the
        // policy scopes content, code, custom, and overlay tabs alike.
        // (`""` maps to EN upstream, so it filters to nothing here — the
        // UniFFI boundary never passes it through.)
        for tab in [
            "EN",
            "words",
            "NE",
            "ne",
            "numbers",
            "js",
            "medical",
            "rust",
            "html",
            "emoji",
            "math",
            "personal",
            "★personal",
            "my_contacts",
            "",
        ] {
            assert!(
                SuggestOpts::policy_for_tab(tab).hard_tab_filter,
                "policy must hard-filter {tab:?}"
            );
        }
        assert!(SuggestOpts::default().hard_tab_filter);
        // The opt-out vehicle is explicit and measurable, never silent.
        assert!(!SuggestOpts::default().with_tab_filter(false).hard_tab_filter);
    }

    #[test]
    fn policy_matches_off_arm_on_prose_tabs() {
        // On single-cat fixtures the policy IS the neighbor-OFF arm
        // exactly (the filter is a no-op with no cross-tab rows present).
        let stack = DictionaryStack::new(base());
        let now = crate::personal::now_quantized();
        let words = |s: Vec<Suggestion>| {
            s.into_iter().map(|x| (x.word, x.score.to_bits())).collect::<Vec<_>>()
        };
        for digits in ["4", "43", "435", "4355", "43556", "43555"] {
            assert_eq!(
                words(stack.suggest_policy_at("", digits, "EN", 5, now)),
                words(stack.suggest_no_neighbor_at("", digits, "EN", 5, now)),
                "policy must equal OFF arm at {digits}",
            );
        }
    }

    #[test]
    fn policy_hard_filter_scopes_code_tabs() {
        // EN "hello"(43556, freq 900) outranks js "hellx"(43559, freq 50)
        // at prefix 4355 under the explicitly unfiltered OFF arm; the
        // js-tab policy must scope the union to js rows only.
        let entries = DictionaryStack::load_base_json(
            r#"[{"w":"hello","freq":900,"cat":"EN"},
                {"w":"hellx","freq":50,"cat":"js"}]"#,
        )
        .unwrap();
        let stack = DictionaryStack::new(entries);
        let now = crate::personal::now_quantized();
        let nofilter = SuggestOpts::neighbor_off().with_tab_filter(false);
        let unfiltered = stack.suggest_with_opts_at("", "4355", "js", 5, now, &nofilter);
        assert!(
            unfiltered.iter().any(|s| s.cat == "EN"),
            "fixture needs EN cross-tab noise, got {:?}",
            unfiltered.iter().map(|s| &s.word).collect::<Vec<_>>()
        );
        let policy = stack.suggest_policy_at("", "4355", "js", 5, now);
        assert!(!policy.is_empty(), "js policy must keep the js row");
        assert!(
            policy.iter().all(|s| s.cat == "js"),
            "hard filter must drop EN rows, got {:?}",
            policy.iter().map(|s| &s.word).collect::<Vec<_>>()
        );
        assert_eq!(policy[0].word, "hellx");
        // And the policy entry point equals the explicit opts arm.
        let explicit = stack.suggest_with_opts_at(
            "",
            "4355",
            "js",
            5,
            now,
            &SuggestOpts::policy_for_tab("js"),
        );
        assert_eq!(
            policy.iter().map(|s| &s.word).collect::<Vec<_>>(),
            explicit.iter().map(|s| &s.word).collect::<Vec<_>>()
        );
    }

    fn words_emoji_stack() -> DictionaryStack {
        // English base (hello/hell/heart) + emoji pack (❤️ seq 432, alt
        // heart): the heart/emoji-vs-English isolation fixture. Digits
        // "43" prefix-match both tabs' rows, so every assertion below is
        // load-bearing (the unfiltered union WOULD mix them).
        let mut stack = DictionaryStack::new(
            DictionaryStack::load_base_json(
                r#"[{"w":"hello","freq":900,"cat":"EN"},
                    {"w":"hell","freq":100,"cat":"EN"},
                    {"w":"heart","freq":80,"cat":"EN"}]"#,
            )
            .unwrap(),
        );
        let pack = load_pack_str(
            r#"{"id":"emoji","title":"Emoji","version":"1.0.0",
                "words":[{"w":"❤️","seq":"432","freq":9000,"cat":"emoji","alt":["heart"]}]}"#,
        )
        .unwrap();
        assert!(pack.validate().is_empty());
        stack.add_pack(&pack, 40);
        stack
    }

    #[test]
    fn tab_key_folds_legacy_aliases() {
        // Legacy display labels and pack ids canonicalize together, so IME
        // asset ids (`words`/`ne`) and core legacy cats (`EN`/`NE`) scope
        // identically. Custom cats canonicalize to themselves.
        assert_eq!(SuggestOpts::tab_key("EN"), "words");
        assert_eq!(SuggestOpts::tab_key("en"), "words");
        assert_eq!(SuggestOpts::tab_key("words"), "words");
        assert_eq!(SuggestOpts::tab_key("NE"), "ne");
        assert_eq!(SuggestOpts::tab_key("ne"), "ne");
        assert_eq!(SuggestOpts::tab_key("nepali"), "ne");
        assert_eq!(SuggestOpts::tab_key("★personal"), "personal");
        assert_eq!(SuggestOpts::tab_key("personal"), "personal");
        assert_eq!(SuggestOpts::tab_key("emoji"), "emoji");
        assert_eq!(SuggestOpts::tab_key("my_contacts"), "my_contacts");
        assert!(SuggestOpts::is_personal_tab("personal"));
        assert!(SuggestOpts::is_personal_tab("★personal"));
        assert!(!SuggestOpts::is_personal_tab("words"));
    }

    #[test]
    fn tab_isolation_heart_never_under_words() {
        // heart/emoji must never appear under the English tab: English
        // shows only English, on full seqs and prefix slices alike.
        let stack = words_emoji_stack();
        let now = crate::personal::now_quantized();
        let heart_seq = encode_word("heart");
        let s = stack.suggest_policy_at("", &heart_seq, "words", 30, now);
        assert_eq!(s[0].word, "heart", "English heart must rank 1");
        assert!(
            s.iter().all(|c| SuggestOpts::tab_key(&c.cat) == "words"),
            "words tab must show only English, got {:?}",
            s.iter().map(|c| (&c.word, &c.cat)).collect::<Vec<_>>()
        );
        for digits in ["4", "43", "432", &heart_seq] {
            let s = stack.suggest_policy_at("", digits, "words", 30, now);
            assert!(
                !s.is_empty(),
                "words tab must keep its own rows at {digits}"
            );
            assert!(
                s.iter().all(|c| c.word != "❤️"),
                "❤️ must never show under words at {digits}, got {:?}",
                s.iter().map(|c| &c.word).collect::<Vec<_>>()
            );
        }
        // Legacy entry points filter identically (not opt-in).
        let s = stack.suggest_at("", "43", "words", 30, now);
        assert!(s.iter().all(|c| c.word != "❤️"));
    }

    #[test]
    fn tab_isolation_hello_never_under_emoji() {
        // ... and symmetrically: hello must never appear under emoji.
        let stack = words_emoji_stack();
        let now = crate::personal::now_quantized();
        let s = stack.suggest_policy_at("", "432", "emoji", 30, now);
        assert_eq!(s[0].word, "❤️", "❤️ must rank 1 under emoji");
        // Every suggest path scopes, not just the policy vehicle.
        let reg = LayoutRegistry::builtins_cached();
        let t99 = reg.get_or_default("t9-9");
        let paths: Vec<Vec<Suggestion>> = vec![
            stack.suggest_at("", "43", "emoji", 30, now),
            stack.suggest_no_neighbor_at("", "43", "emoji", 30, now),
            stack.suggest_policy_at("", "43", "emoji", 30, now),
            stack.suggest_for_layout_at("", "43", t99, "emoji", 30, now),
            stack.suggest_for_layout_policy_at("", "43", t99, "emoji", 30, now),
            stack.suggest_with_opts_at("", "43", "emoji", 30, now, &SuggestOpts::default()),
        ];
        for (i, s) in paths.iter().enumerate() {
            assert!(
                !s.is_empty(),
                "path {i}: emoji tab must keep its own rows"
            );
            assert!(
                s.iter().all(|c| SuggestOpts::tab_key(&c.cat) == "emoji"),
                "path {i}: emoji tab must show only emoji, got {:?}",
                s.iter().map(|c| (&c.word, &c.cat)).collect::<Vec<_>>()
            );
            assert!(
                s.iter().all(|c| c.word != "hello" && c.word != "heart"),
                "path {i}: English words must never show under emoji",
            );
        }
        // The probe agrees (hello is outside the emoji union entirely).
        let p = stack.probe_target_at("", "43", "emoji", "hello", 30, now, &SuggestOpts::default());
        assert!(!p.in_union && p.top_rank.is_none());
        // Explicit opt-out restores the cross-tab union for measurement.
        let mixed = stack.suggest_with_opts_at(
            "",
            "43",
            "emoji",
            30,
            now,
            &SuggestOpts::default().with_tab_filter(false),
        );
        assert!(
            mixed.iter().any(|c| c.word == "hello"),
            "opt-out must restore cross-tab rows, got {:?}",
            mixed.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn personal_overlay_scoped_to_learn_category() {
        // An OOV learned under emoji must not leak into English (and vice
        // versa); it shows under its own tab and the ★personal aggregate.
        let mut stack = words_emoji_stack();
        let now = crate::personal::now_quantized();
        let seq = encode_word("heaz"); // 4329: no static row matches
        assert!(
            stack
                .suggest_policy_at("", &seq, "emoji", 30, now)
                .iter()
                .all(|c| c.word != "heaz")
        );
        stack.personal.learn("heaz", "emoji");
        stack.personal.learn("heaz", "emoji");
        let under_emoji = stack.suggest_policy_at("", &seq, "emoji", 30, now);
        assert!(
            under_emoji.iter().any(|c| c.word == "heaz"),
            "learned OOV must show under its tab, got {:?}",
            under_emoji.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
        let under_words = stack.suggest_policy_at("", &seq, "words", 30, now);
        assert!(
            under_words.iter().all(|c| c.word != "heaz"),
            "emoji-learned OOV must not leak into English, got {:?}",
            under_words.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
        let under_personal = stack.suggest_policy_at("", &seq, "personal", 30, now);
        assert!(
            under_personal.iter().any(|c| c.word == "heaz"),
            "★personal aggregates the user's own words, got {:?}",
            under_personal.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn personal_tab_aggregates_own_words_only() {
        // Static rows the user never touched never appear under ★personal;
        // touched ones do (any learned category).
        let mut stack = words_emoji_stack();
        let now = crate::personal::now_quantized();
        let untouched = stack.suggest_policy_at("", "43", "personal", 30, now);
        assert!(
            untouched.iter().all(|c| c.word != "hello" && c.word != "❤️"),
            "untouched static rows must not show under ★personal, got {:?}",
            untouched.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
        stack.personal.learn("hello", "words");
        stack.personal.learn("hello", "words");
        let touched = stack.suggest_policy_at("", "43556", "personal", 30, now);
        assert!(
            touched.iter().any(|c| c.word == "hello"),
            "touched words aggregate under ★personal, got {:?}",
            touched.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn neighbor_opt_in_restores_default_arm() {
        // The per-tab override vehicle: opting back in reproduces the
        // frozen neighbor-ON output exactly (measured, never silent).
        assert_eq!(
            SuggestOpts::policy_for_tab("EN").with_neighbor(true).include_neighbor,
            SuggestOpts::default().include_neighbor
        );
        let stack = DictionaryStack::new(base());
        let now = crate::personal::now_quantized();
        let words = |s: Vec<Suggestion>| {
            s.into_iter().map(|x| (x.word, x.score.to_bits())).collect::<Vec<_>>()
        };
        // "43555" is a 1-edit neighbor of hello's 43556: ON finds it, the
        // default-OFF policy does not, the opt-in does.
        let on = words(stack.suggest_at("", "43555", "EN", 5, now));
        assert!(on.iter().any(|(w, _)| *w == "hello"));
        let policy = words(stack.suggest_policy_at("", "43555", "EN", 5, now));
        assert!(!policy.iter().any(|(w, _)| *w == "hello"));
        let opt_in = words(stack.suggest_with_opts_at(
            "",
            "43555",
            "EN",
            5,
            now,
            &SuggestOpts::policy_for_tab("EN").with_neighbor(true),
        ));
        assert_eq!(opt_in, on, "opt-in must reproduce the ON arm exactly");
    }

    #[test]
    fn exact_only_is_subset_of_default() {
        // Disabling prefix+neighbor can only remove candidates, never add
        // or reorder survivors (same union minus prefix/neighbor rows, same
        // scores, same total order).
        let stack = DictionaryStack::new(base());
        let now = crate::personal::now_quantized();
        for digits in ["4", "43", "435", "4355", "43556"] {
            let full = stack.suggest_with_opts_at("", digits, "EN", 5, now, &SuggestOpts::default());
            let exact = stack.suggest_with_opts_at("", digits, "EN", 5, now, &SuggestOpts::exact_only());
            assert!(exact.len() <= full.len(), "exact-only must not add rows at {digits}");
            for s in &exact {
                assert!(
                    full.iter().any(|f| f.word == s.word && f.score == s.score),
                    "exact-only row {:?} must survive identically in default at {digits}",
                    s.word
                );
            }
        }
    }

    #[test]
    fn mixed_opts_cache_sequences_match_cold_runs() {
        // The prefix cache reinterprets tags through the CURRENT opts, so
        // alternating opts across keystrokes must equal cold-cache runs.
        let stack = DictionaryStack::new(base());
        let now = crate::personal::now_quantized();
        let words = |s: Vec<Suggestion>| {
            s.into_iter().map(|x| (x.word, x.score.to_bits())).collect::<Vec<_>>()
        };
        let dflt = SuggestOpts::default();
        let off = SuggestOpts::neighbor_off();
        let exact = SuggestOpts::exact_only();
        // Warm the cache with interleaved opts (prefix-bearing + exact-only).
        for digits in ["4", "43", "435", "4355"] {
            stack.suggest_with_opts_at("", digits, "EN", 5, now, &dflt);
            stack.suggest_with_opts_at("", digits, "EN", 5, now, &exact);
            stack.suggest_with_opts_at("", digits, "EN", 5, now, &off);
        }
        for digits in ["4", "43", "435", "4355", "43556"] {
            for opts in [&dflt, &off, &exact] {
                let fresh = DictionaryStack::new(base());
                assert_eq!(
                    words(stack.suggest_with_opts_at("", digits, "EN", 5, now, opts)),
                    words(fresh.suggest_with_opts_at("", digits, "EN", 5, now, opts)),
                    "mixed-opts cache mismatch at {digits}"
                );
            }
        }
    }

    #[test]
    fn probe_agrees_with_suggest_ranks() {
        // `probe_target_at` top_rank must reproduce the real output rank,
        // and the stage flags must be self-consistent.
        let stack = DictionaryStack::new(base());
        let now = crate::personal::now_quantized();
        let opts = SuggestOpts::default();
        for (digits, target) in [("4355", "hello"), ("4355", "hell"), ("999", "hello")] {
            let p = stack.probe_target_at("", digits, "EN", target, 5, now, &opts);
            let rank = stack
                .suggest_with_opts_at("", digits, "EN", 5, now, &opts)
                .iter()
                .position(|s| s.word == target)
                .map(|i| i + 1);
            assert_eq!(p.top_rank, rank, "probe/suggest rank mismatch for {target}");
            assert_eq!(p.in_union, p.pre_truncate_rank.is_some());
            assert_eq!(p.in_truncate200, p.pre_truncate_rank.map(|r| r <= 200).unwrap_or(false));
            if p.in_union {
                assert!(p.keyfit.is_some());
            }
        }
        // Invalid input yields an all-absent probe, never a panic.
        let bad = stack.probe_target_at("", "", "EN", "hello", 5, now, &opts);
        assert!(!bad.in_union && bad.top_rank.is_none());
    }

    #[test]
    fn multi_seq_row_merges_once_on_every_cache_path() {
        // One row whose primary AND alias both match a prefix (basnyat
        // 2276928 + basnet 227638 under digits 2276) must merge exactly
        // once: cold, parent-hit, and exact-hit paths return identical
        // scores, and the frequency counts once. Regression test for the
        // freq-doubling bug (same row re-merged per matching seq: phantom
        // +0.3 score inflation on cold queries, cache-hit/cold
        // disagreement).
        let mk = || {
            DictEntry::from_parts(
                "basnyat".to_string(),
                None,
                None,
                vec!["basnet".to_string()],
                2510,
                "EN".to_string(),
                "en".to_string(),
                0,
            )
            .unwrap()
        };
        let now = crate::personal::now_quantized();
        let bits = |s: Vec<Suggestion>| {
            s.into_iter().map(|x| (x.word, x.score.to_bits())).collect::<Vec<_>>()
        };
        // Repeat calls on one stack: cold, then exact-hit replays.
        let stack = DictionaryStack::new(vec![mk()]);
        let a = stack.suggest_no_neighbor_at("", "2276", "EN", 5, now);
        let b = stack.suggest_no_neighbor_at("", "2276", "EN", 5, now);
        let c = stack.suggest_no_neighbor_at("", "2276", "EN", 5, now);
        assert_eq!(bits(a.clone()), bits(b.clone()), "cold vs exact-hit replay diverged");
        assert_eq!(bits(b.clone()), bits(c), "replay instability");
        // Typing chain (parent-hit path) vs cold query.
        let chained = DictionaryStack::new(vec![mk()]);
        for d in ["2", "22", "227"] {
            chained.suggest_no_neighbor_at("", d, "EN", 5, now);
        }
        let via_parent = chained.suggest_no_neighbor_at("", "2276", "EN", 5, now);
        assert_eq!(bits(a.clone()), bits(via_parent), "parent-hit vs cold diverged");
        // Single-count pin: single-contributor rows score the quantized
        // base (plan/05 #7): dequantize(quantize(2510)) + w_cat * 1.0 +
        // w_keyfit * 0.9. A double-counted row would score +0.301 higher.
        // (Weights read from the default — this pins single-counting, not
        // the tuned values.)
        assert_eq!(a.len(), 1);
        let w = crate::rank::RankWeights::default();
        let expect = dequantize_base(quantize_base(2510)) + w.w_cat * 1.0 + w.w_keyfit * 0.9;
        assert!(
            (a[0].score - expect).abs() < 1e-9,
            "freq must count once: got {} want {expect}",
            a[0].score
        );
    }

    #[test]
    fn mapping_and_exact_ranking() {
        let stack = DictionaryStack::new(base());
        let s = stack.suggest("", "43556", "EN", 5);
        assert!(!s.is_empty());
        assert_eq!(s[0].word, "hello");
    }

    #[test]
    fn fuzzy_neighbor_found() {
        // "43555": differs from hello's 43556 in last digit (6 vs 5, adjacent).
        let stack = DictionaryStack::new(base());
        let s = stack.suggest("", "43555", "EN", 5);
        assert!(s.iter().any(|c| c.word == "hello"));
    }

    #[test]
    fn stack_priority_display_cat_scoped_to_tab() {
        let mut stack = DictionaryStack::new(base());
        // Base "hello" is cat EN prio 0; extension redefines it as js prio 50.
        let pack = load_pack_str(
            r#"{"id":"js","title":"JS","version":"1.0.0",
                "words":[{"w":"hello","freq":10,"cat":"js"}]}"#,
        )
        .unwrap();
        stack.add_pack(&pack, 50);
        let now = crate::personal::now_quantized();
        // Isolation scopes BEFORE the merge: under EN only the EN row
        // contributes, so display stays EN (the higher-priority js row
        // cannot leak its label into the English tab — and the word is
        // never lost from its home tab either).
        let s = stack.suggest_at("", "43556", "EN", 5, now);
        let hello = s.iter().find(|c| c.word == "hello").unwrap();
        assert_eq!(hello.cat, "EN");
        // ... while the js tab shows the js contributor alone.
        let s = stack.suggest_at("", "43556", "js", 5, now);
        assert_eq!(s.iter().find(|c| c.word == "hello").unwrap().cat, "js");
        // The explicitly unfiltered arm keeps the legacy highest-priority
        // display (js) for measurement continuity.
        let s = stack.suggest_with_opts_at(
            "",
            "43556",
            "EN",
            5,
            now,
            &SuggestOpts::default().with_tab_filter(false),
        );
        assert_eq!(s.iter().find(|c| c.word == "hello").unwrap().cat, "js");
    }

    #[test]
    fn personal_overlay_promotes_and_blocks() {
        let mut stack = DictionaryStack::new(base());
        // "help" (4357) vs learned path: learn boosts personal words.
        stack.personal.learn("hell", "EN");
        stack.personal.learn("hell", "EN");
        let s = stack.suggest("", "4355", "EN", 5);
        assert_eq!(s[0].word, "hell");
        // Block hides everywhere.
        stack.personal.forget("hell");
        let s2 = stack.suggest("", "4355", "EN", 5);
        assert!(s2.iter().all(|c| c.word != "hell"));
    }

    #[test]
    fn prefix_search_via_fst() {
        let stack = DictionaryStack::new(base());
        assert!(stack.fst_len() >= 3);
        let seqs = stack.decode_prefix("4355", 10);
        assert!(seqs.contains(&"43556".to_string()));
    }

    #[test]
    fn category_boost_prefers_active_tab() {
        let mut stack = DictionaryStack::new(DictionaryStack::load_base_json(
            r#"[{"w":"fun","freq":100,"cat":"EN"}]"#,
        )
        .unwrap());
        let pack = load_pack_str(
            r#"{"id":"emoji","title":"Emoji","version":"1.0.0",
                "words":[{"w":"fun","freq":100,"cat":"emoji"}]}"#,
        )
        .unwrap();
        stack.add_pack(&pack, 50);
        // Merged display cat is emoji (higher priority); with active tab
        // emoji it gets the full category boost. (Under isolation only the
        // emoji row contributes here, so display is emoji either way.)
        let s = stack.suggest("", "386", "emoji", 5);
        assert_eq!(s[0].cat, "emoji");
    }

    fn names_stack() -> DictionaryStack {
        // Custom names category colliding with EN on seq 262
        // (bob/coa/cob): the isolation fixture for user packs.
        let mut stack = DictionaryStack::new(
            DictionaryStack::load_base_json(r#"[{"w":"bob","freq":900,"cat":"EN"}]"#).unwrap(),
        );
        let pack = load_pack_str(
            r#"{"id":"names","title":"Names","version":"1.0.0",
                "words":[{"w":"coa","freq":100,"cat":"names"}]}"#,
        )
        .unwrap();
        assert!(pack.validate().is_empty());
        stack.add_pack(&pack, 50);
        stack
    }

    #[test]
    fn all_tabs_isolate_by_default() {
        // Isolation helpers still classify tabs the same way, but the
        // policy no longer branches on them: EVERY tab hard-filters (the
        // filter is default-ON, not opt-in).
        for tab in ["js", "medical", "names", "my_contacts"] {
            assert!(SuggestOpts::is_custom_tab(tab) || SuggestOpts::is_code_tab(tab), "{tab}");
            assert!(SuggestOpts::is_isolated_tab(tab), "{tab}");
            assert!(
                SuggestOpts::policy_for_tab(tab).hard_tab_filter,
                "policy must hard-filter {tab:?}"
            );
        }
        for tab in ["EN", "words", "NE", "ne", "numbers", "emoji", "personal", "★personal", ""] {
            assert!(!SuggestOpts::is_custom_tab(tab), "{tab:?} is not custom");
            assert!(
                SuggestOpts::policy_for_tab(tab).hard_tab_filter,
                "policy must hard-filter {tab:?} too"
            );
        }
    }

    #[test]
    fn custom_pack_suggest_isolates_to_own_words() {
        // EN "bob"(262, freq 900) outranks names "coa"(262, freq 100) in
        // the explicitly unfiltered union; the names-tab policy must scope
        // to names.
        let stack = names_stack();
        let now = crate::personal::now_quantized();
        let nofilter = SuggestOpts::neighbor_off().with_tab_filter(false);
        let unfiltered = stack.suggest_with_opts_at("", "262", "names", 5, now, &nofilter);
        assert!(
            unfiltered.iter().any(|s| s.cat == "EN"),
            "fixture needs EN cross-tab noise, got {:?}",
            unfiltered.iter().map(|s| &s.word).collect::<Vec<_>>()
        );
        let policy = stack.suggest_policy_at("", "262", "names", 5, now);
        assert!(!policy.is_empty(), "names policy must keep the names row");
        assert!(
            policy.iter().all(|s| s.cat == "names"),
            "isolated tab must show its own words, got {:?}",
            policy.iter().map(|s| &s.word).collect::<Vec<_>>()
        );
        assert_eq!(policy[0].word, "coa");
    }

    #[test]
    fn personal_learn_overlay_works_under_custom_tab() {
        // OOV "cob" (262) learned under `names` suggests under the names
        // tab with the personal overlay, like every built-in tab.
        let mut stack = names_stack();
        let now = crate::personal::now_quantized();
        assert!(
            stack
                .suggest_policy_at("", "262", "names", 5, now)
                .iter()
                .all(|s| s.word != "cob")
        );
        stack.personal.learn("cob", "names");
        stack.personal.learn("cob", "names");
        let s = stack.suggest_policy_at("", "262", "names", 5, now);
        assert!(
            s.iter().any(|s| s.word == "cob"),
            "learned OOV must suggest under its custom tab, got {:?}",
            s.iter().map(|s| &s.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn disable_cat_hides_tab_and_reenable_restores() {
        let mut stack = names_stack();
        let now = crate::personal::now_quantized();
        assert!(stack.is_cat_enabled("names"));
        assert!(!stack.suggest_policy_at("", "262", "names", 5, now).is_empty());
        stack.set_cat_enabled("names", false);
        assert!(!stack.is_cat_enabled("names"));
        assert!(
            stack.suggest_policy_at("", "262", "names", 5, now).is_empty(),
            "disabled tab must suggest nothing"
        );
        // Other tabs are unaffected by the toggle.
        assert!(!stack.suggest_policy_at("", "262", "words", 5, now).is_empty());
        stack.set_cat_enabled("names", true);
        assert!(stack.is_cat_enabled("names"));
        assert!(!stack.suggest_policy_at("", "262", "names", 5, now).is_empty());
    }

    fn ne_stack() -> DictionaryStack {
        // Minimal Nepali pack: Devanagari display + Roman tr.
        let pack = load_pack_str(
            r#"{"id":"ne","title":"Nepali","version":"1.0.0",
                "words":[
                    {"w":"नमस्ते","tr":"namaste","freq":9000,"cat":"NE","lang":"ne"},
                    {"w":"नेपाल","tr":"nepal","freq":8500,"cat":"NE","lang":"ne"},
                    {"w":"पानी","tr":"pani","alt":["paani"],"freq":7000,"cat":"NE","lang":"ne"},
                    {"w":"काठमाडौं","tr":"kathmandu","freq":7000,"cat":"NE","lang":"ne"}]}"#,
        )
        .unwrap();
        assert!(pack.validate().is_empty());
        let mut stack = DictionaryStack::new(Vec::new());
        stack.add_pack(&pack, 10);
        stack
    }

    #[test]
    fn roman_typing_surfaces_devanagari_words() {
        // plan/03 acceptance: `k->2`-style Roman T9 finds Devanagari words.
        let stack = ne_stack();
        let s = stack.suggest("", &encode_word("namaste"), "NE", 5);
        assert!(
            s.iter().any(|c| c.word == "नमस्ते"),
            "roman full-seq must surface नमस्ते, got {:?}",
            s.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
        // Prefix-4 (top-3@4 shape): नमस्ते's tr prefix must be present.
        let pre4 = &encode_word("namaste")[..4];
        let s4 = stack.suggest("", pre4, "NE", 3);
        assert!(
            s4.iter().any(|c| c.word == "नमस्ते"),
            "roman prefix-4 {pre4} must surface नमस्ते in top-3, got {:?}",
            s4.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn skeleton_fallback_still_matches() {
        // Direct-Devanagari digits hit the skeleton alias (fallback only).
        let stack = ne_stack();
        let s = stack.suggest("", &encode_word("नमस्ते"), "NE", 5);
        assert!(
            s.iter().any(|c| c.word == "नमस्ते"),
            "skeleton digits must still match नमस्ते, got {:?}",
            s.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn alt_variant_spelling_matches() {
        // `paani` (double-a) finds पानी whose primary tr is `pani`.
        let stack = ne_stack();
        let s = stack.suggest("", &encode_word("paani"), "NE", 5);
        assert!(
            s.iter().any(|c| c.word == "पानी"),
            "variant paani must surface पानी, got {:?}",
            s.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
        // ... while the primary spelling keeps working.
        let s2 = stack.suggest("", &encode_word("pani"), "NE", 5);
        assert!(s2.iter().any(|c| c.word == "पानी"));
        // No duplicate rows for one word.
        assert_eq!(s.iter().filter(|c| c.word == "पानी").count(), 1);
    }

    #[test]
    fn alias_rows_merge_never_duplicate() {
        // Same word in two rows with distinct explicit seqs (the alias-row
        // encoding): both seqs retrieve it, merged into a single row.
        let mut stack = DictionaryStack::new(Vec::new());
        let mk = |seq: &str| {
            DictEntry::from_parts(
                "पानी".to_string(),
                Some(seq.to_string()),
                None,
                Vec::new(),
                100,
                "NE".to_string(),
                "ne".to_string(),
                0,
            )
            .unwrap()
        };
        stack.add_entries(vec![mk("7264"), mk("72264")]);
        for digits in ["7264", "72264"] {
            let s = stack.suggest("", digits, "NE", 5);
            assert_eq!(
                s.iter().filter(|c| c.word == "पानी").count(),
                1,
                "digits {digits} must yield exactly one पानी row"
            );
        }
    }

    #[test]
    fn t9_12_shares_t99_aliases() {
        // Locks the `aliases` sharing assumption: t9-9 and t9-12 encode
        // latin + Devanagari identically (only the bottom control row
        // differs, and it never emits text codes).
        let r = LayoutRegistry::with_builtins();
        let t9 = r.get_or_default("t9-9");
        let t12 = r.get_or_default("t9-12");
        for w in [
            "namaste",
            "kathmandu",
            "paani",
            "pani",
            "sathi",
            "wxyz",
            "नमस्ते",
            "काठमाडौं",
            "पानी",
            "कि",
            "कमल",
            "hello",
        ] {
            assert_eq!(t9.encode_word(w), t12.encode_word(w), "word {w:?}");
        }
    }

    #[test]
    fn tr_primary_holds_under_t9_16() {
        // t9-16 splits pq/rs + wx/yz: the tr primary must be re-encoded
        // for that pad (precomputed), not frozen to the t9-9 seq.
        let r = LayoutRegistry::with_builtins();
        let t16 = r.get_or_default("t9-16");
        // "sathi": s->rs-split, matched under t9-16's own codes.
        let pack = load_pack_str(
            r#"{"id":"ne","title":"Nepali","version":"1.0.0",
                "words":[{"w":"साथी","tr":"sathi","freq":6500,"cat":"NE","lang":"ne"}]}"#,
        )
        .unwrap();
        let mut st = DictionaryStack::new(Vec::new());
        st.add_pack(&pack, 10);
        let digits = t16.encode_word("sathi");
        assert_ne!(digits, encode_word("sathi"), "t16 must split s/p codes here");
        let s = st.suggest_for_layout("", &digits, t16, "NE", 5);
        assert!(
            s.iter().any(|c| c.word == "साथी"),
            "t9-16 roman digits {digits} must surface साथी, got {:?}",
            s.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn layout02_precomputed_golden_vectors() {
        // Acceptance vectors (plan/02): hello identical on t9-9/t9-12,
        // fun splits 386 vs 396, pqrs is t9-16-only 7788.
        let entries = DictionaryStack::load_base_json(
            r#"[{"w":"hello","freq":900,"cat":"EN"},
                {"w":"fun","freq":150,"cat":"EN"},
                {"w":"pqrs","freq":50,"cat":"EN"}]"#,
        )
        .unwrap();
        let hello = entries.iter().find(|e| e.word == "hello").unwrap();
        assert_eq!(hello.seq, "43556");
        assert_eq!(hello.seq_for_layout("t9-12"), "43556");
        let fun = entries.iter().find(|e| e.word == "fun").unwrap();
        assert_eq!(fun.seq, "386");
        assert_eq!(fun.seq_for_layout("t9-12"), "386");
        assert_eq!(fun.seq_for_layout("t9-16"), "396");
        let pqrs = entries.iter().find(|e| e.word == "pqrs").unwrap();
        assert_eq!(pqrs.seq_for_layout("t9-16"), "7788");
    }

    #[test]
    fn layout02_per_layout_fst_and_suggest() {
        let stack = DictionaryStack::new(
            DictionaryStack::load_base_json(
                r#"[{"w":"hello","freq":900,"cat":"EN"},
                    {"w":"fun","freq":150,"cat":"EN"}]"#,
            )
            .unwrap(),
        );
        // Every built-in layout owns an index; no silent build errors.
        for id in ["t9-9", "t9-12", "t9-16"] {
            assert!(stack.fst_len_for(id) >= 2, "index {id}");
            assert!(stack.fst_build_error(id).is_none(), "index {id}");
        }
        // t9-16 index holds the split seq, t9-9 does not.
        assert!(stack.decode_prefix("39", 10).is_empty());
        let r = LayoutRegistry::with_builtins();
        let t16 = r.get_or_default("t9-16");
        let s = stack.suggest_for_layout("", "396", t16, "EN", 5);
        assert!(s.iter().any(|c| c.word == "fun"));
        assert_eq!(s.iter().find(|c| c.word == "fun").unwrap().seq, "396");
        assert_eq!(s[0].layout_id, "t9-16");
        // Same word under t9-9 keeps its own seq (deterministic per
        // layout, not a shared label).
        let t9 = r.get_or_default("t9-9");
        let s9 = stack.suggest_for_layout("", "386", t9, "EN", 5);
        assert_eq!(s9.iter().find(|c| c.word == "fun").unwrap().seq, "386");
    }

    #[test]
    fn layout02_pack_layout_hint_seeds_ne() {
        let mut stack = DictionaryStack::new(Vec::new());
        assert_eq!(stack.pack_layout_hint("NE"), None);
        let pack = load_pack_str(
            r#"{"id":"ne","title":"Nepali","version":"1.0.0","layout":"t9-16",
                "words":[{"w":"साथी","tr":"sathi","freq":6500,"cat":"NE","lang":"ne"}]}"#,
        )
        .unwrap();
        assert!(pack.validate().is_empty());
        stack.add_pack(&pack, 10);
        assert_eq!(stack.pack_layout_hint("NE"), Some("t9-16".to_string()));
        assert_eq!(stack.pack_layout_hint("EN"), None);
    }

    #[test]
    fn next_word_cold_is_empty_never_a_guess() {
        // No bigram history => no followers, even for in-vocab prev words.
        // Cold-start connections are honest silence, not static guesses.
        let stack = DictionaryStack::new(base());
        let now = crate::personal::now_quantized();
        assert!(stack.suggest_next_at("hello", "EN", 5, now).is_empty());
        assert!(stack.suggest_next_at("", "EN", 5, now).is_empty());
        assert!(stack.suggest_next_at("hello", "EN", 0, now).is_empty());
    }

    #[test]
    fn next_word_ranks_learned_followers_by_count() {
        let mut stack = DictionaryStack::new(base());
        let now = crate::personal::now_quantized();
        stack.personal.learn("hello", "EN");
        stack.personal.learn("world", "EN");
        stack.personal.record_bigram("hello", "world");
        stack.personal.record_bigram("hello", "world");
        stack.personal.record_bigram("hello", "hell");
        let next = stack.suggest_next_at("hello", "EN", 5, now);
        assert_eq!(next.len(), 2, "got {next:?}");
        assert_eq!(next[0].word, "world");
        assert_eq!(next[0].count, 2);
        assert_eq!(next[1].word, "hell");
        // Multi-word ctx uses the last token.
        let next2 = stack.suggest_next_at("say hello", "EN", 5, now);
        assert_eq!(next, next2);
        // Limit truncates deterministically (count-desc order kept).
        let top1 = stack.suggest_next_at("hello", "EN", 1, now);
        assert_eq!(top1, vec![next[0].clone()]);
    }

    #[test]
    fn next_word_respects_blocks_and_tabs() {
        let mut stack = DictionaryStack::new(base());
        let now = crate::personal::now_quantized();
        stack.personal.learn("hello", "EN");
        stack.personal.learn("world", "EN");
        stack.personal.learn("dost", "NE");
        stack.personal.record_bigram("hello", "world");
        stack.personal.record_bigram("hello", "dost");
        // Cross-tab follower hidden under EN …
        let en = stack.suggest_next_at("hello", "EN", 5, now);
        assert_eq!(en.iter().map(|n| n.word.as_str()).collect::<Vec<_>>(), ["world"]);
        // … but visible under the ★personal aggregate.
        let agg = stack.suggest_next_at("hello", "★personal", 5, now);
        assert_eq!(agg.len(), 2);
        // Blocked followers never surface.
        stack.personal.forget("world");
        assert!(stack.suggest_next_at("hello", "EN", 5, now).is_empty());
        let agg2 = stack.suggest_next_at("hello", "★personal", 5, now);
        assert_eq!(agg2.iter().map(|n| n.word.as_str()).collect::<Vec<_>>(), ["dost"]);
    }

    #[test]
    fn perf_full_packs_plus_10k_personal_p95_under_50ms() {
        // Plan/05 acceptance bench: full shipped packs (words_en 5150 +
        // nepali 8005 ≈ 13k static, above the 6613-word target) + 10k
        // personal rows, production policy path. Fixed-count workload —
        // never a wall-clock cutoff in logic; the clock is measurement
        // only, and `now` is a fixed hour bucket so output is deterministic.
        // Target: p95 < 50ms on minSdk26. Host release/debug timings print
        // for the record; the hard assertion carries a wide margin so the
        // test never flakes on a loaded host.
        use crate::pack::load_pack_str;
        let en = load_pack_str(include_str!("../../packs/words_en.json")).unwrap();
        let ne = load_pack_str(include_str!("../../packs/nepali.json")).unwrap();
        let n_en = en.words.len();
        let n_ne = ne.words.len();
        let mut stack = DictionaryStack::new(en.to_entries(0));
        stack.add_pack(&ne, 0);
        let n_static = stack.fst_len();
        for i in 0..10_000 {
            stack.personal.learn(&format!("zpersonal{i:05}"), "EN");
        }
        assert_eq!(stack.personal.len(), 10_000);

        // Fixed query set: prefixes (len 2..=6) of real pack words under
        // t9-9, plus fixed miss shapes. Sorted+deduped => fixed order,
        // fixed count, every run.
        let reg = LayoutRegistry::with_builtins();
        let t9 = reg.get_or_default("t9-9");
        let words = [
            "the", "be", "to", "of", "and", "a", "in", "that", "have", "hello", "hell",
            "help", "world", "water", "house", "time", "people", "good", "first", "know",
            "like", "over", "think", "after", "such", "only", "come", "could", "than",
            "then", "them", "these", "some", "would", "make", "year", "into", "more",
            "long", "down", "go", "about", "again", "against", "money", "mother",
            "father", "night", "seven", "eight", "three", "where", "which", "their",
            "there", "other", "those", "should", "never", "every", "under", "while",
            "might", "seven",
        ];
        let mut queries: Vec<String> = Vec::new();
        for w in words {
            // Whole typing chains from the first digit (len 1..=6), exactly
            // what the IME issues per keystroke.
            let s = t9.encode_word(w);
            for l in 1..=s.len().min(6) {
                queries.push(s[..l].to_string());
            }
        }
        queries.extend(["999999".to_string(), "111111".to_string(), "000".to_string()]);
        queries.sort();
        queries.dedup();
        assert!(queries.len() > 100, "workload must be non-trivial");

        let now = 1_726_000_000i64; // fixed 1h-bucket timestamp, not wall clock
        let words_of = |s: Vec<Suggestion>| {
            s.into_iter().map(|x| (x.word, x.score.to_bits())).collect::<Vec<_>>()
        };
        let mut first: Vec<Vec<(String, u64)>> = Vec::with_capacity(queries.len());
        let mut dts: Vec<std::time::Duration> = Vec::with_capacity(queries.len());
        for digits in &queries {
            let t0 = std::time::Instant::now();
            let out = stack.suggest_policy_at("", digits, "EN", 10, now);
            dts.push(t0.elapsed());
            first.push(words_of(out));
        }
        dts.sort();
        let mean = dts.iter().sum::<std::time::Duration>() / dts.len() as u32;
        let p95 = dts[dts.len() * 95 / 100];
        let max = dts[dts.len() - 1];
        println!(
            "perf: packs words_en={n_en} nepali={n_ne} fst_seqs={n_static} personal=10000 \
             queries={} mean={mean:?} p95={p95:?} max={max:?} (target p95<50ms)",
            queries.len(),
        );
        assert!(
            p95 < std::time::Duration::from_millis(50),
            "p95 {p95:?} exceeds the 50ms budget over {} queries",
            queries.len(),
        );
        // Second pass must be byte-identical: the prefix cache is
        // behavior-neutral (speed only), never a second result set.
        for (digits, expect) in queries.iter().zip(first.iter()) {
            let again = words_of(stack.suggest_policy_at("", digits, "EN", 10, now));
            assert_eq!(&again, expect, "cache moved results at {digits}");
        }
        // Attribution scratch (temporary): same workload with an empty
        // personal dict isolates the static-index cost from the 10k
        // personal-OOV scan.
        stack.personal = crate::personal::PersonalDict::new();
        let mut dts0: Vec<std::time::Duration> = Vec::with_capacity(queries.len());
        for digits in &queries {
            let t0 = std::time::Instant::now();
            let _ = stack.suggest_policy_at("", digits, "EN", 10, now);
            dts0.push(t0.elapsed());
        }
        dts0.sort();
        println!(
            "perf/attribution: no-personal mean={:?} p95={:?} max={:?}",
            dts0.iter().sum::<std::time::Duration>() / dts0.len() as u32,
            dts0[dts0.len() * 95 / 100],
            dts0[dts0.len() - 1],
        );
        for (digits, expect) in queries.iter().zip(first.iter()) {
            let again = words_of(stack.suggest_policy_at("", digits, "EN", 10, now));
            assert_eq!(&again, expect, "cache moved results at {digits}");
        }
    }

    // ---- Plan/17 single-press fallback (len<=2 synthetic key_label
    // injection, truncate-exempt, no-learn, short-exact boost). ----

    fn single_press_stack() -> DictionaryStack {
        // `7*` union: some/school/sad are multi-letter prefixes; the
        // single letters s/p/q/r can only come from the synthetic
        // fallback (no len-1 rows seeded — fallback and seed rows must
        // never coexist, or merge_row would double-sum them).
        DictionaryStack::new(
            DictionaryStack::load_base_json(
                r#"[{"w":"some","freq":5000,"cat":"EN"},
                    {"w":"school","freq":3000,"cat":"EN"},
                    {"w":"sad","freq":1000,"cat":"EN"},
                    {"w":"so","freq":200,"cat":"EN"},
                    {"w":"an","freq":8000,"cat":"EN"}]"#,
            )
            .unwrap(),
        )
    }

    #[test]
    fn single_press_7_leads_with_s() {
        // plan/17 acceptance: `suggest("7","words")` contains s (+p/q/r)
        // with s first — never empty/wrong.
        let stack = single_press_stack();
        let s = stack.suggest("", "7", "words", 10);
        assert!(!s.is_empty(), "single press must never be empty");
        assert_eq!(s[0].word, "s", "got {:?}", s.iter().map(|c| &c.word).collect::<Vec<_>>());
        for letter in ["s", "p", "q", "r"] {
            assert!(
                s.iter().any(|c| c.word == letter),
                "fallback must contain {letter:?}, got {:?}",
                s.iter().map(|c| &c.word).collect::<Vec<_>>()
            );
        }
        // Repeat-stable: the post-truncate append is behavior-neutral
        // across cold/cache paths.
        let again = stack.suggest("", "7", "words", 10);
        assert_eq!(
            s.iter().map(|c| &c.word).collect::<Vec<_>>(),
            again.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn single_press_2_contains_a_first() {
        let stack = single_press_stack();
        let s = stack.suggest("", "2", "words", 10);
        assert_eq!(s[0].word, "a", "got {:?}", s.iter().map(|c| &c.word).collect::<Vec<_>>());
    }

    #[test]
    fn short_exact_boost_orders_len2_exact_first() {
        // `so` (exact len-2, freq 200) must outrank `some` (prefix,
        // freq 5000): without SHORT_EXACT_BOOST 4.003 < 5.379, with it
        // 7.003 > 5.379 — this test fails pre-boost.
        let stack = single_press_stack();
        let s = stack.suggest("", "76", "words", 10);
        assert_eq!(s[0].word, "so", "got {:?}", s.iter().map(|c| &c.word).collect::<Vec<_>>());
        assert!(
            s.iter().position(|c| c.word == "so").unwrap()
                < s.iter().position(|c| c.word == "some").unwrap()
        );
    }

    #[test]
    fn len3_and_up_untouched_by_fallback() {
        // No synthetics at len>=3: `suggest("766")` holds only real rows
        // (prefix keyfit decides — `song` keeps its pre-boost order over
        // exact `son`), and punct/space digits inject nothing.
        let stack = DictionaryStack::new(
            DictionaryStack::load_base_json(
                r#"[{"w":"son","freq":200,"cat":"EN"},
                    {"w":"song","freq":5000,"cat":"EN"}]"#,
            )
            .unwrap(),
        );
        let s = stack.suggest("", "766", "words", 10);
        assert_eq!(s[0].word, "song", "got {:?}", s.iter().map(|c| &c.word).collect::<Vec<_>>());
        assert!(
            s.iter().all(|c| c.word.chars().count() > 1),
            "len>=3 must inject no single-letter rows, got {:?}",
            s.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
        assert!(stack.suggest("", "1", "words", 10).is_empty());
        assert!(stack.suggest("", "0", "words", 10).is_empty());
    }

    #[test]
    fn fallback_never_double_counts_seed_rows() {
        // A real len-1 row wins outright: no synthetic is appended for it,
        // so its frequency counts exactly once (single-contributor
        // quantized fast path). A double-sum (50 + synthetic 1000) would
        // score +1.3 higher.
        let stack = DictionaryStack::new(
            DictionaryStack::load_base_json(r#"[{"w":"s","freq":50,"cat":"EN"}]"#).unwrap(),
        );
        let s = stack.suggest("", "7", "words", 10);
        let matches: Vec<_> = s.iter().filter(|c| c.word == "s").collect();
        assert_eq!(matches.len(), 1, "exactly one `s` row, got {s:?}");
        let w = crate::rank::RankWeights::default();
        let expect =
            dequantize_base(quantize_base(50)) + w.w_cat * 1.0 + w.w_keyfit * 1.0 + SHORT_EXACT_BOOST;
        assert!(
            (matches[0].score - expect).abs() < 1e-9,
            "seed row must count once: got {} want {expect}",
            matches[0].score
        );
    }

    // ---- Plan/22 QWERTY parity (literal + letter-graph corrections +
    // confidence). T9 arms stay frozen: these tests pin the new path only,
    // plus a control that the T9 policy cannot explain graph corrections.

    fn qwerty_stack() -> DictionaryStack {
        DictionaryStack::new(
            DictionaryStack::load_base_json(
                r#"[{"w":"the","freq":9000,"cat":"EN"},
                    {"w":"was","freq":5000,"cat":"EN"},
                    {"w":"open","freq":4000,"cat":"EN"}]"#,
            )
            .unwrap(),
        )
    }

    fn t9_mapping() -> LayoutRegistry {
        LayoutRegistry::with_builtins()
    }

    #[test]
    fn qwerty_graph_matches_physical_board() {
        // Fat-finger pairs share an edge; diagonal non-neighbors don't.
        assert!(qwerty_neighbors('q').contains(&'w'));
        assert!(qwerty_neighbors('w').contains(&'q'));
        assert!(qwerty_neighbors('o').contains(&'p'));
        assert!(qwerty_neighbors('a').contains(&'s'));
        assert!(!qwerty_neighbors('q').contains(&'s'));
        assert!(!qwerty_neighbors('m').contains(&'l'));
        assert!(!qwerty_neighbors('q').contains(&'q'));
        assert!(qwerty_neighbors('1').is_empty());
        // Symmetry spot-check over the whole graph.
        for c in 'a'..='z' {
            for n in qwerty_neighbors(c) {
                assert!(
                    qwerty_neighbors(n).contains(&c),
                    "adjacency must be symmetric: {c} -> {n}"
                );
            }
        }
    }

    #[test]
    fn qwerty_variants_cover_typo_shapes_deterministically() {
        // `teh`Transpose->`the`, `qas`-substitute->`was`, `ooen`-insert
        // shapes all present; output sorted + capped + never raw itself.
        for (raw, expect) in [("teh", "the"), ("qas", "was"), ("ooen", "open"), ("aas", "was")] {
            let v = qwerty_variants(raw);
            assert!(
                v.contains(&expect.to_string()),
                "{raw:?} variants must contain {expect:?}"
            );
            assert!(!v.contains(&raw.to_string()), "variants never echo raw");
            let mut sorted = v.clone();
            sorted.sort();
            assert_eq!(v, sorted, "variants must be sorted");
            assert!(v.len() <= QWERTY_VARIANT_CAP);
        }
        // Twice-typed determinism (fixed-count, never wall-clock).
        assert_eq!(qwerty_variants("teh"), qwerty_variants("teh"));
        // Non-letters take no variants (Sym-taps stay literal-only).
        assert!(qwerty_variants("teh!").is_empty());
        assert!(qwerty_variants("123").is_empty());
        assert!(qwerty_variants("").is_empty());
        // Apostrophe rides along, letters still expand.
        assert!(qwerty_variants("don't").iter().any(|v| v.contains('\'')));
    }

    #[test]
    fn qwerty_correction_classifier_marks_completions() {
        assert!(is_qwerty_correction("teh", "teh"));
        assert!(is_qwerty_correction("teh", "the")); // transpose
        assert!(is_qwerty_correction("qas", "was")); // adjacent sub
        assert!(is_qwerty_correction("teh", "te")); // delete
        assert!(is_qwerty_correction("te", "teh")); // insert
        assert!(is_qwerty_correction("ooen", "open")); // adjacent sub
        assert!(!is_qwerty_correction("teh", "tehran")); // completion
        assert!(!is_qwerty_correction("teh", "tep")); // h->p not adjacent
        assert!(!is_qwerty_correction("teh", "xyz"));
    }

    #[test]
    fn qwerty_teh_corrects_to_the_with_confidence() {
        // Plan/22 acceptance: typo `teh` → center `the` bold.
        let stack = qwerty_stack();
        let reg = t9_mapping();
        let t9 = reg.get_or_default("t9-9");
        let now = crate::personal::now_quantized();
        let r = stack.suggest_qwerty_at("teh", "", "EN", 5, t9, now, AUTOCORRECT_THRESHOLD_DEFAULT);
        assert_eq!(r.literal, "teh", "literal slot is always verbatim raw");
        assert!(!r.corrections.is_empty());
        assert_eq!(r.corrections[0].word, "the");
        assert!(r.corrections[0].is_correction);
        assert!(r.confident, "the(9000) must clear the 1.0 delta over OOV teh");
    }

    #[test]
    fn qwerty_graph_proof_beyond_t9_digits() {
        // `q->w` (T9 7 vs 9, non-adjacent) and `a->w` (T9 2 vs 9,
        // non-adjacent) corrections can only come from the letter graph:
        // the T9 neighbor-OFF policy on the same digits finds nothing.
        let stack = qwerty_stack();
        let reg = t9_mapping();
        let t9 = reg.get_or_default("t9-9");
        let now = crate::personal::now_quantized();
        for (raw, expect) in [("qas", "was"), ("aas", "was"), ("ooen", "open")] {
            let r = stack.suggest_qwerty_at(raw, "", "EN", 5, t9, now, AUTOCORRECT_THRESHOLD_DEFAULT);
            assert_eq!(r.literal, raw);
            assert!(
                r.corrections.iter().any(|c| c.word == expect && c.is_correction),
                "{raw:?} must correct to {expect:?}, got {:?}",
                r.corrections.iter().map(|c| &c.word).collect::<Vec<_>>()
            );
            let digits = t9.encode_word(raw);
            let t9pol = stack.suggest_policy_at("", &digits, "EN", 5, now);
            assert!(
                t9pol.iter().all(|c| c.word != expect),
                "T9 policy must not explain {expect:?} at digits {digits:?}"
            );
        }
    }

    #[test]
    fn qwerty_exact_word_is_literal_not_confident() {
        // Typing the word itself: literal == word, no self-correction,
        // delta ~0 means Space keeps exactly what was typed.
        let stack = qwerty_stack();
        let reg = t9_mapping();
        let t9 = reg.get_or_default("t9-9");
        let now = crate::personal::now_quantized();
        let r = stack.suggest_qwerty_at("the", "", "EN", 5, t9, now, AUTOCORRECT_THRESHOLD_DEFAULT);
        assert_eq!(r.literal, "the");
        assert!(
            r.corrections.iter().all(|c| c.word != "the"),
            "the literal must not correct to itself, got {:?}",
            r.corrections.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
        assert!(!r.confident);
        // Case preserved verbatim: shouting stays shouting.
        let r2 = stack.suggest_qwerty_at("Teh", "", "EN", 5, t9, now, AUTOCORRECT_THRESHOLD_DEFAULT);
        assert_eq!(r2.literal, "Teh");
        assert!(r2.corrections.iter().any(|c| c.word == "the"));
    }

    #[test]
    fn qwerty_threshold_gates_confidence_not_recall() {
        // The threshold moves the Space decision only: recall is
        // identical, confidence flips. INFINITY = Suggest-only mode.
        let stack = qwerty_stack();
        let reg = t9_mapping();
        let t9 = reg.get_or_default("t9-9");
        let now = crate::personal::now_quantized();
        let strict = stack.suggest_qwerty_at("teh", "", "EN", 5, t9, now, 100.0);
        let loose = stack.suggest_qwerty_at("teh", "", "EN", 5, t9, now, 0.0);
        let suggest_only = stack.suggest_qwerty_at("teh", "", "EN", 5, t9, now, f64::INFINITY);
        for r in [&strict, &loose, &suggest_only] {
            assert_eq!(r.corrections[0].word, "the");
        }
        assert!(!strict.confident);
        assert!(loose.confident);
        assert!(!suggest_only.confident, "Suggest-only never autocorrects");
    }

    #[test]
    fn qwerty_literal_always_present_off_means_literal_only() {
        let stack = qwerty_stack();
        let reg = t9_mapping();
        let t9 = reg.get_or_default("t9-9");
        let now = crate::personal::now_quantized();
        // Total miss: literal slot only, never confident.
        let r = stack.suggest_qwerty_at("zxqj", "", "EN", 5, t9, now, AUTOCORRECT_THRESHOLD_DEFAULT);
        assert_eq!(r.literal, "zxqj");
        assert!(r.corrections.is_empty());
        assert!(!r.confident);
        // Empty raw: empty literal, no panic.
        let r = stack.suggest_qwerty_at("", "", "EN", 5, t9, now, AUTOCORRECT_THRESHOLD_DEFAULT);
        assert_eq!(r.literal, "");
        assert!(!r.confident);
        // Off mode (limit 0): literal only, no corrections, not confident.
        let r = stack.suggest_qwerty_at("teh", "", "EN", 0, t9, now, AUTOCORRECT_THRESHOLD_DEFAULT);
        assert_eq!(r.literal, "teh");
        assert!(r.corrections.is_empty());
        assert!(!r.confident);
    }
}

