package com.kb.ime

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.ScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

val DEFAULT_CATEGORIES = listOf(
    "numbers", "words", "ne", "js", "rust", "html", "emoji", "math", "★personal"
)

/**
 * Zone separation (plan 01): this row owns horizontal scroll/swipe ONLY.
 * Tap / h-scroll switches category (sets dict filter + layout resolve);
 * no delete, no accept, no hide here. The Pad must never switch categories
 * (rejected: conflicts with delete/accept flings).
 */
@Composable
fun CategoryTabs(
    categories: List<String> = DEFAULT_CATEGORIES,
    selected: Int = 1,
    onSelect: (Int) -> Unit = {},
    onExpandAll: () -> Unit = {},
    modifier: Modifier = Modifier
) {
    Row(modifier = modifier.fillMaxWidth()) {
        ScrollableTabRow(
            selectedTabIndex = selected,
            modifier = Modifier.weight(1f),
            edgePadding = 0.dp
        ) {
            categories.forEachIndexed { index, cat ->
                Tab(
                    selected = index == selected,
                    onClick = { onSelect(index) },
                    text = { Text(cat) }
                )
            }
        }
        TextButton(onClick = onExpandAll) { Text("∨") }
    }
}
