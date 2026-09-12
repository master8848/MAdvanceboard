package com.kb.ime

import android.content.Context
import android.util.Log

/**
 * Dictionary-stack ordering (plan/08): base `0` fixed bottom, personal `100`
 * fixed top, extensions float `10-90` user-ordered.
 *
 * Source of truth is per-category **priority values**, persisted in two
 * lean SharedPreferences files (same no-DataStore rationale as
 * [LayoutStore]): built-ins in [PackOrderStore] (`kb_stack_order`),
 * customs in [CustomPackStore] (`kb_custom_packs`). Stack order is the
 * deterministic sort `priority desc, id asc`; reorder = reassign priorities
 * (see [DictStackOrder.respace]) so the engine install order matches the UI
 * exactly (ties break identically on both sides).
 *
 * Engine wiring (live — see [StackEngineCaps]): the Rust
 * `DictionaryStack::set_cat_enabled` / `is_cat_enabled` and
 * `Predictor::placement()` APIs are `#[uniffi::export]`ed, so the generated
 * bindings (`uniffi/kbcore/kbcore.kt`) expose `setCatEnabled` /
 * `isCatEnabled` / `placement`. Toggles persist locally AND apply to the
 * running engine immediately (verified with an `isCatEnabled` read-back);
 * reorder persists locally and applies at the next engine init (packs
 * install in priority order — only enable/disable has a live FFI).
 */

/** One row of the user-visible stack (built-in, custom, or fixed pin). */
data class StackSlot(
    val id: String,
    val title: String,
    val priority: Int,
    val enabled: Boolean,
    /** Base (0) / personal (100): shown, never moved, toggled, or edited. */
    val fixed: Boolean,
    val custom: Boolean,
    /** Manifest `privacy.learn` (numbers/math false); customs always true. */
    val learns: Boolean,
    val layoutId: String? = null
)

/** One built-in entry of the export/import `stack` section. */
data class StackEntry(val id: String, val priority: Int, val enabled: Boolean)

/** Built-in pack registry: defaults mirror the service install table. */
object BuiltinPacks {
    const val BASE_PRIORITY = 0
    const val PERSONAL_PRIORITY = 100

    /**
     * Default extension priorities. MUST match
     * `KbInputMethodService.EXTENSION_PACK_ASSETS` (numbers 15, js 20,
     * rust 25, html 30, emoji 40, math 50, medical 60); a drift breaks the
     * on-device tie-break parity with the core per-tab report.
     */
    val DEFAULT_PRIORITIES: Map<String, Int> = mapOf(
        "numbers" to 15,
        "js" to 20,
        "rust" to 25,
        "html" to 30,
        "emoji" to 40,
        "math" to 50,
        "medical" to 60
    )

    val TITLES: Map<String, String> = mapOf(
        "numbers" to "Numbers",
        "js" to "JS",
        "rust" to "Rust",
        "html" to "HTML",
        "emoji" to "Emoji",
        "math" to "Math",
        "medical" to "Medical",
        "words" to "EN",
        "ne" to "NE",
        "personal" to "★personal"
    )

    /** Categories whose manifest sets `privacy.learn=false`. */
    val NO_LEARN_CATS: Set<String> = setOf("numbers", "math")

    fun isBuiltin(id: String): Boolean = id in DEFAULT_PRIORITIES
}

/** Pure stack-order math (no Android imports — covered by JVM unit tests). */
object DictStackOrder {
    const val PRIORITY_MIN = 10
    const val PRIORITY_MAX = 90

    /** Null when valid, else the exact user-facing error. */
    fun validatePriority(priority: Int): String? =
        if (priority in PRIORITY_MIN..PRIORITY_MAX) {
            null
        } else {
            "priority $priority out of range $PRIORITY_MIN-$PRIORITY_MAX " +
                "(base 0 and personal 100 are fixed)"
        }

    /**
     * Movable slots in stack order (top = higher priority): priority desc,
     * id asc. The tie-break is load-bearing — the engine install uses the
     * same order, so equal priorities resolve identically on both sides.
     */
    fun sortMovable(slots: List<StackSlot>): List<StackSlot> =
        slots.filter { !it.fixed }
            .sortedWith(compareByDescending<StackSlot> { it.priority }.thenBy { it.id })

