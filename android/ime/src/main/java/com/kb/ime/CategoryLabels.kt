package com.kb.ime

/**
 * Single source of truth for category UI badges (compact tab strip).
 *
 * Tabs show one compact badge per engine cat id (FlorisBoard-style,
 * single row, 40-48dp): `123` / `E` / `ने` / `JS` / `Rs` / `HTML` /
 * `😀` / `∑` / `⚕` / `★`. The engine cat id stays unchanged everywhere
 * below the UI: pack envelopes, [BuiltinPacks] ids, [DictStackOrder]
 * slots, export/import `stack` entries, and the `suggest/learn` FFI
 * `active_tab` all keep the raw id (`words`, `ne`, …). Only human-facing
 * strings go through [label]. The reverse trip (a visible badge back to
 * the engine id) goes through [canonicalId], which also keeps every
 * legacy spelling working (`General`, `EN`, `NE`, `nepali`, full ids).
 *
 * Round-trip rule (unit-tested): `canonicalId(label(id)) == id` for every
 * built-in id. This is load-bearing for emoji: the strip shows `😀`, and
 * without the badge → `emoji` mapping the host would forward `😀` as
 * `active_tab` and lose the category boost (previously broken).
 */
object CategoryLabels {
    /** Engine cat id for the general English vocabulary (never shown). */
    const val WORDS_ID = "words"

    /** UI badge for [WORDS_ID] (tab strip, Settings, popups). */
    const val WORDS_LABEL = "E"

    /**
     * Centered spacebar language label for an engine cat id: full language
     * name where one exists (`words` → `English`, `ne` → `नेपाली`), the
     * compact [label] badge otherwise (custom packs show their own id).
     */
    fun spaceLabel(catId: String): String = when (catId) {
        WORDS_ID -> "English"
        "ne" -> "नेपाली"
        else -> label(catId)
    }

    /**
     * Compact badge for an engine cat id. Unknown ids pass through
     * unchanged (custom packs show their own id until titled elsewhere).
     */
    fun label(catId: String): String = when (catId) {
        WORDS_ID -> WORDS_LABEL
        "ne" -> "ने"
        "numbers" -> "123"
        "js" -> "JS"
        "rust" -> "Rs"
        "html" -> "HTML"
        "emoji" -> "\uD83D\uDE00"
        "math" -> "∑"
        "medical" -> "⚕"
        "personal", "★personal", "★" -> "★"
        else -> catId
    }

    /**
     * Engine cat id for a visible tab badge. Every [label] badge resolves
     * back (`E` → [WORDS_ID], `ने` → `ne`, `😀` → `emoji`, …) plus the
     * legacy spellings: `General` (any case), `EN`/`en` → [WORDS_ID];
     * `NE`/`nepali` → `ne`; full ids (`numbers`, `js`, …) map to
     * themselves; `★personal`/`★` → `personal`. Anything else lowercases
     * through (custom pack ids are already lowercase).
     */
    fun canonicalId(tabLabel: String): String = when (tabLabel) {
        WORDS_LABEL, "e", "General", "general", "EN", "en", "words", "WORDS" -> WORDS_ID
        "ने", "NE", "ne", "nepali", "NEPALI" -> "ne"
        "123", "numbers", "NUMBERS" -> "numbers"
        "JS", "js", "Js" -> "js"
        "Rs", "rs", "RS", "rust", "RUST" -> "rust"
        "HTML", "html", "Html" -> "html"
        "\uD83D\uDE00", "emoji", "EMOJI" -> "emoji"
        "∑", "math", "MATH" -> "math"
        "⚕", "Med", "med", "MED", "medical", "MEDICAL" -> "medical"
        "★personal", "★", "personal", "PERSONAL" -> "personal"
        else -> tabLabel.lowercase()
    }
}
