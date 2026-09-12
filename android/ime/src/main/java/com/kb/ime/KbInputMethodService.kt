package com.kb.ime

import android.content.Context
import android.inputmethodservice.InputMethodService
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.text.InputType
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.LinearLayout
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.ComposeView
import androidx.lifecycle.Lifecycle
import com.kb.bridge.KbCore
import com.kb.bridge.Predictor
import com.kb.bridge.PredictorFactory
import com.kb.bridge.StubPredictor
import com.kb.plugin.CategoryLoadOutcome
import com.kb.plugin.CategoryRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * Minimal IME skeleton. Prediction runs on Dispatchers.Default via core-bridge;
 * this service only inspects [EditorInfo], commits text, and hosts the views.
 *
 * Engine sharing: all pad encoders feed the SAME [Predictor.suggest]/[learn]
 * interface — T9 codes ([onPadCode]) vs QWERTY raw text ([onQwertyKey]); only
 * the encoder (selected by [qwertyFallback] + [activeLayoutId]) differs.
 * `layout_id` travels as an encoder flag via [suggestWithLayout]/
 * [learnWithLayout] so 9/12/16 seqs stay distinguishable; scoring is shared.
 *
 * Learning is suppressed when [shouldLearnNow] is false: password fields,
 * incognito ([EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING]/privateImeOptions),
 * non-TEXT input classes, email/URI variations, and categories whose manifest
 * sets `privacy.learn=false` (numbers/math).
 *
 * The Tier-1 snippet helper runs under a SEPARATE gate ([snippetNow],
 * plan 12): `math` keeps snippets while staying no-learn (math-exempt
 * rule), `numbers` shows neither, email/URI keeps snippets. The snippet
 * buffer ([SnippetBuffer]) is RAM-only, max 3 entries, ~2min TTL with lazy
 * sweep on each keystroke (no timer thread), wiped on expansion commit,
 * tab switch, IME hide, and incognito/password entry.
 *
 * The IME process stays lean on purpose: no `:sync` dependency (no Room / Work /
 * DataStore here). Personal-dict persistence/flush is owned by the `:app`
 * process; this service holds no sync refs to release.
 */
class KbInputMethodService : InputMethodService() {

    private var cachedInputView: View? = null
    private var isPasswordField: Boolean = false
    private var isIncognito: Boolean = false
    private var isEmailOrUri: Boolean = false
    private var predictionEnabled: Boolean = true
    private var inputMode: InputMode = InputMode.TEXT
    /** QWERTY fallback flag; observable so the FAB label recomposes on toggle. */
    private var qwertyFallback by mutableStateOf(false)
    /**
     * Active pad layout id (`t9-9` / `t9-12` / `t9-16`). Encoder flag only —
     * plumbed to [suggestWithLayout]/[learnWithLayout]. Resolved per
     * keystroke-source tab via [LayoutStore] (per-cat override -> global
     * default -> `t9-9`, plan/02); re-resolved in [onCreate],
     * [onStartInputView] (catches Settings changes while alive) and
     * [onCategoryChanged].
     */
    internal var activeLayoutId: String = DEFAULT_LAYOUT_ID
    /** Parsed spec for [activeLayoutId]; source of key roles/symbols. */
    private var activeSpec: LayoutSpecUi? = null
    /** Set by the t16 Shift key; capitalizes the next committed candidate. */
    private var capsNext: Boolean = false
    private val seq = StringBuilder()
    private val qwertyBuffer = StringBuilder()

    /** Active category as canonical asset id (`words`, `ne`, …).
     * Legacy display labels `EN`/`NE` still map (compat); consults manifest learn flags. */
    private var activeAssetId: String = "words"
    private var learnByCategory: Map<String, Boolean> = DEFAULT_LEARN_FLAGS

    /** Live candidates fed by [Predictor.suggest]; read by the Compose strip. */
    private var liveCandidates by mutableStateOf(emptyList<String>())

    /**
     * Live Tier-1 snippet palette ([SnippetItem]s, plan 07 second zone below
     * the top-3 hits). Fed from [snippetBuffer] follow-tokens + static
     * [SnippetPalettes.staticSnippets]; empty unless [snippetNow].
     */
    internal var liveSnippets by mutableStateOf(emptyList<SnippetItem>())

    /**
     * Ephemeral 2-min snippet buffer (plan 07:12, 12:57). RAM-only: never
     * SQLite, never sync, never feeds rank weights. Field (not local) so
     * tests can pre-load triggers via [snippetBuffer].
     */
    internal val snippetBuffer = SnippetBuffer()

    /** Live pad layout id; read by [ImeScreen]'s toggle, written by [setPadLayout]. */
    private var liveLayoutId by mutableStateOf(DEFAULT_LAYOUT_ID)

    /** Live tab label for [ImeScreen]'s per-tab pad caption. */
    private var liveTabLabel by mutableStateOf("words")

    /**
     * Live tab strip: built-ins + enabled custom categories
     * ([CustomPackStore]), `★personal` pinned last. Disabled customs are
     * absent — that IS the hide-tab mechanism. Refreshed in [onCreate]
     * and [onStartInputView] so Settings changes apply without killing
     * the IME; a corrupt custom pack keeps the built-ins and names the
     * cause on the status line instead of breaking the strip.
     */
    internal var liveTabs by mutableStateOf(DEFAULT_CATEGORIES)

    // -- Plan 01 gesture state (all read by ImeScreen overlays). --
    private var gestureThresholds: GestureThresholds = GestureThresholds()
    private var allowTwelveKeyFlings: Boolean = false
    private var flingsAllowedAT: Boolean = true
    private var gestureError by mutableStateOf<String?>(null)
    private var deletePreviewWords by mutableStateOf(0)
    private var undoAvailable by mutableStateOf(false)
    private var symbolsOptions by mutableStateOf(emptyList<String>())
    private var symbolsTitle by mutableStateOf("")
    /**
     * Long-press placement popup (plan/08): null = hidden. The info text
     * always carries the explicit engine-availability notes (live
     * `placement()` / `set_cat_enabled` are not UniFFI-exported — see
     * [StackEngineCaps]), so the popup never implies a live reorder.
     */
    private var placementPopup by mutableStateOf<PlacementState?>(null)
    private var undoText: String = ""
    private var lastCommitWord: String? = null
    private var lastCommitTs: Long = 0L
    private val mainHandler = Handler(Looper.getMainLooper())
    private var undoExpiry: Runnable? = null
    private lateinit var gestureLog: GestureLog

    /** Current (category, layout) gesture policy; recomputed per gesture. */
    private fun gesturePolicy(): CategoryGesturePolicy =
        policyForCategory(activeAssetId, activeLayoutId, allowTwelveKeyFlings)

    private fun gateFor(policy: CategoryGesturePolicy): PadGestureDetector.FlingGate =
        PadGestureDetector.FlingGate(
            left = policy.flingLeftEnabled,
            up = policy.flingUpEnabled,
            right = policy.flingRightEnabled,
            down = policy.flingDownEnabled
        )

    private fun effectiveThresholds(): GestureThresholds =
        if (activeLayoutId == "t9-12" && allowTwelveKeyFlings) {
            gestureThresholds.scaledForTwelveKey()
        } else {
            gestureThresholds
        }

    private fun reportGestureError(message: String) {
        gestureError = message
        try {
            gestureLog.record("pad", "error", "status", message)
        } catch (_: Exception) {
        }
    }

    /** Service-scoped scope for suggest/learn; cancelled in [onDestroy]. */
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /**
     * Lifecycle owner for the [ComposeView] strip. `InputMethodService`
     * windows provide no view-tree owners; without them the strip crashes
     * on attach (`ViewTreeLifecycleOwner not found`, then
     * `ViewTreeSavedStateRegistryOwner` — both caught on the first
     * on-device smoke; the keyboard could never open). Implemented in
     * [ImeLifecycle] (Java — see its KDoc for why), which supplies all
     * three owners Compose requires (lifecycle + saved-state registry +
     * view-model store). Driven by the service lifecycle: CREATED in
     * [onCreate], RESUMED in [onStartInputView], PAUSED in
     * [onFinishInputView], DESTROYED in [onDestroy].
     */
    private val imeLifecycleOwner = ImeLifecycle()

