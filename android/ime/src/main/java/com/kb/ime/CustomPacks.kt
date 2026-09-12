package com.kb.ime

import android.content.Context

/**
 * User custom categories (names flow, plan/08).
 *
 * A custom category is a first-class tab backed by the user's own wordlist:
 * isolated ranking under its tab (engine custom-tab policy), personal-learn
 * overlay via the active category id, enable/disable toggle, priority
 * within 10-90. There is NO built-in names starter list (no in-repo names
 * source exists) — the flow is import-only by design, kept trivially easy
 * (see `docs/CUSTOM_CATEGORIES.md`).
 *
 * Split by testability: [CustomPackValidation] is pure Kotlin (no Android
 * imports — covered by JVM unit tests); [CustomPackStore] is the thin
 * SharedPreferences persistence layer (same lean-IME rationale as
 * [LayoutStore]: no DataStore/Room in the `:ime` process). Personal
 * learning stays in SQLite (`:sync`); custom packs are static vocab, so
 * prefs + engine install is the whole durability story. Export carries
 * packs (envelope + priority + enabled) as one JSON document.
 */
data class CustomPack(
    val id: String,
    val title: String,
    /** `PackFile` envelope JSON (`{id, title, version, words[]}`). */
    val packJson: String,
    val priority: Int,
    val enabled: Boolean
)

/** One parsed wordlist row. */
data class CustomWord(val word: String, val freq: Long)

/** Strict wordlist parse outcome: rows plus one exact error per bad row. */
data class WordlistOutcome(val words: List<CustomWord>, val errors: List<String>)

object CustomPackValidation {
    private val ID_REGEX = Regex("^[a-z][a-z0-9_]{1,23}$")

    /** Category ids a user pack must not claim (built-in pack cats + overlay). */
    val RESERVED_IDS: Set<String> = setOf(
        "words", "ne", "numbers", "js", "rust", "html", "emoji", "math", "medical", "personal"
    )

    const val PRIORITY_MIN = 10
    const val PRIORITY_MAX = 90
    const val FREQ_DEFAULT = 100L
    const val FREQ_MAX = 1_000_000L

    /** Null when valid, else the exact user-facing error. */
    fun validateId(id: String): String? {
        if (!ID_REGEX.matches(id)) {
            return "category id \"$id\" invalid: use 2-24 chars of a-z, 0-9, _ starting with a letter"
        }
        if (id in RESERVED_IDS) {
            return "category id \"$id\" invalid: reserved by a built-in pack (pick another id)"
        }
        return null
    }

    /** Null when valid, else the exact user-facing error. */
    fun validatePriority(priority: Int): String? {
        if (priority !in PRIORITY_MIN..PRIORITY_MAX) {
            return "priority $priority out of range $PRIORITY_MIN-$PRIORITY_MAX " +
                "(base 0 and personal 100 are fixed)"
        }
        return null
    }

    /**
     * Parse pasted/imported wordlist text: one `word [freq]` per line.
     * Blank lines and `#` comments are skipped. Never drops silently:
     * every bad row lands in [WordlistOutcome.errors] as `line N: ...`
     * (bad freq, trailing text, duplicate, unencodable). An empty wordlist
     * is itself one error. The ASCII-letter pre-check mirrors the engine's
     * empty-seq rejection (`core-rust/src/pack.rs`); the engine install
     * (`add_pack_json`) remains authoritative and fails loudly too.
     */
    fun parseWordlist(text: String): WordlistOutcome {
        val words = mutableListOf<CustomWord>()
        val errors = mutableListOf<String>()
        val seen = mutableSetOf<String>()
        text.lines().forEachIndexed { index, raw ->
            val lineNo = index + 1
            val line = raw.trim()
            if (line.isEmpty() || line.startsWith("#")) return@forEachIndexed
            val parts = line.split(Regex("\\s+"))
            val word = parts[0]
            val freq: Long = if (parts.size == 1) {
                FREQ_DEFAULT
            } else {
                val parsed = parts[1].toLongOrNull()
                if (parsed == null || parsed < 1 || parsed > FREQ_MAX) {
                    errors.add("line $lineNo \"$word\": freq \"${parts[1]}\" out of range 1-$FREQ_MAX")
                    return@forEachIndexed
                }
                parsed
            }
            if (parts.size > 2) {
                errors.add("line $lineNo \"$word\": expected `word [freq]`, trailing text rejected")
                return@forEachIndexed
            }
            val key = word.lowercase()
            if (!seen.add(key)) {
                errors.add("line $lineNo \"$word\": duplicate word")
                return@forEachIndexed
            }
            if (!word.any { it in 'a'..'z' || it in 'A'..'Z' }) {
                errors.add("line $lineNo \"$word\": encodes to empty seq (latin letters required)")
                return@forEachIndexed
            }
            words.add(CustomWord(word, freq))
        }
        if (words.isEmpty() && errors.isEmpty()) {
            errors.add("wordlist is empty: add one `word [freq]` per line")
        }
        return WordlistOutcome(words, errors)
    }

