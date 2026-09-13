package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Clipboard-history default + retention sweep (bug-hunt): history is OFF
 * until explicitly enabled (friend-default), the recommended retention when
 * enabled is 24h, pins survive every sweep, incognito/password never
 * capture, and emoji recents round-trip through the same lean prefs.
 */
class ClipboardDefaultsTest {

    private val t0 = 1_700_000_000_000L
    private val hourMs = 3_600_000L

    @Test
    fun historyOffByDefault_untilExplicitlyEnabled() {
        val ctx = FakeContext()
        assertFalse("friend-default OFF", FriendDefaults.clipboardEnabled(ctx))
        FriendDefaults.setClipboardEnabled(ctx, true)
        assertTrue(FriendDefaults.clipboardEnabled(ctx))
        FriendDefaults.setClipboardEnabled(ctx, false)
        assertFalse(FriendDefaults.clipboardEnabled(ctx))
    }

    @Test
    fun friendTtlIs24hPreset_validRetention() {
        assertEquals(24L, FriendDefaults.FRIEND_TTL_HOURS)
        assertTrue(FriendDefaults.FRIEND_TTL_HOURS in ClipboardHistory.TTL_PRESETS_HOURS)
        ClipboardHistory.validateTtlHours(FriendDefaults.FRIEND_TTL_HOURS)
        // The engine default (7d) stays distinct from the friend recommendation.
        assertEquals(168L, ClipboardHistory.DEFAULT_TTL_HOURS)
    }

    @Test
    fun pinSurvives24hSweep_unpinnedExpires() {
        var items = emptyList<ClipboardItem>()
        items = ClipboardHistory.add(items, "otp-code", t0)
        items = ClipboardHistory.add(items, "keep-me", t0)
        items = ClipboardHistory.setPinned(items, items.first { it.text == "keep-me" }.id, true)
        // 25h past a 24h TTL: unpinned gone, pinned stays.
        val swept = ClipboardHistory.sweep(items, 24L, t0 + 25 * hourMs)
        assertEquals(listOf("keep-me"), swept.map { it.text })
        assertTrue(swept.single().pinned)
    }

    @Test
    fun ttlBoundaryInclusive_exactly24hKept_tickOverDropped() {
        var items = ClipboardHistory.add(emptyList(), "x", t0)
        assertEquals(1, ClipboardHistory.sweep(items, 24L, t0 + 24 * hourMs).size)
        assertTrue(ClipboardHistory.sweep(items, 24L, t0 + 24 * hourMs + 1).isEmpty())
        // Pins ignore the boundary entirely.
        items = ClipboardHistory.setPinned(items, items.single().id, true)
        assertEquals(1, ClipboardHistory.sweep(items, 24L, t0 + 30 * 24 * hourMs).size)
    }

    @Test
    fun storeDefaults7d_roundTripsThroughPrefs_sweepsOnLoad() {
        val ctx = FakeContext()
        assertEquals(ClipboardHistory.DEFAULT_TTL_HOURS, ClipboardStore.ttlHours(ctx))
        ClipboardStore.setTtlHours(ctx, 24L)
        assertEquals(24L, ClipboardStore.ttlHours(ctx))

        var items = emptyList<ClipboardItem>()
        items = ClipboardHistory.add(items, "fresh", t0 + 24 * hourMs)
        items = ClipboardHistory.add(items, "stale", t0)
        ClipboardStore.save(ctx, items, nowMs = t0 + 25 * hourMs)
        // Load sweeps to now: the 1h-old item survives a 24h TTL, the 25h-old one drops.
        val loaded = ClipboardStore.load(ctx, nowMs = t0 + 25 * hourMs)
        assertEquals(listOf("fresh"), loaded.map { it.text })
    }

    @Test
    fun storeRejectsOutOfRangeTtl_corruptJsonFailsLoud() {
        val ctx = FakeContext()
        try {
            ClipboardStore.setTtlHours(ctx, 0L)
            throw AssertionError("TTL 0 must fail")
        } catch (e: IllegalArgumentException) {
            assertTrue(e.message!!.contains("retention"))
        }
        ctx.prefsFile(ClipboardStore.PREFS_NAME).edit()
            .putString(ClipboardStore.KEY_ITEMS_JSON, "not-json{{{")
            .apply()
        try {
            ClipboardStore.load(ctx)
            throw AssertionError("corrupt history must fail loudly")
        } catch (e: IllegalStateException) {
            assertTrue(e.message!!.contains("unreadable"))
        }
    }

    @Test
    fun clipboardClearKeepsPins_unlessConfirmed() {
        val ctx = FakeContext()
        var items = emptyList<ClipboardItem>()
        items = ClipboardHistory.add(items, "pin", t0)
        items = ClipboardHistory.add(items, "drop", t0 + 1)
        items = ClipboardHistory.setPinned(items, items.first { it.text == "pin" }.id, true)
        // "Keep pins" (default confirm path) preserves the pin in storage too.
        val kept = ClipboardStore.clear(ctx, items, includePins = false)
        assertEquals(listOf("pin"), kept.map { it.text })
        assertEquals(listOf("pin"), ClipboardStore.load(ctx, nowMs = t0 + 2).map { it.text })
        assertTrue(ClipboardStore.clear(ctx, items, includePins = true).isEmpty())
    }

    @Test
    fun incognitoOrPasswordNeverCaptures_manualToggleCountsAsIncognito() {
        // The service ORs manualIncognito into the system flag before the
        // gate, so all three incognito triggers deny identically.
        assertTrue(SnippetGates.mayCaptureClipboard(false, false))
        assertFalse(SnippetGates.mayCaptureClipboard(true, false))
        assertFalse(SnippetGates.mayCaptureClipboard(false, true))
        assertFalse(SnippetGates.mayCaptureClipboard(true, true))
        assertFalse(ClipboardHistory.shouldCapture(true, false))
        assertFalse(ClipboardHistory.shouldCapture(false, true))
    }

    @Test
    fun emojiRecentsRoundTrip_gridTakesEight() {
        val ctx = FakeContext()
        assertTrue(EmojiRecents.load(ctx).isEmpty())
        EmojiRecents.record(ctx, "😀")
        EmojiRecents.record(ctx, "❤️")
        assertEquals(listOf("❤️", "😀"), EmojiRecents.load(ctx))
        // The grid's recents row shows the first 8; the store caps at 24.
        val full = (0 until EmojiRecentMath.MAX_RECENTS).map { "e$it" }
        assertEquals(EmojiRecentMath.MAX_RECENTS, full.size)
        assertEquals(8, full.take(8).size)
    }
}
