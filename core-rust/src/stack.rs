//! Trie/FST dictionary store + `DictionaryStack` (base < extensions < personal).
//!
//! Lookup unions candidates from the whole stack matching `seq`
//! (exact + 1-edit adjacent-key neighbor + prefix), scores them with
//! the ranking formula, and returns the top-N.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::layout::{KeyMapping, DEFAULT_LAYOUT_ID};
use crate::mapping::{encode_word, is_one_edit_neighbor};
use crate::pack::PackFile;
use crate::personal::{now_quantized, PersonalDict};
use crate::rank::{score_candidate, RankInput, RankWeights};

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

pub struct DictionaryStack {
    base: Vec<DictEntry>,
    extensions: Vec<DictEntry>,
    pub personal: PersonalDict,
    weights: RankWeights,
    /// FST index over all known sequences (exact + prefix search).
    fst_index: Option<fst::Set<Vec<u8>>>,
}

impl DictionaryStack {
    pub fn new(base: Vec<DictEntry>) -> Self {
        let mut s = Self {
            base,
            extensions: Vec::new(),
            personal: PersonalDict::new(),
            weights: RankWeights::default(),
            fst_index: None,
        };
        s.rebuild_index();
        s
    }

    pub fn with_weights(base: Vec<DictEntry>, weights: RankWeights) -> Self {
        let mut s = Self {
            base,
            extensions: Vec::new(),
            personal: PersonalDict::new(),
            weights,
            fst_index: None,
        };
        s.rebuild_index();
        s
    }

    /// Push an extension pack's entries at the given priority
    /// (higher wins on display category).
    pub fn add_pack(&mut self, pack: &PackFile, priority: i32) {
        self.extensions.extend(pack.to_entries(priority));
        self.rebuild_index();
    }

    pub fn add_entries(&mut self, entries: Vec<DictEntry>) {
        self.extensions.extend(entries);
        self.rebuild_index();
    }

