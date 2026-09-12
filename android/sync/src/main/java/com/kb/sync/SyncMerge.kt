package com.kb.sync

import org.json.JSONObject

/**
 * Local-only JSONL export / import for the personal dictionary.
 *
 * Single-device scope: there is NO multi-device merge. An earlier
 * whole-row LWW `mergeWord`/`mergeAll` silently dropped accept/reject
 * counters, so it was removed — see git history. Importing a file
 * replaces (upserts) local rows as-is; nothing reconciles counters
 * across devices. Export/import works locally; merge does not exist.
 *
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
    /**
     * No merge: multi-device reconciliation is not supported (single-device
     * scope). Imported rows are applied as-is by the caller; calling this
     * is a programming error, so it throws loudly instead of silently
     * picking a row and dropping the other side's counters.
     */
    fun mergeWord(local: PersonalWordEntity, remote: PersonalWordEntity): PersonalWordEntity =
        throw UnsupportedOperationException(
            "SyncMerge.mergeWord: no multi-device merge (single-device scope); " +
                "import rows as-is instead of reconciling"
        )

    /** See [mergeWord]: multi-device reconciliation is not supported. */
    fun mergeAll(
        local: List<PersonalWordEntity>,
        remote: List<PersonalWordEntity>
    ): List<PersonalWordEntity> =
        throw UnsupportedOperationException(
            "SyncMerge.mergeAll: no multi-device merge (single-device scope); " +
                "import rows as-is instead of reconciling"
        )

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
