package com.kb.ime

import android.content.Context

/**
 * Emoji recents for the friend-ready picker (plan/24 §5): last-used emoji
 * first, capped, persisted in the lean `:ime` prefs file (same rationale
 * as [LayoutStore] — no DataStore/Room in the IME process).
 *
 * [push] is the pure reorder primitive (unit-tested): most-recent-first,
 * deduped, capped at [MAX_RECENTS]. [EmojiRecents] is the thin prefs
 * layer over it.
 */
object EmojiRecentMath {
    const val MAX_RECENTS = 24

    fun push(recents: List<String>, picked: String, cap: Int = MAX_RECENTS): List<String> {
        if (picked.isEmpty()) return recents
        return (listOf(picked) + recents.filter { it != picked }).take(cap.coerceAtLeast(1))
    }

    fun filter(pool: List<String>, query: String): List<String> {
        val q = query.trim()
        if (q.isEmpty()) return pool.toList()
        return pool.filter { it.contains(q) }
    }
}

object EmojiRecents {
    const val PREFS_NAME = "kb_emoji_prefs"
    private const val KEY_RECENTS = "recents_csv"

    fun load(context: Context): List<String> = try {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .getString(KEY_RECENTS, null)
            ?.split(",")
            ?.filter { it.isNotEmpty() }
            ?.take(EmojiRecentMath.MAX_RECENTS)
            ?: emptyList()
    } catch (_: Exception) {
        emptyList()
    }

    fun record(context: Context, picked: String) {
        try {
            val next = EmojiRecentMath.push(load(context), picked)
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit().putString(KEY_RECENTS, next.joinToString(",")).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save emoji recents: ${e.message}", e)
        }
    }
}
