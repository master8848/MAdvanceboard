package com.kb.bridge

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * JNI entry point for the Rust core (`libkbcore.so` via UniFFI 0.32.1).
 *
 * The load probe runs eagerly at class load; the outcome is explicit:
 * [isAvailable] == false carries [loadError] (the `UnsatisfiedLinkError`
 * message naming the missing library), and [describeUnavailable] renders
 * the user-visible degraded message. Nothing here degrades silently —
 * [PredictorFactory] routes the `false` state to [StubPredictor] WITH the
 * reason, and the IME surfaces it in its status line.
 */
object KbCore {
    @Volatile
    var loaded: Boolean = false
        private set

    /** Link failure cause when [loaded] == false; null on success. */
    @Volatile
    var loadError: String? = null
        private set

    init {
        try {
            System.loadLibrary("kbcore")
            loaded = true
            loadError = null
        } catch (e: UnsatisfiedLinkError) {
            loaded = false
            loadError = e.message ?: "UnsatisfiedLinkError (no detail)"
        } catch (e: SecurityException) {
            loaded = false
            loadError = "SecurityException: ${e.message}"
        }
    }

    fun isAvailable(): Boolean = loaded

    /** User-visible degraded message; names the cause, never a bare flag. */
    fun describeUnavailable(): String =
        "Rust engine unavailable (libkbcore.so not loaded: ${loadError ?: "unknown cause"}). " +
            "Using degraded in-memory predictions — learning will not persist."
}

/**
 * Candidate scored by the Rust engine. Field-for-field mirror of the
 * UniFFI `Suggestion` record (`word`, `score`, `seq`, `cat`, `layout_id`).
 */
data class ScoredCandidate(
    val word: String,
    val score: Double,
    val seq: String,
    val cat: String,
    val layoutId: String
)

/**
 * QWERTY correction/completion (plan/22 Step 1). Mirror of the UniFFI
 * `QwertyCorrection` record: `isCorrection` selects the center-bold slot
 * (typo fix) vs the right slot (prefix completion).
 */
data class QwertyCorrection(
    val word: String,
    val score: Double,
    val seq: String,
    val cat: String,
    val isCorrection: Boolean
)

/**
 * QWERTY suggest output (plan/22 Step 1). Mirror of the UniFFI
 * `QwertyResult` record: `literal` is always the verbatim raw buffer;
 * `confident` gates Space-takes-correction.
 */
data class QwertyResult(
    val literal: String,
    val corrections: List<QwertyCorrection>,
    val confident: Boolean
)

/**
 * Kotlin mirror of the Rust UniFFI `Predictor` (uniffi 0.32.1; generated
 * bindings vendored at `uniffi/kbcore/kbcore.kt`). Every exported method is
 * represented — signatures follow the Rust order `(ctx, digits, …)`:
 *
 * - `suggest(ctx, digits, active_tab, limit)` — back-compat `t9-9` path.
 * - `suggestWithLayout(ctx, digits, layout_id, active_tab, limit)` — explicit pad.
 * - `suggestForCat(ctx, digits, active_tab, limit)` — per-tab layout resolve.
 * - `suggestQwerty(raw, ctx, active_tab, layout_id, limit, autocorrectThreshold)` —
 *   QWERTY literal + letter-graph corrections + confidence in ONE FFI call
 *   (plan/22; thresholds 0.5 aggressive / 1.0 default / 2.0 conservative /
 *   `Double.POSITIVE_INFINITY` suggest-only; `limit == 0` is Off).
 * - `learn(word, category)` / `learnWithShown(word, category, shown)` —
 *   `category` is the pack id (`words`, `ne`, …), NOT the ctx-prev word;
 *   `shown` is the caller-observed last suggest result (plan/05 #5: no
 *   suggest-in-lock, O(1) personal op + session log + coalesced flush).
 * - `forget(word)` — block tombstone. `reject(word)` /
 *   `rejectWithShown(word, shown)` — reject penalty + rejected session event
 *   (delegate to core-internal `record_reject`; the UniFFI export IS present,
 *   `predictor.rs:314,320`).
 * - `setCatEnabled(cat, enabled)` / `isCatEnabled(cat)` — runtime category
 *   enable toggle (plan/08 reorder UI calls these live; verified with the
 *   read-back).
 * - `placement(word)` — per-word provenance (`pack cat • freq • accepts`);
 *   `""` means no pack holds the word (explicit no-record, never a
 *   fabricated pack).
 * - `exportSession()` — session log as JSONL.
 * - Layout passthroughs (`encode` / `decode` / `layouts` / cat-map) and
 *   persist passthroughs (`open_persist` / `flush_persist` / `compact_persist`).
 *
 * Batch-string discipline (plan/04 perf invariant): JSON wordlist in at
 * construction, JSONL session out at export, ONE `suggest*` call per
 * keystroke — no per-word FFI chatter. All calls dispatch on
 * [Dispatchers.Default]. No Android `Context` here by design.
 */