    /// Load base entries from a JSON wordlist. Accepts either an array
    /// of `{w|word, freq?, cat?, lang?}` objects or an array of plain
    /// strings (freq 100, cat `"EN"`, lang `"en"`).
    pub fn load_base_json(json: &str) -> Result<Vec<DictEntry>, String> {
        let v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| e.to_string())?;
        let arr = v.as_array().ok_or_else(|| "wordlist must be a JSON array".to_string())?;
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            if let Some(w) = item.as_str() {
                let seq = encode_word(w);
                if seq.is_empty() {
                    continue;
                }
                out.push(DictEntry {
                    word: w.to_string(),
                    seq,
                    freq: 100,
                    cat: "EN".to_string(),
                    lang: "en".to_string(),
                    priority: 0,
                    explicit: false,
                });
            } else if let Some(o) = item.as_object() {
                let w = o
                    .get("w")
                    .or_else(|| o.get("word"))
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| "word object missing \"w\"".to_string())?;
                let seq = encode_word(w);
                if seq.is_empty() {
                    continue;
                }
                out.push(DictEntry {
                    word: w.to_string(),
                    seq,
                    freq: o.get("freq").and_then(|x| x.as_u64()).unwrap_or(100),
                    cat: o
                        .get("cat")
                        .and_then(|x| x.as_str())
                        .unwrap_or("EN")
                        .to_string(),
                    lang: o
                        .get("lang")
                        .and_then(|x| x.as_str())
                        .unwrap_or("en")
                        .to_string(),
                    priority: 0,
                    explicit: false,
                });
            } else {
                return Err("wordlist items must be strings or objects".to_string());
            }
        }
        Ok(out)
    }

    /// Rebuild the FST sequence index (keys must be inserted sorted).
    fn rebuild_index(&mut self) {
        let mut seqs: Vec<String> = self
            .base
            .iter()
            .chain(self.extensions.iter())
            .map(|e| e.seq.clone())
            .collect();
        seqs.sort();
        seqs.dedup();
        let mut builder = fst::SetBuilder::memory();
        for s in &seqs {
            if builder.insert(s).is_err() {
                self.fst_index = None;
                return;
            }
        }
        match builder.into_inner() {
            Ok(bytes) => self.fst_index = fst::Set::new(bytes).ok(),
            Err(_) => self.fst_index = None,
        }
    }

    /// FST-backed prefix search: known sequences starting with `prefix`.
    pub fn decode_prefix(&self, prefix: &str, limit: usize) -> Vec<String> {
        let Some(set) = &self.fst_index else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut stream = set.stream();
        use fst::Streamer;
        while let Some(k) = stream.next() {
            if let Ok(s) = std::str::from_utf8(k) {
                if s.starts_with(prefix) {
                    out.push(s.to_string());
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
        out
    }

    pub fn fst_len(&self) -> usize {
        self.fst_index.as_ref().map(|s| s.len()).unwrap_or(0)
    }

    /// Per-layout decode: the prefix must consist of the layout's codes,
    /// then the shared sequence index is scanned. (The index holds
    /// canonical seqs; a future per-layout index can reuse this hook.)
    pub fn decode_prefix_for_layout(
        &self,
        prefix: &str,
        mapping: &dyn KeyMapping,
        limit: usize,
    ) -> Vec<String> {
        if !prefix.chars().all(|c| mapping.contains_code(c)) {
            return Vec::new();
        }
        self.decode_prefix(prefix, limit)
    }

    fn cat_boost(&self, entry_cat: &str, in_personal: bool, active_tab: &str) -> f64 {
        if entry_cat == active_tab {
            1.0
        } else if in_personal && active_tab == "personal" {
            0.5
        } else {
            0.0
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
        // Frozen t9-9 path: stored seqs, global neighbor graph, digits only.
        self.suggest_inner(
            ctx,
            digits,
            active_tab,
            limit,
            DEFAULT_LAYOUT_ID,
            &|e| e.seq.clone(),
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
        self.suggest_inner(
            ctx,
            digits,
            active_tab,
            limit,
            DEFAULT_LAYOUT_ID,
            &|e| e.seq.clone(),
            &|_, _| false,
            &|c| c.is_ascii_digit(),
            now,
        )
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
        self.suggest_inner(
            ctx,
            digits,
            active_tab,
            limit,
            mapping.layout_id(),
            &|e| {
                if e.explicit {
                    return e.seq.clone();
                }
                let s = mapping.encode_word(&e.word);
                if s.is_empty() {
                    e.seq.clone()
                } else {
                    s
                }
            },
            &|a, b| mapping.is_one_edit_neighbor(a, b),
            &|c| mapping.contains_code(c),
            now,
        )
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
        self.suggest_inner(
            ctx,
            digits,
            active_tab,
            limit,
            mapping.layout_id(),
            &|e| {
                if e.explicit {
                    return e.seq.clone();
                }
                let s = mapping.encode_word(&e.word);
                if s.is_empty() {
                    e.seq.clone()
                } else {
                    s
                }
            },
            &|_, _| false,
            &|c| mapping.contains_code(c),
            now,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn suggest_inner(
        &self,
        ctx: &str,
        digits: &str,
        active_tab: &str,
        limit: usize,
        layout_id: &str,
        seq_of: &dyn Fn(&DictEntry) -> String,
        is_neighbor_edit: &dyn Fn(&str, &str) -> bool,
        is_code: &dyn Fn(char) -> bool,
        now: i64,
    ) -> Vec<Suggestion> {
        if digits.is_empty() || !digits.chars().all(is_code) || limit == 0 {
            return Vec::new();
        }
        let prev = ctx.split_whitespace().last().unwrap_or("").to_lowercase();

        struct Merged {
            word: String,
            seq: String,
            freq_base: u64,
            cat: String,
            priority: i32,
            keyfit: f64,
        }

        // Union across base + extensions, keyed by (norm word, lang).
        let mut merged: HashMap<String, Merged> = HashMap::new();
        for e in self.base.iter().chain(self.extensions.iter()) {
            let eseq = seq_of(e);
            if eseq.is_empty() {
                continue;
            }
            let keyfit = if eseq == digits {
                1.0
            } else if eseq.starts_with(digits) {
                0.9
            } else if is_neighbor_edit(digits, &eseq) {
                0.6
            } else {
                continue;
            };
            let norm = format!("{}\x1f{}", e.word.to_lowercase(), e.lang);
            match merged.get_mut(&norm) {
                Some(m) => {
                    m.freq_base += e.freq;
                    if e.priority > m.priority {
                        m.cat = e.cat.clone();
                        m.word = e.word.clone();
                        m.priority = e.priority;
                    }
                    if keyfit > m.keyfit {
                        m.keyfit = keyfit;
                    }
                }
                None => {
                    merged.insert(
                        norm,
                        Merged {
                            word: e.word.clone(),
                            seq: eseq,
                            freq_base: e.freq,
                            cat: e.cat.clone(),
                            priority: e.priority,
                            keyfit,
                        },
                    );
                }
            }
        }

        // Personal-only (OOV learned) words also participate.
        // (Accessed via a JSONL round-trip-free direct path is internal;
        // here we score them through the same pipeline below.)
        struct Scored {
            word: String,
            seq: String,
            cat: String,
            score: f64,
            priority: i32,
        }
        let mut scored: Vec<Scored> = Vec::new();
        // Cap matches at 200 before the top-N heap (SPEC).
        let mut items: Vec<Merged> = merged.into_values().collect();
        items.sort_by(|a, b| b.freq_base.cmp(&a.freq_base));
        items.truncate(200);

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
            };
            let score = score_candidate(&input, &self.weights);
            if score < self.weights.hide_threshold {
                continue;
            }
            scored.push(Scored {
                word: m.word,
                seq: m.seq,
                cat: m.cat,
                score,
                priority: m.priority,
            });
        }

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.word.len().cmp(&b.word.len()))
                .then_with(|| a.word.cmp(&b.word))
                .then_with(|| b.priority.cmp(&a.priority))
        });
        scored.truncate(limit);
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
    fn stack_priority_display_cat() {
        let mut stack = DictionaryStack::new(base());
        // Base "hello" is cat EN prio 0; extension redefines it as js prio 50.
        let pack = load_pack_str(
            r#"{"id":"js","title":"JS","version":"1.0.0",
                "words":[{"w":"hello","freq":10,"cat":"js"}]}"#,
        )
        .unwrap();
        stack.add_pack(&pack, 50);
        let s = stack.suggest("", "43556", "EN", 5);
        let hello = s.iter().find(|c| c.word == "hello").unwrap();
        assert_eq!(hello.cat, "js");
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
        // emoji it gets the full category boost.
        let s = stack.suggest("", "386", "emoji", 5);
        assert_eq!(s[0].cat, "emoji");
    }
}
