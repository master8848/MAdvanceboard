package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * JVM tests for the emoji recents math (plan/24 §5): most-recent-first,
 * deduped, capped; grid search is a dumb substring filter.
 */
class EmojiRecentsTest {

    @Test
    fun pushPrependsAndDedupes() {
        assertEquals(listOf("😀", "❤️"), EmojiRecentMath.push(listOf("❤️"), "😀"))
        assertEquals(listOf("❤️", "😀"), EmojiRecentMath.push(listOf("😀", "❤️"), "❤️"))
    }

    @Test
    fun pushCapsAtMax() {
        val full = (0 until EmojiRecentMath.MAX_RECENTS).map { "e$it" }
        val out = EmojiRecentMath.push(full, "new")
        assertEquals(EmojiRecentMath.MAX_RECENTS, out.size)
        assertEquals("new", out.first())
        assertTrue("e${EmojiRecentMath.MAX_RECENTS - 1}" !in out)
    }

    @Test
    fun pushIgnoresBlanks() {
        val recents = listOf("😀")
        assertEquals(recents, EmojiRecentMath.push(recents, ""))
    }

    @Test
    fun blankQueryReturnsWholePool() {
        val pool = listOf("😀", "❤️")
        assertEquals(pool, EmojiRecentMath.filter(pool, "  "))
    }

    @Test
    fun queryFiltersSubstring() {
        val pool = listOf("😀", "❤️")
        assertEquals(listOf("😀"), EmojiRecentMath.filter(pool, "😀"))
        assertTrue(EmojiRecentMath.filter(pool, "zzz").isEmpty())
    }
}
