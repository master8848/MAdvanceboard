package com.kb.ime

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/**
 * Incognito entry banner (plan/11 §2): dark strip badge + explicit
 * "nothing saved" notice. Dismissible; re-arms on the next incognito
 * entry (the host owns that state). Rendered dark even under a light
 * theme so incognito is unmistakable.
 */
@Composable
fun IncognitoBanner(
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(Color(0xFF1B1B1F))
            .padding(horizontal = 16.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween
    ) {
        Text(
            text = "🎭 Incognito — nothing saved",
            color = Color.White,
            style = MaterialTheme.typography.labelLarge,
            modifier = Modifier
                .weight(1f)
                .padding(vertical = 12.dp)
        )
        TextButton(onClick = onDismiss) {
            Text("Dismiss", color = Color(0xFFB3C5FF))
        }
    }
}

/**
 * Clipboard history panel (plan/11 §3): paste strip contents.
 *
 * - Pins section on top (📌), recency below; substring search filter.
 * - Tap = paste (host commits raw — never learned, never logged).
 * - Long-press = pin/unpin + delete + full-text preview row.
 * - Clear-all is two-step: first tap arms the confirm row ("Clear pins
 *   too?" → keep-pins vs wipe-everything), never a one-tap wipe.
 * - Every host callback failure must surface via [onError] (status line),
 *   never a silent no-op — the host wraps accordingly.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ClipboardPanel(
    items: List<ClipboardItem>,
    onPaste: (ClipboardItem) -> Unit,
    onTogglePin: (ClipboardItem) -> Unit,
    onDelete: (ClipboardItem) -> Unit,
    onClearUnpinned: () -> Unit,
    onClearAll: () -> Unit,
    onClose: () -> Unit,
    onError: (String) -> Unit = {},
    modifier: Modifier = Modifier
) {
    var query by remember { mutableStateOf("") }
    var confirmClear by remember { mutableStateOf(false) }
    var expandedId by remember { mutableStateOf<String?>(null) }
    val shown = remember(items, query) { ClipboardHistory.search(items, query) }
    val pins = remember(shown) { shown.filter { it.pinned } }
    val rest = remember(shown) { shown.filterNot { it.pinned } }

    Column(modifier = modifier.fillMaxWidth().padding(8.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Text(
                text = "Clipboard (${items.size})",
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(vertical = 12.dp)
            )
            TextButton(onClick = onClose) { Text("Close") }
        }
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            label = { Text("Search clipboard") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth()
        )
        if (shown.isEmpty()) {
            Text(
                text = if (items.isEmpty()) "Clipboard empty — copies while typing appear here."
                else "No matches for \"$query\".",
                style = MaterialTheme.typography.labelMedium,
                modifier = Modifier.padding(16.dp)
            )
        } else {
            LazyColumn(modifier = Modifier.fillMaxWidth()) {
                if (pins.isNotEmpty()) {
                    item {
                        Text(
                            text = "📌 Pinned",
                            style = MaterialTheme.typography.labelMedium,
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                        )
                    }
                    items(pins, key = { it.id }) { item ->
                        ClipboardRow(
                            item = item,
                            expanded = expandedId == item.id,
                            onPaste = onPaste,
                            onToggleExpand = {
                                expandedId = if (expandedId == item.id) null else item.id
                            },
                            onTogglePin = onTogglePin,
                            onDelete = onDelete,
                            onError = onError
                        )
                    }
                }
                if (rest.isNotEmpty()) {
                    item {
                        Text(
                            text = "Recent",
                            style = MaterialTheme.typography.labelMedium,
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                        )
                    }
                    items(rest, key = { it.id }) { item ->
                        ClipboardRow(
                            item = item,
                            expanded = expandedId == item.id,
                            onPaste = onPaste,
                            onToggleExpand = {
                                expandedId = if (expandedId == item.id) null else item.id
                            },
                            onTogglePin = onTogglePin,
                            onDelete = onDelete,
                            onError = onError
                        )
                    }
                }
            }
        }
        if (!confirmClear) {
            TextButton(onClick = { confirmClear = true }) { Text("Clear…") }
        } else {
            Column(modifier = Modifier.fillMaxWidth()) {
                Text(
                    text = "Clear pins too?",
                    style = MaterialTheme.typography.labelLarge,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                )
                Row(modifier = Modifier.fillMaxWidth()) {
                    TextButton(onClick = {
                        confirmClear = false
                        try {
                            onClearUnpinned()
                        } catch (e: Exception) {
                            onError("Clear failed: ${e.message}")
                        }
                    }) { Text("Keep pins") }
                    TextButton(onClick = {
                        confirmClear = false
                        try {
                            onClearAll()
                        } catch (e: Exception) {
                            onError("Clear failed: ${e.message}")
                        }
                    }) { Text("Clear everything") }
                    TextButton(onClick = { confirmClear = false }) { Text("Cancel") }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ClipboardRow(
    item: ClipboardItem,
    expanded: Boolean,
    onPaste: (ClipboardItem) -> Unit,
    onToggleExpand: () -> Unit,
    onTogglePin: (ClipboardItem) -> Unit,
    onDelete: (ClipboardItem) -> Unit,
    onError: (String) -> Unit
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .combinedClickable(
                    onClick = {
                        try {
                            onPaste(item)
                        } catch (e: Exception) {
                            onError("Paste failed: ${e.message}")
                        }
                    },
                    onLongClick = onToggleExpand
                )
                .padding(horizontal = 16.dp, vertical = 12.dp),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Text(
                text = (if (item.pinned) "📌 " else "") + item.text,
                maxLines = if (expanded) Int.MAX_VALUE else 2,
                overflow = if (expanded) TextOverflow.Visible else TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f)
            )
        }
        if (expanded) {
            Row(modifier = Modifier.fillMaxWidth()) {
                TextButton(onClick = {
                    try {
                        onTogglePin(item)
                    } catch (e: Exception) {
                        onError("Pin failed: ${e.message}")
                    }
                }) { Text(if (item.pinned) "Unpin" else "Pin") }
                TextButton(onClick = {
                    try {
                        onDelete(item)
                    } catch (e: Exception) {
                        onError("Delete failed: ${e.message}")
                    }
                }) { Text("Delete") }
                TextButton(onClick = {
                    try {
                        onPaste(item)
                    } catch (e: Exception) {
                        onError("Paste failed: ${e.message}")
                    }
                }) { Text("Paste") }
            }
        }
    }
}
