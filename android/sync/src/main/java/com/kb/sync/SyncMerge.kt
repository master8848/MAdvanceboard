package com.kb.sync

import org.json.JSONObject

/**
 * Last-writer-wins per-word merge over `{count, last_seen, del, device_id}`.
 * Newer `lastSeen` wins; ties break toward lexicographically larger `deviceId`.
 * Tombstones replicate like any other row and are never resurrected by merge.
 * Wire name is `del` (canonical, SPEC §5); `deleted` (SPEC §4) is still
 * accepted on read for compat.
 */
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

    /** Import JSONL previously produced by [toJsonl]; skips malformed lines. */
    fun fromJsonl(jsonl: String): List<PersonalWordEntity> =
        jsonl.lineSequence()
            .map { it.trim() }
            .filter { it.isNotEmpty() }
            .mapNotNull { line ->
                try {
                    val o = JSONObject(line)
                    // Compat: accept legacy `deleted` (SPEC §4) as well as `del` (SPEC §5).
                    val tombstone = if (o.has("del")) o.optBoolean("del", false)
                        else o.optBoolean("deleted", false)
                    PersonalWordEntity(
                        word = o.getString("word"),
                        lang = o.optString("lang", "en"),
                        count = o.optLong("count", 1L),
                        lastSeen = o.optLong("last_seen", 0L),
                        deleted = tombstone,
                        deviceId = o.optString("device_id", "")
                    )
                } catch (_: Exception) {
                    null
                }
            }.toList()
}
