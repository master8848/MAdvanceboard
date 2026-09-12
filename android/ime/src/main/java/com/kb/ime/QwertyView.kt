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

    private val rows = listOf(
        "qwertyuiop",
        "asdfghjkl",
        "zxcvbnm"
    )

    /** Tap cycles these when Sym is pressed (symbols-sheet TBD). */
    private val symCycle = listOf("@", "#", "$", "%", "&", "*", "-", "+", "(", ")")
    private var symIndex = 0

    init {
        orientation = VERTICAL
        val density = resources.displayMetrics.density
        val minHeight = (MIN_KEY_TARGET_DP * density).toInt()
        for (rowChars in rows) {
            val row = LinearLayout(context).apply { orientation = HORIZONTAL }
            for (c in rowChars) {
                val key = c.toString()
                row.addView(Button(context).apply {
                    text = key
                    gravity = Gravity.CENTER
                    minimumHeight = minHeight
                    setOnClickListener { onKey(key) }
                }, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            }
            addView(row)
        }
        // Bottom action row: Sym | Space | Delete | Enter | Next-IME.
        val actions = LinearLayout(context).apply { orientation = HORIZONTAL }
        val minTarget = (MIN_KEY_TARGET_DP * density).toInt()
        actions.addView(Button(context).apply {
            text = "Sym"
            minimumWidth = minTarget
            minimumHeight = minTarget
            setOnClickListener {
                onKey(symCycle[symIndex])
                symIndex = (symIndex + 1) % symCycle.size
            }
        }, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        actions.addView(Button(context).apply {
            text = "Space"
            minimumHeight = minTarget
            setOnClickListener { onKey(" ") }
        }, LayoutParams(0, LayoutParams.WRAP_CONTENT, 2f))
        actions.addView(Button(context).apply {
            text = "⌫"
            minimumWidth = minTarget
            minimumHeight = minTarget
            setOnClickListener { onDelete() }
        })
        actions.addView(Button(context).apply {
            text = "⏎"
            minimumWidth = minTarget
            minimumHeight = minTarget
            setOnClickListener { onEnter() }
        })
        actions.addView(Button(context).apply {
            text = "🌐"
            minimumWidth = minTarget
            minimumHeight = minTarget
            setOnClickListener { onSwitchIme() }
        })
        addView(actions)
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
