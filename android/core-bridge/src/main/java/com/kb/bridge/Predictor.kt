package com.kb.bridge

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** JNI entry point for the Rust core (`libkbcore.so` via UniFFI). Stub only. */
object KbCore {
    @Volatile
    var loaded: Boolean = false
        private set

    init {
        try {
            System.loadLibrary("kbcore")
            loaded = true
        } catch (_: UnsatisfiedLinkError) {
            loaded = false
        }
    }

    fun isAvailable(): Boolean = loaded
}

/** Candidate scored by the Rust engine. */
data class ScoredCandidate(
    val word: String,
    val score: Double,
    val category: String
)

/**
 * Kotlin mirror of the Rust UniFFI `Predictor`
 * (`new` / `suggest` / `learn` / `forget` / `export_session`, UniFFI 0.32.1).
 * `reject` has no Rust counterpart yet — it is declared here so the
 * delete-within-5s path has a call site; the real core export + regenerated
 * bindings arrive with the JDK-side `uniffi-bindgen 0.32` regen (see
 * `android/FIX_REPORT.md`).
 * No Android `Context` here by design; callers dispatch via [Dispatchers.Default].
 */
interface Predictor : AutoCloseable {
    suspend fun suggest(seq: String, prev: String?, limit: Int = 30): List<ScoredCandidate>
    suspend fun learn(word: String, prev: String?)
    suspend fun forget(word: String)
    suspend fun reject(word: String)
    suspend fun exportSession(): String
    override fun close()
}

/** In-memory stub used until the real UniFFI binding lands. */
class StubPredictor : Predictor {
    private val learned = mutableListOf<Pair<String, String?>>()
    private val rejected = mutableListOf<String>()

    override suspend fun suggest(seq: String, prev: String?, limit: Int): List<ScoredCandidate> =
        withContext(Dispatchers.Default) {
            learned.filter { it.first.isNotEmpty() }.take(limit).map {
                ScoredCandidate(word = it.first, score = 0.0, category = "words")
            }
        }

    override suspend fun learn(word: String, prev: String?): Unit =
        withContext(Dispatchers.Default) {
            // Bound the in-memory stub (20k LRU-ish cap per SPEC §4); the real
            // engine owns the persistent personal dict + tombstones.
            if (learned.size >= 20_000) learned.removeAt(0)
            learned.add(word to prev)
            Unit
        }

    /** Stub for Rust `Predictor::forget` (block tombstone). */
    override suspend fun forget(word: String): Unit =
        withContext(Dispatchers.Default) {
            learned.removeAll { it.first.equals(word, ignoreCase = true) }
            Unit
        }

    /**
     * Stub for the planned core `reject(word)` export (`record_reject` is
     * core-internal today). Records the rejection locally; no-op remotely
     * until the UniFFI regen.
     */
    override suspend fun reject(word: String): Unit =
        withContext(Dispatchers.Default) {
            rejected.add(word)
            Unit
        }

    /** Stub for Rust `Predictor::export_session` (JSONL session log). */
    override suspend fun exportSession(): String =
        withContext(Dispatchers.Default) {
            (learned.map { (w, _) -> """{"w":"$w","action":"accepted"}""" } +
                rejected.map { w -> """{"w":"$w","action":"rejected"}""" })
                .joinToString("\n")
        }

    override fun close() = Unit
}
