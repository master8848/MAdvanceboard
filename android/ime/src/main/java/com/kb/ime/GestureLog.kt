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
            file().appendText(line + "\n")
        } catch (e: Exception) {
            onError("Gesture log write failed: ${e.message}")
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
    }
}