    /**
     * Injected predictor. Framework-instantiated services can't take
     * constructor params, so this is field-injected: [initEngine] assigns
     * the [PredictorFactory] decision ([UniFfiPredictor] default,
     * [StubPredictor] ONLY as the explicit degraded path with [engineStatus]
     * set). Tests assign a fake directly. Null until [initEngine] finishes;
     * call sites treat null as "engine loading" (the status line says so)
     * and retry on the next keystroke — never a hard-coded default engine.
     */
    internal var predictor: Predictor? = null

    /**
     * User-visible engine state. Null = live Rust engine. Non-null =
     * loading or degraded-with-cause ([KbCore.isAvailable] == false or init
     * failure; see `PredictorFactory.Decision.Degraded.reason`). Rendered in
     * the IME status line — the degraded state NEVER degrades silently.
     */
    internal var engineStatus by mutableStateOf<String?>(null)

    /** Last suggest result (words): caller-observed `shown` for learn/reject. */
    private var lastShown: List<String> = emptyList()

    override fun onCreate() {
        super.onCreate()
        imeLifecycleOwner.handle(Lifecycle.Event.ON_CREATE)
        gestureLog = GestureLog(this, onError = { gestureError = it })
        // Restore the user's persisted layout (per-tab resolve; the
        // default tab is `words`) before first inflate.
        activeLayoutId = LayoutStore.layoutForCat(this, activeAssetId)
        liveLayoutId = activeLayoutId
        liveTabLabel = activeAssetId
        activeSpec = loadLayoutSpec(this, activeLayoutId)
        reloadGesturePrefs()
        flingsAllowedAT = AccessibilityGates.evaluate(this).flingsAllowed
        refreshTabs()
        // Warm category learn-flags + native-lib probe off the main thread.
        serviceScope.launch {
            val ids = try {
                CategoryRegistry.listIds(assets).ifEmpty { DEFAULT_LEARN_FLAGS.keys.toList() }
            } catch (_: Exception) {
                DEFAULT_LEARN_FLAGS.keys.toList()
            }
            val manifests = try {
                CategoryRegistry.loadManifestsStrict(assets, ids)
            } catch (_: Exception) {
                CategoryLoadOutcome(emptyList(), emptyList())
            }
            if (manifests.loaded.isNotEmpty()) {
                learnByCategory = manifests.loaded.associate { it.id to it.learn }
            }
            if (manifests.errors.isNotEmpty()) {
                // A missing/corrupt tab manifest is user-visible state, never
                // a silent skip: the status line names the tab + cause.
                val missing = manifests.errors.joinToString("; ") { "${it.id} (${it.cause})" }
                val msg = "Category manifests unreadable: $missing"
                android.util.Log.e("KbIME", msg)
                gestureError = msg
            }
            @Suppress("unused")
            val nativeAvailable = KbCore.isAvailable()
        }
        initEngine()
    }

    /**
     * Engine bring-up off the main thread (asset I/O + wordlist parse +
     * per-pack FST rebuild). Merges `words[]` of [BASE_PACK_ASSETS] into one
     * base JSON array for `Predictor.new`, installs [EXTENSION_PACK_ASSETS]
     * envelopes (`assets/categories/<id>.json`, words embedded) via
     * `add_pack_json` in priority order, then loads `layouts/cat_map.json`
     * and calls [PredictorFactory.create] (no per-word FFI — batch strings).
     * Real engine → [engineStatus] = null. ANY failure (missing asset,
     * malformed pack, missing `.so`, non-empty `takeLastLayoutError` after
     * install) → degraded stub + [engineStatus] carries the cause for the
     * status line + gesture log. Pack/cat-map failures are never silent:
     * [PredictorFactory] fails the whole decision with the pack id + cause.
     */
    private fun initEngine() {
        engineStatus = "Engine loading…"
        val t0 = System.currentTimeMillis()
        serviceScope.launch {
            val decision = try {
                val baseJson = loadBaseWordlist()
                val packs = loadExtensionPacks()
                val catMap = loadAssetTextOrNull("layouts/cat_map.json")
                PredictorFactory.create(baseJson, packs, catMap)
            } catch (e: Exception) {
                PredictorFactory.Decision.Degraded(
                    StubPredictor(),
                    "Engine asset load failed: ${e.message}"
                )
            }
            predictor = decision.predictor
            when (decision) {
                is PredictorFactory.Decision.Real -> {
                    engineStatus = null
                    android.util.Log.i(
                        "KbIME",
                        "Rust engine live in ${System.currentTimeMillis() - t0}ms " +
                            "(KbCore.available=${KbCore.isAvailable()})"
                    )
                }
                is PredictorFactory.Decision.Degraded -> {
                    engineStatus = decision.reason
                    android.util.Log.e("KbIME", "Engine degraded: ${decision.reason}")
                    try {
                        gestureLog.record("engine", "init", "degraded", decision.reason)
                    } catch (_: Exception) {
                    }
                }
            }
        }
    }

    /** Merged `words[]` of the base packs → single base JSON array string. */
    private fun loadBaseWordlist(): String {
        val merged = StringBuilder("[")
        var first = true
        for (path in BASE_PACK_ASSETS) {
            val text = try {
                assets.open(path).bufferedReader().use { it.readText() }
            } catch (e: Exception) {
                throw IllegalStateException("IME asset missing: $path (${e.message})")
            }
            val words = try {
                org.json.JSONObject(text).getJSONArray("words")
            } catch (e: Exception) {
                throw IllegalStateException("Pack $path: missing/invalid words[] (${e.message})")
            }
            for (i in 0 until words.length()) {
                if (!first) merged.append(",")
                first = false
                merged.append(words.get(i).toString())
            }
        }
        merged.append("]")
        if (first) throw IllegalStateException("Base wordlist empty: $BASE_PACK_ASSETS")
        return merged.toString()
    }

    private fun loadAssetTextOrNull(path: String): String? =
        try {
            assets.open(path).bufferedReader().use { it.readText() }
        } catch (_: Exception) {
            null
        }

    /**
     * Extension pack envelopes (`assets/categories/<id>.json`, `words`
     * embedded by `scripts/sync_android_assets.py`) as
     * `(packJson, priority)` for `PredictorFactory.create`, PLUS enabled
     * user custom packs ([CustomPackStore]) in stack order.
     *
     * Plan/08 ordering: stored priorities ([PackOrderStore]) override the
     * compiled defaults and disabled packs are SKIPPED (the engine has no
     * live `set_cat_enabled` FFI — see [StackEngineCaps] — so enable state
     * applies at init). The whole list installs priority-desc, id-asc, so
     * on-device tie-breaks match the settings strip exactly. A missing
     * asset or an out-of-range custom priority throws loudly — the factory
     * turns it into a degraded engine WITH the cause (status line), never
     * a half-loaded stack. Rust-side `add_pack_json` parses these
     * envelopes as `PackFile` (extra manifest fields ignored); a malformed
     * pack or unknown layout affinity fails the whole factory decision the
     * same loud way. A corrupt custom pack fails the same way WITH its id
     * (Settings validates at save, so this is defense-in-depth, not the
     * primary check).
     */
    private fun loadExtensionPacks(): List<Pair<String, Int>> {
        val prios = PackOrderStore.priorityMap(this)
        val enabled = PackOrderStore.enabledMap(this)
        val staged = mutableListOf<Triple<String, Int, String>>()
        for ((path, defaultPrio) in EXTENSION_PACK_ASSETS) {
            val id = path.substringAfterLast("/").removeSuffix(".json")
            val priority = prios[id] ?: defaultPrio
            if (enabled[id] != false) {
                val text = try {
                    assets.open(path).bufferedReader().use { it.readText() }
                } catch (e: Exception) {
                    throw IllegalStateException("IME asset missing: $path (${e.message})")
                }
                staged.add(Triple(text, priority, id))
            }
        }
        try {
            for (pack in CustomPackStore.enabledPacks(this)) {
                if (pack.packJson.isBlank()) {
                    throw IllegalStateException("Custom pack \"${pack.id}\": envelope empty")
                }
                DictStackOrder.validatePriority(pack.priority)?.let {
                    throw IllegalStateException("Custom pack \"${pack.id}\": $it")
                }
                staged.add(Triple(pack.packJson, pack.priority, pack.id))
            }
        } catch (e: IllegalStateException) {
            throw e
        } catch (e: Exception) {
            throw IllegalStateException("Custom packs unreadable (${e.message})")
        }
        return staged
            .sortedWith(compareByDescending<Triple<String, Int, String>> { it.second }.thenBy { it.third })
            .map { it.first to it.second }
    }

