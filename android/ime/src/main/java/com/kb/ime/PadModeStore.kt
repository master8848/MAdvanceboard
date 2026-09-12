package com.kb.ime

import android.content.Context
import android.content.SharedPreferences

/**
 * Persists the user's pad-size toggle (`t9-9` / `t9-12` / `t9-16`).
 *
 * SharedPreferences (not DataStore) on purpose: the `:ime` process stays
 * lean (no Room/Work/DataStore — see the module's no-`:sync` rule), and a
 * framework prefs file is readable from both the IME service and the host
 * settings UI without new Gradle dependencies.
 */
object PadModeStore {
    const val PREFS_NAME = "kb_pad_prefs"
    const val KEY_LAYOUT_ID = "pad_layout_id"

    private fun prefs(context: Context): SharedPreferences =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    /** Stored layout id, or [DEFAULT_LAYOUT_ID] when unset/unknown. */
    fun load(context: Context): String {
        val stored = try {
            prefs(context).getString(KEY_LAYOUT_ID, DEFAULT_LAYOUT_ID)
        } catch (_: Exception) {
            DEFAULT_LAYOUT_ID
        }
        return if (stored in SUPPORTED_LAYOUT_IDS) stored!! else DEFAULT_LAYOUT_ID
    }

    /** Persists [layoutId]; unknown ids are ignored. Returns stored value. */
    fun save(context: Context, layoutId: String): String {
        if (layoutId !in SUPPORTED_LAYOUT_IDS) return load(context)
        try {
            prefs(context).edit().putString(KEY_LAYOUT_ID, layoutId).apply()
        } catch (_: Exception) {
        }
        return layoutId
    }
}
