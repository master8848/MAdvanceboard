package com.kb.ime

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

/**
 * Hybrid IME screen: [CategoryTabs] + [SuggestionStrip] + expand-all paged
 * sheet + QWERTY FAB toggle + 9/12/16 pad-size toggle + plan 01 overlays.
 *
 * [candidates] are live results fed by the host service from
 * Predictor.suggest (shared T9/QWERTY path) — never hardcoded here.
 * [activeLayoutId] reflects the service's pad; [onLayoutChanged] persists
 * the pick as the per-tab override ([LayoutStore]) and swaps the pad.
 * [activeTabLabel] names the tab the toggle applies to.
 *
 * Plan 01 additions (all optional params, all duplicated by visible UI):
 * - [symbolsOptions]/[onSymbolPick]/[onSymbolsDismiss]: long-press symbols
 *   sheet for the pressed key (SPEC §1 alt table). Tapping a symbol commits.
 * - [deletePreviewWords]: live delete-slide count (`←` slide preview);
 *   0 = no preview.
 * - [undoAvailable]/[onUndo]: 5s undo affordance after a fling-delete commit.
 * - [fallbackActionsVisible] + [fallbackActions]: 48dp AT button cluster
 *   shown when TalkBack/SwitchAccess/VoiceAccess disables Pad flings.
 * - First-run 3-step gesture coach (Skip + replayable via Gesture Tuning)
 *   and the faint `← del · ↑ space · → accept` footer (first 3 days,
 *   dismissable) — persisted in [GestureTuningStore].
 */
@Composable
fun ImeScreen(
    candidates: List<String> = emptyList(),
    onCandidatePicked: (String) -> Unit,
    onExpandAll: () -> Unit = {},
    onToggleQwerty: () -> Unit = {},
    onCategoryChanged: (String) -> Unit = {},
    activeLayoutId: String = "t9-9",
    onLayoutChanged: (String) -> Unit = {},
    /** Tab the pad-size toggle applies to (per-tab override caption). */
    activeTabLabel: String = "words",
    symbolsOptions: List<String> = emptyList(),
    symbolsTitle: String = "",
    onSymbolPick: (String) -> Unit = {},
    onSymbolsDismiss: () -> Unit = {},
    deletePreviewWords: Int = 0,
    undoAvailable: Boolean = false,
    onUndo: () -> Unit = {},
    onAcceptFirst: () -> Unit = {},
    onFling: (zone: String, gesture: String, action: String) -> Unit = { _, _, _ -> },
    fallbackActionsVisible: Boolean = false,
    fallbackActions: FallbackActions = FallbackActions(),
    qwertyActive: Boolean = false,
    /** Explicit error/status line (gesture failures surface here, never silent). */
    statusLine: String? = null,
    /** Sink for overlay prefs failures (coach/footer), shown via [statusLine]. */
    onError: (String) -> Unit = {}
) {
    val context = LocalContext.current
    var selectedTab by remember { mutableIntStateOf(1) }
    var expanded by remember { mutableStateOf(false) }
    var coachVisible by remember {
        mutableStateOf(!GestureTuningStore.isCoachSeen(context))
    }
    var footerVisible by remember {
        mutableStateOf(GestureTuningStore.showFooterHint(context))
    }

    Column(modifier = Modifier.fillMaxWidth()) {
        CategoryTabs(
            selected = selectedTab,
            onSelect = {
                selectedTab = it
                onCategoryChanged(DEFAULT_CATEGORIES.getOrElse(it) { "" })
            },
            onExpandAll = {
                expanded = true
                onExpandAll()
            }
        )
        Row(modifier = Modifier.fillMaxWidth()) {
            SuggestionStrip(
                candidates = candidates,
                onPick = onCandidatePicked,
                onAcceptFirst = onAcceptFirst,
                onExpandAll = {
                    expanded = true
                    onExpandAll()
                },
                onFling = onFling,
                modifier = Modifier.weight(1f)
            )
            // QWERTY FAB: button-first toggle (plan 01). No single-finger Pad
            // gesture toggles this; optional advanced 2-finger swipe ↑ TBD.
            TextButton(onClick = onToggleQwerty) {
                Text(if (qwertyActive) "9-KEY" else "QWERTY")
            }
        }
        if (deletePreviewWords > 0) {
            Text(
                text = "Delete $deletePreviewWords word(s) — release to commit, slide back to shrink",
                style = MaterialTheme.typography.labelMedium,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
            )
        }
        if (undoAvailable) {
            UndoBar(onUndo = onUndo)
        }
        if (fallbackActionsVisible) {
            // Next-Category cycles the tab order owned here, then notifies
            // the host (dict filter + layout resolve follow).
            FallbackActionBar(
                actions = fallbackActions.copy(
                    onNextCategory = {
                        selectedTab = (selectedTab + 1) % DEFAULT_CATEGORIES.size
                        onCategoryChanged(DEFAULT_CATEGORIES[selectedTab])
                    }
                )
            )
        }
        // Pad-size toggle: 9/12/16 per-tab override (plan/02). Switching
        // tabs re-resolves the engine + pad via the host; labels come
        // from the newly loaded spec.
        PadSizeToggle(
            activeLayoutId = activeLayoutId,
            activeTabLabel = activeTabLabel,
            onSelect = onLayoutChanged
        )
        if (statusLine != null) {
            Text(
                text = statusLine,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 2.dp)
            )
        }
        if (footerVisible) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Text(
                    text = "← del · ↑ space · → accept",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 2.dp)
                )
                TextButton(onClick = {
                    try {
                        GestureTuningStore.dismissFooterHint(context)
                        footerVisible = false
                    } catch (e: Exception) {
                        onError("Could not dismiss hint: ${e.message}")
                    }
                }) { Text("hide") }
            }
        }
        if (expanded) {
            ExpandAllList(
                items = candidates.take(EXPAND_LIMIT),
                onPick = {
                    expanded = false
                    onCandidatePicked(it)
                },
                onClose = { expanded = false }
            )
        }
        if (symbolsOptions.isNotEmpty()) {
            SymbolsSheet(
                title = symbolsTitle,
                options = symbolsOptions,
                onPick = onSymbolPick,
                onDismiss = onSymbolsDismiss
            )
        }
        if (coachVisible) {
            GestureCoach(
                onSkip = {
                    try {
                        GestureTuningStore.markCoachSeen(context)
                        coachVisible = false
                    } catch (e: Exception) {
                        onError("Could not save coach state: ${e.message}")
                    }
                },
                onDone = {
                    try {
                        GestureTuningStore.markCoachSeen(context)
                        coachVisible = false
                    } catch (e: Exception) {
                        onError("Could not save coach state: ${e.message}")
                    }
                }
            )
        }
    }
}

