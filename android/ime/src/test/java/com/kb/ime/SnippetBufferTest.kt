package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** JVM tests for the RAM-only ephemeral snippet buffer (plan 12:57). */
class SnippetBufferTest {

    @Test
    fun offerThenPeekReturnsTokens() {
        val buf = SnippetBuffer(clock = { 0L })
        buf.offer("if", listOf("if (", "{", "}"), now = 0L)
        assertEquals(listOf("if (", "{", "}"), buf.peek(now = 1L))
    }

    @Test
    fun ttlExpiryDropsEntriesLazily() {
        val buf = SnippetBuffer(clock = { 0L })
        buf.offer("if", listOf("if ("), now = 0L)
        assertTrue(buf.peek(now = SnippetPalettes.SNIPPET_TTL_MS - 1).isNotEmpty())
        assertTrue(buf.peek(now = SnippetPalettes.SNIPPET_TTL_MS).isEmpty())
        assertTrue(buf.isEmpty(now = SnippetPalettes.SNIPPET_TTL_MS + 1))
    }

    @Test
    fun maxThreeEntriesEvictsOldest() {
        val buf = SnippetBuffer(clock = { 0L })
        buf.offer("a", listOf("a1"), now = 0L)
        buf.offer("b", listOf("b1"), now = 1L)
        buf.offer("c", listOf("c1"), now = 2L)
        buf.offer("d", listOf("d1"), now = 3L)
        assertEquals(3, buf.size(now = 4L))
        assertEquals(listOf("b1", "c1", "d1"), buf.peek(now = 4L))
    }

    @Test
    fun consumeWipesBuffer() {
        val buf = SnippetBuffer(clock = { 0L })
        buf.offer("if", listOf("if ("), now = 0L)
        assertEquals(listOf("if ("), buf.consume(now = 1L))
        assertTrue(buf.isEmpty(now = 1L))
        assertTrue(buf.consume(now = 1L).isEmpty())
    }

    @Test
    fun clearWipesImmediately() {
        val buf = SnippetBuffer(clock = { 0L })
        buf.offer("if", listOf("if ("), now = 0L)
        buf.clear()
        assertTrue(buf.isEmpty(now = 0L))
    }

    @Test(expected = IllegalArgumentException::class)
    fun blankTriggerRejected() {
        SnippetBuffer(clock = { 0L }).offer("  ", listOf("x"), now = 0L)
    }

    @Test(expected = IllegalArgumentException::class)
    fun emptyTokensRejected() {
        SnippetBuffer(clock = { 0L }).offer("if", emptyList(), now = 0L)
    }
}
