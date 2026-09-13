package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Settings-prefs sweep (bug-hunt): layout global/per-tab resolution,
 * suggestion flags, friend defaults, and gesture-tuning ship values — all
 * read by the IME on its next refresh without a restart, so a wrong
 * default here ships a wrong keyboard. Runs against [FakeContext] as plain
 * JUnit4 (no Robolectric).
 */
class LayoutPrefsTest {

    // -- Layout: global default + per-tab overrides (plan/02). --

    @Test
    fun globalDefaultIsT9_9_unknownRejected() {
        val ctx = FakeContext()
        assertEquals(DEFAULT_LAYOUT_ID, LayoutStore.global(ctx))
        assertEquals("t9-9", LayoutStore.global(ctx))
        assertTrue(SUPPORTED_LAYOUT_IDS.containsAll(listOf("t9-9", "t9-12", "t9-16")))
        LayoutStore.setGlobal(ctx, "t9-12")
        assertEquals("t9-12", LayoutStore.global(ctx))
        try {
            LayoutStore.setGlobal(ctx, "t9-99")
            throw AssertionError("unknown layout must fail")
        } catch (e: IllegalArgumentException) {
            assertTrue(e.message!!.contains("unknown layout_id"))
        }
        // Failed write leaves the previous value intact.
        assertEquals("t9-12", LayoutStore.global(ctx))
    }

    @Test
    fun perTabOverrideBeatsGlobal_clearFallsBack() {
        val ctx = FakeContext()
        LayoutStore.setGlobal(ctx, "t9-9")
        // No override: tab resolves the global default.
        assertEquals("t9-9", LayoutStore.layoutForCat(ctx, "ne"))
        LayoutStore.setCatLayout(ctx, "ne", "t9-16")
        assertEquals("t9-16", LayoutStore.layoutForCat(ctx, "ne"))
        // Other tabs are untouched: the sheet edits the current tab only.
        assertEquals("t9-9", LayoutStore.layoutForCat(ctx, "words"))
        LayoutStore.clearCatLayout(ctx, "ne")
        assertEquals("t9-9", LayoutStore.layoutForCat(ctx, "ne"))
    }

    @Test
    fun overridesMapCarriesValidatedIdsOnly() {
        val ctx = FakeContext()
        assertTrue(LayoutStore.overrides(ctx).isEmpty())
        LayoutStore.setCatLayout(ctx, "js", "t9-12")
        assertEquals(mapOf("js" to "t9-12"), LayoutStore.overrides(ctx))
        try {
            LayoutStore.setCatLayout(ctx, "", "t9-12")
            throw AssertionError("blank cat must fail")
        } catch (e: IllegalArgumentException) {
            assertTrue(e.message!!.contains("cat must not be empty"))
        }
    }

    // -- Suggestions flags: strip visible + auto-space by default. --

    @Test
    fun suggestionFlagsDefaultOn_roundTrip() {
        val ctx = FakeContext()
        assertTrue(SuggestionStore.showSuggestions(ctx))
        assertTrue(SuggestionStore.autoSpace(ctx))
        SuggestionStore.setShowSuggestions(ctx, false)
        SuggestionStore.setAutoSpace(ctx, false)
        assertFalse(SuggestionStore.showSuggestions(ctx))
        assertFalse(SuggestionStore.autoSpace(ctx))
    }

    // -- Friend defaults: QWERTY-first, code tabs hidden, clipboard OFF. --

    @Test
    fun friendDefaults_qwertyFirst_codeHidden_clipboardOff() {
        val ctx = FakeContext()
        assertTrue("friends start on QWERTY", FriendDefaults.qwertyDefault(ctx))
        assertFalse("code tabs hidden for friends", FriendDefaults.showCodeTabs(ctx))
        assertFalse("clipboard OFF until enabled", FriendDefaults.clipboardEnabled(ctx))
        assertEquals(24L, FriendDefaults.FRIEND_TTL_HOURS)
    }

    @Test
    fun friendTogglesRoundTrip() {
        val ctx = FakeContext()
        FriendDefaults.setQwertyDefault(ctx, false)
        FriendDefaults.setShowCodeTabs(ctx, true)
        assertFalse(FriendDefaults.qwertyDefault(ctx))
        assertTrue(FriendDefaults.showCodeTabs(ctx))
    }

    // -- Gesture tuning: ship thresholds, 12-key OFF, space terminator. --

    @Test
    fun tuningShipDefaults_matchThresholds() {
        val ctx = FakeContext()
        assertEquals(GestureThresholds(), GestureTuningStore.loadThresholds(ctx))
        assertFalse(GestureTuningStore.allowTwelveKeyFlings(ctx))
        assertEquals(" ", GestureTuningStore.codeTerminator(ctx))
    }

    @Test
    fun tuningRoundTrip_outOfRangeLongPressCoerced() {
        val ctx = FakeContext()
        val tuned = GestureThresholds(flingThresholdDp = 30f, longPressMs = 500L)
        GestureTuningStore.saveThresholds(ctx, tuned)
        assertEquals(tuned, GestureTuningStore.loadThresholds(ctx))
        // A tampered long-press value clamps into 300-600 instead of
        // crashing the detector.
        ctx.prefsFile(GestureTuningStore.PREFS_NAME).edit().putLong("long_press_ms", 999L).apply()
        assertEquals(600L, GestureTuningStore.loadThresholds(ctx).longPressMs)
    }

    @Test
    fun coachAndFooterDefaults_firstRunShowsHint() {
        val ctx = FakeContext()
        assertFalse(GestureTuningStore.isCoachSeen(ctx))
        GestureTuningStore.markCoachSeen(ctx)
        assertTrue(GestureTuningStore.isCoachSeen(ctx))
        GestureTuningStore.requestCoachReplay(ctx)
        assertFalse(GestureTuningStore.isCoachSeen(ctx))
        // First run arms the footer hint; dismiss hides it for good.
        assertTrue(GestureTuningStore.showFooterHint(ctx, nowMs = 1_000L))
        GestureTuningStore.dismissFooterHint(ctx)
        assertFalse(GestureTuningStore.showFooterHint(ctx, nowMs = 2_000L))
    }
}