    /**
     * Build the `PackFile` envelope JSON for engine install via the
     * existing `add_pack_json` FFI (no new native surface needed).
     * Throws [IllegalArgumentException] naming every problem — callers
     * surface the message, never a half-built envelope.
     */
    fun buildPackJson(id: String, title: String, words: List<CustomWord>, lang: String = "en"): String {
        validateId(id)?.let { throw IllegalArgumentException(it) }
        require(title.isBlank().not()) { "pack title must not be empty" }
        require(words.isNotEmpty()) { "pack \"$id\" has no words" }
        val rows = words.joinToString(",") { w ->
            """{"w":${jsonString(w.word)},"freq":${w.freq},"cat":${jsonString(id)},"lang":${jsonString(lang)}}"""
        }
        return """{"id":${jsonString(id)},"title":${jsonString(title)},"version":"1.0.0","words":[$rows]}"""
    }

    /**
     * Resolve the visible tab strip: built-ins (minus pinned `★personal`)
     * + enabled customs in priority order, `★personal` pinned last.
     * Disabled customs are simply absent from the input — that IS the
     * hide-tab mechanism. Pure and unit-tested.
     */
    fun resolveTabs(enabledCustomIds: List<String>): List<String> {
        val customs = enabledCustomIds
            .filter { it !in RESERVED_IDS }
            .distinct()
            .filter { it !in DEFAULT_CATEGORIES }
        val base = DEFAULT_CATEGORIES.filter { it != "★personal" }
        return base + customs + "★personal"
    }

    /**
     * Build the export document carrying every custom pack (envelope +
     * priority + enabled). Import reverses it ([CustomPackStore.importJson]).
     */
    fun exportJson(packs: List<CustomPack>): String {
        val sorted = packs.sortedWith(compareByDescending<CustomPack> { it.priority }.thenBy { it.id })
        val items = sorted.joinToString(",") { p ->
            """{"id":${jsonString(p.id)},"title":${jsonString(p.title)},"priority":${p.priority},"enabled":${p.enabled},"pack":${p.packJson}}"""
        }
        return """{"packs":[$items]}"""
    }

    internal fun jsonString(s: String): String {
        val escaped = s
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\n")
            .replace("\r", "\\r")
            .replace("\t", "\\t")
        return "\"$escaped\""
    }
}

/**
 * SharedPreferences persistence for custom packs (`kb_custom_packs`).
 * Every write validates loudly ([IllegalArgumentException] carrying the
 * exact row errors); every read failure names the pack id + cause
 * ([IllegalStateException]) — a corrupt custom pack degrades engine init
 * with its cause, never a silent skip.
 */
object CustomPackStore {
    const val PREFS_NAME = "kb_custom_packs"
    private const val KEY_IDS = "custom_ids"

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    private fun idSet(prefs: android.content.SharedPreferences): MutableSet<String> =
        prefs.getStringSet(KEY_IDS, emptySet())?.toMutableSet() ?: mutableSetOf()

    fun list(context: Context): List<CustomPack> {
        val p = try {
            prefs(context)
        } catch (e: Exception) {
            throw IllegalStateException("Custom packs unreadable: ${e.message}", e)
        }
        return try {
            idSet(p).sorted().map { id -> readOne(p, id) }
        } catch (e: IllegalStateException) {
            throw e
        } catch (e: Exception) {
            throw IllegalStateException("Custom packs unreadable: ${e.message}", e)
        }
    }

    /** Enabled packs in stack order (priority desc, then id). */
    fun enabledPacks(context: Context): List<CustomPack> =
        list(context).filter { it.enabled }
            .sortedWith(compareByDescending<CustomPack> { it.priority }.thenBy { it.id })

    /** Enabled custom ids in stack order (feeds [CustomPackValidation.resolveTabs]). */
    fun enabledIds(context: Context): List<String> = enabledPacks(context).map { it.id }

    private fun readOne(p: android.content.SharedPreferences, id: String): CustomPack {
        val title = p.getString("pack_${id}_title", null)
            ?: throw IllegalStateException("Custom pack \"$id\": title missing")
        val json = p.getString("pack_${id}_json", null)
            ?: throw IllegalStateException("Custom pack \"$id\": envelope missing")
        val priority = p.getInt("pack_${id}_prio", -1)
        if (priority == -1) throw IllegalStateException("Custom pack \"$id\": priority missing")
        val enabled = p.getBoolean("pack_${id}_enabled", true)
        return CustomPack(id, title, json, priority, enabled)
    }

