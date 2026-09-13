//! Frozen learn/snippet/temporal/voice/clipboard gates (plan `12`, `11`, `13`, `15`).
//!
//! Snippet is current-document scratch, learn is keyboard-long memory — so
//! the old single `shouldLearnNow()` gate is split in two, plus the allied
//! read/write gates that must agree with it. These are pure functions over
//! [`GateInput`], mirroring `android/ime/.../SnippetGates.kt` exactly (one
//! frozen truth table, two definitions kept in lockstep by tests on both
//! sides). Deterministic: booleans in, booleans out, no clock, no I/O.
//!
//! Truth table (plan `12:43-53`):
//!
//! | case                  | learnNow | snippetNow |
//! |-----------------------|----------|------------|
//! | normal EN/NE/js       | true     | true       |
//! | numbers tab           | false    | false      |
//! | math tab              | false    | true       |
//! | password field        | false    | false      |
//! | incognito             | false    | false      |
//! | email/URI             | false    | true       |
//! | NUMBER/PHONE/DATETIME | false    | false      |

/// Inputs to every gate. `tab` is the canonical asset id (`words`, `ne`,
/// `js`, …, `numbers`, `math`); `learn_for_tab` mirrors the pack manifest
/// `privacy.learn` (`numbers`/`math` false — storage hygiene, plan `11:1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateInput {
    pub password: bool,
    pub incognito: bool,
    /// True only for free-TEXT input class (NUMBER/PHONE/DATETIME → false).
    pub text_class: bool,
    pub email_or_uri: bool,
    pub tab: String,
    pub learn_for_tab: bool,
}

impl GateInput {
    pub fn normal(tab: &str, learn_for_tab: bool) -> Self {
        Self {
            password: false,
            incognito: false,
            text_class: true,
            email_or_uri: false,
            tab: tab.to_string(),
            learn_for_tab,
        }
    }
}

/// Persistent learn: personal dict + bigrams + session events. Email/URI
/// suggests but never learns (high-entropy, low-reuse).
pub fn learn_now(g: &GateInput) -> bool {
    !g.password && !g.incognito && g.text_class && !g.email_or_uri && g.learn_for_tab
}

/// Ephemeral snippet helper. `numbers` is blocked (digit-commit layout
/// cannot show `{ }`, and OTP digits lingering even 2 min in RAM is risk
/// for zero benefit). `math`/`js`/`rust`/`html` are allowed even though
/// `math` has `learn=false` — the helpers ARE the point of those tabs.
/// Email/URI allows snippet (syntax chars still useful) while learn stays
/// off. Default is allow-everywhere-except-`numbers` (plan `12:78`):
/// simpler, one exclusion.
pub fn snippet_now(g: &GateInput) -> bool {
    !g.password && !g.incognito && g.text_class && g.tab != "numbers"
}

/// Temporal read/write gate (plan `13:122`): `temporalNow = learnNow` —
/// incognito/password never read or write temporal counts.
pub fn temporal_now(g: &GateInput) -> bool {
    learn_now(g)
}

/// Clipboard capture gate (plan `11:3`, `12:74`): password fields and
/// incognito sessions are never stored, never shown.
pub fn may_capture_clipboard(password: bool, incognito: bool) -> bool {
    !password && !incognito
}

