//! Temporal / periodic smart suggestions (plan `13`).
//!
//! Two surfaces, cheapest first:
//!
//! - **Surface A (P1) — prefix re-rank, always on.** Adds
//!   `w_temporal * temporal_term` to the `rank.rs` score, computed only
//!   for the ≤200 already-scored candidates. One tiny table
//!   `context_counts(word, bucket, count)` where
//!   `bucket = tod(4: night/morning/afternoon/evening) × dow(2:
//!   weekday/weekend)` = 8 buckets max. Cap: top 500 words only, `COUNT`
//!   as `u16`, total <50 KB. Update piggybacks the existing 2 s coalesced
//!   flush (zero extra wakeups); query is one hash lookup + integer add
//!   per candidate. Deterministic: buckets derive from the 1 h-quantized
//!   clock (`personal::quantize_ts`), never exact minutes.
//! - **Surface B (P2) — routine row, zero-keypress, gated, toggleable.**
//!   When `seq` is empty only bucket-matched routines show (max 3 chips,
//!   never in the top-3 prediction slots). A routine qualifies after ≥5
//!   accepts in the same bucket per 14 d; it is suppressed while its
//!   tap-through precision is <60% and removed entirely <40% (kill rule).
//!   One user toggle hides row + open-button together. Respects
//!   incognito/password (hidden) and `numbers` (never).
//!
//! P3 (monthly/long-period) is explicitly DEFERRED (plan `13:88`) — not
//! built here.
//!
//! Scoring integration: [`crate::rank::temporal_term`] converts a bucket
//! count to the additive term; [`crate::rank::RankWeights::w_temporal`]
//! scales it (default `0.0` = dormant: scores stay byte-identical until the
//! P1 lift is measured; enable explicitly after the 14 d falsification
//! window — see the kill rule below).

use std::collections::HashMap;

/// (word, bucket) counts cap: top 500 words only, total <50 KB.
pub const CONTEXT_WORD_CAP: usize = 500;

/// Routine qualification: ≥5 accepts in the same bucket per 14 d window.
pub const ROUTINE_MIN_ACCEPTS: u64 = 5;
/// 14-day mining window in seconds.
pub const ROUTINE_WINDOW_SECS: i64 = 14 * 24 * 3600;
/// Routine row shows at most 3 chips.
pub const ROUTINE_MAX_CHIPS: usize = 3;
/// Precision (accepted/shown) below which a routine is suppressed…
pub const ROUTINE_PRECISION_MIN: f64 = 0.60;
/// …and below which it is removed entirely (kill rule, plan `13:113`).
pub const ROUTINE_PRECISION_KILL: f64 = 0.40;

/// Time-of-day bucket: night 0–5, morning 5–12, afternoon 12–18,
/// evening 18–24 (local-hour interpretation is the caller's; the mapping
/// hour→bucket is frozen here so every caller agrees).
pub fn tod_bucket(hour_0_23: u8) -> u8 {
    match hour_0_23 {
        0..=4 => 0,
        5..=11 => 1,
        12..=17 => 2,
        _ => 3,
    }
}

/// Day-of-week bucket: 0 = weekday, 1 = weekend (Sat/Sun).
/// `dow_monday_0`: Monday = 0 … Sunday = 6.
pub fn dow_bucket(dow_monday_0: u8) -> u8 {
    if dow_monday_0 >= 5 { 1 } else { 0 }
}

/// Combined bucket `tod * 2 + dow`, in `0..8`.
pub fn bucket(tod: u8, dow: u8) -> u8 {
    debug_assert!(tod < 4 && dow < 2);
    tod * 2 + dow
}

/// Bucket for a Unix timestamp (seconds, UTC civil math, deterministic):
/// hour = `(ts / 3600) % 24`, Monday-based weekday with 1970-01-01
/// (a Thursday) as anchor. Callers SHOULD pass a 1 h-quantized `ts`
/// ([`crate::personal::quantize_ts`]) so buckets never flap per-second.
pub fn bucket_at(ts_secs: i64) -> u8 {
    let ts = ts_secs.max(0) as u64;
    let hour = ((ts / 3600) % 24) as u8;
    // Days since epoch, Monday = 0: 1970-01-01 was a Thursday (offset 3).
    let dow = ((ts / 86_400 + 3) % 7) as u8;
    bucket(tod_bucket(hour), dow_bucket(dow))
}

/// P1 store: `(word-lower, bucket) -> count` (u16, saturating).
/// SQLite is durability only (same rule as `06:3`); this map is the RAM
/// read path. Cap: [`CONTEXT_WORD_CAP`] distinct words; overflow evicts
/// the lowest-total-count word (ties: lexicographically largest first, so
/// the survivor set is deterministic — never HashMap order).
#[derive(Clone, Debug, Default)]
pub struct ContextCounts {
    counts: HashMap<(String, u8), u16>,
}

