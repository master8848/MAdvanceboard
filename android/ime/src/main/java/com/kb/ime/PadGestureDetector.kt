package com.kb.ime

import android.os.Handler
import android.os.Looper
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View

/**
 * Pad touch state machine (plan 01 §Detection). Feed every MotionEvent from
 * the host view's `onInterceptTouchEvent` + `onTouchEvent`; returns true
 * when the host should intercept (steal the stream from child keys).
 *
 * Contract:
 * - tap (< 200ms, < 8dp): never intercepts — child Button click proceeds.
 * - slop exceeded without fling thresholds: intercepts, no action (prevents
 *   accidental key taps on ambiguous drags; logged as `swallow`).
 * - fling thresholds crossed: haptic tick immediately, ACTION_UP classifies
 *   via [classifyFling] and fires exactly one of the fling callbacks.
 * - long-press ([GestureThresholds.longPressMs] with no move beyond slop):
 *   fires [Listener.onKeyLongPress]; the host must intercept so the child
 *   does not also click (caller should treat the pending tap as consumed).
 * - leftward continuation after the fling threshold: live
 *   [Listener.onDeleteSlide] word counts (slide-back shrinks the count);
 *   ACTION_UP fires [Listener.onDeleteRelease].
 * - touches starting in the 8dp edge dead-zone never produce gestures.
 *
 * Disabled wholesale via [enabled] (accessibility services: TalkBack /
 * SwitchAccess / VoiceAccess — flings off, 48dp fallback buttons on).
 */