/** Visible duplicate of every Pad gesture for accessibility + discovery. */
data class FallbackActions(
    val onDelete: () -> Unit = {},
    val onSpace: () -> Unit = {},
    val onAcceptFirst: () -> Unit = {},
    val onNextCategory: () -> Unit = {},
    val onToggleQwerty: () -> Unit = {},
    val onSym: () -> Unit = {}
)

/**
 * 48dp-minimum focusable button cluster. Shown when an accessibility service
 * disables Pad flings; also the D-pad (TV) / VR-controller target (arrows
 * move focus Tabs→Bar→Pad→FAB by default focus order, Center=tap) and the
 * gaze/dwell cluster (800ms dwell=tap, 1500ms=long-press handled by the AT
 * stack — no gaze-fling exists by design).
 */
@Composable
fun FallbackActionBar(actions: FallbackActions, modifier: Modifier = Modifier) {
    // Every button is forced to 48dp minimum (TextButton defaults to 40dp
    // height — below the plan 01 accessibility target).
    val minTarget = Modifier
        .heightIn(min = 48.dp)
        .widthIn(min = 48.dp)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(4.dp),
        horizontalArrangement = Arrangement.SpaceEvenly
    ) {
        TextButton(onClick = actions.onDelete, modifier = minTarget) { Text("Delete") }
        TextButton(onClick = actions.onSpace, modifier = minTarget) { Text("Space") }
        TextButton(onClick = actions.onAcceptFirst, modifier = minTarget) { Text("Accept #1") }
        TextButton(onClick = actions.onNextCategory, modifier = minTarget) { Text("Next cat") }
        TextButton(onClick = actions.onToggleQwerty, modifier = minTarget) { Text("QWERTY") }
        TextButton(onClick = actions.onSym, modifier = minTarget) { Text("Sym") }
    }
}

/** 5s undo affordance after a fling-delete commit (plan 01). */
@Composable
fun UndoBar(onUndo: () -> Unit, modifier: Modifier = Modifier) {
    var secondsLeft by remember { mutableIntStateOf(5) }
    LaunchedEffect(Unit) {
        for (i in 4 downTo 1) {
            delay(1000)
            secondsLeft = i
        }
    }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.SpaceBetween
    ) {
        Text(
            text = "Deleted — undo available (${secondsLeft}s)",
            style = MaterialTheme.typography.labelMedium
        )
        TextButton(onClick = onUndo) { Text("Undo") }
    }
}

