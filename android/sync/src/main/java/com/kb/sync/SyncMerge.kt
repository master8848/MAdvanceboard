package com.kb.sync

import org.json.JSONObject

/**
 * Last-writer-wins per-word merge over `{count, last_seen, del, device_id}`.
 * Newer `lastSeen` wins; ties break toward lexicographically larger `deviceId`.
 * Tombstones replicate like any other row and are never resurrected by merge.
 * Wire name is `del` (canonical, SPEC §5); `deleted` (SPEC §4) is still
 * accepted on read for compat.
 */
/** One skipped JSONL input line with its cause. Replaces silent drops:
 * every malformed line lands here so importers can surface the count. */
data class JsonlSkippedLine(val lineNumber: Int, val cause: String)

/**
 * Strict JSONL import outcome: parsed rows plus every malformed line with
 * its cause. `skipped.isEmpty()` means a lossless import.
 */
data class JsonlImportOutcome(
    val rows: List<PersonalWordEntity>,
    val skipped: List<JsonlSkippedLine>
)

object SyncMerge {
    fun mergeWord(local: PersonalWordEntity, remote: PersonalWordEntity): PersonalWordEntity {
        if (remote.lastSeen > local.lastSeen) return remote
        if (local.lastSeen > remote.lastSeen) return local
        return if (remote.deviceId >= local.deviceId) remote else local
    }

    fun mergeAll(
        local: List<PersonalWordEntity>,
        remote: List<PersonalWordEntity>
    ): List<PersonalWordEntity> {
        val out = local.associateBy { it.word }.toMutableMap()
        for (r in remote) {
            val l = out[r.word]
            out[r.word] = if (l == null) r else mergeWord(l, r)
        }
        return out.values.toList()
    }

    /** Export one row per line as JSONL. Tombstone key is `del` (canonical). */
    fun toJsonl(rows: List<PersonalWordEntity>): String =
        rows.joinToString("\n") { e ->
            JSONObject()
                .put("word", e.word)
                .put("lang", e.lang)
                .put("count", e.count)
                .put("last_seen", e.lastSeen)
                .put("del", e.deleted)
                .put("device_id", e.deviceId)
                .toString()
        }

    /**
     * Import JSONL previously produced by [toJsonl].
     *
     * Lenient entry point (kept for compat): malformed lines are skipped.
     * Prefer [fromJsonlWithDiagnostics] — it returns the same rows PLUS the
     * per-line [JsonlSkippedLine] causes this method drops. A non-empty
     * `skipped` means the peer sent corrupt rows: log the count, never
     * assume a lossless import.
     */
    fun fromJsonl(jsonl: String): List<PersonalWordEntity> =
        fromJsonlWithDiagnostics(jsonl).rows

    /**
     * Strict import: never drops silently. Every non-blank line yields
     * either a row in [JsonlImportOutcome.rows] or an entry in
     * [JsonlImportOutcome.skipped] with the 1-based line number + cause.
     */
    fun fromJsonlWithDiagnostics(jsonl: String): JsonlImportOutcome {
        val rows = mutableListOf<PersonalWordEntity>()
        val skipped = mutableListOf<JsonlSkippedLine>()
        jsonl.lineSequence()
            .map { it.trim() }
            .forEachIndexed { index, line ->
                if (line.isEmpty()) return@forEachIndexed
                try {
                    val o = JSONObject(line)
                    // Compat: accept legacy `deleted` (SPEC §4) as well as `del` (SPEC §5).
                    val tombstone = if (o.has("del")) o.optBoolean("del", false)
                        else o.optBoolean("deleted", false)
                    rows.add(
                        PersonalWordEntity(
                            word = o.getString("word"),
                            lang = o.optString("lang", "en"),
                            count = o.optLong("count", 1L),
                            lastSeen = o.optLong("last_seen", 0L),
                            deleted = tombstone,
                            deviceId = o.optString("device_id", "")
                        )
                    )
                } catch (e: Exception) {
                    skipped.add(JsonlSkippedLine(index + 1, e.message ?: e.javaClass.simpleName))
                }
            }
        return JsonlImportOutcome(rows, skipped)
    }
}
