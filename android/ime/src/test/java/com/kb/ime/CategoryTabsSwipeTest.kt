package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Pure JVM tests for the CategoryTabs swipe-vs-tap decision (plan 01 zone
 * separation). No device, no Compose runtime: only [classifyCategoryTabsSwipe]
 * / [stepCategoryIndex] plus the Pad-policy half of the disambiguation rule.
 *
 * Convention under test: fling `←` (dx < 0) = NEXT, fling `→` = PREVIOUS.
 */
class CategoryTabsSwipeTest {

    private val density = 2f // 1dp = 2px

    /** 30dp in 100ms = 300dp/s: comfortably above both thresholds. */
    private fun flingLeft() =
        classifyCategoryTabsSwipe(dxPx = -60f, dyPx = 0f, dtMs = 100L, density = density)

    private fun flingRight() =
        classifyCategoryTabsSwipe(dxPx = 60f, dyPx = 0f, dtMs = 100L, density = density)

    @Test
    fun flingLeftStepsNext_flingRightStepsPrevious() {
        assertEquals(CategoryTabsSwipe.NEXT, flingLeft())
        assertEquals(CategoryTabsSwipe.PREVIOUS, flingRight())
    }

    @Test
    fun tapWins_smallFastMoveIsNull() {
        // 4dp in 50ms: fast but inside the 8dp slop → tap, handled by onClick.
        assertNull(classifyCategoryTabsSwipe(8f, 0f, 50L, density))
    }

    @Test
    fun slowDragIsNull_belowVelocity() {
        // 30dp in 1000ms = 30dp/s, far below the 200dp/s floor.
        assertNull(classifyCategoryTabsSwipe(-60f, 0f, 1000L, density))
    }

    @Test
    fun shortDragIsNull_belowDistance() {
        // 10dp in 30ms is fast but below the 24dp displacement floor.
        assertNull(classifyCategoryTabsSwipe(-20f, 0f, 30L, density))
    }

    @Test
    fun verticalDominantIsNull_neverSwitchesTabs() {
        assertNull(classifyCategoryTabsSwipe(0f, -60f, 100L, density))
        assertNull(classifyCategoryTabsSwipe(10f, -60f, 100L, density))
    }

    @Test
    fun diagonalBelowAxisRatioIsNull() {
        // 30dp x vs 25dp y: ratio 1.2 < 1.4 → ambiguous, no switch.
        assertNull(classifyCategoryTabsSwipe(-60f, -50f, 100L, density))
    }

    @Test
    fun diagonalAboveAxisRatioSwitches() {
        // 30dp x vs 10dp y: ratio 3.0 → horizontal fling.
        assertEquals(
            CategoryTabsSwipe.NEXT,
            classifyCategoryTabsSwipe(-60f, -20f, 100L, density)
        )
    }

    @Test
    fun degenerateInputsAreNull() {
        assertNull(classifyCategoryTabsSwipe(0f, 0f, 100L, density))
        assertNull(classifyCategoryTabsSwipe(-60f, 0f, 0L, density))
        assertNull(classifyCategoryTabsSwipe(-60f, 0f, -5L, density))
    }

    @Test
    fun stepMovesOneAndClampsAtEnds_noWrap() {
        assertEquals(2, stepCategoryIndex(1, CategoryTabsSwipe.NEXT, 9))
        assertEquals(0, stepCategoryIndex(1, CategoryTabsSwipe.PREVIOUS, 9))
        // Overswipe past either end is a no-op, never a wrap-around jump.
        assertEquals(8, stepCategoryIndex(8, CategoryTabsSwipe.NEXT, 9))
        assertEquals(0, stepCategoryIndex(0, CategoryTabsSwipe.PREVIOUS, 9))
        assertEquals(0, stepCategoryIndex(0, CategoryTabsSwipe.NEXT, 1))
    }

    @Test
    fun stepClampsOutOfRangeCurrent() {
        assertEquals(8, stepCategoryIndex(99, CategoryTabsSwipe.NEXT, 9))
        assertEquals(0, stepCategoryIndex(-3, CategoryTabsSwipe.PREVIOUS, 9))
    }

    @Test
    fun disambiguation_padLeftRightNeverSwitchTabs() {
        // The Pad half of the zone rule: ←/→ resolve to delete/accept policy
        // (or an explicit noop for numbers), never to a tab switch — the Pad
        // has no tab-switch path for this classifier to collide with.
        for (cat in listOf("words", "ne", "numbers", "emoji", "js", "personal")) {
            val policy = policyForCategory(cat, "t9-9", allowTwelveKeyFlings = false)
            if (cat == "numbers") {
                // No prediction: right fling is an explicit noop, still not a switch.
                assertEquals(false, policy.flingRightEnabled)
            }
            // Left is delete-or-disabled, right is accept-or-noop; neither
            // carries a tab target (no such field exists on the policy).
            assertEquals(true, policy.flingDownEnabled)
        }
        // And from the tabs side: a Pad-style vertical fling observed on the
        // strip (↑ space / ↓ hide) must not switch tabs either.
        assertNull(classifyCategoryTabsSwipe(0f, -60f, 100L, density))
        assertNull(classifyCategoryTabsSwipe(0f, 60f, 100L, density))
    }

    @Test
    fun twelveKeyLockoutDoesNotCreateTabSwitch() {
        // 12-key disables Pad ←/↑; that lockout must not reroute sideways —
        // tabs switching stays owned by the strip classifier only.
        val locked = policyForCategory("words", "t9-12", allowTwelveKeyFlings = false)
        assertEquals(false, locked.flingLeftEnabled)
        assertEquals(false, locked.flingUpEnabled)
        assertEquals(CategoryTabsSwipe.NEXT, flingLeft())
    }
}
