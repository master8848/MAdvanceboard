package com.kb.ime

/**
 * Clipboard history core (plan/11): pure JVM, no Android imports — unit
 * tested on the JVM without Robolectric.
 *
 * Privacy contract (enforced by callers, documented here):
 * - Clipboard content is RAM-first and TTL-enforced; unpinned items older
 *   than [ttlHours] are dropped by [sweep].
 * - Pinned items are the user's explicit "keep": exempt from TTL and from
 *   the unpinned ring cap, persist until unpinned/deleted.
 * - Capture is gated by [shouldCapture]: password fields and incognito
 *   sessions (system OR manual) never store, never show.
 * - Pasted text commits raw (no `learn`/`record_bigram`/session log) and
 *   clipboard items are excluded from sync/export files.
 *
 * Time is injected (`nowMs`) so tests can advance the clock past the TTL.
 */
data class ClipboardItem(
    /** Stable id: hash of text + creation ts (dedupes identical pastes). */
    val id: String,
    val text: String,
    val createdMs: Long,
    val pinned: Boolean = false
)

object ClipboardHistory {
    /** Per-item char cap: longer pastes truncate (documented, not silent). */
    const val MAX_ITEM_CHARS = 5_000

    /** Ring cap for UNPINNED items; pins are exempt (explicit keep). */
    const val MAX_UNPINNED = 100

    /** Default retention: 7 days (plan/11 §3). */
    const val DEFAULT_TTL_HOURS = 168L

    /** Settings presets: 1h / 24h / 7d / 30d. */
    val TTL_PRESETS_HOURS: List<Long> = listOf(1L, 24L, 168L, 720L)

    /** Custom TTL bounds (hours): 1h minimum, 30d maximum like presets. */
    const val MIN_TTL_HOURS = 1L
    const val MAX_TTL_HOURS = 720L

    fun validateTtlHours(hours: Long) {
        require(hours in MIN_TTL_HOURS..MAX_TTL_HOURS) {
            "Clipboard retention must be $MIN_TTL_HOURS-$MAX_TTL_HOURS hours, got: $hours"
        }
    }

    /**
     * Capture gate: exactly the two plan/11 exclusions. Callers pass the
     * field state; blank/overlong handling lives in [sanitize].
     */
    fun shouldCapture(isPasswordField: Boolean, isIncognito: Boolean): Boolean =
        !isPasswordField && !isIncognito

    /**
     * Sanitizes raw clipboard text. Returns null for blanks (nothing to
     * store); truncates to [MAX_ITEM_CHARS] otherwise.
     */
    fun sanitize(raw: String?): String? {
        if (raw == null) return null
        val trimmed = raw.trim()
        if (trimmed.isEmpty()) return null
        return if (trimmed.length > MAX_ITEM_CHARS) trimmed.substring(0, MAX_ITEM_CHARS) else trimmed
    }

    /**
     * Adds [text] (already sanitized or raw — sanitized again here) to
     * [items]. Identical text moves to top with a fresh timestamp instead
     * of duplicating; pinned rows keep their pin. Unpinned overflow past
     * [MAX_UNPINNED] drops oldest-first (pins never count, never drop).
     */
    fun add(items: List<ClipboardItem>, rawText: String, nowMs: Long): List<ClipboardItem> {
        val text = sanitize(rawText) ?: return items
        val withoutDup = items.filterNot { it.text == text }
        val keptPin = items.firstOrNull { it.text == text }?.pinned ?: false
        val withNew = listOf(
            ClipboardItem(id = stableId(text, nowMs), text = text, createdMs = nowMs, pinned = keptPin)
        ) + withoutDup
        val pinned = withNew.filter { it.pinned }.sortedByDescending { it.createdMs }
        val unpinned = withNew.filterNot { it.pinned }.take(MAX_UNPINNED)
        // Pins on top (recency), then unpinned recency — matches panel order.
        // The new item carries nowMs (newest ts) so it lands first in its
        // section; the take() above already kept recency order for unpinned.
        return pinned + unpinned
    }