impl ContextCounts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one accept of `word` in `bucket` (gated by
    /// [`crate::gates::temporal_now`] at the call site — this store never
    /// sees incognito/password writes).
    pub fn record(&mut self, word: &str, bucket: u8) {
        let w = word.to_lowercase();
        let c = self.counts.entry((w, bucket)).or_insert(0);
        *c = c.saturating_add(1);
        self.evict_if_needed();
    }

    /// Accepts of `word` in `bucket` (0 when unseen).
    pub fn count(&self, word: &str, bucket: u8) -> u16 {
        self.counts
            .get(&(word.to_lowercase(), bucket))
            .copied()
            .unwrap_or(0)
    }

    /// Distinct words tracked (cap denominator).
    pub fn words(&self) -> usize {
        let mut seen: Vec<&str> = self.counts.keys().map(|(w, _)| w.as_str()).collect();
        seen.sort();
        seen.dedup();
        seen.len()
    }

    fn evict_if_needed(&mut self) {
        // Count distinct words (cheap enough at ≤501 entries on the accept
        // path — accepts are rare vs keystrokes).
        let mut totals: HashMap<&str, u64> = HashMap::new();
        for ((w, _), c) in &self.counts {
            *totals.entry(w.as_str()).or_insert(0) += u64::from(*c);
        }
        if totals.len() <= CONTEXT_WORD_CAP {
            return;
        }
        // Evict the lowest-total word; ties break to the lexicographically
        // LARGEST (deterministic, keeps the natural vocab head).
        let victim = totals
            .iter()
            .min_by(|(wa, ca), (wb, cb)| ca.cmp(cb).then_with(|| wb.cmp(wa)))
            .map(|(w, _)| (*w).to_string());
        if let Some(v) = victim {
            self.counts.retain(|(w, _), _| *w != v);
        }
    }
}

/// P2 routine candidate: a word learned in one bucket, with tap-through
/// stats for the precision gate.
#[derive(Clone, Debug, PartialEq)]
pub struct Routine {
    pub word: String,
    pub bucket: u8,
    pub accepts_14d: u64,
    pub shown: u64,
    pub accepted: u64,
}

impl Routine {
    /// Tap-through precision (`accepted / shown`); `1.0` when never shown
    /// (unmeasured routines are not punished — suppression needs data).
    pub fn precision(&self) -> f64 {
        if self.shown == 0 {
            1.0
        } else {
            self.accepted as f64 / self.shown as f64
        }
    }

    /// Qualification: ≥5 accepts in-bucket per 14 d AND precision ≥60%.
    pub fn qualifies(&self) -> bool {
        self.accepts_14d >= ROUTINE_MIN_ACCEPTS && self.precision() >= ROUTINE_PRECISION_MIN
    }

    /// Kill rule: precision <40% with data → remove the row entirely.
    pub fn killed(&self) -> bool {
        self.shown > 0 && self.precision() < ROUTINE_PRECISION_KILL
    }
}

