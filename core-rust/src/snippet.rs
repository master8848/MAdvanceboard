//! Ephemeral 2-minute snippet helper (plan `07:12`, `12:55-65`).
//!
//! RAM only: max [`MAX_ENTRIES`] entries of
//! `(trigger, next-tokens[], expires_at)`, `expires_at = now + 2 min`.
//! Never SQLite, never sync, never feeds rank weights beyond the visible
//! strip. No timer thread — expiry is checked lazily on the next keystroke
//! ([`SnippetBuffer::sweep`]) to avoid wakeups (plan `06` power budget).
//!
//! Lifecycle owner (plan `12` task B — who clears): the IME service owns the
//! single buffer instance and MUST clear it on every [`ClearCause`]. The
//! [`CLEAR_ON`] table is the executable contract: every cause maps to
//! `true` (wipe), enforced by [`should_clear_on`]. The buffer itself only
//! expires entries lazily via [`SnippetBuffer::sweep`]; all owner-driven
//! wipes are explicit [`SnippetBuffer::clear`] calls at the listed sites.
//!
//! Determinism: all times are caller-supplied `now_ms` (the service passes
//! its clock; tests inject fixed times), so identical call sequences yield
//! identical buffers.

/// Ephemeral helper TTL: ~2 minutes (plan `07:12`, `12:57`).
pub const SNIPPET_TTL_MS: i64 = 120_000;

/// Max live entries in the RAM-only buffer (plan `12:57`).
pub const MAX_ENTRIES: usize = 3;

/// Every event that must wipe the snippet buffer (plan `12:61-62`).
/// The service owns the buffer; each variant names its clear site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearCause {
    /// Snippet expansion committed (delete-after-expand).
    ExpansionCommit,
    /// Active tab switched (helpers belong to one tab's scratch).
    TabSwitch,
    /// IME hidden / input finished (nothing carries over).
    ImeHide,
    /// Incognito entered (no TTL carryover into the session).
    IncognitoEntry,
    /// Incognito exited (no TTL carryover out of the session).
    IncognitoExit,
    /// Password field focused (neither gate fires there).
    PasswordFocus,
    /// TTL sweep on next keystroke (lazy expiry, no timer).
    TtlSweep,
}

/// All clear sites that wipe the buffer. Every [`ClearCause`] maps to
/// `true` today: the table exists so a future "keep across X" decision is
/// an explicit, reviewed `false` — never a silently missed wipe.
pub const CLEAR_ON: &[(ClearCause, bool)] = &[
    (ClearCause::ExpansionCommit, true),
    (ClearCause::TabSwitch, true),
    (ClearCause::ImeHide, true),
    (ClearCause::IncognitoEntry, true),
    (ClearCause::IncognitoExit, true),
    (ClearCause::PasswordFocus, true),
    (ClearCause::TtlSweep, true),
];

