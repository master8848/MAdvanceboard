package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Plan/11 clipboard acceptance (pure JVM, clock injected):
 * copy 3 items, pin 1, advance 8 days → unpinned gone, pinned stays;
 * password/incognito copies never captured; TTL change applies on next
 * sweep without touching pins.
 */
class ClipboardHistoryTest {

    private val t0 = 1_700_000_000_000L
    private val dayMs = 24L * 3_600_000L

    @Test
    fun acceptance_copy3Pin1Advance8Days() {
        var items = emptyList<ClipboardItem>()
        items = ClipboardHistory.add(items, "first", t0)
        items = ClipboardHistory.add(items, "second", t0 + 1)
        items = ClipboardHistory.add(items, "third", t0 + 2)
        assertEquals(3, items.size)

        val pinnedId = items.first { it.text == "second" }.id
        items = ClipboardHistory.setPinned(items, pinnedId, true)

        // Advance clock 8 days past the default 7d TTL and sweep.
        val swept = ClipboardHistory.sweep(items, ClipboardHistory.DEFAULT_TTL_HOURS, t0 + 8 * dayMs)
        assertEquals(listOf("second"), swept.map { it.text })
        assertTrue(swept.single().pinned)
    }

    @Test
    fun passwordAndIncognitoNeverCapture() {
        assertFalse(ClipboardHistory.shouldCapture(isPasswordField = true, isIncognito = false))
        assertFalse(ClipboardHistory.shouldCapture(isPasswordField = false, isIncognito = true))
        assertFalse(ClipboardHistory.shouldCapture(isPasswordField = true, isIncognito = true))
        assertTrue(ClipboardHistory.shouldCapture(isPasswordField = false, isIncognito = false))
    }

    @Test
    fun ttlChange7dTo24hAppliesOnNextSweepKeepsPins() {
        var items = emptyList<ClipboardItem>()
        items = ClipboardHistory.add(items, "old-unpinned", t0)
        items = ClipboardHistory.add(items, "old-pinned", t0)
        items = ClipboardHistory.setPinned(items, items.first { it.text == "old-pinned" }.id, true)
        items = ClipboardHistory.add(items, "fresh", t0 + 2 * dayMs)

        // 7d TTL at +2d: everything survives.
        assertEquals(3, ClipboardHistory.sweep(items, 168L, t0 + 2 * dayMs).size)
        // Tighten to 24h: old unpinned drops, old PINNED stays, fresh stays.
        val swept = ClipboardHistory.sweep(items, 24L, t0 + 2 * dayMs + 1)
        assertEquals(setOf("old-pinned", "fresh"), swept.map { it.text }.toSet())
    }

    @Test
    fun itemCap5kAndRing100Unpinned() {
        val long = "x".repeat(6_000)
        var items = ClipboardHistory.add(emptyList(), long, t0)
        assertEquals(ClipboardHistory.MAX_ITEM_CHARS, items.single().text.length)

        items = emptyList()
        repeat(ClipboardHistory.MAX_UNPINNED + 10) { i ->
            items = ClipboardHistory.add(items, "item-$i", t0 + i)
        }
        assertEquals(ClipboardHistory.MAX_UNPINNED, items.size)
        // Ring keeps newest; oldest 10 dropped.
        assertTrue(items.none { it.text == "item-0" })
        assertTrue(items.any { it.text == "item-109" })

        // Pins are exempt from the ring: pin 5, overflow, pins survive.
        repeat(5) { i ->
            items = ClipboardHistory.setPinned(
                items, items.first { it.text == "item-${109 - i}" }.id, true
            )
        }
        repeat(20) { i ->
            items = ClipboardHistory.add(items, "new-$i", t0 + 1_000 + i)
        }
        assertEquals(5, items.count { it.pinned })
        assertEquals(ClipboardHistory.MAX_UNPINNED, items.count { !it.pinned })
    }

    @Test
    fun blanksRejectedDedupMovesToTop() {
        assertNull(ClipboardHistory.sanitize(null))
        assertNull(ClipboardHistory.sanitize("   "))
        var items = ClipboardHistory.add(emptyList(), "   ", t0)
        assertTrue(items.isEmpty())

        items = ClipboardHistory.add(items, "a", t0)
        items = ClipboardHistory.add(items, "b", t0 + 1)
        items = ClipboardHistory.add(items, "a", t0 + 2)
        assertEquals(listOf("a", "b"), items.map { it.text })
    }

    @Test
    fun searchIsSubstringCaseInsensitive() {
        var items = emptyList<ClipboardItem>()
        items = ClipboardHistory.add(items, "Hello World", t0)
        items = ClipboardHistory.add(items, "goodbye", t0 + 1)
        assertEquals(listOf("Hello World"), ClipboardHistory.search(items, "hello").map { it.text })
        assertEquals(2, ClipboardHistory.search(items, "").size)
    }

    @Test
    fun deleteAndClearAllConfirmSemantics() {
        var items = emptyList<ClipboardItem>()
        items = ClipboardHistory.add(items, "keep", t0)
        items = ClipboardHistory.add(items, "drop", t0 + 1)
        items = ClipboardHistory.setPinned(items, items.first { it.text == "keep" }.id, true)

        // Clear-all keeps pins unless confirmed with includePins.
        assertEquals(listOf("keep"), ClipboardHistory.clear(items, includePins = false).map { it.text })
        assertTrue(ClipboardHistory.clear(items, includePins = true).isEmpty())

        val id = items.first { it.text == "drop" }.id
        items = ClipboardHistory.delete(items, id)
        assertEquals(listOf("keep"), items.map { it.text })
    }

    @Test(expected = IllegalArgumentException::class)
    fun unknownPinIdFailsLoudly() {
        ClipboardHistory.setPinned(emptyList(), "cb-nope-0", true)
    }

    @Test(expected = IllegalArgumentException::class)
    fun unknownDeleteIdFailsLoudly() {
        ClipboardHistory.delete(emptyList(), "cb-nope-0")
    }

    @Test
    fun ttlBoundsRejected() {
        for (bad in listOf(0L, -5L, 721L)) {
            try {
                ClipboardHistory.validateTtlHours(bad)
                throw AssertionError("TTL $bad must fail")
            } catch (e: IllegalArgumentException) {
                assertTrue(e.message!!.contains("retention"))
            }
        }
        for (ok in listOf(1L, 24L, 168L, 720L)) ClipboardHistory.validateTtlHours(ok)
    }

    @Test
    fun jsonRoundTripWithSpecialChars() {
        var items = emptyList<ClipboardItem>()
        items = ClipboardHistory.add(items, "quote \" back\\slash newline\nhere\ttab ünï", t0)
        items = ClipboardHistory.setPinned(items, items.single().id, true)
        val json = ClipboardHistory.toJson(items)
        val back = ClipboardHistory.fromJson(json)
        assertEquals(items, back)
        assertTrue(ClipboardHistory.fromJson("[]").isEmpty())
    }

    @Test
    fun corruptJsonFailsLoudly() {
        for (bad in listOf("", "not-json", "[{]", "[{\"id\":\"x\"}]", "[[1]]")) {
            try {
                ClipboardHistory.fromJson(bad)
                throw AssertionError("must fail: $bad")
            } catch (e: IllegalStateException) {
                assertTrue(e.message!!.contains("unreadable"))
            }
        }
    }
}