    /**
     * Reloads Gesture Tuning prefs (thresholds + 12-key opt-in). Called from
     * [onCreate] and [onStartInputView] so Tuning changes apply without
     * killing the IME. Corrupt prefs surface via [gestureError], never
     * silently: thresholds fall back to ship defaults for this session.
     */
    private fun reloadGesturePrefs() {
        try {
            gestureThresholds = GestureTuningStore.loadThresholds(this)
        } catch (e: Exception) {
            gestureThresholds = GestureThresholds()
            reportGestureError("Gesture tuning unreadable, using defaults: ${e.message}")
        }
        allowTwelveKeyFlings = GestureTuningStore.allowTwelveKeyFlings(this)
    }

    override fun onCreateInputView(): View {
        cachedInputView?.let { return it }
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
        }
        val strip = ComposeView(this).apply {
            setContent {
                ImeScreen(
                    candidates = liveCandidates,
                    categories = liveTabs,
                    onCandidatePicked = { commitCandidate(it) },
                    onExpandAll = { gestureLog.record("bar", "tap-expand", "expand-all") },
                    onToggleQwerty = {
                        qwertyFallback = !qwertyFallback
                        gestureLog.record("qwerty", "tap-fab", if (qwertyFallback) "show-qwerty" else "show-pad")
                        refreshPad(root)
                    },
                    onCategoryChanged = { onCategoryChanged(it) },
                    onTabLongPress = { onTabLongPress(it) },
                    placement = placementPopup,
                    onPlacementMoveUp = { onPlacementMove(-1) },
                    onPlacementMoveDown = { onPlacementMove(1) },
                    onPlacementToggle = { onPlacementToggle() },
                    onPlacementDismiss = { onPlacementDismiss() },
                    activeLayoutId = liveLayoutId,
                    activeTabLabel = liveTabLabel,
                    onLayoutChanged = { setTabLayout(it) },
                    symbolsOptions = symbolsOptions,
                    symbolsTitle = symbolsTitle,
                    onSymbolPick = { onSymbolPick(it) },
                    onSymbolsDismiss = {
                        symbolsOptions = emptyList()
                        symbolsTitle = ""
                    },
                    deletePreviewWords = deletePreviewWords,
                    undoAvailable = undoAvailable,
                    onUndo = { undoDelete() },
                    onAcceptFirst = { acceptTopCandidate("strip-button") },
                    onFling = { zone, gesture, action -> gestureLog.record(zone, gesture, action) },
                    fallbackActionsVisible = !flingsAllowedAT,
                    fallbackActions = FallbackActions(
                        onDelete = { onPadFlingDelete(1) },
                        onSpace = { onPadSpace() },
                        onAcceptFirst = { acceptTopCandidate("fallback") },
                        onToggleQwerty = {
                            qwertyFallback = !qwertyFallback
                            refreshPad(root)
                        },
                        onSym = { showGenericSymbolsSheet() }
                    ),
                    statusLine = engineStatus ?: gestureError,
                    onError = { gestureError = it },
                    qwertyActive = qwertyFallback,
                    snippets = liveSnippets,
                    onSnippetPick = { commitSnippet(it) }
                )
            }
        }
        val pad = makePad()
        pad.tag = "pad"
        root.addView(strip)
        root.addView(pad)
        // The service window owns no lifecycle: attach ours before the
        // window attaches (ComposeView resolves it in onAttachedToWindow).
        ImeLifecycle.attachTo(root, imeLifecycleOwner)
        cachedInputView = root
        return root
    }

    override fun onStartInput(attribute: EditorInfo?, restarting: Boolean) {
        super.onStartInput(attribute, restarting)
        isPasswordField = attribute?.isPasswordField() ?: false
        isIncognito = attribute.isIncognitoRequest()
        isEmailOrUri = attribute.isEmailOrUri()
        inputMode = attribute.resolveInputMode()
        // Only free TEXT gets prediction; NUMBER/PHONE/DATETIME commit raw.
        predictionEnabled = inputMode == InputMode.TEXT
        // Split-gate hygiene (plan 12:62): a password field or an incognito
        // session wipes the snippet buffer on entry (no TTL carryover) and
        // hides the strip. Exit needs no action — the buffer is already gone.
        if (isPasswordField || isIncognito) {
            snippetBuffer.clear()
            liveSnippets = emptyList()
        }
        seq.clear()
        qwertyBuffer.clear()
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        imeLifecycleOwner.handle(Lifecycle.Event.ON_RESUME)
        // Layout already reflects inputMode via predictionEnabled (raw commit when
        // false). Pad swaps only happen on explicit QWERTY toggle to avoid desync.
        // Re-read Tuning + AT state so Settings changes apply without IME restart.
        reloadGesturePrefs()
        val gate = AccessibilityGates.evaluate(this)
        flingsAllowedAT = gate.flingsAllowed
        // Fresh input view: AT status is authoritative; stale errors clear.
        gestureError = if (!gate.flingsAllowed) gate.reason else null
        // Pick up Settings → Layout changes (global or per-tab) made while
        // the IME was alive: re-resolve for the current tab. Same for
        // Settings → custom categories (new/enabled/disabled tabs appear
        // without killing the IME; the engine itself picks them up on its
        // next init — a restart applies ranking for freshly added packs).
        val resolved = LayoutStore.layoutForCat(this, activeAssetId)
        if (resolved != activeLayoutId) setPadLayout(resolved)
        // Settings → dict-stack order (plan/08): new/enabled/disabled tabs
        // appear without killing the IME; the engine itself picks up
        // priority/enable changes on its next init (a restart applies them
        // to ranking — live reorder is not UniFFI-exported, see
        // [StackEngineCaps]).
        refreshTabs()
    }

    /**
     * Re-resolve the visible tab strip from the persisted stack order
     * (plan/08): numbers pinned first, base words/ne, enabled movable
     * packs in priority order, ★personal pinned last. A corrupt store
     * keeps the built-in strip and names the cause on the status line
     * instead of breaking the keyboard.
     */
    private fun refreshTabs() {
        liveTabs = try {
            DictStackOrder.stripOrder(DictStackOrder.assembleSlots(this))
        } catch (e: Exception) {
            android.util.Log.e("KbIME", "Stack order unreadable, using built-ins", e)
            gestureError = "Dictionary stack order unreadable: ${e.message}"
            DEFAULT_CATEGORIES
        }
    }

    override fun onFinishInput() {
        // Flush point: finish composing so no stale text leaks into the next
        // field, clear encoder buffers + candidates. (Personal-dict persistence
        // is owned by the :app process; the IME holds no sync/DB refs.)
        try {
            currentInputConnection?.finishComposingText()
        } catch (_: Exception) {
            // Editor gone; buffers below still get cleared.
        }
        seq.clear()
        qwertyBuffer.clear()
        liveCandidates = emptyList()
        // IME hide wipes snippet state (plan 12:62): nothing carries over.
        snippetBuffer.clear()
        liveSnippets = emptyList()
        super.onFinishInput()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        imeLifecycleOwner.handle(Lifecycle.Event.ON_PAUSE)
        try {
            currentInputConnection?.finishComposingText()
        } catch (_: Exception) {
        }
        seq.clear()
        qwertyBuffer.clear()
        snippetBuffer.clear()
        liveSnippets = emptyList()
        super.onFinishInputView(finishingInput)
    }

    override fun onUpdateSelection(
        oldSelStart: Int,
        oldSelEnd: Int,
        newSelStart: Int,
        newSelEnd: Int,
        candidatesStart: Int,
        candidatesEnd: Int
    ) {
        super.onUpdateSelection(oldSelStart, oldSelEnd, newSelStart, newSelEnd, candidatesStart, candidatesEnd)
        // Cursor moved by the user/editor. No cached offsets kept; the next
        // keystroke re-reads getTextBeforeCursor() for the bigram prev word.
    }

    override fun onDestroy() {
        serviceScope.cancel()
        imeLifecycleOwner.handle(Lifecycle.Event.ON_DESTROY)
        try {
            predictor?.close()
        } catch (_: Exception) {
        }
        cachedInputView = null
        super.onDestroy()
    }

    internal fun switchToNextIme() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            try {
                switchToNextInputMethod(false)
                return
            } catch (_: Exception) {
                // Fall through to the picker.
            }
        }
        (getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager)
            ?.showInputMethodPicker()
    }

    /**
     * Enter key: honor the editor's IME_ACTION_* when set (DONE/GO/NEXT/SEARCH/
     * SEND via performEditorAction), else commit a newline.
     */
    internal fun handleEnter(): Boolean {
        val ic = currentInputConnection ?: return false
        val action = currentInputEditorInfo?.imeOptions?.and(EditorInfo.IME_MASK_ACTION)
            ?: EditorInfo.IME_ACTION_UNSPECIFIED
        return try {
            if (action != EditorInfo.IME_ACTION_NONE && action != EditorInfo.IME_ACTION_UNSPECIFIED) {
                ic.performEditorAction(action)
            } else {
                ic.commitText("\n", 1)
            }
            true
        } catch (_: Exception) {
            false
        }
    }

    /**
     * Layout-driven pad input. Punctuation-only keys (e.g. t9 `1`) commit
     * their first symbol directly (tap-cycle TBD); all other text codes
     * extend the seq fed to the shared Predictor path with [activeLayoutId].
     *
     * Per-category tap deltas (plan 01):
     * - numbers (no prediction): tap commits the digit raw, no seq.
     * - emoji: tap appends and immediately commits the top match (tap
     *   commits immediately; full emoji label remap arrives with plan 02
     *   layout resolve — until then the T9 keyword path is reused and the
     *   reuse is logged once per session batch, not silently assumed).
     */
    internal fun onPadCode(code: String) {
        android.util.Log.d(
            "KbIME",
            "onPadCode code=$code tab=$activeAssetId layout=$activeLayoutId seq=$seq"
        )
        val key = activeSpec?.keyByCode(code)
        val symbols = key?.symbols.orEmpty()
        if (symbols.isNotEmpty() && symbols.none { it.isLetter() }) {
            commitText(symbols.substring(0, 1))
            seq.clear()
            return
        }
        val policy = gesturePolicy()
        if (!policy.predictionEnabled) {
            // Numbers: digit-commit, no prediction state.
            commitRaw(code.filter { it.isDigit() }.takeIf { it.isNotEmpty() } ?: code)
            return
        }
        if (!predictionEnabled) {
            // NUMBER/PHONE/DATETIME: raw code commit, no prediction.
            currentInputConnection?.commitText(code, 1)
            return
        }
        seq.append(code)
        // Show composing text so the field reflects in-progress T9 seq.
        currentInputConnection?.setComposingText(seq.toString(), 1)
        // Lazy snippet TTL sweep + strip refresh on every keystroke.
        refreshSnippetStrip()
        if (policy.tapCommitsImmediately && activeAssetId == "emoji") {
            commitTopMatchNow(seq.toString())
        } else {
            refreshSuggestions(seq.toString())
        }
    }

    /**
     * Emoji tap-commits-immediately: suggest off the current seq and commit
     * the top match right away; keeps composing when nothing matches (with
     * an explicit log entry — an empty emoji tap is data, not silence).
     */
    private fun commitTopMatchNow(query: String) {
        val snapshot = query
        val layoutId = activeLayoutId
        val tab = activeAssetId
        serviceScope.launch {
            val p = predictor
            if (p == null) {
                gestureLog.record("pad", "tap", "emoji-no-engine", snapshot)
                liveCandidates = emptyList()
                return@launch
            }
            val prev = prevWord()
            val result = try {
                suggestWithLayout(p, snapshot, prev, layoutId, tab, limit = EXPAND_LIMIT)
            } catch (e: Exception) {
                reportGestureError("Emoji lookup failed: ${e.message}")
                emptyList()
            }
            if (result.isNotEmpty()) {
                lastShown = result.map { it.word }
                liveCandidates = result.map { it.word }
                commitCandidate(result.first().word)
            } else {
                gestureLog.record("pad", "tap", "emoji-no-match", snapshot)
                liveCandidates = emptyList()
            }
        }
    }

    /** Space key (t9-12 `0`, t9-16 `0`/`wx␣`): accept top candidate, else space. */
    internal fun onPadSpace() {
        val top = liveCandidates.firstOrNull()
        if (seq.isNotEmpty() && top != null) {
            commitCandidate(top)
            return
        }
        if (qwertyBuffer.isNotEmpty()) {
            commitCandidate(qwertyBuffer.toString())
            return
        }
        seq.clear()
        commitText(" ")
    }

    /** Sym key: commits the key's first symbol; breaks the word, no learn. */
    internal fun onPadSym(symbols: String) {
        val glyph = symbols.firstOrNull()?.toString() ?: return
        seq.clear()
        qwertyBuffer.clear()
        commitText(glyph)
    }

    /**
     * Control keys by code: `mode`/`#` (t9-9) cycles 9→12→16; `*` (t9-9
     * back/cycle) deletes; t16 `C`/shift arms [capsNext], `D`/lang switches
     * IME. Unknown controls are no-ops (symbols-sheet TBD).
     */
    internal fun onPadControl(code: String) {
        when (code) {
            "#", "mode" -> cyclePadLayout()
            "*" -> deleteLast()
            "C", "shift" -> capsNext = true
            "D", "lang" -> switchToNextIme()
            else -> Unit
        }
    }

    /** Cycles t9-9 → t9-12 → t9-16 and swaps the pad in place. */
    internal fun cyclePadLayout() {
        val next = when (activeLayoutId) {
            "t9-9" -> "t9-12"
            "t9-12" -> "t9-16"
            else -> "t9-9"
        }
        setTabLayout(next)
    }

    internal fun setPadLayout(layoutId: String) {
        if (layoutId !in SUPPORTED_LAYOUT_IDS) return
        activeLayoutId = layoutId
        liveLayoutId = layoutId
        activeSpec = loadLayoutSpec(this, layoutId)
        seq.clear()
        (cachedInputView as? LinearLayout)?.let { refreshPad(it) }
    }

    /**
     * Pad-size toggle target: persists [layoutId] as the per-tab override
     * for the current tab ([LayoutStore.setCatLayout] throws loudly on
     * unknown ids) and swaps the pad. The global default is owned by
     * Settings → Layout; per-tab picks win over it (plan/02).
     */
    internal fun setTabLayout(layoutId: String) {
        LayoutStore.setCatLayout(this, activeAssetId, layoutId)
        setPadLayout(layoutId)
    }

    /**
     * QWERTY encoder: raw committed letters feed the SAME [Predictor.suggest]
     * as T9 digits — only this encoder differs ([qwertyFallback] selects it).
     */
    internal fun onQwertyKey(text: String) {
        if (text == " ") {
            // Space breaks the word: accept the buffer (learn path reused).
            if (qwertyBuffer.isNotEmpty()) commitCandidate(qwertyBuffer.toString()) else commitText(" ")
            return
        }
        if (!predictionEnabled) {
            currentInputConnection?.commitText(text, 1)
            return
        }
        qwertyBuffer.append(text)
        currentInputConnection?.setComposingText(qwertyBuffer.toString(), 1)
        // Lazy snippet sweep; the html/math static palette filters on this prefix.
        refreshSnippetStrip()
        refreshQwertySuggestions(qwertyBuffer.toString())
    }

    internal fun onQwertyEnter() {
        if (qwertyBuffer.isNotEmpty()) commitCandidate(qwertyBuffer.toString())
        handleEnter()
    }

    /**
     * Single-fetch suggest: ONE engine call per keystroke (batch-string
     * discipline — no per-word FFI), fetching [EXPAND_LIMIT] candidates.
     * The strip pages [STRIP_LIMIT] inline from that same list; expand-all
     * shows all [EXPAND_LIMIT]. [activeAssetId] selects the category boost,
     * [activeLayoutId] the pad that encoded the seq.
     */
    private fun refreshSuggestions(query: String, limit: Int = EXPAND_LIMIT) {
        val snapshot = query
        val layoutId = activeLayoutId
        val tab = activeAssetId
        serviceScope.launch {
            val p = predictor
            // Engine still loading (status line says so): skip this
            // keystroke's fetch; the next keystroke retries.
            if (p == null) return@launch
            val prev = prevWord()
            val result = try {
                suggestWithLayout(p, snapshot, prev, layoutId, tab, limit)
            } catch (e: Exception) {
                reportGestureError("Suggest failed: ${e.message}")
                emptyList()
            }
            android.util.Log.d(
                "KbIME",
                "suggest seq=$snapshot tab=$tab layout=$layoutId -> ${result.size} " +
                    "top=${result.firstOrNull()?.word}"
            )
            lastShown = result.map { it.word }
            liveCandidates = result.map { it.word }
        }
    }

    /**
     * QWERTY encoder: raw letters → engine `encode` under the active pad →
     * digit seq → shared suggest path. Only this encoder differs from T9
     * (two FFI calls per keystroke: encode + suggest; still no per-word
     * chatter). Unencodable buffers clear the strip explicitly.
     */
    private fun refreshQwertySuggestions(raw: String) {
        val snapshot = raw
        val layoutId = activeLayoutId
        val tab = activeAssetId
        serviceScope.launch {
            val p = predictor
            if (p == null) return@launch
            val prev = prevWord()
            val seq = try {
                p.encode(snapshot, layoutId)
            } catch (e: Exception) {
                reportGestureError("Encode failed: ${e.message}")
                return@launch
            }
            if (seq.isEmpty()) {
                lastShown = emptyList()
                liveCandidates = emptyList()
                return@launch
            }
            val result = try {
                suggestWithLayout(p, seq, prev, layoutId, tab, EXPAND_LIMIT)
            } catch (e: Exception) {
                reportGestureError("Suggest failed: ${e.message}")
                emptyList()
            }
            lastShown = result.map { it.word }
            liveCandidates = result.map { it.word }
        }
    }

    /** Previous-word context for bigrams; null under no-learn (no harvesting). */
    private fun prevWord(): String? {
        if (isPasswordField || isIncognito) return null
        return try {
            currentInputConnection?.getTextBeforeCursor(CURSOR_CONTEXT_CHARS, 0)?.toString()
                ?.split(Regex("\\s+"))?.lastOrNull()?.takeIf { it.isNotEmpty() }
        } catch (_: Exception) {
            null
        }
    }

    /**
     * Single construction site for both split gates (plan 12:24): the
     * service reads editor/field state, [SnippetGates] owns the frozen
     * truth table. `learnForTab` mirrors the pack manifest
     * `privacy.learn` (numbers/math false).
     */
    private fun gateInput(): GateInput = GateInput(
        password = isPasswordField,
        incognito = isIncognito,
        textClass = predictionEnabled,
        emailOrUri = isEmailOrUri,
        tab = activeAssetId,
        learnForTab = learnByCategory[activeAssetId] ?: true,
    )

    private fun shouldLearnNow(): Boolean = SnippetGates.learnNow(gateInput())

    /**
     * Snippet gate (plan 12:34): like learn but exempt from
     * `privacy.learn=false` (math helpers are the point of the tab) and
     * from the email/URI suppression; `numbers` stays fully blocked.
     */
    internal fun snippetNow(): Boolean = SnippetGates.snippetNow(gateInput())

    /**
     * Recomputes the Tier-1 strip synchronously (pure map lookups, <50ms
     * pattern): live buffer follow-tokens first, then the static palette
     * for the active tab/prefix, capped at 6. Runs on every keystroke path
     * so TTL expiry applies lazily with no timer thread. Empty unless
     * [snippetNow] — a closed gate hides the strip instead of showing
     * stale helpers.
     */
    internal fun refreshSnippetStrip() {
        if (!snippetNow()) {
            liveSnippets = emptyList()
            return
        }
        val follow = snippetBuffer.peek().map {
            SnippetItem(display = it, body = it, source = "buffer")
        }
        val prefix = qwertyBuffer.toString()
        val statics = try {
            SnippetPalettes.staticSnippets(activeAssetId, prefix)
        } catch (e: Exception) {
            reportGestureError("Snippet palette failed: ${e.message}")
            emptyList()
        }
        liveSnippets = (follow + statics).take(6)
    }

    /**
     * Expands a Tier-1 snippet at the cursor (plan 07 second zone).
     *
     * Commits [item.body] verbatim, parks the caret [SnippetItem.cursorBack]
     * chars left via relative DPAD_LEFT presses (no absolute editor offsets
     * stored — no tabstop field exists), then wipes the buffer
     * (delete-after-expand). The committed text is ordinary committed text:
     * it joins the 5s reject window and the learn path consults
     * [shouldLearnNow] exactly like any other commit; any present-or-future
     * clipboard capture must consult
     * [SnippetGates.mayCaptureClipboard] first. Every failure surfaces via
     * [gestureError] on the status line — never silent.
     */
    internal fun commitSnippet(item: SnippetItem) {
        if (!snippetNow()) {
            reportGestureError("Snippets unavailable here (numbers, password, incognito, or non-text field)")
            return
        }
        val ic = currentInputConnection ?: run {
            reportGestureError("Snippet expand failed: no input connection")
            return
        }
        try {
            ic.finishComposingText()
            ic.commitText(item.body, 1)
        } catch (e: Exception) {
            reportGestureError("Snippet expand failed: ${e.message}")
            return
        }
        seq.clear()
        qwertyBuffer.clear()
        liveCandidates = emptyList()
        lastShown = emptyList()
        // Delete-after-expand: the helper dies with its expansion.
        snippetBuffer.consume()
        if (item.cursorBack > 0) {
            try {
                repeat(item.cursorBack) { sendDownUpKeyEvents(KeyEvent.KEYCODE_DPAD_LEFT) }
            } catch (e: Exception) {
                reportGestureError("Snippet committed, caret parking failed: ${e.message}")
            }
        }
        lastCommitWord = item.body
        lastCommitTs = System.currentTimeMillis()
        gestureLog.record("snippet", "expand", item.commit.name, item.body)
        if (shouldLearnNow()) {
            val p = predictor
            if (p == null) {
                reportGestureError("Snippet committed, learn dropped (engine loading)")
            } else {
                val layoutId = activeLayoutId
                serviceScope.launch {
                    try {
                        learnWithLayout(p, item.body, activeAssetId, emptyList(), layoutId)
                    } catch (e: Exception) {
                        reportGestureError("Snippet learn failed: ${e.message}")
                    }
                }
            }
        }
        refreshSnippetStrip()
    }

    private fun commitCandidate(word: String) {
        commitCandidateWithTerminator(word, " ")
    }

    /**
     * Commits [word] + [terminator] and runs the learn path (accept() →
     * learning) when [shouldLearnNow]. Tracks the commit for the 5s
     * reject window (SPEC §2: deleting a committed word within 5s, or
     * picking another candidate, records a reject).
     */
    private fun commitCandidateWithTerminator(word: String, terminator: String) {
        val layoutId = activeLayoutId
        val text = if (capsNext) word.replaceFirstChar { it.uppercase() } else word
        capsNext = false
        commitText("$text$terminator")
        seq.clear()
        qwertyBuffer.clear()
        liveCandidates = emptyList()
        lastCommitWord = word
        lastCommitTs = System.currentTimeMillis()
        // Fire-and-forget learn on Dispatchers.Default inside Predictor,
        // gated on password + incognito + input class + category.
        // Category is the pack id (activeAssetId), NOT the ctx-prev word;
        // `shown` is the caller-observed last suggest result (plan/05 #5:
        // O(1) personal op, no suggest-in-lock).
        if (shouldLearnNow()) {
            val p = predictor
            val shown = lastShown
            lastShown = emptyList()
            if (p == null) {
                reportGestureError("Learn dropped (engine loading): $word")
            } else {
                serviceScope.launch {
                    try {
                        learnWithLayout(p, word, activeAssetId, shown, layoutId)
                    } catch (e: Exception) {
                        reportGestureError("Learn failed: ${e.message}")
                    }
                }
            }
        }
        // Ephemeral follow-token feed (plan 07:12): a committed trigger
        // (`if` in js…) arms 2-3 syntax tokens for ~2min. Gated by
        // snippetNow (never in numbers/password/incognito/non-text);
        // failures are impossible here (pure lookup) so no error path.
        if (snippetNow()) {
            val follow = SnippetPalettes.followTokens(activeAssetId, word)
            if (follow.isNotEmpty()) {
                try {
                    snippetBuffer.offer(word, follow)
                } catch (e: IllegalArgumentException) {
                    reportGestureError("Snippet feed rejected: ${e.message}")
                }
            }
        }
        refreshSnippetStrip()
    }

    /** Commits raw text: no prediction state, no learning (symbols/digits). */
    private fun commitRaw(text: String) {
        seq.clear()
        qwertyBuffer.clear()
        liveCandidates = emptyList()
        commitText(text)
    }

    private fun commitText(text: String) {
        currentInputConnection?.commitText(text, 1)
    }

    /** Wired to pad ⌫ keys (all pad sizes + QWERTY bottom row). */
    internal fun deleteLast() {
        val ic = currentInputConnection ?: return
        if (seq.isNotEmpty()) {
            seq.deleteCharAt(seq.length - 1)
            if (seq.isEmpty()) ic.finishComposingText() else ic.setComposingText(seq.toString(), 1)
            refreshSuggestions(seq.toString())
            return
        }
        if (qwertyBuffer.isNotEmpty()) {
            qwertyBuffer.deleteCharAt(qwertyBuffer.length - 1)
            if (qwertyBuffer.isEmpty()) ic.finishComposingText()
            else ic.setComposingText(qwertyBuffer.toString(), 1)
            refreshQwertySuggestions(qwertyBuffer.toString())
            return
        }
        // Real text deletion: a commit inside the 5s window counts as a
        // reject of that word (SPEC §2 learning rule).
        maybeRejectLastCommit()
        ic.deleteSurroundingText(1, 0)
        // Deletion can also expire helpers: sweep, no timer thread.
        refreshSnippetStrip()
    }

    /** Records a reject when the user deletes within 5s of a commit. */
    private fun maybeRejectLastCommit() {
        val word = lastCommitWord ?: return
        if (System.currentTimeMillis() - lastCommitTs > REJECT_WINDOW_MS) {
            lastCommitWord = null
            return
        }
        lastCommitWord = null
        gestureLog.record("pad", "delete-within-5s", "reject", word)
        val p = predictor
        if (p == null) {
            reportGestureError("Reject dropped (engine loading): $word")
            return
        }
        val shown = lastShown
        serviceScope.launch {
            try {
                p.rejectWithShown(word, shown)
            } catch (e: Exception) {
                reportGestureError("Reject logging failed: ${e.message}")
            }
        }
    }

    // -- Plan 01 Pad gesture actions (flings + long-press + delete-slide). --

    /**
     * Fling `←`: short (1 word, still composing) reverts the last keypress;
     * otherwise deletes [words] words from committed text. Release commits;
     * the deleted text stays restorable via [undoDelete] for 5s.
     */
    internal fun onPadFlingDelete(words: Int) {
        deletePreviewWords = 0
        val policy = gesturePolicy()
        if (!policy.flingLeftEnabled) {
            gestureLog.record("pad", "fling-left", "ignored-disabled", policy.disabledReason.orEmpty())
            gestureError = policy.disabledReason
            return
        }
        if (words <= 1 && (seq.isNotEmpty() || qwertyBuffer.isNotEmpty())) {
            gestureLog.record("pad", "fling-left", "commit-revert-keypress")
            deleteLast()
            return
        }
        deleteWords(words.coerceAtLeast(1))
    }

    /** Live delete-slide preview (slide-back shrinks the count to 0). */
    internal fun onDeleteSlidePreview(words: Int) {
        deletePreviewWords = words
    }

    /**
     * Deletes [n] words (plus trailing whitespace) before the cursor using
     * explicit word-boundary analysis of text-before-cursor. Every failure
     * surfaces via [gestureError]; success arms the 5s [undoDelete].
     */
    private fun deleteWords(n: Int) {
        val ic = currentInputConnection ?: run {
            reportGestureError("Delete failed: no input connection")
            return
        }
        maybeRejectLastCommit()
        val before = try {
            ic.getTextBeforeCursor(DELETE_LOOKBACK_CHARS, 0)?.toString().orEmpty()
        } catch (e: Exception) {
            reportGestureError("Delete failed: could not read text (${e.message})")
            return
        }
        if (before.isEmpty()) {
            try {
                ic.deleteSurroundingText(1, 0)
            } catch (e: Exception) {
                reportGestureError("Delete failed: ${e.message}")
            }
            return
        }
        var i = before.length
        val end = i
        var count = 0
        while (i > 0 && count < n) {
            while (i > 0 && before[i - 1].isWhitespace()) i--
            if (i == 0) break
            while (i > 0 && !before[i - 1].isWhitespace()) i--
            count++
        }
        val span = end - i
        if (span <= 0) return
        undoText = before.substring(i, end)
        try {
            ic.deleteSurroundingText(span, 0)
        } catch (e: Exception) {
            reportGestureError("Delete of $count word(s) failed: ${e.message}")
            return
        }
        gestureLog.record("pad", "fling-left", "commit-delete-words", "$count")
        armUndo()
    }

    /** 5s undo window after a fling-delete commit (shown in SuggestionBar). */
    private fun armUndo() {
        undoExpiry?.let { mainHandler.removeCallbacks(it) }
        undoAvailable = true
        val expiry = Runnable {
            undoAvailable = false
            undoText = ""
        }
        undoExpiry = expiry
        mainHandler.postDelayed(expiry, UNDO_WINDOW_MS)
    }

    internal fun undoDelete() {
        undoExpiry?.let { mainHandler.removeCallbacks(it) }
        undoExpiry = null
        undoAvailable = false
        if (undoText.isEmpty()) {
            reportGestureError("Nothing to undo: deleted text already expired")
            return
        }
        commitText(undoText)
        gestureLog.record("pad", "tap-undo", "undo-delete", undoText)
        undoText = ""
    }

    /**
     * Fling `↑` (policy-driven):
     * - numbers → commit `0` (the Space/0 position on a digit pad);
     * - js/rust/html/math → commit top + space or `;` (Gesture Tuning);
     * - otherwise → space + commit top candidate, plain space when not
     *   composing (shared with the space key path).
     */
    internal fun onPadFlingSpace() {
        val policy = gesturePolicy()
        if (!policy.flingUpEnabled) {
            gestureLog.record("pad", "fling-up", "ignored-disabled", policy.disabledReason.orEmpty())
            gestureError = policy.disabledReason
            return
        }
        when (policy.flingUpAction) {
            FlingUpAction.ZERO -> {
                gestureLog.record("pad", "fling-up", "commit-zero")
                commitRaw("0")
            }
            FlingUpAction.COMMIT_PLUS_TERMINATOR -> {
                val top = liveCandidates.firstOrNull()
                if (top != null && (seq.isNotEmpty() || qwertyBuffer.isNotEmpty())) {
                    val term = GestureTuningStore.codeTerminator(this)
                    gestureLog.record("pad", "fling-up", "commit-top-terminator", term)
                    commitCandidateWithTerminator(top, term)
                } else {
                    gestureLog.record("pad", "fling-up", "commit-terminator-only")
                    commitRaw(GestureTuningStore.codeTerminator(this))
                }
            }
            FlingUpAction.SPACE_COMMIT_TOP -> {
                gestureLog.record("pad", "fling-up", "space-commit-top")
                onPadSpace()
            }
        }
    }

    /**
     * Fling `→`: accept top suggestion (= tap candidate #1). Explicit noop
     * with a log entry when there is nothing to accept (numbers category or
     * empty strip) — never a silent swallow.
     */
    internal fun onPadFlingAccept() {
        val policy = gesturePolicy()
        if (!policy.flingRightEnabled) {
            gestureLog.record("pad", "fling-right", "noop-no-prediction", activeAssetId)
            return
        }
        acceptTopCandidate("pad-fling")
    }

    internal fun acceptTopCandidate(source: String) {
        val top = liveCandidates.firstOrNull()
        if (top == null) {
            gestureLog.record("pad", "accept", "noop-empty-strip", source)
            return
        }
        gestureLog.record("pad", "accept", "accept-#1", "$source:$top")
        commitCandidate(top)
    }

    /** Fling `↓`: hide keyboard. Never symbols (rejected, stays rejected). */
    internal fun onPadFlingHide() {
        gestureLog.record("pad", "fling-down", "hide")
        try {
            requestHideSelf(0)
        } catch (e: Exception) {
            reportGestureError("Hide keyboard failed: ${e.message}")
        }
    }

    /**
     * Long-press (~400ms, no move): symbols sheet for the pressed key.
     * - default: key's SPEC §1 alt chars + its digit;
     * - emoji ([LongPressKind.TONE_PICKER]): key chars are the variant pool;
     * - js/rust/html/math ([LongPressKind.VARIANT_PREVIEW]): same pool, titled
     *   as a variant preview — full snippet skeletons arrive with plan 07.
     * Unknown key (empty code): explicit digit sheet + logged miss, never an
     * empty sheet.
     */
    internal fun onKeyLongPress(code: String, symbols: String) {
        val policy = gesturePolicy()
        if (code.isEmpty() || symbols.isEmpty()) {
            gestureLog.record("pad", "long-press", "unknown-key-sheet", code)
            symbolsTitle = "Key unknown — pick a digit"
            symbolsOptions = listOf("1", "2", "3", "4", "5", "6", "7", "8", "9", "0")
            return
        }
        val digit = code.filter { it.isDigit() }.take(1)
        val chars = (symbols.toList().map { it.toString() } + digit)
            .distinct()
            .filter { it.isNotEmpty() }
        if (chars.isEmpty()) {
            reportGestureError("No symbols for key $code")
            gestureLog.record("pad", "long-press", "empty-symbols", code)
            return
        }
        symbolsTitle = when (policy.longPressKind) {
            LongPressKind.TONE_PICKER -> "Variant — $code"
            LongPressKind.VARIANT_PREVIEW -> "Variant preview — $code"
            LongPressKind.SYMBOLS_SHEET -> "Symbols — $code"
        }
        symbolsOptions = chars
        gestureLog.record("pad", "long-press", "symbols-sheet", code)
    }

    private fun showGenericSymbolsSheet() {
        symbolsTitle = "Symbols"
        symbolsOptions = listOf("@", "#", "$", "%", "&", "*", "-", "+", "(", ")")
        gestureLog.record("pad", "tap-sym", "symbols-sheet", "fallback-bar")
    }

    /** Tapping a symbols-sheet entry commits it raw (breaks word, no learn). */
    internal fun onSymbolPick(symbol: String) {
        symbolsOptions = emptyList()
        symbolsTitle = ""
        if (symbol.isEmpty()) {
            reportGestureError("Empty symbol pick ignored")
            return
        }
        gestureLog.record("pad", "tap-symbol", "commit-raw", symbol)
        commitRaw(symbol)
    }

    internal fun onCategoryChanged(tabLabel: String) {
        activeAssetId = canonicalTabId(tabLabel)
        liveTabLabel = activeAssetId
        // Snippet buffer dies on tab switch (plan 12:62): helpers belong to
        // one tab's document scratch — switching to EN hides the js strip,
        // and coming back later finds it gone. The new tab's static palette
        // (html/math) appears via the refresh at the end of this function.
        snippetBuffer.clear()
        liveSnippets = emptyList()
        // Engine re-resolves per active tab (plan/02): swap the pad when
        // the tab's layout (override -> global -> t9-9) differs. Key
        // labels re-encode from the new spec; the in-progress seq clears
        // because codes belong to the previous pad.
        //
        // Per-tab isolation: every suggest carries the active tab, so a
        // switch must re-query instead of showing the previous tab's strip.
        // Pending input survives only when the pad is unchanged (same codes
        // still valid); otherwise buffers clear and the strip empties.
        val pendingSeq = seq.toString().takeIf { it.isNotEmpty() }
        val pendingQwerty = qwertyBuffer.toString().takeIf { it.isNotEmpty() }
        val resolved = LayoutStore.layoutForCat(this, activeAssetId)
        if (resolved != activeLayoutId) {
            setPadLayout(resolved)
            qwertyBuffer.clear()
            lastShown = emptyList()
            liveCandidates = emptyList()
        } else if (pendingSeq != null) {
            refreshSuggestions(pendingSeq)
        } else if (pendingQwerty != null) {
            refreshQwertySuggestions(pendingQwerty)
        } else {
            lastShown = emptyList()
            liveCandidates = emptyList()
        }
        // New tab's static palette (html skeletons, math flat tokens) shows
        // immediately; buffer follow-tokens are gone (cleared above).
        refreshSnippetStrip()
    }

    /**
     * Canonical asset id for a tab label (legacy display labels `EN`/`NE`
     * still map). Shared by tap select and long-press placement so both
     * resolve identically.
     */
    internal fun canonicalTabId(tabLabel: String): String = when (tabLabel) {
        "EN" -> "words"
        "NE" -> "ne"
        "nepali" -> "ne" // legacy manifest id (renamed to `ne`)
        "★personal" -> "personal"
        else -> tabLabel.lowercase()
    }

    /**
     * Long-press on a tab (plan/08): opens the placement popup for that
     * category — pack record (id, priority, enabled, learn + layout
     * badges) with move up/down + enable/disable for movable packs.
     *
     * The live engine `placement(word)` API (`core-rust/src/stack.rs`)
     * answers per-WORD provenance (`packId • freq • accepts`) but is not
     * UniFFI-exported ([StackEngineCaps.placementExported] == false), so
     * the popup shows the pack record with [StackEngineCaps.placementError]
     * attached — per-word provenance stays explicitly unavailable, never
     * a plausible-looking fabrication.
     */
    internal fun onTabLongPress(tabLabel: String) {
        val id = canonicalTabId(tabLabel)
        val slots = try {
            DictStackOrder.assembleSlots(this)
        } catch (e: Exception) {
            placementPopup = PlacementState(
                tab = tabLabel,
                info = "Stack record unreadable: ${e.message}"
            )
            gestureLog.record("tabs", "long-press", "placement-unreadable", tabLabel)
            return
        }
        val slot = slots.find { it.id == id }
        if (slot == null) {
            placementPopup = PlacementState(
                tab = tabLabel,
                info = "Unknown category \"$tabLabel\" (not in the installed stack)."
            )
            gestureLog.record("tabs", "long-press", "placement-unknown", tabLabel)
            return
        }
        placementPopup = placementFor(slot, slots, note = null)
        gestureLog.record("tabs", "long-press", "placement-open", id)
    }

    /** Rebuild the popup for [slot] (move/toggle targets re-resolve order). */
    private fun placementFor(slot: StackSlot, slots: List<StackSlot>, note: String?): PlacementState {
        val movable = DictStackOrder.sortMovable(slots)
        val rank = movable.indexOfFirst { it.id == slot.id }
        val record = buildString {
            append("pack ${slot.id} • prio ${slot.priority} • ")
            append(if (slot.enabled) "enabled" else "disabled")
            append(" • ${if (slot.learns) "learns" else "no-learn"}")
            slot.layoutId?.let { append(" • layout $it") }
            if (slot.fixed) append(" • fixed pin")
            if (slot.custom) append(" • custom")
        }
        val info = buildString {
            append(record)
            append("\n")
            append(StackEngineCaps.placementError())
            if (note != null) {
                append("\n")
                append(note)
            }
        }
        return PlacementState(
            tab = slot.id,
            info = info,
            canMoveUp = !slot.fixed && rank > 0,
            canMoveDown = !slot.fixed && rank >= 0 && rank < movable.size - 1,
            enabled = slot.enabled,
            movable = !slot.fixed
        )
    }

    /** Persist a stack reorder from the placement popup (move ±1). */
    internal fun onPlacementMove(delta: Int) {
        val current = placementPopup ?: run {
            reportGestureError("Placement popup closed: move ignored")
            return
        }
        try {
            val slots = DictStackOrder.assembleSlots(this)
            val order = DictStackOrder.sortMovable(slots).map { it.id }
            val moved = DictStackOrder.moveOrder(order, current.tab, delta)
            val customIds = slots.filter { it.custom }.map { it.id }.toSet()
            DictStackOrder.persistPriorities(this, DictStackOrder.respace(moved), customIds)
            refreshTabs()
            val fresh = DictStackOrder.assembleSlots(this)
            val slot = fresh.find { it.id == current.tab }
            placementPopup = if (slot == null) {
                current.copy(info = current.info + "\nMove saved but \"${current.tab}\" vanished from the stack.")
            } else {
                placementFor(
                    slot, fresh,
                    note = "Order saved — engine applies it on keyboard restart " +
                        "(packs reinstall in priority order; live reorder is not " +
                        "UniFFI-exported)."
                )
            }
            gestureLog.record("tabs", "placement", "move", "${current.tab}:$delta")
        } catch (e: Exception) {
            reportGestureError("Placement move failed: ${e.message}")
        }
    }

    /** Persist an enable/disable from the placement popup. */
    internal fun onPlacementToggle() {
        val current = placementPopup ?: run {
            reportGestureError("Placement popup closed: toggle ignored")
            return
        }
        try {
            val slots = DictStackOrder.assembleSlots(this)
            val slot = slots.find { it.id == current.tab }
                ?: throw IllegalArgumentException("unknown category \"${current.tab}\"")
            if (slot.fixed) {
                throw IllegalArgumentException(
                    "Fixed pin \"${current.tab}\" cannot be disabled (base 0 / personal 100)."
                )
            }
            val next = !slot.enabled
            if (slot.custom) CustomPackStore.setEnabled(this, slot.id, next)
            else PackOrderStore.setEnabled(this, slot.id, next)
            refreshTabs()
            val fresh = DictStackOrder.assembleSlots(this)
            val updated = fresh.find { it.id == current.tab } ?: slot.copy(enabled = next)
            placementPopup = placementFor(
                updated, fresh, note = StackEngineCaps.liveToggleError(current.tab)
            )
            gestureLog.record("tabs", "placement", "toggle", "${current.tab}:$next")
        } catch (e: Exception) {
            reportGestureError("Placement toggle failed: ${e.message}")
        }
    }

    internal fun onPlacementDismiss() {
        placementPopup = null
    }

    private fun makePad(): View = if (qwertyFallback) {
        QwertyView(this).apply {
            onKey = { onQwertyKey(it) }
            onDelete = { deleteLast() }
            onEnter = { onQwertyEnter() }
            onSwitchIme = { switchToNextIme() }
            onFlingAccept = { acceptTopCandidate("qwerty-fling") }
            onGestureRejected = { reason -> gestureLog.record("qwerty", "fling", "rejected", reason) }
            gesturesEnabled = flingsAllowedAT
            updateThresholds(effectiveThresholds())
        }
    } else {
        padViewFor(this, activeLayoutId).apply {
            onCode = { code -> onPadCode(code) }
            onSpace = { onPadSpace() }
            onDelete = { deleteLast() }
            onSym = { symbols -> onPadSym(symbols) }
            onEnter = { handleEnter() }
            onControl = { code -> onPadControl(code) }
            onSwitchIme = { switchToNextIme() }
            onFlingDelete = { words -> onPadFlingDelete(words) }
            onDeleteSlide = { words -> onDeleteSlidePreview(words) }
            onFlingSpace = { onPadFlingSpace() }
            onFlingAccept = { onPadFlingAccept() }
            onFlingHide = { onPadFlingHide() }
            onKeyLongPress = { code, symbols -> onKeyLongPress(code, symbols) }
            onGestureRejected = { reason -> gestureLog.record("pad", "touch", "rejected", reason) }
            gesturesEnabled = flingsAllowedAT
            flingGate = gateFor(gesturePolicy())
            updateThresholds(effectiveThresholds())
        }
    }

    private fun refreshPad(root: LinearLayout) {
        val old = root.findViewWithTag<View>("pad")
        if (old != null) {
            val index = root.indexOfChild(old)
            root.removeViewAt(index)
            val replacement = makePad()
            replacement.tag = "pad"
            root.addView(replacement, index)
        }
    }

    companion object {
        /** Fallback learn flags mirroring assets privacy.learn (numbers/math false). */
        val DEFAULT_LEARN_FLAGS: Map<String, Boolean> = mapOf(
            "numbers" to false,
            "math" to false
        )

        /** Base wordlist packs merged into `Predictor.new` (all prio 0). */
        val BASE_PACK_ASSETS: List<String> = listOf(
            "packs/words_en.json",
            "packs/nepali.json"
        )

        /**
         * Extension packs installed via `add_pack_json` at engine bring-up
         * (`(asset path, priority)`; envelopes carry manifest + `words`).
         * Priorities mirror `core-rust/tests/per_tab.rs` CORPUS so
         * on-device tie-breaks match the core per-tab report (numbers 15,
         * js 20, rust 25, html 30, emoji 40, math 50, medical 60).
         */
        val EXTENSION_PACK_ASSETS: List<Pair<String, Int>> = listOf(
            "categories/numbers.json" to 15,
            "categories/js.json" to 20,
            "categories/rust.json" to 25,
            "categories/html.json" to 30,
            "categories/emoji.json" to 40,
            "categories/math.json" to 50,
            "categories/medical.json" to 60
        )

        /**
         * Reject window (SPEC §2): deleting a committed word within this long
         * after commit records a reject. Shared with the undo affordance so
         * the two 5s windows never drift apart.
         */
        const val REJECT_WINDOW_MS = 5_000L

        /** Undo availability after a fling-delete commit ([REJECT_WINDOW_MS]). */
        const val UNDO_WINDOW_MS = 5_000L

        /** Chars of text-before-cursor read for the bigram prev word. */
        const val CURSOR_CONTEXT_CHARS = 32

        /** Chars of text-before-cursor scanned for word-boundary deletes. */
        const val DELETE_LOOKBACK_CHARS = 256
    }
}