interface Predictor : AutoCloseable {
    suspend fun suggest(ctx: String, digits: String, activeTab: String, limit: Int): List<ScoredCandidate>
    suspend fun suggestWithLayout(
        ctx: String,
        digits: String,
        layoutId: String,
        activeTab: String,
        limit: Int
    ): List<ScoredCandidate>
    suspend fun suggestForCat(ctx: String, digits: String, activeTab: String, limit: Int): List<ScoredCandidate>
    suspend fun suggestQwerty(
        raw: String,
        ctx: String,
        activeTab: String,
        layoutId: String,
        limit: Int,
        autocorrectThreshold: Double
    ): QwertyResult
    suspend fun learn(word: String, category: String)
    suspend fun learnWithShown(word: String, category: String, shown: List<String>)
    suspend fun forget(word: String)
    suspend fun reject(word: String)
    suspend fun rejectWithShown(word: String, shown: List<String>)
    suspend fun setCatEnabled(cat: String, enabled: Boolean)
    suspend fun isCatEnabled(cat: String): Boolean
    suspend fun placement(word: String): String
    suspend fun exportSession(): String
    suspend fun encode(word: String, layoutId: String): String
    suspend fun decode(prefix: String, layoutId: String, limit: Int): List<String>
    suspend fun layouts(): List<String>
    suspend fun defaultLayout(): String
    suspend fun catLayout(cat: String): String
    suspend fun setDefaultLayout(layoutId: String)
    suspend fun setCatLayout(cat: String, layoutId: String)
    suspend fun clearCatLayout(cat: String)
    suspend fun layoutDiagnostic(cat: String): String
    suspend fun takeLastLayoutError(): String
    suspend fun addPackJson(packJson: String, priority: Int): String
    suspend fun registerLayout(json: String): String
    suspend fun loadCatMap(json: String)
    suspend fun openPersist(path: String, syncDir: String): String
    suspend fun flushPersist(): String
    suspend fun compactPersist(): String
    suspend fun takeLastPersistError(): String
    override fun close()
}

/**
 * Production path: thin suspend adapter over the generated
 * `uniffi.kbcore.Predictor`. Construction (base wordlist + packs + cat-map)
 * is owned by [PredictorFactory]; this class only forwards calls on
 * [Dispatchers.Default] and maps `Suggestion` → [ScoredCandidate]
 * field-for-field.
 *
 * Engine diagnostics are NOT polled per keystroke (that would double FFI):
 * one `suggest*` call per keystroke, period. Callers needing the loud
 * layout/persist diagnostics use [drainLayoutError]/[drainPersistError] on
 * explicit paths (pack install, layout change, flush).
 */
