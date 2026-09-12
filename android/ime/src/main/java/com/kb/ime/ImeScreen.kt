package com.kb.ime

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * Hybrid IME screen: [CategoryTabs] + [SuggestionStrip] + expand-all fullscreen
 * [LazyColumn] list + QWERTY fallback toggle + 9/12/16 pad-size toggle.
 *
 * [candidates] are live results fed by the host service from
 * Predictor.suggest (shared T9/QWERTY path) — never hardcoded here.
 * [activeLayoutId] reflects the service's pad; [onLayoutChanged] persists
 * the user's toggle (PadModeStore) and swaps the pad.
 */
@Composable
fun ImeScreen(
    candidates: List<String> = emptyList(),
    onCandidatePicked: (String) -> Unit,
    onExpandAll: () -> Unit = {},
    onToggleQwerty: () -> Unit = {},
    onCategoryChanged: (String) -> Unit = {},
    activeLayoutId: String = "t9-9",
    onLayoutChanged: (String) -> Unit = {}
) {
    var selectedTab by remember { mutableIntStateOf(1) }
    var expanded by remember { mutableStateOf(false) }

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
                modifier = Modifier.weight(1f)
            )
            TextButton(onClick = onToggleQwerty) { Text("QWERTY") }
        }
        // Pad-size toggle: 9/12/16 segmented selector. Category tabs above
        // are unaffected; the host swaps the PadView on selection.
        PadSizeToggle(
            activeLayoutId = activeLayoutId,
            onSelect = onLayoutChanged
        )
        if (expanded) {
            ExpandAllList(
                items = candidates.take(30),
                onPick = {
                    expanded = false
                    onCandidatePicked(it)
                },
                onClose = { expanded = false }
            )
        }
    }
}

/** Fullscreen paged candidate list (30 items) opened by the expand-all button. */
@Composable
fun ExpandAllList(
    items: List<String>,
    onPick: (String) -> Unit,
    onClose: () -> Unit
) {
    Column(modifier = Modifier.fillMaxSize()) {
        Button(onClick = onClose, modifier = Modifier.padding(8.dp)) {
            Text("Close")
        }
        LazyColumn(modifier = Modifier.fillMaxSize()) {
            items(items) { word ->
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
 * selection flows to the host, which persists it (PadModeStore) and swaps
 * the PadView. Category tabs and the QWERTY toggle are unaffected.
 */
@Composable
fun PadSizeToggle(
    activeLayoutId: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    val options = listOf("t9-9" to "9", "t9-12" to "12", "t9-16" to "16")
    Row(modifier = modifier.fillMaxWidth()) {
        Text(
            text = "Pad:",
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)
        )
        options.forEach { (id, short) ->
            val label = if (id == activeLayoutId) "[$short]" else short
            TextButton(onClick = { onSelect(id) }) { Text(label) }
        }
    }
}
