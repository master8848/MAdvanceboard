package com.kb.ime

import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp

/**
 * SuggestionBar (plan 01 zone: tap + horizontal fling ONLY).
 *
 * - tap a candidate = accept that word (+auto-space is the service's commit).
 * - fling `→` = accept candidate #1 (top suggestion).
 * - fling `←` = cycle overflow: next page of 3 through the fetched list.
 * - tap `∨` / swipe-up = expand-all sheet (30, paged) — owned by [ImeScreen].
 *
 * No vertical gestures here; no delete. Every gesture duplicates a visible
 * control: candidates are tappable, overflow shows a pager label, and the
 * expand-all button sits in [CategoryTabs].
 */
@Composable
fun SuggestionStrip(
    candidates: List<String>,
    onPick: (String) -> Unit,
    onAcceptFirst: () -> Unit = {},
    onFling: (zone: String, gesture: String, action: String) -> Unit = { _, _, _ -> },
    modifier: Modifier = Modifier
) {
    var page by remember(candidates) { mutableIntStateOf(0) }
    val pageSize = 3
    val pageCount = (candidates.size + pageSize - 1) / pageSize
    val shown = if (candidates.isEmpty()) {
        emptyList()
    } else {
        candidates.drop((page % pageCount.coerceAtLeast(1)) * pageSize).take(pageSize)
    }
    var dragTotal by remember { mutableIntStateOf(0) }

    Column(modifier = modifier.fillMaxWidth()) {
        LazyRow(
            modifier = Modifier
                .fillMaxWidth()
                .pointerInput(candidates) {
                    detectHorizontalDragGestures(
                        onDragStart = { dragTotal = 0 },
                        onDragEnd = {
                            if (dragTotal > 0) {
                                onFling("bar", "fling-right", "accept-#1")
                                onAcceptFirst()
                            } else if (dragTotal < 0) {
                                if (pageCount > 1) {
                                    page = (page + 1) % pageCount
                                }
                                onFling("bar", "fling-left", "cycle-overflow")
                            }
                            dragTotal = 0
                        },
                        onDragCancel = { dragTotal = 0 },
                        onHorizontalDrag = { _, delta -> dragTotal += delta.toInt() }
                    )
                }
        ) {
            items(shown) { word ->
                Text(
                    text = word,
                    modifier = Modifier
                        .clickable { onPick(word) }
                        .padding(horizontal = 16.dp, vertical = 12.dp)
                )
            }
        }
        if (pageCount > 1) {
            Text(
                text = "… ${page + 1}/$pageCount — fling ← for more",
                style = MaterialTheme.typography.labelSmall,
                modifier = Modifier.padding(horizontal = 16.dp)
            )
        }
    }
}