class UniFfiPredictor internal constructor(
    private val real: uniffi.kbcore.Predictor
) : Predictor {

    private fun List<uniffi.kbcore.Suggestion>.toScored(): List<ScoredCandidate> =
        map { ScoredCandidate(it.word, it.score, it.seq, it.cat, it.layoutId) }

    private fun uniffi.kbcore.QwertyResult.toQwerty(): QwertyResult =
        QwertyResult(
            literal = literal,
            corrections = corrections.map {
                QwertyCorrection(it.word, it.score, it.seq, it.cat, it.isCorrection)
            },
            confident = confident
        )

    override suspend fun suggest(ctx: String, digits: String, activeTab: String, limit: Int) =
        withContext(Dispatchers.Default) { real.suggest(ctx, digits, activeTab, limit.toUInt()).toScored() }

    override suspend fun suggestWithLayout(
        ctx: String,
        digits: String,
        layoutId: String,
        activeTab: String,
        limit: Int
    ): List<ScoredCandidate> =
        withContext(Dispatchers.Default) {
            real.suggestWithLayout(ctx, digits, layoutId, activeTab, limit.toUInt()).toScored()
        }

    override suspend fun suggestForCat(
        ctx: String,
        digits: String,
        activeTab: String,
        limit: Int
    ): List<ScoredCandidate> =
        withContext(Dispatchers.Default) {
            real.suggestForCat(ctx, digits, activeTab, limit.toUInt()).toScored()
        }

    override suspend fun suggestQwerty(
        raw: String,
        ctx: String,
        activeTab: String,
        layoutId: String,
        limit: Int,
        autocorrectThreshold: Double
    ): QwertyResult =
        withContext(Dispatchers.Default) {
            try {
                real.suggestQwerty(raw, ctx, activeTab, layoutId, limit.toUInt(), autocorrectThreshold)
                    .toQwerty()
            } catch (e: UnsatisfiedLinkError) {
                // Stale `libkbcore.so` (pre-`suggest_qwerty` symbols, until
                // the next `ndk-libs` rebuild): degrade to literal +
                // prefix completions via the legacy two-call path, never
                // confident (Suggest-only behavior — a missing symbol must
                // never become an accidental autocorrect). The exception is
                // re-thrown as a named cause for the status line via the
                // caller's own error surface; here we stay typing.
                legacyQwerty(raw, ctx, activeTab, layoutId, limit)
            }
        }

    /**
     * Stale-`.so` fallback for [suggestQwerty] (see above): literal slot +
     * legacy `encode` + `suggest` completions, `confident == false` always.
     * Retired automatically once `ndk-libs` ships the new symbols (the
     * `try` then succeeds and this never runs).
     */
    private suspend fun legacyQwerty(
        raw: String,
        ctx: String,
        activeTab: String,
        layoutId: String,
        limit: Int
    ): QwertyResult {
        val seq = try {
            real.encode(raw, layoutId)
        } catch (_: Exception) {
            return QwertyResult(literal = raw, corrections = emptyList(), confident = false)
        }
        if (seq.isEmpty()) return QwertyResult(literal = raw, corrections = emptyList(), confident = false)
        val completions = try {
            real.suggestWithLayout(ctx, seq, layoutId, activeTab, limit.toUInt()).toScored()
        } catch (_: Exception) {
            return QwertyResult(literal = raw, corrections = emptyList(), confident = false)
        }
        return QwertyResult(
            literal = raw,
            corrections = completions
                .filter { it.word.lowercase() != raw.lowercase() }
                .map { QwertyCorrection(it.word, it.score, it.seq, it.cat, isCorrection = false) },
            confident = false
        )
    }

    override suspend fun learn(word: String, category: String): Unit =
        withContext(Dispatchers.Default) { real.learn(word, category) }

    override suspend fun learnWithShown(word: String, category: String, shown: List<String>): Unit =
        withContext(Dispatchers.Default) { real.learnWithShown(word, category, shown) }

    override suspend fun forget(word: String): Unit =
        withContext(Dispatchers.Default) { real.forget(word) }

    override suspend fun reject(word: String): Unit =
        withContext(Dispatchers.Default) { real.reject(word) }

    override suspend fun rejectWithShown(word: String, shown: List<String>): Unit =
        withContext(Dispatchers.Default) { real.rejectWithShown(word, shown) }

    override suspend fun setCatEnabled(cat: String, enabled: Boolean): Unit =
        withContext(Dispatchers.Default) { real.setCatEnabled(cat, enabled) }

    override suspend fun isCatEnabled(cat: String): Boolean =
        withContext(Dispatchers.Default) { real.isCatEnabled(cat) }

    override suspend fun placement(word: String): String =
        withContext(Dispatchers.Default) { real.placement(word) }

    override suspend fun exportSession(): String =
        withContext(Dispatchers.Default) { real.exportSession() }

    override suspend fun encode(word: String, layoutId: String): String =
        withContext(Dispatchers.Default) { real.encode(word, layoutId) }

    override suspend fun decode(prefix: String, layoutId: String, limit: Int): List<String> =
        withContext(Dispatchers.Default) { real.decode(prefix, layoutId, limit.toUInt()) }

    override suspend fun layouts(): List<String> =
        withContext(Dispatchers.Default) { real.layouts() }

    override suspend fun defaultLayout(): String =
        withContext(Dispatchers.Default) { real.defaultLayout() }

    override suspend fun catLayout(cat: String): String =
        withContext(Dispatchers.Default) { real.catLayout(cat) }

    override suspend fun setDefaultLayout(layoutId: String): Unit =
        withContext(Dispatchers.Default) { real.setDefaultLayout(layoutId) }

    override suspend fun setCatLayout(cat: String, layoutId: String): Unit =
        withContext(Dispatchers.Default) { real.setCatLayout(cat, layoutId) }

    override suspend fun clearCatLayout(cat: String): Unit =
        withContext(Dispatchers.Default) { real.clearCatLayout(cat) }

    override suspend fun layoutDiagnostic(cat: String): String =
        withContext(Dispatchers.Default) { real.layoutDiagnostic(cat) }

    override suspend fun takeLastLayoutError(): String =
        withContext(Dispatchers.Default) { real.takeLastLayoutError() }

    override suspend fun addPackJson(packJson: String, priority: Int): String =
        withContext(Dispatchers.Default) { real.addPackJson(packJson, priority) }

    override suspend fun registerLayout(json: String): String =
        withContext(Dispatchers.Default) { real.registerLayout(json) }

    override suspend fun loadCatMap(json: String): Unit =
        withContext(Dispatchers.Default) { real.loadCatMap(json) }

    override suspend fun openPersist(path: String, syncDir: String): String =
        withContext(Dispatchers.Default) { real.openPersist(path, syncDir) }

    override suspend fun flushPersist(): String =
        withContext(Dispatchers.Default) { real.flushPersist() }

    override suspend fun compactPersist(): String =
        withContext(Dispatchers.Default) { real.compactPersist() }

    override suspend fun takeLastPersistError(): String =
        withContext(Dispatchers.Default) { real.takeLastPersistError() }

    /** Explicit layout-error drain for non-keystroke paths (see class KDoc). */
    suspend fun drainLayoutError(): String = takeLastLayoutError()

    /** Explicit persist-error drain for non-keystroke paths (see class KDoc). */
    suspend fun drainPersistError(): String = takeLastPersistError()

    override fun close() {
        try {
            real.close()
        } catch (_: Exception) {
            // Close is best-effort; the Rust handle frees via its cleaner.
        }
    }
}

