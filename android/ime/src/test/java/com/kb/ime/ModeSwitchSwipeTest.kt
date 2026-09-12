package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pure JVM tests for the strip swipe-down mode-switch decision (UX: pad
 * 9-key ↔ QWERTY). No device, no Compose runtime: only [classifyModeSwitch].
 *
 * Convention under test: fast downward fling (dy > 0, past 32dp) = true;
 * EVERYTHING else (tap, short/slow, horizontal, up) = false, so no existing
 * gesture can be stolen.
 */
class ModeSwitchSwipeTest {

    private val density = 2f // 1dp = 2px

    /** 40dp down in 100ms = 400dp/s: comfortably above every gate. */
    private fun swipeDown() =
        classifyModeSwitch(dxPx = 0f, dyPx = 80f, dtMs = 100L, density = density)

    @Test
    fun clearSwipeDownIsTrue() {
        assertTrue(swipeDown())
    }

    @Test
    fun swipeUpIsFalse_expandAllPreserved() {
        // 40dp UP in 100ms: fast and far, but up is expand-all, never mode.
        assertFalse(classifyModeSwitch(0f, -80f, 100L, density))
    }

    @Test
    fun flingRightIsFalse_acceptFirstPreserved() {
        assertFalse(classifyModeSwitch(80f, 0f, 100L, density))
    }

    @Test
    fun flingLeftIsFalse_cycleOverflowPreserved() {
        assertFalse(classifyModeSwitch(-80f, 0f, 100L, density))
    }

    @Test
    fun tapWins_smallFastMoveIsFalse() {
        // 4dp in 50ms: fast but inside the 8dp slop → tap, handled by onClick.
        assertFalse(classifyModeSwitch(0f, 8f, 50L, density))
    }

    @Test
    fun slowDragIsFalse_belowVelocity() {
        // 40dp in 1000ms = 40dp/s, far below the 200dp/s floor.
        assertFalse(classifyModeSwitch(0f, 80f, 1000L, density))
    }

    @Test
    fun shortDragIsFalse_stricterThanStandardFling() {
        // 28dp down in 70ms = 400dp/s: passes the standard 24dp fling
        // floor but NOT the 32dp mode-switch floor — no accidents.
        assertTrue(28f > GestureThresholds().flingThresholdDp)
        assertFalse(classifyModeSwitch(0f, 56f, 70L, density))
    }

    @Test
    fun diagonalBelowAxisRatioIsFalse() {
        // 40dp down vs 30dp sideways: ratio 1.33 < 1.4 → ambiguous.
        assertFalse(classifyModeSwitch(60f, 80f, 100L, density))
    }

    @Test
    fun diagonalAboveAxisRatioIsTrue() {
        // 40dp down vs 10dp sideways: ratio 4.0 → vertical fling.
        assertTrue(classifyModeSwitch(20f, 80f, 100L, density))
    }

    @Test
    fun degenerateInputsAreFalse() {
        assertFalse(classifyModeSwitch(0f, 0f, 100L, density))
        assertFalse(classifyModeSwitch(0f, 80f, 0L, density))
        assertFalse(classifyModeSwitch(0f, 80f, -5L, density))
    }

    @Test
    fun zeroDensityThrows() {
        try {
            classifyModeSwitch(0f, 80f, 100L, 0f)
            throw AssertionError("density 0 must fail")
        } catch (e: IllegalArgumentException) {
            assertTrue(e.message!!.contains("density"))
        }
    }

    @Test
    fun disambiguation_floorStaysAboveStandardFling() {
        // The mode floor must stay strictly above the standard fling floor,
        // or ordinary strip/pad flings could toggle mode: pin the gap.
        assertTrue(MODE_SWITCH_DISTANCE_DP > GestureThresholds().flingThresholdDp)
        assertEquals(32f, MODE_SWITCH_DISTANCE_DP)
    }
}
