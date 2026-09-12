//! Suggestion ranking formula (SPEC section 2).
//!
//! ```text
//! S(w) = 1.0*log10(freq_base + freq_personal + 1)
//!      + 1.2*P_personal(w)
//!      + 0.8*Bigram(ctx_prev, w)
//!      + 0.5*Recency(w)
//!      + 0.3*CategoryBoost(w)
//!      + 0.2*KeyFit(seq, w)
//!      - 1.5*RejectPenalty(w)
//! ```
//! Tie-break: shorter word, then lexicographic, then pack priority.

/// Tunable weights (constants in core, tunable per user in Settings).
#[derive(Clone, Debug)]
pub struct RankWeights {
    pub w_base: f64,
    pub w_personal: f64,
    pub w_bigram: f64,
    pub w_recency: f64,
    pub w_cat: f64,
    pub w_keyfit: f64,
    pub w_reject: f64,
    /// Candidates with `S < hide_threshold` are hidden unless expand-all.
    pub hide_threshold: f64,
}

impl Default for RankWeights {
    fn default() -> Self {
        Self {
            w_base: 1.0,
            w_personal: 1.2,
            w_bigram: 0.8,
            w_recency: 0.5,
            w_cat: 0.3,
            w_keyfit: 0.2,
            w_reject: 1.5,
            hide_threshold: -0.5,
        }
    }
}

/// Seconds in time constants.
pub const SEVEN_DAYS_SECS: f64 = 7.0 * 24.0 * 3600.0;

/// `log10(freq_base + freq_personal + 1)`.
pub fn base_term(freq_base: u64, freq_personal: u64) -> f64 {
    ((freq_base + freq_personal + 1) as f64).log10()
}

/// `exp(-dt / 7d)` since last accept; 0 when never accepted.
pub fn recency_term(now_ts: i64, last_seen_ts: i64) -> f64 {
    if last_seen_ts <= 0 {
        return 0.0;
    }
    let dt = (now_ts - last_seen_ts).max(0) as f64;
    (-dt / SEVEN_DAYS_SECS).exp()
}

/// `rejects / (accepts + rejects + 1)`, decayed upstream (30d).
pub fn reject_term(accepts: u64, rejects: u64) -> f64 {
    rejects as f64 / (accepts + rejects + 1) as f64
}

/// Personal bigram log-prob with add-one smoothing:
/// `log10(count(prev,w)+1) - log10(count(prev)+V)`. Cold-start backoff is
/// exactly `0.0` when there is no history for `prev` (`count_prev == 0`),
/// per SPEC ("backoff 0"): a new user with no bigram history must not be
/// penalized, and the score must not drift as `V` (vocab size) grows.
pub fn bigram_term(count_prev_word: u64, count_prev: u64, vocab: u64) -> f64 {
    if count_prev == 0 {
        return 0.0;
    }
    let v = vocab.max(1);
    ((count_prev_word + 1) as f64).log10() - ((count_prev + v) as f64).log10()
}

/// Inputs for scoring one candidate.
#[derive(Clone, Debug, Default)]
pub struct RankInput {
    pub freq_base: u64,
    pub freq_personal: u64,
    /// 0/1 (scaled by accepts upstream) if in personal dict.
    pub in_personal: bool,
    pub bigram_pw: u64,
    pub bigram_prev: u64,
    pub bigram_vocab: u64,
    pub now_ts: i64,
    pub last_seen_ts: i64,
    /// 1.0 if `w.cat == activeTab`, 0.5 if personal-tab boost, else 0.
    pub cat_boost: f64,
    /// 1.0 exact, 0.9 prefix, 0.6 neighbor.
    pub keyfit: f64,
    pub accepts: u64,
    pub rejects: u64,
}

/// Full score `S(w)`.
pub fn score_candidate(input: &RankInput, w: &RankWeights) -> f64 {
    w.w_base * base_term(input.freq_base, input.freq_personal)
        + w.w_personal * if input.in_personal { 1.0 } else { 0.0 }
        + w.w_bigram
            * bigram_term(input.bigram_pw, input.bigram_prev, input.bigram_vocab)
        + w.w_recency * recency_term(input.now_ts, input.last_seen_ts)
        + w.w_cat * input.cat_boost
        + w.w_keyfit * input.keyfit
        - w.w_reject * reject_term(input.accepts, input.rejects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> RankInput {
        RankInput {
            freq_base: 100,
            freq_personal: 0,
            in_personal: false,
            now_ts: 1_726_000_000,
            last_seen_ts: 0,
            cat_boost: 0.0,
            keyfit: 1.0,
            accepts: 0,
            rejects: 0,
            ..Default::default()
        }
    }

    #[test]
    fn exact_outranks_neighbor() {
        let w = RankWeights::default();
        let mut exact = base_input();
        exact.keyfit = 1.0;
        let mut near = base_input();
        near.keyfit = 0.6;
        assert!(score_candidate(&exact, &w) > score_candidate(&near, &w));
    }

    #[test]
    fn personal_boost_wins() {
        let w = RankWeights::default();
        let plain = base_input();
        let mut personal = base_input();
        personal.in_personal = true;
        personal.freq_personal = 5;
        personal.accepts = 5;
        personal.last_seen_ts = personal.now_ts;
        assert!(score_candidate(&personal, &w) > score_candidate(&plain, &w));
    }

    #[test]
    fn reject_penalty_demotion() {
        let w = RankWeights::default();
        let clean = base_input();
        let mut rejected = base_input();
        rejected.rejects = 9;
        assert!(score_candidate(&clean, &w) > score_candidate(&rejected, &w));
    }

    #[test]
    fn recency_decays() {
        assert!(recency_term(1000, 1000) > recency_term(1000 + 30 * 86400, 1000));
        assert_eq!(recency_term(1000, 0), 0.0);
    }

    #[test]
    fn bigram_cold_start_backoff_is_zero() {
        // SPEC "backoff 0": no history for `prev` => exactly 0.0,
        // independent of vocab size (no `log10(1/V)` penalty, no V drift).
        assert_eq!(bigram_term(0, 0, 1), 0.0);
        assert_eq!(bigram_term(0, 0, 10_000), 0.0);
        assert_eq!(bigram_term(5, 0, 10_000), 0.0);
        // With history, smoothing still applies (negative log-prob).
        let s = bigram_term(0, 10, 10_000);
        assert!(s < 0.0, "smoothed bigram must be negative, got {s}");
        let seen = bigram_term(9, 10, 10_000);
        assert!(seen > s, "observed pair must outrank unseen pair");
    }

    #[test]
    fn higher_freq_scores_higher() {
        let w = RankWeights::default();
        let mut low = base_input();
        low.freq_base = 1;
        let mut high = base_input();
        high.freq_base = 100000;
        assert!(score_candidate(&high, &w) > score_candidate(&low, &w));
    }
}