/// P2 routine-row selection (pure): bucket-matched, qualified, not killed,
/// max [`ROUTINE_MAX_CHIPS`] chips, deterministic order (accepts desc,
/// then word asc). Returns `[]` when the toggle is OFF, when
/// `learn_allowed` is false (incognito/password — hidden, plan `13:80`),
/// or when `numbers_tab` (never on the numbers tab).
pub fn routine_row(
    routines: &[Routine],
    bucket_now: u8,
    routines_enabled: bool,
    learn_allowed: bool,
    numbers_tab: bool,
) -> Vec<String> {
    if !routines_enabled || !learn_allowed || numbers_tab {
        return Vec::new();
    }
    let mut v: Vec<&Routine> = routines
        .iter()
        .filter(|r| r.bucket == bucket_now && r.qualifies() && !r.killed())
        .collect();
    v.sort_by(|a, b| {
        b.accepts_14d
            .cmp(&a.accepts_14d)
            .then_with(|| a.word.cmp(&b.word))
    });
    v.into_iter()
        .take(ROUTINE_MAX_CHIPS)
        .map(|r| r.word.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_cover_8_and_are_deterministic() {
        assert_eq!(tod_bucket(0), 0);
        assert_eq!(tod_bucket(4), 0);
        assert_eq!(tod_bucket(5), 1);
        assert_eq!(tod_bucket(11), 1);
        assert_eq!(tod_bucket(12), 2);
        assert_eq!(tod_bucket(17), 2);
        assert_eq!(tod_bucket(18), 3);
        assert_eq!(tod_bucket(23), 3);
        assert_eq!(dow_bucket(0), 0);
        assert_eq!(dow_bucket(4), 0);
        assert_eq!(dow_bucket(5), 1);
        assert_eq!(dow_bucket(6), 1);
        // Combined space is exactly 8 buckets.
        let mut seen = std::collections::HashSet::new();
        for t in 0..4 {
            for d in 0..2 {
                seen.insert(bucket(t, d));
            }
        }
        assert_eq!(seen.len(), 8);
        // Same ts → same bucket (byte-identical re-rank inputs).
        assert_eq!(bucket_at(1_726_000_000), bucket_at(1_726_000_000));
    }

    #[test]
    fn bucket_at_known_instants() {
        // 1970-01-01T00:00Z: Thursday night → tod 0, dow 0 → bucket 0.
        assert_eq!(bucket_at(0), 0);
        // 1970-01-03T06:00Z: Saturday morning → tod 1, dow 1 → bucket 3.
        assert_eq!(bucket_at(2 * 86_400 + 6 * 3600), 3);
    }

    #[test]
    fn p1_morning_boost_mechanism() {
        // Falsifiable mechanism behind the ≥3pt morning-lift bar: a word
        // learned in the morning bucket scores strictly higher there than
        // in the night bucket (same weight, same counts elsewhere zero).
        use crate::rank::{temporal_term, RankInput, RankWeights};
        let mut cc = ContextCounts::new();
        let morning = bucket(1, 0);
        let night = bucket(0, 0);
        for _ in 0..10 {
            cc.record("good morning", morning);
        }
        let w = RankWeights {
            w_temporal: 1.0,
            ..Default::default()
        };
        let boost_morning = w.w_temporal * temporal_term(cc.count("good morning", morning));
        let boost_night = w.w_temporal * temporal_term(cc.count("good morning", night));
        assert!(boost_morning > boost_night + 0.5, "morning lift must be material");
        // Unseen word: zero boost, never negative (no penalty for novelty).
        assert_eq!(temporal_term(cc.count("good morning", 7)), temporal_term(0));
        assert_eq!(temporal_term(0), 0.0);
        let _ = RankInput::default();
    }

    #[test]
    fn p1_cap_500_words_u16_never_grows_ram() {
        let mut cc = ContextCounts::new();
        for i in 0..(CONTEXT_WORD_CAP + 50) {
            cc.record(&format!("word{i:04}"), 0);
        }
        assert!(
            cc.words() <= CONTEXT_WORD_CAP,
            "cap 500 words, got {}",
            cc.words()
        );
        // Saturating u16: no wrap on hot words.
        let mut hot = ContextCounts::new();
        for _ in 0..70_000 {
            hot.record("hot", 0);
        }
        assert_eq!(hot.count("hot", 0), u16::MAX);
    }

    #[test]
    fn p2_routine_row_gated_toggleable() {
        let mk = |word: &str, bucket: u8, accepts_14d: u64, shown: u64, accepted: u64| Routine {
            word: word.to_string(),
            bucket,
            accepts_14d,
            shown,
            accepted,
        };
        let b = bucket(1, 0);
        let routines = vec![
            mk("good morning jaan", b, 9, 10, 8), // qualifies (9≥5, 80%≥60%)
            mk("standup time", b, 6, 10, 3),      // suppressed (30%<60%)
            mk("doomed", b, 8, 10, 2),            // killed (20%<40%)
            mk("night owl", bucket(0, 0), 9, 10, 9), // wrong bucket
            mk("weak", b, 3, 0, 0),               // too few accepts
        ];
        let row = routine_row(&routines, b, true, true, false);
        assert_eq!(row, vec!["good morning jaan".to_string()]);
        // Toggle OFF hides row + button together.
        assert!(routine_row(&routines, b, false, true, false).is_empty());
        // Incognito/password hides (learn gate false).
        assert!(routine_row(&routines, b, true, false, false).is_empty());
        // Numbers tab never.
        assert!(routine_row(&routines, b, true, true, true).is_empty());
    }

    #[test]
    fn p2_precision_kill_rule() {
        let mk = |shown: u64, accepted: u64| Routine {
            word: "x".to_string(),
            bucket: 0,
            accepts_14d: 9,
            shown,
            accepted,
        };
        assert!(!mk(0, 0).killed(), "unmeasured routines are not killed");
        assert!(mk(10, 9).qualifies());
        assert!(!mk(10, 5).qualifies(), "50% < 60%: suppressed");
        assert!(!mk(10, 5).killed(), "50% ≥ 40%: kept dormant, not removed");
        assert!(mk(10, 3).killed(), "30% < 40%: removed");
    }

    #[test]
    fn p2_max_3_chips_deterministic_order() {
        let b = bucket(2, 0);
        let routines: Vec<Routine> = ["d", "c", "b", "a", "e"]
            .iter()
            .map(|w| Routine {
                word: w.to_string(),
                bucket: b,
                accepts_14d: 7,
                shown: 0,
                accepted: 0,
            })
            .collect();
        // Equal accepts → word asc, top 3 only.
        let row = routine_row(&routines, b, true, true, false);
        assert_eq!(row, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }
}
