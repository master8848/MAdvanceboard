package com.kb.ime

import android.content.Context
import android.content.res.Configuration

/**
 * Settings → Appearance → Theme (System / Light / Dark).
 *
 * SharedPreferences in `:ime` (same rationale as [LayoutStore]): readable
 * from both the `:app` settings UI and the IME service without new deps.
 * The `:ime` module holds no appcompat dependency on purpose, so night-mode
 * ints are exposed raw ([toNightMode] values match
 * `AppCompatDelegate.MODE_NIGHT_*`: FOLLOW_SYSTEM=-1, NO=1, YES=2) and the
 * `:app` process maps them onto `AppCompatDelegate.setDefaultNightMode`.
 */
object ThemeStore {
    const val PREFS_NAME = "kb_theme_prefs"
    const val KEY_MODE = "theme_mode"

    const val SYSTEM = "system"
    const val LIGHT = "light"
    const val DARK = "dark"

    val MODES = listOf(SYSTEM, LIGHT, DARK)

    /** Stored theme mode ([SYSTEM] when unset/unknown). */
    fun mode(context: Context): String = try {
        val stored = context
            .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .getString(KEY_MODE, SYSTEM)
        if (stored in MODES) stored!! else SYSTEM
    } catch (_: Exception) {
        SYSTEM
    }

    /** Persists [mode]; throws [IllegalArgumentException] on unknown values. */
    fun setMode(context: Context, mode: String): String {
        require(mode in MODES) { "unknown theme mode: $mode (expected $MODES)" }
        try {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit().putString(KEY_MODE, mode).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save theme mode: ${e.message}", e)
        }
        return mode
    }

    /** Framework night flag (system theme), independent of the stored mode. */
    fun isSystemDark(context: Context): Boolean = try {
        (context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) ==
            Configuration.UI_MODE_NIGHT_YES
    } catch (_: Exception) {
        false
    }

    /** Effective darkness: stored mode wins, [SYSTEM] defers to the framework. */
    fun isDarkEffective(context: Context): Boolean = when (mode(context)) {
        DARK -> true
        LIGHT -> false
        else -> isSystemDark(context)
    }

    /**
     * Maps a mode to the `AppCompatDelegate.MODE_NIGHT_*` int without
     * referencing appcompat (kept as raw ints so `:ime` stays lean).
     */
    fun toNightMode(mode: String): Int = when (mode) {
        DARK -> 2 // MODE_NIGHT_YES
        LIGHT -> 1 // MODE_NIGHT_NO
        else -> -1 // MODE_NIGHT_FOLLOW_SYSTEM
    }
}
