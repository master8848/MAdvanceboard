package com.kb.ime

import android.content.Context
import android.content.SharedPreferences

/**
 * Persists Settings → Gesture Tuning (plan 01 detection defaults, all tunable).
 *
 * SharedPreferences (not DataStore): the `:ime` process stays lean — same
 * rationale as [PadModeStore]. Defaults mirror [GestureThresholds].
 * Every getter falls back to the documented default on missing/corrupt
 * values; setters reject out-of-range input with [IllegalArgumentException]
 * instead of silently clamping, so bad callers fail loudly.
 */
object GestureTuningStore {
    const val PREFS_NAME = "kb_gesture_prefs"
    const val KEY_COACH_SEEN = "coach_seen"
    const val KEY_COACH_REPLAY = "coach_replay_requested"
    const val KEY_FOOTER_DISMISSED = "footer_hint_dismissed"
    const val KEY_FOOTER_FIRST_SHOWN_TS = "footer_first_shown_ts"

    private fun prefs(context: Context): SharedPreferences =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    fun loadThresholds(context: Context): GestureThresholds {
        val p = try {
            prefs(context)
        } catch (e: Exception) {
            throw IllegalStateException("Gesture prefs unreadable: ${e.message}", e)
        }
        val d = GestureThresholds()
        return try {
            GestureThresholds(
                flingThresholdDp = p.getFloat("fling_dp", d.flingThresholdDp),
                flingVelocityDpS = p.getFloat("velocity_dps", d.flingVelocityDpS),
                axisRatio = p.getFloat("axis_ratio", d.axisRatio),
                wordStepDp = p.getFloat("word_step_dp", d.wordStepDp),
                charStepDp = p.getFloat("char_step_dp", d.charStepDp),
                longPressMs = p.getLong("long_press_ms", d.longPressMs)
                    .coerceIn(300L, 600L),
                touchSlopDp = p.getFloat("slop_dp", d.touchSlopDp),
                edgeDeadDp = p.getFloat("edge_dp", d.edgeDeadDp),
                tapMaxMs = d.tapMaxMs,
                twelveKeyScale = d.twelveKeyScale
            )
        } catch (e: Exception) {
            throw IllegalStateException("Stored gesture thresholds corrupt: ${e.message}", e)
        }
    }

    fun saveThresholds(context: Context, t: GestureThresholds) {
        // Validated by GestureThresholds.init; re-check long-press range here so
        // the error names the setting, not the data class.
        require(t.longPressMs in 300L..600L) {
            "long_press_ms out of range 300-600: ${t.longPressMs}"
        }
        try {
            prefs(context).edit()
                .putFloat("fling_dp", t.flingThresholdDp)
                .putFloat("velocity_dps", t.flingVelocityDpS)
                .putFloat("axis_ratio", t.axisRatio)
                .putFloat("word_step_dp", t.wordStepDp)
                .putFloat("char_step_dp", t.charStepDp)
                .putLong("long_press_ms", t.longPressMs)
                .putFloat("slop_dp", t.touchSlopDp)
                .putFloat("edge_dp", t.edgeDeadDp)
                .apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save gesture tuning: ${e.message}", e)
        }
    }

    /**
     * 12-key opt-in: allow Pad ←/↑ flings on `t9-12` with +30% thresholds.
     * Default OFF — the bottom row is authoritative (plan 01 §12-key override).
     */
    fun allowTwelveKeyFlings(context: Context): Boolean = try {
        prefs(context).getBoolean("allow_12key_flings", false)
    } catch (_: Exception) {
        false
    }

    fun setAllowTwelveKeyFlings(context: Context, allow: Boolean) {
        try {
            prefs(context).edit().putBoolean("allow_12key_flings", allow).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save 12-key fling opt-in: ${e.message}", e)
        }
    }

    /**
     * Code/math fling-up terminator: space (default) or `;` per pack setting.
     * Stored as the literal terminator string so the UI shows what it does.
     */
    fun codeTerminator(context: Context): String = try {
        prefs(context).getString("code_terminator", " ") ?: " "
    } catch (_: Exception) {
        " "
    }

    fun setCodeTerminator(context: Context, terminator: String) {
        require(terminator == " " || terminator == ";") {
            "code terminator must be space or ';', got: $terminator"
        }
        try {
            prefs(context).edit().putString("code_terminator", terminator).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save code terminator: ${e.message}", e)
        }
    }

    // -- Discoverability state: 3-step coach + faint footer (first 3 days). --

    fun isCoachSeen(context: Context): Boolean = try {
        prefs(context).getBoolean(KEY_COACH_SEEN, false)
    } catch (_: Exception) {
        false
    }

    fun markCoachSeen(context: Context) {
        try {
            prefs(context).edit().putBoolean(KEY_COACH_SEEN, true).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not persist coach state: ${e.message}", e)
        }
    }

    fun requestCoachReplay(context: Context) {
        try {
            prefs(context).edit().putBoolean(KEY_COACH_SEEN, false).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not reset coach state: ${e.message}", e)
        }
    }

    /**
     * Footer hint `← del · ↑ space · → accept`: visible until dismissed or
     * 3 days after first shown. Returns false when it must be hidden.
     */
    fun showFooterHint(context: Context, nowMs: Long = System.currentTimeMillis()): Boolean {
        try {
            val p = prefs(context)
            if (p.getBoolean(KEY_FOOTER_DISMISSED, false)) return false
            val first = p.getLong(KEY_FOOTER_FIRST_SHOWN_TS, 0L)
            if (first == 0L) {
                p.edit().putLong(KEY_FOOTER_FIRST_SHOWN_TS, nowMs).apply()
                return true
            }
            return nowMs - first < 3L * 24 * 60 * 60 * 1000
        } catch (_: Exception) {
            return false
        }
    }

    fun dismissFooterHint(context: Context) {
        try {
            prefs(context).edit().putBoolean(KEY_FOOTER_DISMISSED, true).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not dismiss footer hint: ${e.message}", e)
        }
    }
}