    /**
     * Create or replace a custom pack from pasted/imported wordlist text.
     * Throws [IllegalArgumentException] with every validation error
     * (id, title, priority, per-row `line N` diagnostics) — nothing is
     * stored unless the whole pack validates.
     */
    fun save(
        context: Context,
        id: String,
        title: String,
        wordlistText: String,
        priority: Int,
        lang: String = "en"
    ): CustomPack {
        val problems = mutableListOf<String>()
        CustomPackValidation.validateId(id)?.let { problems.add(it) }
        if (title.isBlank()) problems.add("pack title must not be empty")
        CustomPackValidation.validatePriority(priority)?.let { problems.add(it) }
        val parsed = CustomPackValidation.parseWordlist(wordlistText)
        problems.addAll(parsed.errors)
        if (problems.isNotEmpty()) {
            throw IllegalArgumentException(
                "custom pack \"$id\" invalid (${problems.size} problem(s)): " + problems.joinToString("; ")
            )
        }
        val envelope = CustomPackValidation.buildPackJson(id, title, parsed.words, lang)
        try {
            val p = prefs(context)
            val ids = idSet(p)
            ids.add(id)
            p.edit()
                .putStringSet(KEY_IDS, ids)
                .putString("pack_${id}_title", title)
                .putString("pack_${id}_json", envelope)
                .putInt("pack_${id}_prio", priority)
                .putBoolean("pack_${id}_enabled", p.getBoolean("pack_${id}_enabled", true))
                .apply()
        } catch (e: IllegalArgumentException) {
            throw e
        } catch (e: Exception) {
            throw IllegalStateException("Could not save custom pack \"$id\": ${e.message}", e)
        }
        return CustomPack(id, title, envelope, priority, list(context).find { it.id == id }?.enabled ?: true)
    }

    /** Enable/disable toggle: `false` hides the tab (engine skips the pack). */
    fun setEnabled(context: Context, id: String, enabled: Boolean) {
        try {
            val p = prefs(context)
            if (!idSet(p).contains(id)) {
                throw IllegalArgumentException("unknown custom pack \"$id\"")
            }
            p.edit().putBoolean("pack_${id}_enabled", enabled).apply()
        } catch (e: IllegalArgumentException) {
            throw e
        } catch (e: Exception) {
            throw IllegalStateException("Could not toggle custom pack \"$id\": ${e.message}", e)
        }
    }

    fun remove(context: Context, id: String) {
        try {
            val p = prefs(context)
            val ids = idSet(p)
            if (!ids.remove(id)) throw IllegalArgumentException("unknown custom pack \"$id\"")
            p.edit()
                .putStringSet(KEY_IDS, ids)
                .remove("pack_${id}_title")
                .remove("pack_${id}_json")
                .remove("pack_${id}_prio")
                .remove("pack_${id}_enabled")
                .apply()
        } catch (e: IllegalArgumentException) {
            throw e
        } catch (e: Exception) {
            throw IllegalStateException("Could not delete custom pack \"$id\": ${e.message}", e)
        }
    }

    /** Export document carrying all packs (see [CustomPackValidation.exportJson]). */
    fun exportJson(context: Context): String = CustomPackValidation.exportJson(list(context))

    /**
     * Import an [exportJson] document. Strict: every pack validates, and
     * ANY failure aborts the whole import with all per-pack causes —
     * never a partial import. Unknown JSON shape is an explicit error.
     */
    fun importJson(context: Context, json: String): List<CustomPack> {
        val parsed: List<Triple<String, String, org.json.JSONObject>> = try {
            val root = org.json.JSONObject(json)
            val arr = root.optJSONArray("packs")
                ?: throw IllegalArgumentException("import: missing \"packs\" array")
            (0 until arr.length()).map { i ->
                val o = arr.getJSONObject(i)
                Triple(
                    o.optString("id", ""),
                    o.optString("title", ""),
                    o
                )
            }
        } catch (e: IllegalArgumentException) {
            throw e
        } catch (e: Exception) {
            throw IllegalArgumentException("import: invalid JSON (${e.message})")
        }
        if (parsed.isEmpty()) throw IllegalArgumentException("import: no packs in document")
        // Validate everything before writing anything.
        data class Staged(val id: String, val pack: CustomPack)
        val staged = parsed.map { (id, title, o) ->
            val problems = mutableListOf<String>()
            CustomPackValidation.validateId(id)?.let { problems.add(it) }
            if (title.isBlank()) problems.add("pack \"$id\": title missing")
            val priority = if (o.has("priority")) o.optInt("priority", -1) else -1
            CustomPackValidation.validatePriority(priority)?.let { problems.add(it) }
            val enabled = o.optBoolean("enabled", true)
            val envelope = o.optJSONObject("pack")?.toString()
                ?: o.optString("pack", "").takeIf { it.isNotEmpty() }
                ?: run { problems.add("pack \"$id\": envelope missing"); "" }
            if (problems.isNotEmpty()) {
                throw IllegalArgumentException(
                    "import: pack \"$id\" invalid: " + problems.joinToString("; ")
                )
            }
            Staged(id, CustomPack(id, title, envelope, priority, enabled))
        }
        try {
            val p = prefs(context)
            val ids = idSet(p)
            val ed = p.edit()
            for (s in staged) {
                ids.add(s.id)
                ed.putString("pack_${s.id}_title", s.pack.title)
                    .putString("pack_${s.id}_json", s.pack.packJson)
                    .putInt("pack_${s.id}_prio", s.pack.priority)
                    .putBoolean("pack_${s.id}_enabled", s.pack.enabled)
            }
            ed.putStringSet(KEY_IDS, ids).apply()
        } catch (e: Exception) {
            throw IllegalStateException("import: could not persist packs (${e.message})", e)
        }
        return staged.map { it.pack }
    }
}
