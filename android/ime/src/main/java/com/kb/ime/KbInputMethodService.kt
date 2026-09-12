package com.kb.ime

import android.content.Context
import android.inputmethodservice.InputMethodService
import android.os.Build
import android.text.InputType
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.LinearLayout
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.ComposeView
import com.kb.bridge.KbCore
import com.kb.bridge.Predictor
import com.kb.bridge.StubPredictor
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
    private var qwertyFallback: Boolean = false
    /**
     * Active pad layout id (`t9-9` / `t9-12` / `t9-16`). Encoder flag only —
     * plumbed to [suggestWithLayout]/[learnWithLayout]; persisted by
     * `PadModeStore` (toggle commit) and reloaded in [onCreate].
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

    /** Service-scoped scope for suggest/learn; cancelled in [onDestroy]. */
    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /**
     * [StubPredictor] keeps the IME functional when `libkbcore.so` is missing
     * ([KbCore.isAvailable] == false). The real UniFFI predictor plugs in here
     * when the native lib ships — same interface, no call-site changes.
     */
    private val predictor: Predictor = StubPredictor()

    override fun onCreate() {
        super.onCreate()
        activeSpec = loadLayoutSpec(this, activeLayoutId)
        // Warm category learn-flags + native-lib probe off the main thread.
        serviceScope.launch {
            val ids = try {
                CategoryRegistry.listIds(assets).ifEmpty { DEFAULT_LEARN_FLAGS.keys.toList() }
            } catch (_: Exception) {
                DEFAULT_LEARN_FLAGS.keys.toList()
            }
            val manifests = try {
                CategoryRegistry.loadManifests(assets, ids)
            } catch (_: Exception) {
                emptyList()
            }
            if (manifests.isNotEmpty()) {
                learnByCategory = manifests.associate { it.id to it.learn }
            }
            @Suppress("unused")
            val nativeAvailable = KbCore.isAvailable()
        }
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
                    onCandidatePicked = { commitCandidate(it) },
                    onExpandAll = { /* bottom-sheet host TBD */ },
                    onToggleQwerty = {
                        qwertyFallback = !qwertyFallback
                        refreshPad(root)
                    },
                    onCategoryChanged = { onCategoryChanged(it) }
                )
            }
        }
        val pad = makePad()
        pad.tag = "pad"
        root.addView(strip)
        root.addView(pad)
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
        seq.clear()
        qwertyBuffer.clear()
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        // Layout already reflects inputMode via predictionEnabled (raw commit when
        // false). Pad swaps only happen on explicit QWERTY toggle to avoid desync.
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
        super.onFinishInput()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        try {
            currentInputConnection?.finishComposingText()
        } catch (_: Exception) {
        }
        seq.clear()
        qwertyBuffer.clear()
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
        try {
            predictor.close()
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
     */
    internal fun onPadCode(code: String) {
        val key = activeSpec?.keyByCode(code)
        val symbols = key?.symbols.orEmpty()
        if (symbols.isNotEmpty() && symbols.none { it.isLetter() }) {
            commitText(symbols.substring(0, 1))
            seq.clear()
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
        refreshSuggestions(seq.toString())
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
        setPadLayout(next)
    }

    internal fun setPadLayout(layoutId: String) {
        if (layoutId !in SUPPORTED_LAYOUT_IDS) return
        activeLayoutId = layoutId
        activeSpec = loadLayoutSpec(this, layoutId)
        seq.clear()
        (cachedInputView as? LinearLayout)?.let { refreshPad(it) }
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
        refreshSuggestions(qwertyBuffer.toString())
    }

    internal fun onQwertyEnter() {
        if (qwertyBuffer.isNotEmpty()) commitCandidate(qwertyBuffer.toString())
        handleEnter()
    }

    private fun refreshSuggestions(query: String) {
        val snapshot = query
        val layoutId = activeLayoutId
        serviceScope.launch {
            val prev = prevWord()
            val result = try {
                // Shared Predictor path; layout_id is an encoder flag only.
                // Explicit limit 30 feeds both the 3-inline strip and the
                // 30-item expand-all list from one fetch.
                suggestWithLayout(predictor, snapshot, prev, layoutId, limit = 30)
            } catch (_: Exception) {
                emptyList()
            }
            liveCandidates = result.map { it.word }
        }
    }

    /** Previous-word context for bigrams; null under no-learn (no harvesting). */
    private fun prevWord(): String? {
        if (isPasswordField || isIncognito) return null
        return try {
            currentInputConnection?.getTextBeforeCursor(32, 0)?.toString()
                ?.split(Regex("\\s+"))?.lastOrNull()?.takeIf { it.isNotEmpty() }
        } catch (_: Exception) {
            null
        }
    }

    private fun shouldLearnNow(): Boolean =
        !isPasswordField && !isIncognito && predictionEnabled && !isEmailOrUri &&
            (learnByCategory[activeAssetId] ?: true)

    private fun commitCandidate(word: String) {
        val prev = prevWord()
        val layoutId = activeLayoutId
        val text = if (capsNext) word.replaceFirstChar { it.uppercase() } else word
        capsNext = false
        commitText("$text ")
        seq.clear()
        qwertyBuffer.clear()
        liveCandidates = emptyList()
        // Fire-and-forget learn on Dispatchers.Default inside Predictor,
        // gated on password + incognito + input class + category.
        if (shouldLearnNow()) {
            serviceScope.launch {
                try {
                    learnWithLayout(predictor, word, prev, layoutId)
                } catch (_: Exception) {
                }
            }
        }
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
            refreshSuggestions(qwertyBuffer.toString())
            return
        }
        ic.deleteSurroundingText(1, 0)
    }

    internal fun onCategoryChanged(tabLabel: String) {
        activeAssetId = when (tabLabel) {
            "EN" -> "words"
            "NE" -> "ne"
            "nepali" -> "ne" // legacy manifest id (renamed to `ne`)
            "★personal" -> "personal"
            else -> tabLabel.lowercase()
        }
    }

    private fun makePad(): View = if (qwertyFallback) {
        QwertyView(this).apply {
            onKey = { onQwertyKey(it) }
            onDelete = { deleteLast() }
            onEnter = { onQwertyEnter() }
            onSwitchIme = { switchToNextIme() }
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