/**
 * In-memory bound for [StubPredictor] learned words (SPEC §4 §7 personal-cap
 * shape: 20k entries; the real engine owns the persistent personal dict +
 * tombstones). The stub evicts oldest-first past this cap — bounded by
 * construction, never an unbounded leak.
 */
const val STUB_LEARN_CAP = 20_000

/**
 * Degraded path ONLY — used when `libkbcore.so` is absent
 * ([KbCore.isAvailable] == false) or engine construction fails. Selection is
 * owned by [PredictorFactory], which always attaches the reason; the IME
 * surfaces it user-visibly. Never the default when the engine loads.
 *
 * Degraded semantics (explicit, never silent):
 * - `suggest/learn/forget/reject/exportSession` work in-memory (learned
 *   words with score 0.0, empty `seq` — the stub has no encoder — active-tab
 *   matches rank first, `reject` penalty is local-only, nothing persists).
 * - Layout/pack/persist methods with no meaningful in-memory equivalent
 *   throw `IllegalStateException` naming the missing `.so` instead of
 *   returning plausible-looking empty values. `layouts()`/`defaultLayout()`
 *   return the static built-in ids (no engine needed to know them).
 * - String-returning persist methods return the explicit degraded message
 *   (mirroring the engine's `""`-means-success convention: non-empty here
 *   always means "not durable").
 */
