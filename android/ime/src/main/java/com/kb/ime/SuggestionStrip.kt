package com.kb.ime

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** Inline suggestion strip: 3 candidates rendered as a [LazyRow]. */
@Composable
fun SuggestionStrip(
    candidates: List<String>,
    onPick: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    LazyRow(modifier = modifier.fillMaxWidth()) {
        items(candidates.take(3)) { word ->
            Text(
                text = word,
                modifier = Modifier
                    .clickable { onPick(word) }
                    .padding(horizontal = 16.dp, vertical = 12.dp)
            )
        }
    }
}
