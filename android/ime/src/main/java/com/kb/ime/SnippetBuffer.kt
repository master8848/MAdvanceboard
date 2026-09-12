package com.kb.ime

/**
 * Ephemeral 2-minute snippet helper (plan 07:12, 12:57).
 *
 * RAM only: max [SnippetPalettes.MAX_ENTRIES] entries of
 * `(trigger, next-tokens[], expires_at)`, `expires_at = now + 2min`.
 * Never SQLite, never sync, never feeds rank weights beyond the visible
 * strip. No timer thread — expiry is checked lazily on the next keystroke
 * ([sweep]) to avoid wakeups (plan 06 power budget).
 *
 * This class is pure Kotlin (no Android imports) so the lifecycle contract
 * is unit-testable on the JVM. The service owns one instance and clears it
 * on: expansion commit ([consume]), tab switch, IME hide, incognito
 * entry/exit, password-field focus, and lazily via [sweep].
 */
class SnippetBuffer(
    private val clock: () -> Long = { System.currentTimeMillis() },
) {
    private data class Entry(
        val trigger: String,
        val tokens: List<String>,
        val expiresAt: Long,
    )

    private val entries: ArrayDeque<Entry> = ArrayDeque()

    /** Live entry count after a lazy TTL sweep (for tests/diagnostics). */
    fun size(now: Long = clock()): Int {
        sweep(now)
        return entries.size
    }

    fun isEmpty(now: Long = clock()): Boolean = size(now) == 0

    /**
     * Pushes a trigger's follow-tokens. Blank triggers and empty token
     * lists are rejected loudly (they indicate a caller bug, not a state
     * worth storing) — the buffer is left unchanged.
     */
    fun offer(trigger: String, tokens: List<String>, now: Long = clock()) {
        require(trigger.isNotBlank()) { "SnippetBuffer.offer: blank trigger rejected" }
        require(tokens.isNotEmpty()) { "SnippetBuffer.offer: empty token list rejected" }
        sweep(now)
        while (entries.size >= SnippetPalettes.MAX_ENTRIES) entries.removeFirst()
        entries.addLast(
            Entry(trigger, tokens.take(SnippetPalettes.MAX_ENTRIES), now + SnippetPalettes.SNIPPET_TTL_MS)
        )
    }

    /** Drops expired entries. Called lazily on every keystroke — no timer. */
    fun sweep(now: Long = clock()) {
        while (entries.isNotEmpty() && entries.first().expiresAt <= now) entries.removeFirst()
        // Entries are FIFO by expiry under a monotonic clock; a non-monotonic
        // jump (clock change) still converges because offer() sweeps first.
        if (entries.any { it.expiresAt <= now }) entries.removeAll { it.expiresAt <= now }
    }

    /** Live follow-tokens (newest last), after a lazy sweep. */
    fun peek(now: Long = clock()): List<String> {
        sweep(now)
        return entries.flatMap { it.tokens }.take(SnippetPalettes.MAX_ENTRIES)
    }

    /**
     * Delete-after-expand: returns the live tokens and wipes the buffer.
     * Every snippet expansion must go through here so nothing lingers.
     */
    fun consume(now: Long = clock()): List<String> {
        val live = peek(now)
        entries.clear()
        return live
    }

    /** Immediate wipe (tab switch, IME hide, incognito/password entry). */
    fun clear() {
        entries.clear()
    }
}