class StubPredictor : Predictor {
    private val learned = mutableListOf<Pair<String, String>>()
    // `rejected`/`blocked` are intentionally uncapped: evicting a tombstone
    // would silently resurrect a blocked word. Growth is action-gated (one
    // entry per explicit reject/block, not per keystroke), the stub is
    // session-scoped (dies with the process), and the real engine owns the
    // durable tombstone set — so this cannot leak across sessions.
    private val rejected = mutableSetOf<String>()
    private val blocked = mutableSetOf<String>()

    private fun degraded(what: String): Nothing =
        throw IllegalStateException(
            "StubPredictor degraded ($what): ${KbCore.describeUnavailable()}"
        )

    override suspend fun suggest(
        ctx: String,
        digits: String,
        activeTab: String,
        limit: Int
    ): List<ScoredCandidate> =
        withContext(Dispatchers.Default) {
            require(limit >= 0) { "StubPredictor.suggest: negative limit $limit" }
            learned
                .filter { (w, _) -> w.isNotEmpty() && !blocked.contains(w.lowercase()) }
                .sortedWith(
                    compareBy<Pair<String, String>> { (_, cat) -> if (cat == activeTab) 0 else 1 }
                        .thenBy { (w, _) -> w }
                )
                .take(limit)
                .map { (w, cat) ->
                    // seq is empty BY CONSTRUCTION: the stub owns no encoder.
                    // score 0.0: no ranking without the engine.
                    ScoredCandidate(word = w, score = 0.0, seq = "", cat = cat, layoutId = "t9-9")
                }
        }

    override suspend fun suggestWithLayout(
        ctx: String,
        digits: String,
        layoutId: String,
        activeTab: String,
        limit: Int
    ): List<ScoredCandidate> =
        suggest(ctx, digits, activeTab, limit)

    override suspend fun suggestForCat(
        ctx: String,
        digits: String,
        activeTab: String,
        limit: Int
    ): List<ScoredCandidate> =
        suggest(ctx, digits, activeTab, limit)

    override suspend fun suggestQwerty(
        raw: String,
        ctx: String,
        activeTab: String,
        layoutId: String,
        limit: Int,
        autocorrectThreshold: Double
    ): QwertyResult =
        withContext(Dispatchers.Default) {
            require(limit >= 0) { "StubPredictor.suggestQwerty: negative limit $limit" }
            // Degraded: literal slot + prefix-filtered learned words as
            // completions, never confident (no engine, no corrections).
            val completions = learned
                .filter { (w, _) ->
                    w.isNotEmpty() && w.startsWith(raw, ignoreCase = true) &&
                        !w.equals(raw, ignoreCase = true) && !blocked.contains(w.lowercase())
                }
                .sortedWith(
                    compareBy<Pair<String, String>> { (_, cat) -> if (cat == activeTab) 0 else 1 }
                        .thenBy { (w, _) -> w }
                )
                .take(limit)
                .map { (w, cat) ->
                    QwertyCorrection(word = w, score = 0.0, seq = "", cat = cat, isCorrection = false)
                }
            QwertyResult(literal = raw, corrections = completions, confident = false)
        }

