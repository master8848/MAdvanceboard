package com.kb.ime

import android.content.Context

/**
 * Persists the user's pad-size toggle (`t9-9` / `t9-12` / `t9-16`).
 *
 * Compat shim over [LayoutStore.global]: new code should use
 * [LayoutStore] (global default + per-category overrides,
 * `plan/02-layout-global-percat.md`). Kept so existing call sites
 * (service startup, toggle commit) keep working unchanged.
 *
 * SharedPreferences (not DataStore) on purpose: the `:ime` process stays
 * lean (no Room/Work/DataStore — see the module's no-`:sync` rule), and a
 * framework prefs file is readable from both the IME service and the host
 * settings UI without new Gradle dependencies.
 */
object PadModeStore {
    const val PREFS_NAME = "kb_pad_prefs"
    const val KEY_LAYOUT_ID = "pad_layout_id"

    /** Stored global layout id, or [DEFAULT_LAYOUT_ID] when unset/unknown. */
    fun load(context: Context): String = LayoutStore.global(context)

    /**
     * Persists [layoutId] as the global default. Unknown ids throw
     * [IllegalArgumentException] (explicit, via [LayoutStore]); the
     * previous value is returned unchanged only when the write itself
     * fails at the framework level.
     */
    fun save(context: Context, layoutId: String): String {
        LayoutStore.setGlobal(context, layoutId)
        return layoutId
    }
}
