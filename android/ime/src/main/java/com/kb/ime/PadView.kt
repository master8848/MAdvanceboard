package com.kb.ime

import android.content.Context
import android.util.AttributeSet
import android.view.Gravity
import android.widget.Button
import android.widget.GridLayout
import androidx.core.view.setPadding

/**
 * Single layout-driven pad. Grid dims, keys, labels and positions all come
 * from [spec] (loaded from `assets/layouts/<id>.json`); no hardcoded 9-key
 * map lives here. Every key target is >= 48dp.
 *
 * Role dispatch (roles defined by the layout JSON):
 * - `text`   → [onCode] with the key code (encoder input for Predictor)
 * - `space`  → [onSpace]
 * - `delete` → [onDelete]
 * - `sym`    → [onSym] with the key's symbols (empty symbols = no-op)
 * - `enter`  → [onEnter]
 * - `control`→ [onControl] with the key code (host maps mode/shift/lang…)
 * - `globe`  → [onSwitchIme]
 *
 * Per SPEC §0: t9-12's bottom row is `Sym | Space | ⌫/⏎` as first-class
 * keys (not control-skip); t9-16 splits `pq/rs`, `wx/yz` into finer keys.
 * Gestures (swipe-left = delete, swipe-up = space, long-press = symbols)
 * attach at this level so every pad size inherits them (TBD).
 */
open class PadView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    val spec: LayoutSpecUi = loadLayoutSpec(context, DEFAULT_LAYOUT_ID)
) : GridLayout(context, attrs) {

    /** Layout id plumbed to Predictor as an encoder flag (see [PadSpec.kt]). */
    val layoutId: String get() = spec.id

    var onCode: (String) -> Unit = {}
    var onSpace: () -> Unit = {}
    var onDelete: () -> Unit = {}
    var onSym: (String) -> Unit = {}
    var onEnter: () -> Unit = {}
    var onControl: (String) -> Unit = {}
    var onSwitchIme: () -> Unit = {}

    init {
        rowCount = spec.rows
        columnCount = spec.cols
        val pad = (16 * resources.displayMetrics.density).toInt()
        setPadding(pad)
        val minTarget = (48 * resources.displayMetrics.density).toInt()
        for (key in spec.keys) {
            val btn = Button(context).apply {
                text = key.label.ifEmpty { key.code }
                gravity = Gravity.CENTER
                minimumWidth = minTarget
                minimumHeight = minTarget
                setOnClickListener { dispatch(key) }
            }
            val params = LayoutParams(
                GridLayout.spec(key.row, 1f),
                GridLayout.spec(key.col, 1f)
            ).apply {
                width = 0
            }
            addView(btn, params)
        }
    }

    private fun dispatch(key: LayoutKeyUi) {
        when (key.role) {
            PadRoles.TEXT -> onCode(key.code)
            PadRoles.SPACE -> onSpace()
            PadRoles.DELETE -> onDelete()
            PadRoles.SYM -> if (key.symbols.isNotEmpty()) onSym(key.symbols)
            PadRoles.ENTER -> onEnter()
            PadRoles.GLOBE -> onSwitchIme()
            else -> onControl(key.code)
        }
    }
}

/** 9-key classic pad (thin constructor over [PadView]). */
class NineKeyPadView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null
) : PadView(context, attrs, loadLayoutSpec(context, "t9-9"))

/** 12-key pad: 9 text keys + `Sym | Space | ⌫` bottom row (thin constructor). */
class TwelveKeyPadView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null
) : PadView(context, attrs, loadLayoutSpec(context, "t9-12"))

/** 16-key pad: 4×4 with pq/rs + wx/yz splits (thin constructor). */
class SixteenKeyPadView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null
) : PadView(context, attrs, loadLayoutSpec(context, "t9-16"))

/** Builds the pad for any supported layout id ([DEFAULT_LAYOUT_ID] fallback). */
fun padViewFor(context: Context, layoutId: String): PadView = when (layoutId) {
    "t9-12" -> TwelveKeyPadView(context)
    "t9-16" -> SixteenKeyPadView(context)
    else -> NineKeyPadView(context)
}