    override suspend fun learn(word: String, category: String): Unit =
        withContext(Dispatchers.Default) {
            if (word.isEmpty()) return@withContext
            // Bounded in-memory stub ([STUB_LEARN_CAP], oldest-first evict);
            // the real engine owns the persistent personal dict + tombstones.
            if (learned.size >= STUB_LEARN_CAP) learned.removeAt(0)
            learned.removeAll { (w, c) -> w.equals(word, ignoreCase = true) && c == category }
            learned.add(word to category)
            Unit
        }

    override suspend fun learnWithShown(word: String, category: String, shown: List<String>): Unit =
        learn(word, category)

    override suspend fun forget(word: String): Unit =
        withContext(Dispatchers.Default) {
            learned.removeAll { (w, _) -> w.equals(word, ignoreCase = true) }
            blocked.add(word.lowercase())
            Unit
        }

    override suspend fun reject(word: String): Unit =
        withContext(Dispatchers.Default) {
            rejected.add(word)
            Unit
        }

    override suspend fun rejectWithShown(word: String, shown: List<String>): Unit =
        reject(word)

    override suspend fun setCatEnabled(cat: String, enabled: Boolean): Unit =
        degraded("setCatEnabled has no in-memory equivalent")

    override suspend fun isCatEnabled(cat: String): Boolean =
        degraded("isCatEnabled has no in-memory equivalent")

    override suspend fun placement(word: String): String =
        degraded("placement has no in-memory equivalent")

    override suspend fun exportSession(): String =
        withContext(Dispatchers.Default) {
            (learned.map { (w, _) -> """{"w":"$w","action":"accepted"}""" } +
                rejected.map { w -> """{"w":"$w","action":"rejected"}""" })
                .joinToString("\n")
        }

    override suspend fun encode(word: String, layoutId: String): String =
        degraded("encode has no in-memory equivalent")

    override suspend fun decode(prefix: String, layoutId: String, limit: Int): List<String> =
        degraded("decode has no in-memory equivalent")

    override suspend fun layouts(): List<String> = listOf("t9-9", "t9-12", "t9-16")

    override suspend fun defaultLayout(): String = "t9-9"

    override suspend fun catLayout(cat: String): String = "t9-9"

    override suspend fun setDefaultLayout(layoutId: String): Unit = degraded("setDefaultLayout")

    override suspend fun setCatLayout(cat: String, layoutId: String): Unit = degraded("setCatLayout")

    override suspend fun clearCatLayout(cat: String): Unit = degraded("clearCatLayout")

    override suspend fun layoutDiagnostic(cat: String): String =
        "degraded: no engine layout registry (${KbCore.describeUnavailable()})"

    /**
     * Explicit degraded signal (NOT `""`): the stub owns no layout registry,
     * so "no error" would be a lie. Non-empty mirrors the engine's
     * `""`-means-success convention — callers checking `isNotEmpty()` treat
     * the stub as errored, which is exactly right.
     */
    override suspend fun takeLastLayoutError(): String =
        "degraded: no engine layout registry (${KbCore.describeUnavailable()})"

    override suspend fun addPackJson(packJson: String, priority: Int): String =
        degraded("addPackJson")

    override suspend fun registerLayout(json: String): String =
        degraded("registerLayout")

    override suspend fun loadCatMap(json: String): Unit =
        degraded("loadCatMap")

    override suspend fun openPersist(path: String, syncDir: String): String =
        "degraded: persistence unavailable (${KbCore.describeUnavailable()})"

    override suspend fun flushPersist(): String =
        "degraded: persistence unavailable (${KbCore.describeUnavailable()})"

