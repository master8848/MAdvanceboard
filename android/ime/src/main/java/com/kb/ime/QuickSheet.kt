package com.kb.ime

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * In-keyboard quick-settings sheet (plan/19 step 2 + plan/20 step 3).
 *
 * The ⚙ key opens THIS sheet — never a deep-link to the full Layout page.
 * It edits the **current tab only**: layout pills for this tab, enable /
 * disable for this tab (movable packs; fixed pins render info-only),
 * undo, incognito toggle, clipboard entry. The Dictionaries row
 * ([DictStackOrder]/Settings) remains the single full editor; the
 * "Details" button jumps there so two editors never diverge.
 *
 * All state arrives via [QuickSheetState] from the host (which owns the
 * live tab + layout resolve); every action is a callback. No prefs writes
 * happen here — the host persists and re-resolves.
 */
data class QuickSheetState(
    /** Engine cat id of the current tab (e.g. `words`, `ne`). */
    val tab: String,
    /** Resolved layout id for [tab]. */
    val layoutId: String,
    /** Persisted enable flag for [tab] (movable packs only). */
    val tabEnabled: Boolean,
    /** False for fixed pins (base / ★personal): info only, no toggle. */
    val tabMovable: Boolean,
    val undoAvailable: Boolean,
    val incognito: Boolean
)

@Composable
fun QuickSheet(
    state: QuickSheetState,
    layouts: List<String> = SUPPORTED_LAYOUT_IDS,
    onLayoutPick: (String) -> Unit = {},
    onToggleTab: (Boolean) -> Unit = {},
    onUndo: () -> Unit = {},
    onToggleIncognito: () -> Unit = {},
    onOpenClipboard: () -> Unit = {},
    onOpenDictionaries: () -> Unit = {},
    onDismiss: () -> Unit = {},
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier.fillMaxWidth().padding(8.dp),
        shape = ExpressiveCardShape,
        colors = expressiveCardColors(),
        elevation = CardDefaults.cardElevation(defaultElevation = 2.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = "Quick settings — ${CategoryLabels.label(state.tab)} (this tab only)",
                    style = MaterialTheme.typography.titleSmall
                )
                TextButton(onClick = onDismiss) { Text("Close") }
            }
            Text(
                text = "Layout for this tab",
                style = MaterialTheme.typography.labelMedium,
                modifier = Modifier.padding(top = 8.dp)
            )
            Row(modifier = Modifier.fillMaxWidth()) {
                layouts.forEach { id ->
                    val short = id.removePrefix("t9-")
                    TextButton(
                        onClick = { onLayoutPick(id) },
                        modifier = Modifier.heightIn(min = 48.dp).widthIn(min = 48.dp)
                    ) { Text(if (id == state.layoutId) "[$short]" else short) }
                }
            }
            if (state.tabMovable) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        text = "Show this tab",
                        modifier = Modifier.weight(1f)
                    )
                    Switch(
                        checked = state.tabEnabled,
                        onCheckedChange = onToggleTab
                    )
                }
            } else {
                Text(
                    text = "Fixed tab — always shown (enable/disable lives in Dictionaries)",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.Start
            ) {
                TextButton(
                    onClick = onUndo,
                    enabled = state.undoAvailable,
                    modifier = Modifier.heightIn(min = 48.dp)
                ) { Text("Undo") }
                TextButton(
                    onClick = onToggleIncognito,
                    modifier = Modifier.heightIn(min = 48.dp)
                ) { Text(if (state.incognito) "Incognito: ON" else "Incognito: OFF") }
                TextButton(
                    onClick = onOpenClipboard,
                    modifier = Modifier.heightIn(min = 48.dp)
                ) { Text("Clipboard") }
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End
            ) {
                TextButton(onClick = onOpenDictionaries) { Text("Details → Dictionaries") }
            }
        }
    }
}
