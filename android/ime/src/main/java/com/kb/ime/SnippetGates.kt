package com.kb.ime

/**
 * Split learn/snippet gates (plan 12:24-53).
 *
 * Snippet is current-document scratch, learn is keyboard-long memory — so
 * the old single `shouldLearnNow()` gate is split in two. These are pure
 * functions over [GateInput] so the frozen truth table is unit-testable;
 * the service delegates to them (single definition, no drift).
 *
 * Truth table (plan 12:43-53):
 *
 * | case                  | learnNow | snippetNow |
 * |-----------------------|----------|------------|
 * | normal EN/NE/js       | true     | true       |
 * | numbers tab           | false    | false      |
 * | math tab              | false    | true       |
 * | password field        | false    | false      |
 * | incognito             | false    | false      |
 * | email/URI             | false    | true       |
 * | NUMBER/PHONE/DATETIME | false    | false      |
 */
data class GateInput(
    val password: Boolean,
    val incognito: Boolean,
    /** True only for free-TEXT input class (NUMBER/PHONE/DATETIME → false). */
    val textClass: Boolean,
    val emailOrUri: Boolean,
    /** Canonical asset id (`words`, `ne`, `js`, …, `numbers`, `math`). */
    val tab: String,
    /** Manifest `privacy.learn` for [tab] (numbers/math → false). */
    val learnForTab: Boolean,
)

object SnippetGates {
    /**
     * Persistent learn: personal dict + bigrams + session events.
     * Email/URI suggests but never learns (high-entropy, low-reuse).
     */
    fun learnNow(g: GateInput): Boolean =
        !g.password && !g.incognito && g.textClass && !g.emailOrUri && g.learnForTab

    /**
     * Ephemeral snippet helper. `numbers` is blocked (digit-commit layout
     * cannot show `{ }`, and OTP digits lingering even 2min in RAM is risk
     * for zero benefit). `math`/`js`/`rust`/`html` are allowed even though
     * `math` has `learn=false` — the helpers ARE the point of those tabs.
     * Email/URI allows snippet (syntax chars still useful) while learn
     * stays off. Default is allow-everywhere-except-`numbers` (plan 12:80):
     * simpler, one exclusion.
     */
    fun snippetNow(g: GateInput): Boolean =
        !g.password && !g.incognito && g.textClass && g.tab != "numbers"

    /**
     * Clipboard capture gate (plan 11:3, 12:74): password fields and
     * incognito sessions are never stored, never shown. Snippet-committed
     * text is ordinary committed text — it must consult this gate before
     * any present-or-future clipboard capture, exactly like any other
     * commit. (No clipboard history infra exists yet; this freezes the
     * rule the clipboard track must enforce.)
     */
    fun mayCaptureClipboard(password: Boolean, incognito: Boolean): Boolean =
        !password && !incognito
}
