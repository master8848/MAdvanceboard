package com.kb.bridge

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Bridge contract tests (plain JVM JUnit4 — no device needed).
 *
 * On the JVM there is no `libkbcore.so` on `java.library.path`, so these
 * pin the degraded half of the contract: [KbCore] reports unavailable WITH
 * a cause, [PredictorFactory] degrades WITH a reason (never silently), and
 * [StubPredictor] behaves explicitly (in-memory core ops; loud failures
 * elsewhere). The live-engine half is covered by `cargo test -p kbcore`
 * plus the on-device smoke in the handoff report.
 */
class PredictorBridgeTest {

    @Test
    fun kbCoreUnavailableOnJvmWithCause() {
        assertFalse("no .so on JVM test classpath", KbCore.isAvailable())
        assertTrue(
            "load cause must name the library, got: ${KbCore.loadError}",
            (KbCore.loadError ?: "").contains("kbcore", ignoreCase = true)
        )
        assertTrue(
            "user message must name the library",
            KbCore.describeUnavailable().contains("libkbcore.so")
        )
    }

    @Test
    fun factoryDegradesWithReasonNeverSilently() {
        val decision = PredictorFactory.create("[]")
        assertTrue("JVM has no .so: must degrade", decision is PredictorFactory.Decision.Degraded)
        val degraded = decision as PredictorFactory.Decision.Degraded
        assertTrue("degraded predictor must be the explicit stub", degraded.predictor is StubPredictor)
        assertTrue("reason must be user-visible, got: ${degraded.reason}", degraded.reason.isNotBlank())
        assertTrue("reason must name the cause", degraded.reason.contains("libkbcore.so"))
    }

    @Test
    fun stubLearnSuggestRejectExportRoundtrip() = runBlocking {
        val stub = StubPredictor()
        stub.learn("hello", "words")
        stub.learn("namaste", "ne")
        val s = stub.suggestWithLayout(
            ctx = "",
            digits = "43556",
            layoutId = "t9-9",
            activeTab = "words",
            limit = 30
        )
        assertTrue("stub must return learned words", s.any { it.word == "hello" })
        val hello = s.first { it.word == "hello" }
        assertEquals("words", hello.cat)
        assertEquals("t9-9", hello.layoutId)
        assertEquals(0.0, hello.score, 0.0)
        // Active-tab matches rank first (deterministic degraded order).
        assertEquals("hello", s.first().word)
        stub.rejectWithShown("hell", listOf("hello"))
        val session = stub.exportSession()
        assertTrue("session JSONL must carry the accept", session.contains("\"hello\""))
        assertTrue("session JSONL must carry the reject", session.contains("\"hell\""))
        stub.forget("hello")
        val after = stub.suggest("", "43556", "words", 30)
        assertTrue("blocked word must hide", after.none { it.word == "hello" })
    }

    @Test
    fun stubLayoutPackMethodsFailLoudly() = runBlocking {
        val stub = StubPredictor()
        for (op in listOf< suspend () -> Unit>(
            { stub.encode("hello", "t9-9") },
            { stub.decode("435", "t9-9", 5) },
            { stub.setDefaultLayout("t9-9") },
            { stub.addPackJson("{}", 10) },
            { stub.setCatEnabled("words", true) },
            { stub.isCatEnabled("words"); Unit },
            { stub.placement("hello"); Unit },
        )) {
            try {
                op()
                throw AssertionError("stub op must throw, not return empty")
            } catch (e: IllegalStateException) {
                assertTrue(
                    "throw must name the missing .so, got: ${e.message}",
                    (e.message ?: "").contains("libkbcore.so")
                )
            }
        }
        assertEquals(listOf("t9-9", "t9-12", "t9-16"), stub.layouts())
        assertTrue("persist degraded string must be explicit", stub.flushPersist().contains("libkbcore.so"))
    }

    @Test
    fun stubErrorDrainsAreExplicitNeverSilentSuccess() = runBlocking {
        val stub = StubPredictor()
        val layoutError = stub.takeLastLayoutError()
        assertTrue(
            "stub layout drain must be explicit degraded signal, got: $layoutError",
            layoutError.contains("libkbcore.so")
        )
        assertTrue(
            "persist drain must be explicit degraded signal",
            stub.takeLastPersistError().contains("libkbcore.so")
        )
        assertTrue(
            "layout diagnostic must name the cause",
            stub.layoutDiagnostic("words").contains("libkbcore.so")
        )
    }

    @Test
    fun scoredCandidateCarriesFullEngineShape() {
        val c = ScoredCandidate(word = "hello", score = 1.5, seq = "43556", cat = "words", layoutId = "t9-9")
        assertEquals("hello", c.word)
        assertEquals("43556", c.seq)
        assertEquals("words", c.cat)
        assertEquals("t9-9", c.layoutId)
    }
}
