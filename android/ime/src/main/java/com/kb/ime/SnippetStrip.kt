package com.kb.ime

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * Tier-1 snippet palette strip (plan 07:17 second zone).
 *
 * Sits BELOW the top-3 `suggest()` hits ([SuggestionStrip]): the top zone
 * answers "what word", this zone answers "what syntax/skeleton next".
 * Items are [SnippetItem] (body + commit behavior), never `SuggestionItem`.
 * Empty list renders nothing — no placeholder, no silent state.
 *
 * Tap = expand the snippet at the cursor (caret parks per
 * [SnippetItem.cursorBack]); the buffer wipes after expansion
 * (delete-after-expand, plan 12:62).
 */
@Composable
fun SnippetStrip(
    snippets: List<SnippetItem>,
    onPick: (SnippetItem) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (snippets.isEmpty()) return
    Column(modifier = modifier.fillMaxWidth()) {
        Text(
            text = "Snippets — tap to expand, expires in ~2 min",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 16.dp),
        )
        LazyRow(
            modifier = Modifier.fillMaxWidth(),
            contentPadding = PaddingValues(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            items(snippets) { item ->
                SuggestionChip(
                    onClick = { onPick(item) },
                    label = { Text(item.display) }
                )
            }
        }
    }
}
