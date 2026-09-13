package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Gesture-path sweep (bug-hunt): every fling must reach the same commit
 * path as its key/button equivalent (no dead-ends), and the TalkBack/D-pad
 * fallback must stay authoritative when flings are off.
 *
 * Pure JVM: [classifyFling] for reachability, [policyForCategory] /
 * [PadGestureDetector.FlingGate] for the per-category wiring,
 * [AccessibilityGates.evaluate] (fail-open, no manager) +
 * [GestureTuningStore] defaults for the fallback half.
 */
class GesturePathsTest {

    private val density = 2f // 1dp = 2px

    // -- Every fling reaches a path: all four dirs classify at ship thresholds. --

    @Test
    fun allFourFlingsClassify_noDeadDirection() {
        // 30dp in 100ms = 300dp/s: above the 24dp / 200dp/s floors.
        assertEquals(FlingDirection.RIGHT, classifyFling(60f, 0f, 100L, density))
        assertEquals(FlingDirection.LEFT, classifyFling(-60f, 0f, 100L, density))
        assertEquals(FlingDirection.UP, classifyFling(0f, -60f, 100L, density))
        assertEquals(FlingDirection.DOWN, classifyFling(0f, 60f, 100L, density))
    }

    @Test
    fun belowThresholdYieldsNull_neverPhantomAction() {
        // Short/slow/ambiguous drags classify to null: the detector swallows
        // them (no tap, no action) instead of firing a wrong path.
        assertNull(classifyFling(20f, 0f, 100L, density)) // 10dp < 24dp
        assertNull(classifyFling(60f, 0f, 1000L, density)) // 30dp/s < 200dp/s
        assertNull(classifyFling(60f, 50f, 100L, density)) // ratio 1.2 < 1.4
        assertNull(classifyFling(0f, 0f, 100L, density))
    }

    // -- Gate presets: the exact on/off shape each surface relies on. --

    @Test
    fun flingGatePresets() {
        assertTrue(PadGestureDetector.FlingGate.ALL.let { it.left && it.up && it.right && it.down })
        assertFalse(PadGestureDetector.FlingGate.NONE.let { it.left || it.up || it.right || it.down })
        val acceptOnly = PadGestureDetector.FlingGate.ACCEPT_ONLY
        // QWERTY honors ONLY fling-right (accept top); every other fling is
        // reported via onGestureRejected for tuning, never silently eaten.
        assertTrue(acceptOnly.right)
        assertFalse(acceptOnly.left)
        assertFalse(acceptOnly.up)
    }

    // -- No dead-ends across the (category × layout) matrix. --

    @Test
    fun hideNeverDead_everyCategoryKeepsFlingDown() {
        for (cat in listOf("words", "ne", "numbers", "emoji", "js", "rust", "html", "math", "medical", "personal", "mystery")) {
            for (layout in listOf("t9-9", "t9-12", "t9-16")) {
                val policy = policyForCategory(cat, layout, allowTwelveKeyFlings = false)
                assertTrue("$cat/$layout must keep hide", policy.flingDownEnabled)
            }
        }
    }

    @Test
    fun acceptUnavailableOnlyForNumbers_explicitNoopNeverSilent() {
        // Numbers has nothing to accept (no prediction): right fling is an
        // explicit logged noop in onPadFlingAccept, not a swallow.
        for (cat in listOf("words", "ne", "emoji", "js", "rust", "html", "math", "medical", "personal")) {
            assertTrue("$cat must accept", policyForCategory(cat, "t9-9", false).flingRightEnabled)
        }
        assertFalse(policyForCategory("numbers", "t9-9", false).flingRightEnabled)
    }

    @Test
    fun twelveKeyLockoutScopesLeftUpOnly_rightAndDownSurvive() {
        val locked = policyForCategory("words", "t9-12", allowTwelveKeyFlings = false)
        assertFalse(locked.flingLeftEnabled)
        assertFalse(locked.flingUpEnabled)
        assertTrue(locked.flingRightEnabled)
        assertTrue(locked.flingDownEnabled)
        assertTrue("lockout must name its cause", locked.disabledReason?.isNotBlank() == true)

        val optedIn = policyForCategory("words", "t9-12", allowTwelveKeyFlings = true)
        assertTrue(optedIn.flingLeftEnabled)
        assertTrue(optedIn.flingUpEnabled)
        assertNull(optedIn.disabledReason)

        val nineKey = policyForCategory("words", "t9-9", allowTwelveKeyFlings = false)
        assertNull(nineKey.disabledReason)
    }