    /**
     * Reassign distinct priorities across 10-90 preserving [orderedIds]
     * (first = top = highest). Spacing is even (`90, 90-step, …`,
     * `step = max(1, 80/(n-1))`, floored at 10) so every reorder lands on
     * distinct engine priorities — no tie-break ambiguity after a move.
     * Throws [IllegalArgumentException] on duplicate ids.
     */
    fun respace(orderedIds: List<String>): Map<String, Int> {
        if (orderedIds.isEmpty()) return emptyMap()
        if (orderedIds.size == 1) return mapOf(orderedIds[0] to PRIORITY_MAX)
        require(orderedIds.distinct().size == orderedIds.size) {
            "respace: duplicate ids in $orderedIds"
        }
        val step = maxOf(1, (PRIORITY_MAX - PRIORITY_MIN) / (orderedIds.size - 1))
        return orderedIds.mapIndexed { index, id ->
            id to (PRIORITY_MAX - index * step).coerceAtLeast(PRIORITY_MIN)
        }.toMap()
    }

    /**
     * Move [id] by [delta] positions (negative = up toward higher
     * priority). Clamps at the ends (no wrap, no error). Throws
     * [IllegalArgumentException] on an unknown id.
     */
    fun moveOrder(ids: List<String>, id: String, delta: Int): List<String> {
        val from = ids.indexOf(id)
        require(from >= 0) { "moveOrder: unknown category \"$id\"" }
        if (delta == 0) return ids.toList()
        val to = (from + delta).coerceIn(0, ids.size - 1)
        if (to == from) return ids.toList()
        val out = ids.toMutableList()
        out.removeAt(from)
        out.add(to, id)
        return out
    }

    /**
     * Pure validator for one import `stack` entry. Null when valid, else
     * the exact user-facing error (unknown built-in id or bad priority).
     */
    fun validateStackEntry(id: String, priority: Int): String? {
        if (!BuiltinPacks.isBuiltin(id)) {
            return "import: stack entry \"$id\": unknown built-in category " +
                "(known: ${BuiltinPacks.DEFAULT_PRIORITIES.keys.sorted()})"
        }
        return validatePriority(priority)?.let { "import: stack entry \"$id\": $it" }
    }

    /**
     * Visible tab strip from the assembled stack: `numbers` pinned first
     * (when enabled), base `words`/`ne` next (fixed, always present), then
     * enabled movable packs in stack order, `★personal` pinned last.
     * Disabled packs are absent — that IS the hide-tab mechanism.
     */
    fun stripOrder(slots: List<StackSlot>): List<String> {
        val byId = slots.associateBy { it.id }
        val out = mutableListOf<String>()
        if (byId["numbers"]?.enabled == true) out.add("numbers")
        out.add("words")
        out.add("ne")
        out.addAll(
            sortMovable(slots.filter { it.enabled && it.id != "numbers" }).map { it.id }
        )
        out.add("★personal")
        return out
    }

    /**
     * Assemble every settings row: fixed base pins, movable built-ins
     * (stored priorities + enable flags), customs, fixed personal pin.
     * Layout badges resolve via [LayoutStore] (never throws — falls back
     * to the global default). A corrupt custom pack throws
     * [IllegalStateException] naming the pack id + cause (callers surface
     * it and keep the built-ins, never a half-built list).
     */
    fun assembleSlots(context: Context): List<StackSlot> {
        val prios = PackOrderStore.priorityMap(context)
        val enabled = PackOrderStore.enabledMap(context)
        fun layoutBadge(cat: String): String = try {
            LayoutStore.layoutForCat(context, cat)
        } catch (e: Exception) {
            Log.e("DictStackOrder", "layoutForCat failed for $cat", e)
            DEFAULT_LAYOUT_ID
        }
        val out = mutableListOf<StackSlot>()
        out.add(
            StackSlot(
                "words", "EN (base)", BuiltinPacks.BASE_PRIORITY, true,
                fixed = true, custom = false, learns = true,
                layoutId = layoutBadge("words")
            )
        )
        out.add(
            StackSlot(
                "ne", "NE (base)", BuiltinPacks.BASE_PRIORITY, true,
                fixed = true, custom = false, learns = true,
                layoutId = layoutBadge("ne")
            )
        )
        for ((id, def) in BuiltinPacks.DEFAULT_PRIORITIES) {
            out.add(
                StackSlot(
                    id, BuiltinPacks.TITLES[id] ?: id, prios[id] ?: def,
                    enabled[id] ?: true, fixed = false, custom = false,
                    learns = id !in BuiltinPacks.NO_LEARN_CATS,
                    layoutId = layoutBadge(id)
                )
            )
        }
        for (pack in CustomPackStore.list(context)) {
            out.add(
                StackSlot(
                    pack.id, pack.title, pack.priority, pack.enabled,
                    fixed = false, custom = true, learns = true,
                    layoutId = layoutBadge(pack.id)
                )
            )
        }
        out.add(
            StackSlot(
                "personal", "★personal (your words)", BuiltinPacks.PERSONAL_PRIORITY,
                true, fixed = true, custom = false, learns = false,
                layoutId = layoutBadge("personal")
            )
        )
        return out
    }