    /**
     * TTL sweep: drops unpinned items older than [ttlHours]. Pins exempt.
     * Runs on load, on every add, and on TTL change — no WorkManager in the
     * lean `:ime` process (documented gap vs plan/11's daily-worker idea;
     * every keyboard start + every capture is a sweep point, so expiry is
     * enforced without a background worker).
     */
    fun sweep(items: List<ClipboardItem>, ttlHours: Long, nowMs: Long): List<ClipboardItem> {
        validateTtlHours(ttlHours)
        val ttlMs = ttlHours * 3_600_000L
        return items.filter { it.pinned || nowMs - it.createdMs <= ttlMs }
    }

    fun setPinned(items: List<ClipboardItem>, id: String, pinned: Boolean): List<ClipboardItem> {
        var found = false
        val out = items.map {
            if (it.id == id) {
                found = true
                it.copy(pinned = pinned)
            } else it
        }
        require(found) { "Clipboard item not found: $id" }
        return out
    }

    fun delete(items: List<ClipboardItem>, id: String): List<ClipboardItem> {
        val out = items.filterNot { it.id == id }
        require(out.size != items.size) { "Clipboard item not found: $id" }
        return out
    }

    /** Clear-all keeps pins unless [includePins] (confirm dialog offers both). */
    fun clear(items: List<ClipboardItem>, includePins: Boolean): List<ClipboardItem> =
        if (includePins) emptyList() else items.filter { it.pinned }

    /** Substring search filter (case-insensitive) for the panel. */
    fun search(items: List<ClipboardItem>, query: String): List<ClipboardItem> {
        val q = query.trim()
        if (q.isEmpty()) return items
        return items.filter { it.text.contains(q, ignoreCase = true) }
    }

    /** Panel order: pins (recency) then unpinned (recency). */
    fun ordered(items: List<ClipboardItem>): List<ClipboardItem> {
        val pinned = items.filter { it.pinned }.sortedByDescending { it.createdMs }
        val rest = items.filterNot { it.pinned }.sortedByDescending { it.createdMs }
        return pinned + rest
    }

    fun stableId(text: String, nowMs: Long): String {
        val h = text.hashCode().toUInt().toString(16)
        return "cb-${h}-${nowMs}"
    }

    // -- Minimal JSON codec (no org.json: not on the JVM test classpath). --

    fun toJson(items: List<ClipboardItem>): String {
        val sb = StringBuilder("[")
        items.forEachIndexed { i, it ->
            if (i > 0) sb.append(",")
            sb.append("{\"id\":\"").append(escape(it.id)).append("\",")
            sb.append("\"text\":\"").append(escape(it.text)).append("\",")
            sb.append("\"ts\":").append(it.createdMs).append(",")
            sb.append("\"pin\":").append(if (it.pinned) "true" else "false").append("}")
        }
        return sb.append("]").toString()
    }

    /**
     * Parses [toJson] output. Corrupt input throws [IllegalStateException]
     * with the cause — callers surface it (status line) and fall back to
     * empty, never a half-loaded history.
     */
    fun fromJson(json: String): List<ClipboardItem> {
        try {
            return parseArray(json.trim())
        } catch (e: IllegalStateException) {
            throw e
        } catch (e: Exception) {
            throw IllegalStateException("Clipboard history unreadable: ${e.message}", e)
        }
    }

    private fun escape(s: String): String {
        val sb = StringBuilder(s.length)
        for (c in s) {
            when (c) {
                '\\' -> sb.append("\\\\")
                '"' -> sb.append("\\\"")
                '\n' -> sb.append("\\n")
                '\r' -> sb.append("\\r")
                '\t' -> sb.append("\\t")
                else -> if (c < ' ') sb.append("\\u%04x".format(c.code)) else sb.append(c)
            }
        }
        return sb.toString()
    }

