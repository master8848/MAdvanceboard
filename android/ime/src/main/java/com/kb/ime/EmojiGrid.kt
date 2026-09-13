package com.kb.ime

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
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
import androidx.compose.ui.unit.dp

/**
 * Friend-ready emoji picker (plan/24 §5): scrollable grid + recents +
 * search (WhatsApp parity). The raw `emoji.json` keyword rows are
 * undiscoverable on their own — this sheet is the visual path.
 *
 * [glyphs] are the display emoji supplied by the host (parsed from the
 * installed emoji pack or the live candidates); [recents] come from
 * [EmojiRecents]. Picks commit via [onPick] (the host also records the
 * recent). Search is a dumb substring filter ([EmojiRecentMath.filter]) —
 * keyword search rides the existing strip candidates, not this grid.
 */
@Composable
fun EmojiPickerSheet(
    glyphs: List<String>,
    recents: List<String>,
    onPick: (String) -> Unit = {},
    onDismiss: () -> Unit = {},
    modifier: Modifier = Modifier
) {
    var query by remember { mutableStateOf("") }
    val shown = remember(glyphs, query) { EmojiRecentMath.filter(glyphs, query) }

    Card(
        modifier = modifier.fillMaxWidth().padding(8.dp),
        shape = ExpressiveCardShape,
        colors = expressiveCardColors(),
        elevation = CardDefaults.cardElevation(defaultElevation = 2.dp)
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Text(
                    text = "Emoji",
                    style = MaterialTheme.typography.titleSmall
                )
                TextButton(onClick = onDismiss) { Text("Close") }
            }
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                label = { Text("Search emoji") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth()
            )
            if (recents.isNotEmpty() && query.isBlank()) {
                Text(
                    text = "Recents",
                    style = MaterialTheme.typography.labelMedium,
                    modifier = Modifier.padding(top = 8.dp)
                )
                Row(modifier = Modifier.fillMaxWidth()) {
                    recents.take(8).forEach { glyph ->
                        Text(
                            text = glyph,
                            style = MaterialTheme.typography.headlineSmall,
                            modifier = Modifier
                                .clickable { onPick(glyph) }
                                .padding(8.dp)
                                .widthIn(min = 48.dp)
                                .heightIn(min = 48.dp)
                        )
                    }
                }
            }
            LazyVerticalGrid(
                columns = GridCells.Adaptive(48.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 240.dp)
            ) {
                items(shown) { glyph ->
                    Text(
                        text = glyph,
                        style = MaterialTheme.typography.headlineSmall,
                        modifier = Modifier
                            .clickable { onPick(glyph) }
                            .padding(8.dp)
                            .widthIn(min = 48.dp)
                            .heightIn(min = 48.dp)
                    )
                }
            }
            if (shown.isEmpty()) {
                Text(
                    text = "No emoji match — try the strip search.",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
    }
}
