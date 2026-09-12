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
 *
 * Zone separation (plan 01): this pad owns 4-direction flings ONLY —
 * `←` delete (with continued-slide word-select), `↑` space+commit-top,
 * `→` accept top suggestion, `↓` hide. Category switching lives in
 * [CategoryTabs] (horizontal scroll), candidate accept/cycle in the
 * SuggestionBar. Rejected and staying rejected: Pad `←→` for category
 * switch, `↓` for symbols.
 *
 * Taps are never intercepted (child Button clicks proceed); flings and
 * long-press (~400ms, symbols sheet per SPEC §1 alt table) steal the stream
 * via [onInterceptTouchEvent] so a gesture never double-fires a key tap.
 * Configure via [gesturesEnabled]/[flingGate]/[updateThresholds]; observe via
 * the `onFling*` / `onKeyLongPress` / `onDeleteSlide` callbacks.
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

    // -- Plan 01 gesture outputs (wired by the host service). --
    /** Short fling ←, or continued-slide release: delete N words (N>=1). */
    var onFlingDelete: (words: Int) -> Unit = {}
    /** Live delete-slide preview word count (0 = slid back, preview cleared). */
    var onDeleteSlide: (words: Int) -> Unit = {}
    var onFlingSpace: () -> Unit = {}
    var onFlingAccept: () -> Unit = {}
    var onFlingHide: () -> Unit = {}
    /**
     * Long-press (~400ms, no move): symbols sheet for the key under the
     * press. Empty code/symbols = press position unknown; host falls back
     * to the generic sheet and logs the miss.
     */
    var onKeyLongPress: (code: String, symbols: String) -> Unit = { _, _ -> }
    /** Below-threshold / disabled-direction / edge touches (tuning signal). */
    var onGestureRejected: (reason: String) -> Unit = {}

    private val gestureListener = object : PadGestureDetector.Listener() {
        override fun onFlingLeftSlideStart() = Unit
        override fun onDeleteSlide(wordsToSelect: Int) = onDeleteSlide(wordsToSelect)
        override fun onDeleteRelease(wordsToSelect: Int) = onFlingDelete(wordsToSelect)
        override fun onFlingUp() = onFlingSpace()
        override fun onFlingRight() = onFlingAccept()
        override fun onFlingDown() = onFlingHide()
        override fun onKeyLongPress() {
            val key = keyAt(lastDownX, lastDownY)
            onKeyLongPress(key?.code.orEmpty(), key?.symbols.orEmpty())
        }
        override fun onGestureRejected(reason: String) = onGestureRejected(reason)
    }
    private val gestures = PadGestureDetector(this, listener = gestureListener)
    private var lastDownX = 0f
    private var lastDownY = 0f

    /** Accessibility kill-switch: AT on ⇒ detector off, fallback buttons on. */
    var gesturesEnabled: Boolean
        get() = gestures.enabled
        set(value) { gestures.enabled = value }

    var flingGate: PadGestureDetector.FlingGate
        get() = gestures.flingsEnabled
        set(value) { gestures.flingsEnabled = value }

    fun updateThresholds(t: GestureThresholds) = gestures.updateThresholds(t)

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

    /** Key whose cell contains ([x],[y]), or null (edge/gap press). */
    private fun keyAt(x: Float, y: Float): LayoutKeyUi? {
        if (width <= 0 || height <= 0) return null
        val cellW = width / spec.cols.coerceAtLeast(1)
        val cellH = height / spec.rows.coerceAtLeast(1)
        if (cellW <= 0 || cellH <= 0) return null
        val col = (x / cellW).toInt().coerceIn(0, spec.cols - 1)
        val row = (y / cellH).toInt().coerceIn(0, spec.rows - 1)
        return spec.keys.firstOrNull { it.row == row && it.col == col }
    }

    override fun onInterceptTouchEvent(ev: android.view.MotionEvent): Boolean {
        if (ev.actionMasked == android.view.MotionEvent.ACTION_DOWN) {
            lastDownX = ev.x
            lastDownY = ev.y
        }
        return gestures.onTouchEvent(ev) || super.onInterceptTouchEvent(ev)
    }

    override fun onTouchEvent(ev: android.view.MotionEvent): Boolean {
        gestures.onTouchEvent(ev)
        return true
    }

    override fun onDetachedFromWindow() {
        gestures.recycle()
        super.onDetachedFromWindow()
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