    /** Built-in entries for the export `stack` section, in stack order. */
    fun builtinEntries(context: Context): List<StackEntry> {
        val prios = PackOrderStore.priorityMap(context)
        val enabled = PackOrderStore.enabledMap(context)
        return BuiltinPacks.DEFAULT_PRIORITIES.keys
            .sortedWith(
                compareByDescending<String> { prios[it] ?: BuiltinPacks.DEFAULT_PRIORITIES[it]!! }
                    .thenBy { it }
            )
            .map { id ->
                StackEntry(
                    id,
                    prios[id] ?: BuiltinPacks.DEFAULT_PRIORITIES[id]!!,
                    enabled[id] ?: true
                )
            }
    }

    /**
     * Persist a respaced priority map across both stores. Validates every
     * entry FIRST (unknown ids, out-of-range priorities) — nothing is
     * written unless the whole map validates.
     */
    fun persistPriorities(
        context: Context,
        priorities: Map<String, Int>,
        customIds: Set<String>
    ) {
        val problems = priorities.mapNotNull { (id, prio) ->
            when {
                id in customIds || BuiltinPacks.isBuiltin(id) ->
                    validatePriority(prio)?.let { "$id: $it" }
                else -> "unknown category \"$id\""
            }
        }
        if (problems.isNotEmpty()) {
            throw IllegalArgumentException(
                "stack order invalid (${problems.size} problem(s)): " + problems.joinToString("; ")
            )
        }
        for ((id, prio) in priorities) {
            if (id in customIds) CustomPackStore.setPriority(context, id, prio)
            else PackOrderStore.setPriority(context, id, prio)
        }
    }
}

/**
 * SharedPreferences persistence for built-in stack order
 * (`kb_stack_order`): per-pack priority (10-90) + enable flag. Reads
 * defend with logged fallbacks to [BuiltinPacks.DEFAULT_PRIORITIES] /
 * enabled; writes validate loudly ([IllegalArgumentException] on unknown
 * ids or out-of-range priorities).
 */
object PackOrderStore {
    const val PREFS_NAME = "kb_stack_order"
    private const val TAG = "PackOrderStore"

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    /** Stored priorities overlaid on defaults (out-of-range repairs loudly). */
    fun priorityMap(context: Context): Map<String, Int> {
        try {
            val p = prefs(context)
            return BuiltinPacks.DEFAULT_PRIORITIES.mapValues { (id, def) ->
                val v = p.getInt("builtin_${id}_prio", def)
                if (v in DictStackOrder.PRIORITY_MIN..DictStackOrder.PRIORITY_MAX) {
                    v
                } else {
                    Log.e(TAG, "priorityMap: stored $v for $id out of range, using default $def")
                    def
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "priorityMap: prefs read failed, using defaults", e)
            return BuiltinPacks.DEFAULT_PRIORITIES.toMap()
        }
    }

    /** Stored enable flags (unset = enabled). */
    fun enabledMap(context: Context): Map<String, Boolean> {
        try {
            val p = prefs(context)
            return BuiltinPacks.DEFAULT_PRIORITIES.keys.associateWith { id ->
                p.getBoolean("builtin_${id}_enabled", true)
            }
        } catch (e: Exception) {
            Log.e(TAG, "enabledMap: prefs read failed, all enabled", e)
            return BuiltinPacks.DEFAULT_PRIORITIES.keys.associateWith { true }
        }
    }

    fun setPriority(context: Context, id: String, priority: Int) {
        if (!BuiltinPacks.isBuiltin(id)) {
            throw IllegalArgumentException(
                "unknown built-in category \"$id\" (custom packs use CustomPackStore.setPriority)"
            )
        }
        DictStackOrder.validatePriority(priority)?.let { throw IllegalArgumentException(it) }
        try {
            prefs(context).edit().putInt("builtin_${id}_prio", priority).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save priority for \"$id\": ${e.message}", e)
        }
    }