/**
 * Long-press symbols sheet: one button per char of the pressed key's alt
 * table (SPEC §1) plus its digit. Tapping commits immediately. For emoji
 * keys this is the tone/variant picker; for js/rust/html/math the symbol
 * variant preview (`{}` vs `function`, `\frac{}{}` skeleton) — the host
 * builds [options] accordingly via [LongPressKind].
 */
@Composable
fun SymbolsSheet(
    title: String,
    options: List<String>,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit
) {
    Column(modifier = Modifier.fillMaxWidth().padding(8.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Text(
                text = if (title.isEmpty()) "Symbols" else "Symbols — $title",
                style = MaterialTheme.typography.titleSmall
            )
            TextButton(onClick = onDismiss) { Text("Close") }
        }
        LazyVerticalGrid(
            columns = GridCells.Adaptive(48.dp),
            modifier = Modifier.fillMaxWidth()
        ) {
            items(options) { symbol ->
                TextButton(onClick = { onPick(symbol) }) { Text(symbol) }
            }
        }
    }
}

/** First-run 3-step gesture coach: ← delete / ↑ space / → accept. */
@Composable
fun GestureCoach(onSkip: () -> Unit, onDone: () -> Unit) {
    var step by remember { mutableIntStateOf(0) }
    val steps = listOf(
        "Fling ← on the pad to delete (slide further for whole words).",
        "Fling ↑ on the pad for space + accept top suggestion.",
        "Fling → on the pad to accept the top suggestion."
    )
    Card(modifier = Modifier.fillMaxWidth().padding(8.dp)) {
        Column(modifier = Modifier.padding(16.dp)) {
            Text(
                text = "Gestures — step ${step + 1} of 3",
                style = MaterialTheme.typography.titleSmall
            )
            Text(text = steps[step], modifier = Modifier.padding(vertical = 8.dp))
            Row(horizontalArrangement = Arrangement.SpaceBetween) {
                TextButton(onClick = onSkip) { Text("Skip") }
                if (step < 2) {
                    TextButton(onClick = { step++ }) { Text("Next") }
                } else {
                    Button(onClick = onDone) { Text("Got it") }
                }
            }
        }
    }
}

/**
 * Expand-all candidate sheet: up to 30 items, paged (10/page) per plan 01.
 * Opened by the `∨` button or swipe-up on the SuggestionBar.
 */
@Composable
fun ExpandAllList(
    items: List<String>,
    onPick: (String) -> Unit,
    onClose: () -> Unit
) {
    var page by remember(items) { mutableIntStateOf(0) }
    val pageSize = 10
    val pageCount = ((items.size + pageSize - 1) / pageSize).coerceAtLeast(1)
    val shown = items.drop(page * pageSize).take(pageSize)
    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Button(onClick = onClose, modifier = Modifier.padding(8.dp)) {
                Text("Close")
            }
            if (pageCount > 1) {
                Row {
                    TextButton(
                        enabled = page > 0,
                        onClick = { page-- }
                    ) { Text("‹ Prev") }
                    Text(
                        text = "${page + 1}/$pageCount",
                        modifier = Modifier.padding(vertical = 12.dp)
                    )
                    TextButton(
                        enabled = page < pageCount - 1,
                        onClick = { page++ }
                    ) { Text("Next ›") }
                }
            }
        }
        LazyColumn(modifier = Modifier.fillMaxSize()) {
            items(shown) { word ->
                Text(
                    text = word,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onPick(word) }
                        .padding(horizontal = 16.dp, vertical = 12.dp)
                )
            }
        }
    }
}

/**
 * 9/12/16 pad-size toggle. Selected id renders bracketed (`[12]`);
 * selection flows to the host, which persists it as the per-tab override
 * ([LayoutStore]) and swaps the PadView. The caption names the owning tab
 * so the per-tab scope is visible. Category tabs and the QWERTY toggle
 * are unaffected.
 */
@Composable
fun PadSizeToggle(
    activeLayoutId: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    activeTabLabel: String = ""
) {
    val options = listOf("t9-9" to "9", "t9-12" to "12", "t9-16" to "16")
    Row(modifier = modifier.fillMaxWidth()) {
        Text(
            text = if (activeTabLabel.isEmpty()) "Pad:" else "Pad ($activeTabLabel):",
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)
        )
        options.forEach { (id, short) ->
            val label = if (id == activeLayoutId) "[$short]" else short
            TextButton(onClick = { onSelect(id) }) { Text(label) }
        }
    }
}