    private fun parseArray(json: String): List<ClipboardItem> {
        if (!json.startsWith("[") || !json.endsWith("]")) {
            throw IllegalStateException("Clipboard history unreadable: not a JSON array")
        }
        val body = json.substring(1, json.length - 1).trim()
        if (body.isEmpty()) return emptyList()
        val out = mutableListOf<ClipboardItem>()
        var i = 0
        while (i < body.length) {
            if (body[i] != '{') throw IllegalStateException("Clipboard history unreadable: expected '{' at $i")
            var depth = 0
            var inStr = false
            var esc = false
            var j = i
            while (j < body.length) {
                val c = body[j]
                if (esc) {
                    esc = false
                } else if (inStr && c == '\\') {
                    esc = true
                } else if (c == '"') {
                    inStr = !inStr
                } else if (!inStr && c == '{') {
                    depth++
                } else if (!inStr && c == '}') {
                    depth--
                    if (depth == 0) break
                }
                j++
            }
            if (depth != 0) throw IllegalStateException("Clipboard history unreadable: unbalanced object")
            out.add(parseObject(body.substring(i, j + 1)))
            i = j + 1
            while (i < body.length && body[i].isWhitespace()) i++
            if (i < body.length) {
                if (body[i] != ',') throw IllegalStateException("Clipboard history unreadable: expected ',' at $i")
                i++
                while (i < body.length && body[i].isWhitespace()) i++
            }
        }
        return out
    }

    private fun parseObject(obj: String): ClipboardItem {
        fun strField(name: String): String {
            val key = "\"$name\":\""
            val start = obj.indexOf(key)
            if (start < 0) throw IllegalStateException("Clipboard history unreadable: missing \"$name\"")
            var i = start + key.length
            val sb = StringBuilder()
            var esc = false
            while (i < obj.length) {
                val c = obj[i]
                if (esc) {
                    when (c) {
                        '\\' -> sb.append('\\')
                        '"' -> sb.append('"')
                        'n' -> sb.append('\n')
                        'r' -> sb.append('\r')
                        't' -> sb.append('\t')
                        'u' -> {
                            if (i + 4 >= obj.length) {
                                throw IllegalStateException("Clipboard history unreadable: bad \\u escape")
                            }
                            sb.append(obj.substring(i + 1, i + 5).toInt(16).toChar())
                            i += 4
                        }
                        else -> throw IllegalStateException("Clipboard history unreadable: bad escape \\$c")
                    }
                    esc = false
                } else if (c == '\\') {
                    esc = true
                } else if (c == '"') {
                    return sb.toString()
                } else {
                    sb.append(c)
                }
                i++
            }
            throw IllegalStateException("Clipboard history unreadable: unterminated \"$name\"")
        }
        fun rawField(name: String): String {
            val key = "\"$name\":"
            val start = obj.indexOf(key)
            if (start < 0) throw IllegalStateException("Clipboard history unreadable: missing \"$name\"")
            var i = start + key.length
            while (i < obj.length && obj[i].isWhitespace()) i++
            // Scope the scan to the tail after the key (values never nest).
            val tail = obj.substring(i)
            val cut = tail.indexOfFirst { it == ',' || it == '}' }
            if (cut < 0) throw IllegalStateException("Clipboard history unreadable: bad \"$name\"")
            return tail.substring(0, cut).trim()
        }
        val id = strField("id")
        val text = strField("text")
        val ts = rawField("ts").toLongOrNull()
            ?: throw IllegalStateException("Clipboard history unreadable: bad \"ts\"")
        val pin = when (rawField("pin")) {
            "true" -> true
            "false" -> false
            else -> throw IllegalStateException("Clipboard history unreadable: bad \"pin\"")
        }
        if (text.length > MAX_ITEM_CHARS) {
            throw IllegalStateException("Clipboard history unreadable: item over ${MAX_ITEM_CHARS} chars")
        }
        return ClipboardItem(id = id, text = text, createdMs = ts, pinned = pin)
    }
}