    // -- Delete-slide granularity (D-pad/fallback deletes whole words too). --

    @Test
    fun deleteSlideStepsOneWordPerStep() {
        // 1 word for crossing the 24dp threshold + one per 20dp step.
        assertEquals(0, deleteSlideWords(0f, density))
        assertEquals(0, deleteSlideWords(-20f, density)) // 10dp: under threshold
        assertEquals(2, deleteSlideWords(-88f, density)) // 44dp: 1 + 20/20
        assertEquals(3, deleteSlideWords(-128f, density)) // 64dp: 1 + 40/20
        // Rightward movement never selects.
        assertEquals(0, deleteSlideWords(200f, density))
    }

    @Test
    fun tapBoundary_slopAndTime() {
        val t = GestureThresholds()
        // 7.5dp inside the 8dp slop, fast: tap (child Button owns it).
        assertTrue(isTap(15f, 0f, 50L, density, t))
        // Exactly 8dp is NOT inside the slop: gesture stream, not a tap.
        assertFalse(isTap(16f, 0f, 50L, density, t))
        // Slow press past tapMaxMs is never a tap (long-press territory).
        assertFalse(isTap(0f, 0f, 201L, density, t))
    }

    @Test
    fun edgeDeadZone_cornersIgnored_centerLive() {
        val t = GestureThresholds()
        // 200×400px pad at density 2: 8dp edge = 16px.
        assertTrue(inEdgeDeadZone(0f, 200f, 200f, 400f, density, t))
        assertTrue(inEdgeDeadZone(100f, 399f, 200f, 400f, density, t))
        assertFalse(inEdgeDeadZone(100f, 200f, 200f, 400f, density, t))
        assertFalse(inEdgeDeadZone(16f, 16f, 200f, 400f, density, t))
    }

    // -- TalkBack / fallback half. --

    @Test
    fun accessibilityProbeFailsOpenWithoutManager() {
        // No AccessibilityManager (plain JVM): taps stay enabled, and the
        // reason is explicit so the Tuning screen can show it.
        val gate = AccessibilityGates.evaluate(FakeContext())
        assertTrue(gate.flingsAllowed)
        assertFalse(gate.touchExploration)
        assertFalse(gate.switchOrVoiceAccess)
        assertTrue(gate.reason?.isNotBlank() == true)
    }

    @Test
    fun gateResultCarriesReasonWhenFlingsOff() {
        // The shape the service relies on: flingsAllowed=false always pairs
        // with a user-visible reason (status line / fallback bar), never bare.
        val off = AccessibilityGates.GateResult(
            flingsAllowed = false,
            touchExploration = true,
            switchOrVoiceAccess = false,
            reason = "TalkBack/ExploreByTouch on: Pad flings disabled, fallback buttons shown"
        )
        assertFalse(off.flingsAllowed)
        assertTrue(off.reason?.contains("fallback buttons") == true)
    }

    @Test
    fun twelveKeyOptInDefaultsOff_bottomRowAuthoritative() {
        val ctx = FakeContext()
        assertFalse(GestureTuningStore.allowTwelveKeyFlings(ctx))
        GestureTuningStore.setAllowTwelveKeyFlings(ctx, true)
        assertTrue(GestureTuningStore.allowTwelveKeyFlings(ctx))
    }

    @Test
    fun codeTerminatorDefaultsSpace_rejectsJunk() {
        val ctx = FakeContext()
        assertEquals(" ", GestureTuningStore.codeTerminator(ctx))
        GestureTuningStore.setCodeTerminator(ctx, ";")
        assertEquals(";", GestureTuningStore.codeTerminator(ctx))
        try {
            GestureTuningStore.setCodeTerminator(ctx, ",")
            throw AssertionError("junk terminator must fail")
        } catch (e: IllegalArgumentException) {
            assertTrue(e.message!!.contains("space or ';'"))
        }
    }
}
