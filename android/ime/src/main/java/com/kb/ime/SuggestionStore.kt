package com.kb.ime

import android.content.Context

/**
 * Settings → Suggestions: strip behavior flags.
 *
 * SharedPreferences in `:ime` (same rationale as [LayoutStore]). Read by the
 * settings UI today; the IME service consults [showSuggestions]/
 * [autoSpace] on its next refresh (persisted now, honored without restart).
 */
object SuggestionStore {
    const val PREFS_NAME = "kb_suggest_prefs"
    const val KEY_SHOW = "show_suggestions"
    const val KEY_AUTO_SPACE = "auto_space"

    fun showSuggestions(context: Context): Boolean = try {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .getBoolean(KEY_SHOW, true)
    } catch (_: Exception) {
        true
    }

    fun setShowSuggestions(context: Context, value: Boolean) {
        try {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit().putBoolean(KEY_SHOW, value).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save show_suggestions: ${e.message}", e)
        }
    }

    fun autoSpace(context: Context): Boolean = try {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .getBoolean(KEY_AUTO_SPACE, true)
    } catch (_: Exception) {
        true
    }

    fun setAutoSpace(context: Context, value: Boolean) {
        try {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit().putBoolean(KEY_AUTO_SPACE, value).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save auto_space: ${e.message}", e)
        }
    }
}
