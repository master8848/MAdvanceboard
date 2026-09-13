package com.kb.ime

import android.content.Context
import android.content.SharedPreferences
import android.util.Log

/**
 * Global layout default + per-category overrides (`plan/02-layout-global-percat.md`).
 *
 * Resolution per keystroke (mirrors core `LayoutRegistry::resolve`):
 * per-cat override -> global default -> [DEFAULT_LAYOUT_ID] (`t9-9`).
 * The IME service re-resolves on every tab switch / field start, so a
 * Settings change applies without restarting the keyboard; changing the
 * layout swaps the spec-driven [PadView] (display labels re-encode from
 * the new spec) and the engine matches under the new pad's codes.
 *
 * SharedPreferences (not DataStore) on purpose: the `:ime` process stays
 * lean (no Room/Work/DataStore — see the module's no-`:sync` rule), and a
 * framework prefs file is readable from both the IME service and the host
 * settings UI without new Gradle dependencies. Writes are validated
 * loudly: unknown ids throw [IllegalArgumentException] (Settings shows a
 * Toast; the service never writes unvalidated ids). Reads defend with a
 * logged fallback to the global default, never a silent wrong pad.
 */
object LayoutStore {
    const val PREFS_NAME = "kb_layout_prefs"
    const val KEY_GLOBAL = "global_layout_id"
    private const val KEY_CAT_PREFIX = "cat_layout_"
    private const val TAG = "LayoutStore"

    /** Category ids shown in Settings (asset ids; see [DEFAULT_CATEGORIES]). */
    val KNOWN_CATS: List<Pair<String, String>> = listOf(
        "numbers" to "numbers",
        "words" to CategoryLabels.WORDS_LABEL,
        "ne" to "NE",
        "js" to "js",
        "rust" to "rust",
        "html" to "html",
        "emoji" to "emoji",
        "math" to "math",
        "medical" to "⚕",
        "personal" to "★personal"
    )

    private fun prefs(context: Context): SharedPreferences =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    private fun checkLayoutId(layoutId: String) {
        if (layoutId !in SUPPORTED_LAYOUT_IDS) {
            throw IllegalArgumentException(
                "unknown layout_id: $layoutId (supported: $SUPPORTED_LAYOUT_IDS)"
            )
        }
    }

    /** Global default layout id ([DEFAULT_LAYOUT_ID] when unset). */
    fun global(context: Context): String {
        migrateLegacy(context)
        val stored = try {
            prefs(context).getString(KEY_GLOBAL, DEFAULT_LAYOUT_ID)
        } catch (e: Exception) {
            Log.e(TAG, "global: prefs read failed, using default", e)
            DEFAULT_LAYOUT_ID
        }
        return if (stored in SUPPORTED_LAYOUT_IDS) {
            stored!!
        } else {
            Log.e(TAG, "global: stored unknown id $stored, using default")
            DEFAULT_LAYOUT_ID
        }
    }

    /**
     * One-time upgrade from the pre-per-cat `PadModeStore` file
     * (`kb_pad_prefs` / `pad_layout_id`): adopts the stored pad as the
     * global default when the new key is unset, then clears the legacy
     * key so the migration runs once.
     */
    private fun migrateLegacy(context: Context) {
        try {
            val fresh = prefs(context)
            if (fresh.contains(KEY_GLOBAL)) return
            val legacy = context.getSharedPreferences("kb_pad_prefs", Context.MODE_PRIVATE)
            val id = legacy.getString("pad_layout_id", null)
            if (id in SUPPORTED_LAYOUT_IDS) {
                fresh.edit().putString(KEY_GLOBAL, id).apply()
                legacy.edit().remove("pad_layout_id").apply()
                Log.i(TAG, "migrateLegacy: adopted $id as global default")
            }
        } catch (e: Exception) {
            Log.e(TAG, "migrateLegacy failed", e)
        }
    }

    /** Sets the global default; throws [IllegalArgumentException] on unknown ids. */
    fun setGlobal(context: Context, layoutId: String): String {
        checkLayoutId(layoutId)
        try {
            prefs(context).edit().putString(KEY_GLOBAL, layoutId).apply()
        } catch (e: Exception) {
            throw IllegalStateException("setGlobal: prefs write failed for $layoutId", e)
        }
        return layoutId
    }

    /**
     * Layout id for [cat] (override, else global). Unknown stored values
     * (only possible via external tampering — writes are validated) fall
     * back to the global default with an explicit error log.
     */
    fun layoutForCat(context: Context, cat: String): String {
        val key = KEY_CAT_PREFIX + cat
        val stored = try {
            prefs(context).getString(key, null)
        } catch (e: Exception) {
            Log.e(TAG, "layoutForCat: prefs read failed for $cat", e)
            null
        }
        if (stored == null) return global(context)
        return if (stored in SUPPORTED_LAYOUT_IDS) {
            stored
        } else {
            Log.e(TAG, "layoutForCat: stored unknown id $stored for $cat, using global")
            global(context)
        }
    }

    /** Sets a per-category override; throws [IllegalArgumentException] on unknown ids. */
    fun setCatLayout(context: Context, cat: String, layoutId: String): String {
        require(cat.isNotEmpty()) { "cat must not be empty" }
        checkLayoutId(layoutId)
        try {
            prefs(context).edit().putString(KEY_CAT_PREFIX + cat, layoutId).apply()
        } catch (e: Exception) {
            throw IllegalStateException("setCatLayout: prefs write failed for $cat", e)
        }
        return layoutId
    }

    /** Drops a per-category override (falls back to the global default). */
    fun clearCatLayout(context: Context, cat: String) {
        try {
            prefs(context).edit().remove(KEY_CAT_PREFIX + cat).apply()
        } catch (e: Exception) {
            Log.e(TAG, "clearCatLayout: prefs write failed for $cat", e)
        }
    }

    /** All stored per-cat overrides (validated ids only). */
    fun overrides(context: Context): Map<String, String> {
        val out = mutableMapOf<String, String>()
        try {
            for ((cat, _) in KNOWN_CATS) {
                prefs(context).getString(KEY_CAT_PREFIX + cat, null)?.let { id ->
                    if (id in SUPPORTED_LAYOUT_IDS) out[cat] = id
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "overrides: prefs read failed", e)
        }
        return out
    }
}
