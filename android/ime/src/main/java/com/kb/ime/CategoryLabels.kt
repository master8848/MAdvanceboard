package com.kb.ime

/**
 * Single source of truth for category UI labels (UX rename: `words` → "General").
 *
 * The engine cat id stays `words` everywhere below the UI: pack envelopes,
 * [BuiltinPacks] ids, [DictStackOrder] slots, export/import `stack` entries,
 * and the `suggest/learn` FFI `active_tab` all keep `words`. Only human-facing
 * strings go through [label]. The reverse trip (a visible label back to the
 * engine id) goes through [canonicalId], which also keeps the legacy `EN`
 * display label working.
 */
object CategoryLabels {
    /** Engine cat id for the general English vocabulary (never shown). */
    const val WORDS_ID = "words"

    /** UI label for [WORDS_ID] (tab strip, Settings, popups). */
    const val WORDS_LABEL = "General"

    /**
     * Human-facing label for an engine cat id. Unknown ids pass through
     * unchanged (custom packs show their own id until titled elsewhere).
     */
    fun label(catId: String): String = when (catId) {
        WORDS_ID -> WORDS_LABEL
        else -> catId
    }

    /**
     * Engine cat id for a visible tab label. `General` (any case) and the
     * legacy `EN` both resolve to [WORDS_ID]; `★personal` resolves to
     * `personal`; `nepali` stays a legacy alias of `ne`. Anything else
     * lowercases through (custom pack ids are already lowercase).
     */
    fun canonicalId(tabLabel: String): String = when (tabLabel) {
        WORDS_LABEL, WORDS_LABEL.lowercase(), "EN", "en" -> WORDS_ID
        "NE", "ne" -> "ne"
        "nepali" -> "ne" // legacy manifest id (renamed to `ne`)
        "★personal" -> "personal"
        else -> tabLabel.lowercase()
    }
}
