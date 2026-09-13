package com.kb.ime

import android.content.Context
import org.json.JSONObject

/**
 * UI mirror of the read-only layouts JSON files source of truth
 * (`t9-9` / `t9-12` / `t9-16`). Keys, labels, grid dims and the
 * fat-finger adjacency graph all come from the spec — never hardcoded.
 *
 * `layout_id` is an encoder flag only: the pad emits `code`s, the service
 * forwards `layoutId` alongside the seq so the shared Predictor path can
 * tell a 9-key seq from a 12/16-key seq. Scoring semantics are unchanged.
 */
data class LayoutKeyUi(
    val code: String,
    val label: String,
    val symbols: String,
    val role: String,
    val row: Int,
    val col: Int
)

data class LayoutSpecUi(
    val id: String,
    val title: String,
    val rows: Int,
    val cols: Int,
    val keys: List<LayoutKeyUi>,
    val adjacency: Map<String, List<String>>
) {
    fun neighbors(a: String, b: String): Boolean = adjacency[a]?.contains(b) == true
    fun keyByCode(code: String): LayoutKeyUi? = keys.firstOrNull { it.code == code }
}

const val DEFAULT_LAYOUT_ID = "t9-9"
val SUPPORTED_LAYOUT_IDS = listOf("t9-9", "t9-12", "t9-16")

/** Minimum touch target for every pad key (48dp accessibility floor). */
const val MIN_KEY_TARGET_DP = 48

/** Compact fixed key height (Gboard-style, 48–52dp band). */
const val KEY_HEIGHT_DP = 50

/** Gaps between keys (dp) — tight Gboard-style grid. */
const val KEY_H_GAP_DP = 3
const val KEY_V_GAP_DP = 3

/** Inner padding of the pad grid (dp) — compact, was 16. */
const val PAD_PADDING_DP = 4

/** Bottom margin above the navigation bar (dp, added to system inset). */
const val PAD_BOTTOM_MARGIN_DP = 4

/** Key roles understood by [PadView]; unknown roles fall back to `text`. */
object PadRoles {
    const val TEXT = "text"
    const val SPACE = "space"
    const val DELETE = "delete"
    const val SYM = "sym"
    const val CONTROL = "control"
    const val ENTER = "enter"
    const val GLOBE = "globe"
}

/**
 * Loads a layout spec from `assets/layouts/<layoutId>.json` (verbatim copy
 * of the canonical `layouts/` source of truth). Falls back to the
 * programmatic built-in for [DEFAULT_LAYOUT_ID] only when assets are
 * unreadable, so the pad never fails to inflate.
 */
fun loadLayoutSpec(context: Context, layoutId: String): LayoutSpecUi {
    val id = if (layoutId in SUPPORTED_LAYOUT_IDS) layoutId else DEFAULT_LAYOUT_ID
    return try {
        context.assets.open("layouts/$id.json").bufferedReader().use { parseLayoutSpec(it.readText()) }
    } catch (_: Exception) {
        builtinT9Spec()
    }
}

fun parseLayoutSpec(json: String): LayoutSpecUi {
    val root = JSONObject(json)
    val grid = root.getJSONObject("grid")
    val keys = mutableListOf<LayoutKeyUi>()
    val arr = root.getJSONArray("keys")
    for (i in 0 until arr.length()) {
        val k = arr.getJSONObject(i)
        keys.add(
            LayoutKeyUi(
                code = k.getString("code"),
                label = k.getString("label"),
                symbols = k.optString("symbols", ""),
                role = k.optString("role", PadRoles.TEXT),
                row = k.getInt("row"),
                col = k.getInt("col")
            )
        )
    }
    val adjacency = mutableMapOf<String, List<String>>()
    val adj = root.optJSONObject("adjacency")
    if (adj != null) {
        val names = adj.keys()
        while (names.hasNext()) {
            val key = names.next()
            val list = adj.getJSONArray(key)
            adjacency[key] = List(list.length()) { list.getString(it) }
        }
    }
    return LayoutSpecUi(
        id = root.getString("id"),
        title = root.optString("title", root.getString("id")),
        rows = grid.getInt("rows"),
        cols = grid.getInt("cols"),
        keys = keys,
        adjacency = adjacency
    )
}

/**
 * Minimal built-in fallback (t9-9 shape) used only when the asset copy is
 * unreadable. Real keys/labels/adjacency always come from JSON above.
 */
private fun builtinT9Spec(): LayoutSpecUi {
    val labels = listOf(".,?!'", "abc", "def", "ghi", "jkl", "mno", "pqrs", "tuv", "wxyz")
    val keys = labels.mapIndexed { i, label ->
        LayoutKeyUi(
            code = (i + 1).toString(),
            label = label,
            symbols = label,
            role = PadRoles.TEXT,
            row = i / 3,
            col = i % 3
        )
    } + listOf(
        LayoutKeyUi("*", "sym", "", PadRoles.SYM, 3, 0),
        LayoutKeyUi("0", "space", " ", PadRoles.SPACE, 3, 1),
        LayoutKeyUi("#", "⌫", "", PadRoles.DELETE, 3, 2)
    )
    return LayoutSpecUi(
        id = DEFAULT_LAYOUT_ID,
        title = "Classic 9-key T9 (built-in fallback)",
        rows = 4,
        cols = 3,
        keys = keys,
        adjacency = emptyMap()
    )
}