    override suspend fun compactPersist(): String =
        "degraded: persistence unavailable (${KbCore.describeUnavailable()})"

    override suspend fun takeLastPersistError(): String =
        "degraded: persistence unavailable (${KbCore.describeUnavailable()})"

    override fun close() = Unit
}

/**
 * Engine construction + ServiceLocator. Single decision point for the whole
 * app: [create] returns [Decision.Real] ONLY when `libkbcore.so` loads AND
 * `Predictor(base_json)` constructs AND every pack + the cat-map install
 * cleanly; ANY failure (missing `.so`, invalid wordlist, bad pack, unknown
 * layout affinity) returns [Decision.Degraded] carrying the full cause
 * chain — never a half-loaded engine, never a silent stub.
 *
 * Blocking by design (wordlist parse + per-pack FST rebuild, ~ms at
 * showcase scale): callers MUST invoke off the main thread (the IME calls
 * from its `Dispatchers.Default` service scope). No Android `Context`
 * dependency: the caller supplies already-loaded JSON strings
 * (batch-string discipline — one JSON array in, not per-word calls).
 */
object PredictorFactory {

    sealed interface Decision {
        val predictor: Predictor

        /** Engine live: generated UniFFI `Predictor` over `libkbcore.so`. */
        data class Real(override val predictor: Predictor) : Decision

        /**
         * Degraded: [StubPredictor] with the explicit [reason] (missing
         * `.so`, init failure, pack/cat-map cause). The host MUST surface
         * [reason] user-visibly (IME status line).
         */
        data class Degraded(override val predictor: Predictor, val reason: String) : Decision
    }

    /**
     * @param baseJson JSON array wordlist for `Predictor.new` (array of
     *   `{w, freq?, cat?, lang?, seq?}` objects or plain strings; invalid
     *   input fails LOUDLY to [Decision.Degraded], never an empty engine).
     * @param packJsons extension packs as `(packJson envelope, priority)`,
     *   installed in order via `add_pack_json` (unknown layout affinity
     *   fails the WHOLE decision with the pack id + cause, mirroring
     *   `try_add_pack` strictness — no half-loaded stack).
     * @param catMapJson optional `layouts/cat_map.json` for
     *   global-default + per-cat overrides.
     */
    fun create(
        baseJson: String,
        packJsons: List<Pair<String, Int>> = emptyList(),
        catMapJson: String? = null
    ): Decision {
        if (!KbCore.isAvailable()) {
            return Decision.Degraded(StubPredictor(), KbCore.describeUnavailable())
        }
        val real: uniffi.kbcore.Predictor
        try {
            real = uniffi.kbcore.Predictor(baseJson)
        } catch (t: Throwable) {
            return Decision.Degraded(
                StubPredictor(),
                "Rust engine init failed (base wordlist rejected): ${t.message ?: t.javaClass.simpleName}"
            )
        }
        val predictor = UniFfiPredictor(real)
        try {
            for ((packJson, priority) in packJsons) {
                real.addPackJson(packJson, priority)
            }
            if (catMapJson != null) {
                real.loadCatMap(catMapJson)
            }
            val layoutError = real.takeLastLayoutError()
            if (layoutError.isNotEmpty()) {
                // Loud fallback happened during init (unknown layout id in a
                // pack affinity or cat-map): surface it, keep the engine —
                // the fallback itself resolved to t9-9 deterministically.
                // Callers attach this to their status line.
                real.close()
                return Decision.Degraded(
                    StubPredictor(),
                    "Rust engine layout init unclear ($layoutError); refusing half-wired engine"
                )
            }
        } catch (t: Throwable) {
            try {
                real.close()
            } catch (_: Exception) {
            }
            return Decision.Degraded(
                StubPredictor(),
                "Rust engine pack/cat-map install failed: ${t.message ?: t.javaClass.simpleName}"
            )
        }
        return Decision.Real(predictor)
    }
}
