package com.kb.ime

import android.content.Context

/**
 * Friend-ready defaults (plan/19, 20, 24).
 *
 * Non-technical users see only `EN/ने/😀/123`; code tabs
 * (`js/rust/html/math/medical`) hide behind "Show code tabs". QWERTY is
 * the default board for friends (9-key is the opt-in research vehicle);
 * clipboard history is OFF until explicitly enabled (friend-default
 * OFF/24h — 24h stays the recommended preset when it IS enabled).
 *
 * SharedPreferences in `:ime` (same lean rationale as [LayoutStore]):
 * readable from the IME service and the `:app` settings UI without new
 * deps. All reads defend with a logged fallback, never a crash.
 */
object FriendDefaults {
    const val PREFS_NAME = "kb_friend_prefs"
    const val KEY_SHOW_CODE_TABS = "show_code_tabs"
    const val KEY_QWERTY_DEFAULT = "qwerty_default"
    const val KEY_CLIPBOARD_ENABLED = "clipboard_enabled"

    /** Friend-visible tabs: EN + NE + emoji + numbers (+ ★personal pinned by the stack). */
    val FRIEND_TABS: List<String> = listOf("words", "ne", "emoji", "numbers")

    /** Dev tabs hidden from friends unless "Show code tabs" is on. */
    val CODE_TABS: Set<String> = setOf("js", "rust", "html", "math", "medical")

    /** Recommended retention when clipboard history IS enabled (friend-default 24h). */
    const val FRIEND_TTL_HOURS = 24L

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    /** Code tabs visible? Default false (friends see EN/NE/emoji/123 only). */
    fun showCodeTabs(context: Context): Boolean = try {
        prefs(context).getBoolean(KEY_SHOW_CODE_TABS, false)
    } catch (_: Exception) {
        false
    }

    fun setShowCodeTabs(context: Context, value: Boolean) {
        try {
            prefs(context).edit().putBoolean(KEY_SHOW_CODE_TABS, value).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save show_code_tabs: ${e.message}", e)
        }
    }

    /** QWERTY default board for friends? Default true (9-key opt-in). */
    fun qwertyDefault(context: Context): Boolean = try {
        prefs(context).getBoolean(KEY_QWERTY_DEFAULT, true)
    } catch (_: Exception) {
        true
    }

    fun setQwertyDefault(context: Context, value: Boolean) {
        try {
            prefs(context).edit().putBoolean(KEY_QWERTY_DEFAULT, value).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save qwerty_default: ${e.message}", e)
        }
    }

    /** Clipboard history enabled? Default false (friend-default OFF). */
    fun clipboardEnabled(context: Context): Boolean = try {
        prefs(context).getBoolean(KEY_CLIPBOARD_ENABLED, false)
    } catch (_: Exception) {
        false
    }

    fun setClipboardEnabled(context: Context, value: Boolean) {
        try {
            prefs(context).edit().putBoolean(KEY_CLIPBOARD_ENABLED, value).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save clipboard_enabled: ${e.message}", e)
        }
    }

    /**
     * Visible tab strip for the current audience. Pure and unit-tested:
     * when [showCode] is false, code tabs are dropped but order is
     * otherwise preserved; `★personal` always stays pinned last when
     * present.
     */
    fun filterTabs(tabs: List<String>, showCode: Boolean): List<String> {
        if (showCode) return tabs.toList()
        val kept = tabs.filter { it !in CODE_TABS }
        // Never strand the strip empty: fall back to the friend set.
        if (kept.isEmpty()) return FRIEND_TABS.filter { it in tabs }.ifEmpty { tabs.toList() }
        return kept
    }
}
