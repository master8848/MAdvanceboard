package com.kb.ime

import android.content.Context
import android.util.AttributeSet
import android.view.Gravity
import android.widget.Button
import android.widget.LinearLayout

/**
 * Full-QWERTY fallback. Prediction engine is shared with the 9-key pad — only
 * the encoder differs (raw letters here vs T9 digits there); keys route through
 * the host service's [com.kb.ime.KbInputMethodService.onQwertyKey], which calls
 * the same Predictor.suggest/learn interface.
 *
 * Letter keys share weight (height target >= 48dp); action row targets are
 * full >= 48dp squares.
 *
 * Zone rule (plan 01): QWERTY is button-first (FAB toggles Pad↔QWERTY, avoiding
 * Samsung spacebar-language-switch false triggers). The only gesture honored
 * here is fling `→` = accept top suggestion; all other flings are disabled
 * ([PadGestureDetector.FlingGate.ACCEPT_ONLY]) and reported via
 * [onGestureRejected] for tuning.
 */
class QwertyView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null
) : LinearLayout(context, attrs) {

    var onKey: (String) -> Unit = {}
    var onDelete: () -> Unit = {}
    var onEnter: () -> Unit = {}
    var onSwitchIme: () -> Unit = {}
    /**
     * Long-press on the 🌐 key: system IME picker (multi-IME / multi-globe
     * menu). Tap still flips via [onSwitchIme]; the key renders full alpha.
     */
    var onSwitchImePicker: () -> Unit = {}
    /** Fling → on the QWERTY view: accept top suggestion (only gesture). */
    var onFlingAccept: () -> Unit = {}
    var onGestureRejected: (reason: String) -> Unit = {}

    private val gestures = PadGestureDetector(
        this,
        listener = object : PadGestureDetector.Listener() {
            override fun onFlingRight() = onFlingAccept()
            override fun onGestureRejected(reason: String) =
                this@QwertyView.onGestureRejected(reason)
        }
    ).apply { flingsEnabled = PadGestureDetector.FlingGate.ACCEPT_ONLY }

    var gesturesEnabled: Boolean
        get() = gestures.enabled
        set(value) { gestures.enabled = value }

    fun updateThresholds(t: GestureThresholds) = gestures.updateThresholds(t)

    private val keyButtons = mutableListOf<Button>()
    private val functionalButtons = mutableListOf<Button>()
    private var spaceButton: Button? = null
    private var enterButton: Button? = null
    /** Centered spacebar language label (`English` / `नेपाली` per tab). */
    var spaceLabel: String = "English"
        private set

    /**
     * Sets the centered spacebar language label. The tap action stays
     * space — only the visible label follows the active tab.
     */
    fun setSpaceLabel(label: String) {
        spaceLabel = label
        spaceButton?.text = label
    }

    /** Re-applies night-mode key tints (theme change without reinstall). */
    fun refreshKeyTheme() {
        for (btn in keyButtons) styleKeyButton(btn, context)
        for (btn in functionalButtons) styleFunctionalKey(btn, context)
        enterButton?.let { styleEnterKey(it, context) }
    }

    private val rows = listOf(
        "qwertyuiop",
        "asdfghjkl",
        "zxcvbnm"
    )

    /** Number-hint superscripts for the QWERTY top row (q→1 … p→0). */
    private val topRowHints = mapOf(
        'q' to '1', 'w' to '2', 'e' to '3', 'r' to '4', 't' to '5',
        'y' to '6', 'u' to '7', 'i' to '8', 'o' to '9', 'p' to '0'
    )

    /**
     * Letter key label with a small superscript number hint (top row only).
     * The tap code stays the bare letter — the hint is visual only.
     */
    internal fun hintLabel(letter: Char): CharSequence {
        val hint = topRowHints[letter] ?: return letter.toString()
        val raw = "$letter $hint"
        return android.text.SpannableString(raw).apply {
            setSpan(
                android.text.style.RelativeSizeSpan(0.55f),
                2, 3,
                android.text.Spanned.SPAN_EXCLUSIVE_EXCLUSIVE
            )
            setSpan(
                android.text.style.SuperscriptSpan(),
                2, 3,
                android.text.Spanned.SPAN_EXCLUSIVE_EXCLUSIVE
            )
        }
    }

    /** Tap cycles these when Sym is pressed (symbols-sheet TBD). */
    private val symCycle = listOf("@", "#", "$", "%", "&", "*", "-", "+", "(", ")")
    private var symIndex = 0

    init {
        orientation = VERTICAL
        // Compact Gboard-style metrics: shared with the T9 pad (see
        // PadSpec.kt) — ~50dp rows, 3dp gaps, 4dp outer padding + 4dp bottom
        // margin (the host adds the system navigation-bar inset on top).
        val density = resources.displayMetrics.density
        val minHeight = (MIN_KEY_TARGET_DP * density).toInt()
        val keyHeight = (KEY_HEIGHT_DP * density).toInt()
        val hGap = (KEY_H_GAP_DP * density).toInt()
        val vGap = (KEY_V_GAP_DP * density).toInt()
        val pad = (PAD_PADDING_DP * density).toInt()
        val bottomExtra = (PAD_BOTTOM_MARGIN_DP * density).toInt()
        setPadding(pad, pad, pad, pad + bottomExtra)
        clipToPadding = false
        // Functional keys (bottom action row) use the same distinct
        // container tint as PadView so Sym/Space/⌫/⏎/🌐 never read as
        // letter keys. All keys render full alpha.
        for (rowChars in rows) {
            val isTopRow = rowChars == rows.first()
            val row = LinearLayout(context).apply { orientation = HORIZONTAL }
            for (c in rowChars) {
                val key = c.toString()
                val btn = Button(context).apply {
                    text = if (isTopRow) hintLabel(c) else key
                    gravity = Gravity.CENTER
                    textSize = 15f
                    alpha = 1f
                    minimumHeight = minHeight
                    setOnClickListener { onKey(key) }
                }
                styleKeyButton(btn, context)
                keyButtons.add(btn)
                row.addView(btn, LayoutParams(0, keyHeight, 1f).apply {
                    setMargins(hGap, vGap, hGap, vGap)
                })
            }
            addView(row)
        }
        // Bottom action row: Sym | Space | Delete | Enter | Next-IME.
        val actions = LinearLayout(context).apply { orientation = HORIZONTAL }
        val minTarget = (MIN_KEY_TARGET_DP * density).toInt()
        fun actionButton(label: String): android.widget.Button =
            Button(context).apply {
                text = label
                textSize = 14f
                alpha = 1f
                gravity = Gravity.CENTER
                typeface = android.graphics.Typeface.DEFAULT_BOLD
                minimumWidth = minTarget
                minimumHeight = minTarget
                functionalButtons.add(this)
            }
        actions.addView(actionButton("Sym").apply {
            styleFunctionalKey(this, context)
            setOnClickListener {
                onKey(symCycle[symIndex])
                symIndex = (symIndex + 1) % symCycle.size
            }
        }, LayoutParams(0, keyHeight, 1f).apply {
            setMargins(hGap, vGap, hGap, vGap)
        })
        val space = actionButton(spaceLabel).apply {
            styleFunctionalKey(this, context)
            setOnClickListener { onKey(" ") }
        }
        spaceButton = space
        actions.addView(space, LayoutParams(0, keyHeight, 2f).apply {
            setMargins(hGap, vGap, hGap, vGap)
        })
        actions.addView(actionButton("⌫").apply {
            styleFunctionalKey(this, context)
            setOnClickListener { onDelete() }
        }, LayoutParams(LayoutParams.WRAP_CONTENT, keyHeight).apply {
            setMargins(hGap, vGap, hGap, vGap)
        })
        // Accent enter: the ONLY circular key (pink slab, dark glyph).
        // Square target so the oval drawable renders as a circle.
        val enter = actionButton("⏎").apply {
            styleEnterKey(this, context)
            setOnClickListener { onEnter() }
        }
        enterButton = enter
        functionalButtons.remove(enter)
        actions.addView(enter, LayoutParams(keyHeight, keyHeight).apply {
            setMargins(hGap, vGap, hGap, vGap)
        })
        actions.addView(actionButton("🌐").apply {
            styleFunctionalKey(this, context)
            alpha = 1f
            setOnClickListener { onSwitchIme() }
            setOnLongClickListener { onSwitchImePicker(); true }
        }, LayoutParams(LayoutParams.WRAP_CONTENT, keyHeight).apply {
            setMargins(hGap, vGap, hGap, vGap)
        })
        addView(actions)
        refreshKeyTheme()
    }

    override fun onInterceptTouchEvent(ev: android.view.MotionEvent): Boolean =
        gestures.onTouchEvent(ev) || super.onInterceptTouchEvent(ev)

    override fun onTouchEvent(ev: android.view.MotionEvent): Boolean {
        gestures.onTouchEvent(ev)
        return true
    }

    override fun onDetachedFromWindow() {
        gestures.recycle()
        super.onDetachedFromWindow()
    }
}