/** Input class bucket: only TEXT gets prediction; others commit raw. */
private enum class InputMode { TEXT, NUMBER, PHONE, DATETIME }

private fun EditorInfo?.resolveInputMode(): InputMode {
    val cls = (this?.inputType ?: InputType.TYPE_CLASS_TEXT) and InputType.TYPE_MASK_CLASS
    return when (cls) {
        InputType.TYPE_CLASS_NUMBER -> InputMode.NUMBER
        InputType.TYPE_CLASS_PHONE -> InputMode.PHONE
        InputType.TYPE_CLASS_DATETIME -> InputMode.DATETIME
        else -> InputMode.TEXT
    }
}

/** True for password / visible-password / web-password variations. */
fun EditorInfo.isPasswordField(): Boolean {
    val variation = inputType and (InputType.TYPE_MASK_CLASS or InputType.TYPE_MASK_VARIATION)
    return variation == (InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD) ||
        variation == (InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD) ||
        variation == (InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD) ||
        variation == (InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD)
}

/**
 * Incognito / no-personalized-learning request: system flag
 * ([EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING], set e.g. by browsers in
 * private mode) or a privateImeOptions token containing `incognito` /
 * `noPersonalizedLearning`.
 */
fun EditorInfo?.isIncognitoRequest(): Boolean {
    if (this == null) return false
    if ((imeOptions and EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING) != 0) return true
    val opts = privateImeOptions ?: return false
    return opts.contains("noPersonalizedLearning", ignoreCase = true) ||
        opts.contains("incognito", ignoreCase = true)
}

/** Email/URI variations: keep suggest, suppress learn. */
fun EditorInfo?.isEmailOrUri(): Boolean {
    if (this == null) return false
    val variation = inputType and (InputType.TYPE_MASK_CLASS or InputType.TYPE_MASK_VARIATION)
    return variation == (InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS) ||
        variation == (InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI)
}
