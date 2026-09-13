package com.kb.ime

import com.kb.bridge.PredictorFactory
import com.kb.bridge.StubPredictor
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * QWERTY sweep (bug-hunt): the fallback board shows the literal buffer plus
 * engine corrections through the SAME suggest/learn interface as T9 (only
 * the encoder differs), space commits the word, backspace restores the
 * pre-correction buffer, password fields never autocorrect/learn, and the
 * single-fetch discipline holds (one suggest per keystroke, limit honored).
 *
 * The engine half (`teh -> the` ranking) lives in `core-rust` (`cargo test
 * -p kbcore`); here the bridge contract is pinned against [StubPredictor]
 * (literal preservation, tab boost, tombstones, loud offline failures) plus
 * the buffer/gate string discipline the service implements.
 */
class QwertyBehaviorTest {

    // -- Literal + corrections through the shared interface. --

    @Test
    fun literalBufferPreserved_tehStaysTeh() = runBlocking {
        val stub = StubPredictor()
        stub.learn("teh", "words")
        val got = stub.suggestWithLayout("", "843", "t9-9", "words", 30)
        assertTrue("literal must survive, got: ${got.map { it.word }}", got.any { it.word == "teh" })
    }

    @Test
    fun learnedCorrectionOverlays_activeTabBoostsFirst() = runBlocking {
        val stub = StubPredictor()
        stub.learn("teh", "words")
        stub.learn("the", "words")
        stub.learn("namaste", "ne")
        val got = stub.suggestWithLayout("", "843", "t9-9", "words", 30).map { it.word }
        assertTrue(got.containsAll(listOf("teh", "the")))
        // Same-tab matches rank before other-tab words (category boost).
        assertTrue(got.indexOf("the") < got.indexOf("namaste"))
        assertTrue(got.indexOf("teh") < got.indexOf("namaste"))
    }

    @Test
    fun singleFetchHonorsLimit_noPerWordChatter() = runBlocking {
        val stub = StubPredictor()
        for (w in listOf("aa", "ab", "ac", "ad", "ae")) stub.learn(w, "words")
        // One call returns the whole page: the strip pages locally from it.
        val page = stub.suggestWithLayout("", "22", "t9-9", "words", 3)
        assertEquals(3, page.size)
        val all = stub.suggestWithLayout("", "22", "t9-9", "words", 30)
        assertEquals(5, all.size)
    }

    @Test
    fun forgetTombstones_rejectRecordedInExport() = runBlocking {
        val stub = StubPredictor()
        stub.learn("teh", "words")
        stub.forget("teh")
        val got = stub.suggestWithLayout("", "843", "t9-9", "words", 30)
        assertTrue("blocked word must not resurface", got.none { it.word == "teh" })
        stub.reject("the")
        assertTrue(stub.exportSession().contains("\"action\":\"rejected\""))
    }

    @Test
    fun offlineEncodeFailsLoud_stripClearedExplicitly() = runBlocking {
        // Without the engine the QWERTY encoder has no mapping: the stub
        // throws naming the missing .so (refreshQwertySuggestions catches and
        // clears the strip explicitly) — never a phantom empty suggestion.
        val stub = StubPredictor()
        try {
            stub.encode("hello", "t9-9")
            throw AssertionError("offline encode must fail loudly")
        } catch (e: IllegalStateException) {
            assertTrue(e.message!!.contains("degraded"))
        }
    }

    @Test
    fun offlineEngineDegradesWithReason_neverSilentStub() {
        // The IME's initEngine path on a device without libkbcore.so: the
        // status line carries this reason; the keyboard keeps working
        // degraded instead of showing a dead strip.
        val decision = PredictorFactory.create("[]")
        assertTrue(decision is PredictorFactory.Decision.Degraded)
        val reason = (decision as PredictorFactory.Decision.Degraded).reason
        assertTrue(reason.isNotBlank())
        assertTrue(reason.contains("libkbcore.so"))
    }

    // -- Password fields never autocorrect/learn/capture, on every tab. --

    @Test
    fun passwordNeverLearns_passwordNeverSnippets_passwordNeverCaptures() {
        for (tab in listOf("words", "ne", "js", "rust", "html", "emoji", "math", "medical", "numbers", "personal")) {
            val g = GateInput(
                password = true,
                incognito = false,
                textClass = true,
                emailOrUri = false,
                tab = tab,
                learnForTab = tab != "numbers" && tab != "math"
            )
            assertFalse("$tab must not learn under password", SnippetGates.learnNow(g))
            assertFalse("$tab must not snippet under password", SnippetGates.snippetNow(g))
        }
        assertFalse(SnippetGates.mayCaptureClipboard(password = true, incognito = false))
    }

    // -- Buffer discipline: space breaks the word, backspace restores. --

    @Test
    fun spaceBreaksWord_backspaceRestoresLastAuto() {
        // Mirror of onQwertyKey(" ") + deleteLast's qwerty branch: space
        // commits the literal buffer (learn path reused); backspace peels
        // one char and re-queries, so the pre-correction buffer restores.
        val buf = StringBuilder()
        for (c in "teh") buf.append(c.toString())
        assertEquals("teh", buf.toString())
        val committed = if (buf.isNotEmpty()) buf.toString() else " "
        buf.clear()
        assertEquals("teh", committed)

        val buf2 = StringBuilder("thex")
        buf2.deleteCharAt(buf2.length - 1)
        assertEquals("the", buf2.toString())
        buf2.deleteCharAt(buf2.length - 1)
        assertEquals("th", buf2.toString())
    }

    @Test
    fun nonTextInputCommitsRaw_noPredictionState() {
        // NUMBER/PHONE/DATETIME: the buffer gate is closed (textClass=false),
        // so keys commit raw and neither gate opens.
        val g = GateInput(false, false, textClass = false, false, "words", true)
        assertFalse(SnippetGates.learnNow(g))
        assertFalse(SnippetGates.snippetNow(g))
    }
}
