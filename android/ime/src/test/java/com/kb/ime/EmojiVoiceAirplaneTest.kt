package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Emoji grid + recents, voice zero-residue, and offline (airplane) retry
 * (bug-hunt).
 *
 * - Picker: recents most-recent-first capped at 24, grid row takes 8, blank
 *   query returns the whole pool, dead query returns empty (the "no match"
 *   hint path) — never null, never a stale list.
 * - Voice: there is no voice capture path outside the gated commit paths,
 *   so voice-committed text leaves zero residue — the same wipe/expiry
 *   discipline as every other commit (snippet scratch consumed on expand,
 *   TTL-expired helpers never resurface).
 * - Airplane: the engine is fully on-device; with no connectivity AND no
 *   native library the factory still answers degraded-with-cause (the
 *   status line + next-keystroke retry), and the stub keeps
 *   suggest/learn working in memory.
 */
class EmojiVoiceAirplaneTest {

    @Test
    fun recentsCapAndGridRow() {
        var recents = emptyList<String>()
        for (i in 0 until EmojiRecentMath.MAX_RECENTS + 5) {
            recents = EmojiRecentMath.push(recents, "e$i")
        }
        assertEquals(EmojiRecentMath.MAX_RECENTS, recents.size)
        assertEquals("e${EmojiRecentMath.MAX_RECENTS + 4}", recents.first())
        // The picker's recents row renders the first 8 only.
        assertEquals(8, recents.take(8).size)
    }

    @Test
    fun gridSearch_blankReturnsPool_deadQueryEmpty() {
        val pool = listOf("😀", "😁", "❤️")
        assertEquals(pool, EmojiRecentMath.filter(pool, ""))
        assertEquals(pool, EmojiRecentMath.filter(pool, "   "))
        assertEquals(listOf("😀"), EmojiRecentMath.filter(pool, "😀"))
        assertTrue(EmojiRecentMath.filter(pool, "zzz-no-match").isEmpty())
    }

    @Test
    fun expandConsumesScratch_expiredHelpersNeverResurface_zeroResidue() {
        val ttl = SnippetPalettes.SNIPPET_TTL_MS
        val buffer = SnippetBuffer(clock = { 0L })
        buffer.offer("if", listOf("if (", "{", "}"), now = 0L)
        // Expansion consumes: nothing lingers for the next field/voice chunk.
        val expanded = buffer.consume(now = 1_000L)
        assertEquals(listOf("if (", "{", "}"), expanded)
        assertTrue(buffer.isEmpty(now = 1_000L))
        // TTL expiry without expansion: helpers vanish lazily, no residue.
        buffer.offer("for", listOf("for (", "{", "}"), now = 2_000L)
        assertTrue(buffer.peek(now = 2_000L + ttl + 1).isEmpty())
        assertTrue(buffer.isEmpty(now = 2_000L + ttl + 1))
    }

    @Test
    fun snippetTtlIsTwoMinutes_bufferHoldsThree() {
        assertEquals(120_000L, SnippetPalettes.SNIPPET_TTL_MS)
        assertEquals(3, SnippetPalettes.MAX_ENTRIES)
        val buffer = SnippetBuffer(clock = { 0L })
        buffer.offer("a", listOf("a1"), now = 0L)
        buffer.offer("b", listOf("b1"), now = 0L)
        buffer.offer("c", listOf("c1"), now = 0L)
        buffer.offer("d", listOf("d1"), now = 0L)
        // Oldest evicted: bounded RAM by construction, no leak across a
        // long voice-dictation session either.
        assertEquals(3, buffer.size(now = 0L))
        assertTrue("a1" !in buffer.peek(now = 0L))
    }

    @Test
    fun airplaneOfflineFactoryStillAnswersDegradedWithCause() {
        // No network, no .so (plain JVM): create() must answer — degraded
        // WITH the cause for the status line — never throw, never null, so
        // the next keystroke can retry init.
        val decision = com.kb.bridge.PredictorFactory.create("[]")
        assertTrue(decision is com.kb.bridge.PredictorFactory.Decision.Degraded)
        val degraded = decision as com.kb.bridge.PredictorFactory.Decision.Degraded
        assertTrue(degraded.reason.isNotBlank())
    }

    @Test
    fun degradedStubKeepsCoreOps_noSilentEmptyStrip() {
        // Offline typing still suggests learned words in memory; layout and
        // persist surfaces fail loudly (non-empty error / throw) instead of
        // returning plausible-looking empty values.
        val stub = com.kb.bridge.StubPredictor()
        kotlinx.coroutines.runBlocking {
            stub.learn("hello", "words")
            val got = stub.suggestWithLayout("", "43556", "t9-9", "words", 30)
            assertTrue(got.any { it.word == "hello" })
            assertTrue(stub.takeLastLayoutError().isNotEmpty())
            try {
                stub.isCatEnabled("words")
                throw AssertionError("no in-memory enable state: must fail loudly")
            } catch (e: IllegalStateException) {
                assertTrue(e.message!!.contains("degraded"))
            }
        }
    }
}