class PadGestureDetector(
    private val host: View,
    private var thresholds: GestureThresholds = GestureThresholds(),
    private val listener: Listener = Listener()
) {
    interface FlingActions {
        fun onFlingLeftSlideStart() = Unit
        fun onDeleteSlide(wordsToSelect: Int) = Unit
        fun onDeleteRelease(wordsToSelect: Int) = Unit
        fun onFlingUp() = Unit
        fun onFlingRight() = Unit
        fun onFlingDown() = Unit
    }

    open class Listener : FlingActions {
        open fun onKeyLongPress() = Unit
        open fun onGestureRejected(reason: String) = Unit
    }

    var enabled: Boolean = true
    var flingsEnabled: FlingGate = FlingGate.ALL

    /** Per-direction kill-switch (12-key override, per-category deltas). */
    data class FlingGate(
        val left: Boolean = true,
        val up: Boolean = true,
        val right: Boolean = true,
        val down: Boolean = true
    ) {
        companion object {
            val ALL = FlingGate()
            val ACCEPT_ONLY = FlingGate(left = false, up = false, right = true, down = true)
            val NONE = FlingGate(left = false, up = false, right = false, down = false)
        }
    }

    fun updateThresholds(t: GestureThresholds) {
        thresholds = t
    }

    private val density: Float get() = host.resources.displayMetrics.density
    private val handler = Handler(Looper.getMainLooper())
    private var downX = 0f
    private var downY = 0f
    private var downT = 0L
    private var longPressFired = false
    private var deleteSliding = false
    private var lastSlideWords = 0
    private var intercepting = false
    private var downRejectedEdge = false
    private var lastEventTime = -1L
    private var lastAction = -1
    private var lastX = Float.NaN
    private var lastY = Float.NaN

    private val longPressTask = Runnable {
        if (!intercepting && !downRejectedEdge) {
            longPressFired = true
            intercepting = true
            host.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
            listener.onKeyLongPress()
        }
    }

    /**
     * @return true if the host should intercept this stream (take it from
     *   child keys). Taps return false so Button clicks work.
     */
    fun onTouchEvent(ev: MotionEvent): Boolean {
        if (!enabled) return false
        // The framework re-delivers the intercept-triggering event to
        // onTouchEvent; the host feeds both paths, so skip exact duplicates
        // to keep every gesture firing exactly once.
        if (ev.eventTime == lastEventTime && ev.actionMasked == lastAction &&
            ev.x == lastX && ev.y == lastY
        ) {
            return intercepting
        }
        lastEventTime = ev.eventTime
        lastAction = ev.actionMasked
        lastX = ev.x
        lastY = ev.y
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                downX = ev.x
                downY = ev.y
                downT = ev.eventTime
                longPressFired = false
                deleteSliding = false
                lastSlideWords = 0
                intercepting = false
                downRejectedEdge = inEdgeDeadZone(
                    ev.x, ev.y, host.width.toFloat(), host.height.toFloat(),
                    density, thresholds
                )
                if (downRejectedEdge) {
                    listener.onGestureRejected("edge-dead-zone")
                } else {
                    handler.postDelayed(longPressTask, thresholds.longPressMs)
                }
            }
            MotionEvent.ACTION_MOVE -> {
                if (downRejectedEdge) return false
                val dx = ev.x - downX
                val dy = ev.y - downY
                val slopPx = thresholds.touchSlopDp * density
                if (!longPressFired &&
                    (kotlin.math.abs(dx) > slopPx || kotlin.math.abs(dy) > slopPx)
                ) {
                    handler.removeCallbacks(longPressTask)
                }
                if (longPressFired) {
                    intercepting = true
                    return true
                }
                val threshPx = thresholds.flingThresholdDp * density
                if (!intercepting && (kotlin.math.abs(dx) > slopPx || kotlin.math.abs(dy) > slopPx)) {
                    intercepting = true
                }
                // Live preview only for delete-selection; also tick on cross.
                if (intercepting && dx < -threshPx && kotlin.math.abs(dx) >= kotlin.math.abs(dy)) {
                    if (!deleteSliding) {
                        deleteSliding = true
                        host.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
                        listener.onFlingLeftSlideStart()
                    }
                    val words = deleteSlideWords(dx, density, thresholds)
                    if (words != lastSlideWords) {
                        lastSlideWords = words
                        host.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
                        listener.onDeleteSlide(words)
                    }
                } else if (deleteSliding) {
                    // Slid back out of the left corridor: shrink to zero.
                    deleteSliding = false
                    lastSlideWords = 0
                    listener.onDeleteSlide(0)
                }
                return intercepting
            }
            MotionEvent.ACTION_UP -> {
                handler.removeCallbacks(longPressTask)
                if (downRejectedEdge) {
                    return false
                }
                if (longPressFired) {
                    // Long-press already consumed the stream.
                    return true
                }
                val dx = ev.x - downX
                val dy = ev.y - downY
                val dt = (ev.eventTime - downT).coerceAtLeast(1L)
                if (isTap(dx, dy, dt, density, thresholds)) {
                    return false
                }
                if (deleteSliding) {
                    val words = deleteSlideWords(dx, density, thresholds)
                    host.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
                    listener.onDeleteRelease(words)
                    return true
                }
                val dir = classifyFling(dx, dy, dt, density, thresholds)
                if (dir == null) {
                    listener.onGestureRejected("below-threshold")
                    return true // swallow ambiguous drags; never phantom-tap.
                }
                host.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
                when (dir) {
                    FlingDirection.LEFT -> if (flingsEnabled.left) {
                        listener.onDeleteRelease(1)
                    } else {
                        listener.onGestureRejected("left-disabled")
                    }
                    FlingDirection.UP -> if (flingsEnabled.up) {
                        listener.onFlingUp()
                    } else {
                        listener.onGestureRejected("up-disabled")
                    }
                    FlingDirection.RIGHT -> if (flingsEnabled.right) {
                        listener.onFlingRight()
                    } else {
                        listener.onGestureRejected("right-disabled")
                    }
                    FlingDirection.DOWN -> if (flingsEnabled.down) {
                        listener.onFlingDown()
                    } else {
                        listener.onGestureRejected("down-disabled")
                    }
                }
                return true
            }
            MotionEvent.ACTION_CANCEL -> {
                handler.removeCallbacks(longPressTask)
                if (deleteSliding) {
                    deleteSliding = false
                    listener.onDeleteSlide(0)
                }
            }
        }
        return intercepting
    }

    fun recycle() {
        handler.removeCallbacks(longPressTask)
    }
}