/// Mic visibility (plan `15:37-38`): mic is hidden in `password` fields and
/// on the `numbers` tab (never voice digits into PIN/OTP). In incognito the
/// mic stays visible but transcripts insert without learning (same
/// [`learn_now`] gate as keys — no second rule).
pub fn mic_visible(password: bool, numbers_tab: bool) -> bool {
    !password && !numbers_tab
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(
        password: bool,
        incognito: bool,
        text_class: bool,
        email_or_uri: bool,
        tab: &str,
        learn_for_tab: bool,
    ) -> GateInput {
        GateInput {
            password,
            incognito,
            text_class,
            email_or_uri,
            tab: tab.to_string(),
            learn_for_tab,
        }
    }

    #[test]
    fn truth_table_normal_tabs_learn_and_snippet() {
        // Row 1: normal EN/NE/js (learn-for-tab true) → both gates.
        for tab in ["words", "EN", "ne", "NE", "js", "rust", "html", "emoji", "medical"] {
            let g = gate(false, false, true, false, tab, true);
            assert!(learn_now(&g), "{tab}: learnNow");
            assert!(snippet_now(&g), "{tab}: snippetNow");
            assert!(temporal_now(&g), "{tab}: temporalNow == learnNow");
        }
    }

    #[test]
    fn truth_table_numbers_tab_neither() {
        // Row 2: numbers tab → nothing in personal, nothing in snippet
        // strip (OTP must not linger even 2 min in RAM).
        let g = gate(false, false, true, false, "numbers", false);
        assert!(!learn_now(&g));
        assert!(!snippet_now(&g));
        assert!(!temporal_now(&g));
    }

    #[test]
    fn truth_table_math_tab_snippet_exempt_from_no_learn() {
        // Row 3: math keeps snippets while staying no-learn — the helpers
        // are the whole point of the tab (`\frac{}{}` though nothing learned).
        let g = gate(false, false, true, false, "math", false);
        assert!(!learn_now(&g));
        assert!(snippet_now(&g));
        // Temporal still follows learn (suppressed), never snippet.
        assert!(!temporal_now(&g));
    }

    #[test]
    fn truth_table_password_field_neither() {
        // Row 4: password → neither gate fires, no clipboard capture.
        let g = gate(true, false, true, false, "js", true);
        assert!(!learn_now(&g));
        assert!(!snippet_now(&g));
        assert!(!temporal_now(&g));
        assert!(!may_capture_clipboard(true, false));
        assert!(!mic_visible(true, false), "mic hidden in password fields");
    }

    #[test]
    fn truth_table_incognito_neither() {
        // Row 5: incognito → both gates false, no difference between them;
        // clipboard never captured; mic stays visible but inserts learn-free.
        let g = gate(false, true, true, false, "js", true);
        assert!(!learn_now(&g));
        assert!(!snippet_now(&g));
        assert!(!temporal_now(&g));
        assert!(!may_capture_clipboard(false, true));
        assert!(mic_visible(false, false), "mic visible in incognito (no-learn insert)");
    }

    #[test]
    fn truth_table_email_uri_snippets_but_never_learns() {
        // Row 6: email/URI → suggest + snippet, learn suppressed.
        let g = gate(false, false, true, true, "words", true);
        assert!(!learn_now(&g));
        assert!(snippet_now(&g));
        assert!(!temporal_now(&g));
    }

    #[test]
    fn truth_table_non_text_input_class_neither() {
        // Row 7: NUMBER/PHONE/DATETIME → commit raw, no prediction/learn.
        let g = gate(false, false, false, false, "js", true);
        assert!(!learn_now(&g));
        assert!(!snippet_now(&g));
        assert!(!temporal_now(&g));
    }

    #[test]
    fn clipboard_gate_all_four_combinations() {
        assert!(may_capture_clipboard(false, false));
        assert!(!may_capture_clipboard(true, false));
        assert!(!may_capture_clipboard(false, true));
        assert!(!may_capture_clipboard(true, true));
    }

    #[test]
    fn mic_hidden_in_password_and_numbers_only() {
        assert!(mic_visible(false, false));
        assert!(!mic_visible(true, false), "password hides mic");
        assert!(!mic_visible(false, true), "numbers tab hides mic");
        assert!(!mic_visible(true, true));
    }

    #[test]
    fn temporal_gate_is_learn_gate_everywhere() {
        // Plan 13:122 — one gate, no drift. Exhaust the boolean cube over
        // two representative tabs (learn-true tab + no-learn tab).
        for password in [false, true] {
            for incognito in [false, true] {
                for text_class in [false, true] {
                    for email_or_uri in [false, true] {
                        for (tab, learn) in [("words", true), ("math", false)] {
                            let g = gate(password, incognito, text_class, email_or_uri, tab, learn);
                            assert_eq!(
                                temporal_now(&g),
                                learn_now(&g),
                                "temporal must equal learn at {g:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