/// True when `cause` must wipe the buffer (see [`CLEAR_ON`]).
pub fn should_clear_on(cause: ClearCause) -> bool {
    CLEAR_ON
        .iter()
        .find(|(c, _)| *c == cause)
        .map(|(_, wipe)| *wipe)
        .unwrap_or(false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    trigger: String,
    tokens: Vec<String>,
    expires_at_ms: i64,
}

/// RAM-only snippet buffer. Times are `i64` millis supplied by the caller
/// (service clock in production, fixed values in tests).
#[derive(Clone, Debug, Default)]
pub struct SnippetBuffer {
    entries: Vec<Entry>,
}

impl SnippetBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Live entry count after a lazy TTL sweep.
    pub fn size(&mut self, now_ms: i64) -> usize {
        self.sweep(now_ms);
        self.entries.len()
    }

    pub fn is_empty(&mut self, now_ms: i64) -> bool {
        self.size(now_ms) == 0
    }

    /// Push a trigger's follow-tokens. Blank triggers and empty token
    /// lists are rejected with `Err` (caller bug, buffer unchanged).
    pub fn offer(
        &mut self,
        trigger: &str,
        tokens: &[String],
        now_ms: i64,
    ) -> Result<(), String> {
        if trigger.trim().is_empty() {
            return Err("SnippetBuffer::offer: blank trigger rejected".to_string());
        }
        if tokens.is_empty() {
            return Err("SnippetBuffer::offer: empty token list rejected".to_string());
        }
        self.sweep(now_ms);
        while self.entries.len() >= MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push(Entry {
            trigger: trigger.to_string(),
            tokens: tokens.iter().take(MAX_ENTRIES).cloned().collect(),
            expires_at_ms: now_ms.saturating_add(SNIPPET_TTL_MS),
        });
        Ok(())
    }

    /// Drop expired entries. Called lazily on every keystroke — no timer.
    pub fn sweep(&mut self, now_ms: i64) {
        self.entries.retain(|e| e.expires_at_ms > now_ms);
    }

    /// Live follow-tokens (oldest first), after a lazy sweep.
    pub fn peek(&mut self, now_ms: i64) -> Vec<String> {
        self.sweep(now_ms);
        self.entries
            .iter()
            .flat_map(|e| e.tokens.iter().cloned())
            .take(MAX_ENTRIES)
            .collect()
    }

    /// Delete-after-expand: returns the live tokens and wipes the buffer.
    /// Every snippet expansion must go through here so nothing lingers.
    pub fn consume(&mut self, now_ms: i64) -> Vec<String> {
        let live = self.peek(now_ms);
        self.entries.clear();
        live
    }

    /// Immediate wipe (tab switch, IME hide, incognito/password entry).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn every_lifecycle_owner_wipes() {
        // Task B contract: the owner table must cover expansion commit, tab
        // switch, IME hide, incognito entry/exit, password focus, TTL sweep.
        for (cause, wipe) in CLEAR_ON {
            assert!(wipe, "{cause:?} must wipe the buffer");
            assert!(should_clear_on(*cause));
        }
        assert_eq!(CLEAR_ON.len(), 7, "a new cause must extend this table");
    }

    #[test]
    fn if_suggests_if_paren_for_2min_then_expires() {
        // Plan 12 acceptance: type `if` in js → `if (` suggested; accept →
        // `{ }` strip for 2 min; after 3 min → gone (lazy sweep).
        let mut b = SnippetBuffer::new();
        b.offer("if", &toks(&["if (", "{", "}"]), 0).unwrap();
        assert_eq!(b.peek(0), toks(&["if (", "{", "}"]));
        // Still live just before TTL…
        assert!(!b.peek(SNIPPET_TTL_MS - 1).is_empty());
        // …gone after 3 min via the lazy keystroke sweep (no timer).
        assert!(b.peek(3 * 60 * 1000).is_empty());
        assert!(b.is_empty(3 * 60 * 1000));
    }

    #[test]
    fn delete_after_expand_wipes() {
        let mut b = SnippetBuffer::new();
        b.offer("if", &toks(&["if ("]), 0).unwrap();
        let live = b.consume(1_000);
        assert_eq!(live, toks(&["if ("]));
        assert!(b.is_empty(1_000), "consume must wipe (no lingering)");
    }

    #[test]
    fn tab_switch_hides_strip_reopen_after_ttl_stays_empty() {
        // Switch to EN → strip gone; reopen js after 3 min → still gone.
        let mut b = SnippetBuffer::new();
        b.offer("if", &toks(&["if (", "{"]), 0).unwrap();
        b.clear(); // TabSwitch owner event.
        assert!(b.is_empty(1_000));
        assert!(b.peek(3 * 60 * 1000).is_empty());
    }

    #[test]
    fn incognito_entry_and_exit_wipe_no_carryover() {
        let mut b = SnippetBuffer::new();
        b.offer("if", &toks(&["if ("]), 0).unwrap();
        b.clear(); // IncognitoEntry.
        assert!(b.is_empty(1_000));
        b.offer("for", &toks(&["for ("]), 2_000).unwrap();
        b.clear(); // IncognitoExit.
        assert!(b.is_empty(3_000));
    }

    #[test]
    fn max_3_entries_fifo() {
        let mut b = SnippetBuffer::new();
        for t in ["a", "b", "c", "d"] {
            b.offer(t, &toks(&[t]), 0).unwrap();
        }
        assert_eq!(b.size(0), MAX_ENTRIES);
        // Oldest ("a") evicted; newest three survive.
        assert_eq!(b.peek(0), toks(&["b", "c", "d"]));
    }

    #[test]
    fn blank_trigger_and_empty_tokens_rejected_loudly() {
        let mut b = SnippetBuffer::new();
        assert!(b.offer("", &toks(&["x"]), 0).is_err());
        assert!(b.offer("  ", &toks(&["x"]), 0).is_err());
        assert!(b.offer("if", &[], 0).is_err());
        assert!(b.is_empty(0), "rejected offers must leave the buffer unchanged");
    }

    #[test]
    fn never_feeds_rank_weights() {
        // Structural: this module imports no ranking state — the buffer can
        // only surface tokens to the visible strip, never to scores. The
        // test pins the observable half: peek/consume return exactly the
        // offered tokens, unweighted and unranked (insertion order kept).
        let mut b = SnippetBuffer::new();
        b.offer("z", &toks(&["z1", "z2"]), 0).unwrap();
        b.offer("a", &toks(&["a1"]), 0).unwrap();
        assert_eq!(b.peek(0), toks(&["z1", "z2", "a1"]));
    }
}
