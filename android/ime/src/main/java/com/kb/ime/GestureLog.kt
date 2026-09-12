package com.kb.ime

import android.content.Context
import java.io.File

/**
 * False-trigger tuning log (`log.jsonl`, plan 01 §Detection).
 *
 * One JSON object per line: `{ts, zone, gesture, action, detail}`.
 * Zones: `pad` | `bar` | `tabs` | `qwerty`. Used by Settings → Gesture
 * Tuning to show volume + undo rate (false-trigger estimate = undone
 * deletes / total fling-left commits).
 *
 * All failures surface via [onError] — never swallowed: the service routes
 * them to a user-visible status line in Gesture Tuning, and log-stats read
 * failures return [LogStats] with `unreadable=true` instead of zeros that
 * would look like "no false triggers".
 *
 * The file is bounded: appends past [MAX_LOG_BYTES] rotate to the newest
 * half of lines plus one `log/rotated` marker line carrying the kept/total
 * counts, so the tuning signal can never grow the app's disk footprint
 * without bound.
 */
class GestureLog(
    private val context: Context,
    private val onError: (String) -> Unit = {}
) {
    data class LogStats(
        val events: Int,
        val flingLeftCommits: Int,
        val undoneDeletes: Int,
        val unreadable: Boolean,
        val error: String? = null
    )

    private fun file(): File = File(context.filesDir, "log.jsonl")

    @Synchronized
    fun record(zone: String, gesture: String, action: String, detail: String = "") {
        require(zone in ZONES) { "unknown gesture zone: $zone" }
        val line = buildString {
            append("{\"ts\":").append(System.currentTimeMillis())
            append(",\"zone\":\"").append(zone).append('"')
            append(",\"gesture\":\"").append(gesture).append('"')
            append(",\"action\":\"").append(action).append('"')
            append(",\"detail\":\"").append(detail.replace("\"", "'")).append("\"}")
        }
        try {
            val f = file()
            f.appendText(line + "\n")
            if (f.length() > MAX_LOG_BYTES) rotate(f)
        } catch (e: Exception) {
            onError("Gesture log write failed: ${e.message}")
        }
    }

    /**
     * Drops the oldest half of lines once the log passes [MAX_LOG_BYTES],
     * keeping a `log/rotated` marker with kept/total counts so the trim is
     * observable in stats instead of a silent hole. Never throws.
     */
    private fun rotate(f: File) {
        try {
            val lines = f.readLines()
            if (lines.size <= 1) return
            val keep = lines.takeLast(lines.size / 2)
            val marker = "{\"ts\":" + System.currentTimeMillis() +
                ",\"zone\":\"pad\",\"gesture\":\"log\",\"action\":\"rotated\"," +
                "\"detail\":\"kept ${keep.size} of ${lines.size}\"}"
            f.writeText((listOf(marker) + keep).joinToString("\n", postfix = "\n"))
        } catch (e: Exception) {
            onError("Gesture log rotation failed: ${e.message}")
        }
    }

    /** Undo-rate stats for the Tuning screen. Never throws. */
    fun stats(): LogStats {
        val f = file()
        if (!f.exists()) return LogStats(0, 0, 0, false)
        return try {
            var events = 0
            var commits = 0
            var undone = 0
            f.bufferedReader().useLines { lines ->
                lines.forEach { line ->
                    if (line.isBlank()) return@forEach
                    events++
                    if ("\"gesture\":\"fling-left\"" in line && "\"action\":\"commit" in line) commits++
                    if ("\"action\":\"undo-delete\"" in line) undone++
                }
            }
            LogStats(events, commits, undone, false)
        } catch (e: Exception) {
            onError("Gesture log read failed: ${e.message}")
            LogStats(0, 0, 0, true, e.message)
        }
    }

    /** Clears the log; failure surfaces via [onError] and returns false. */
    fun clear(): Boolean = try {
        val f = file()
        if (f.exists() && !f.delete()) {
            onError("Could not clear gesture log: delete returned false")
            false
        } else {
            true
        }
    } catch (e: Exception) {
        onError("Could not clear gesture log: ${e.message}")
        false
    }

    companion object {
        val ZONES = setOf("pad", "bar", "tabs", "qwerty")

        /** Append-only tuning log never exceeds this on disk (see [rotate]). */
        const val MAX_LOG_BYTES = 256 * 1024
    }
}