    fun setEnabled(context: Context, id: String, enabled: Boolean) {
        if (!BuiltinPacks.isBuiltin(id)) {
            throw IllegalArgumentException("unknown built-in category \"$id\"")
        }
        try {
            prefs(context).edit().putBoolean("builtin_${id}_enabled", enabled).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not toggle built-in \"$id\": ${e.message}", e)
        }
    }

    /**
     * Apply validated import `stack` entries (see
     * [DictStackOrder.validateStackEntry]): priorities + enable flags.
     * Every entry is re-validated here — a caller bypass is still loud.
     */
    fun applyStack(context: Context, entries: List<StackEntry>) {
        val problems = entries.mapNotNull { e ->
            DictStackOrder.validateStackEntry(e.id, e.priority)
        }
        if (problems.isNotEmpty()) {
            throw IllegalArgumentException(
                "import: stack section invalid (${problems.size} problem(s)): " +
                    problems.joinToString("; ")
            )
        }
        try {
            val ed = prefs(context).edit()
            for (e in entries) {
                ed.putInt("builtin_${e.id}_prio", e.priority)
                ed.putBoolean("builtin_${e.id}_enabled", e.enabled)
            }
            ed.apply()
        } catch (e: IllegalArgumentException) {
            throw e
        } catch (e: Exception) {
            throw IllegalStateException("import: could not persist stack order (${e.message})", e)
        }
    }
}

/**
 * Engine capability probe for the stack-ordering surface. The Rust methods
 * (`core-rust/src/predictor.rs:set_cat_enabled` / `is_cat_enabled` /
 * `placement`) are `#[uniffi::export]`ed, so the generated bindings
 * (`uniffi/kbcore/kbcore.kt`) expose them — this probe reads the generated
 * class metadata WITHOUT initializing it (`Class.forName(...,
 * initialize=false)` — no native library load, JVM-test safe) and reports
 * exactly what the bridge can call. Callers use the live FFI and surface
 * the [liveToggleFailure]/[placementUnknown] strings user-visibly wherever
 * a live engine action fails or finds nothing; adding a same-named shim
 * anywhere in Kotlin instead is a stub and is forbidden.
 */
object StackEngineCaps {
    const val UNI_FFI_CLASS = "uniffi.kbcore.Predictor"

    /** Exported method names on the generated UniFFI `Predictor`. */
    fun methodNames(): Set<String> {
        try {
            val clazz = Class.forName(
                UNI_FFI_CLASS, false, StackEngineCaps::class.java.classLoader
            )
            return clazz.methods.map { it.name }.toSet()
        } catch (e: Exception) {
            throw IllegalStateException(
                "Engine bindings unreadable ($UNI_FFI_CLASS: ${e.message}); " +
                    "cannot verify stack-order FFI — reorder locally only.", e
            )
        }
    }

    /** True iff `Predictor.setCatEnabled` is exported via UniFFI. */
    val setCatEnabledExported: Boolean
        get() = "setCatEnabled" in methodNames()

    /** True iff `Predictor.isCatEnabled` is exported via UniFFI. */
    val isCatEnabledExported: Boolean
        get() = "isCatEnabled" in methodNames()

    /** True iff `Predictor.placement` is exported via UniFFI. */
    val placementExported: Boolean
        get() = "placement" in methodNames()

    /** Note after a verified live toggle (`set` + `is` read-back agree). */
    fun liveToggleApplied(cat: String, enabled: Boolean): String =
        "Engine live: \"$cat\" ${if (enabled) "enabled" else "disabled"} " +
            "applied + verified."

    /** Explicit note when the live toggle cannot reach the engine. */
    fun liveToggleFailure(cat: String, cause: String): String =
        "Preference saved, but the live engine toggle failed for \"$cat\" " +
            "($cause) — applies on keyboard restart when packs reinstall " +
            "in the new order."

    /** Explicit note when live `placement()` holds no record for [word]. */
    fun placementUnknown(word: String): String =
        "Engine live: no pack holds \"$word\"."
}
