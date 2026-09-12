package com.kb.ime

import android.os.SystemClock
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import kotlin.math.abs

/**
 * SuggestionBar (plan 01 zone: tap + horizontal fling + vertical swipe).
 *
 * - tap a candidate = accept that word (+auto-space is the service's commit).
 * - fling `→` = accept candidate #1 (top suggestion).
 * - fling `←` = cycle overflow: next page of 3 through the fetched list.
 * - tap `∨` / swipe-up = expand-all sheet (30, paged) — owned by [ImeScreen].
 * - swipe-DOWN = toggle pad 9-key ↔ QWERTY ([classifyModeSwitch]: the only
 *   free axis on the strip; duplicates the TalkBack QWERTY FAB, which stays
 *   authoritative). Vertical down was previously ignored — no gesture moves.
 *
 * No delete here. Every gesture duplicates a visible control: candidates
 * are tappable, overflow shows a pager label, the expand-all button sits in
 * [CategoryTabs], and the QWERTY FAB sits beside this strip.
 */
@Composable
fun SuggestionStrip(
    candidates: List<String>,
    onPick: (String) -> Unit,
    onAcceptFirst: () -> Unit = {},
    onExpandAll: () -> Unit = {},
    /** Strip swipe-down (see [classifyModeSwitch]); defaults to no-op. */
    onToggleMode: () -> Unit = {},
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
    val density = LocalDensity.current
    // Single drag classifier (both axes): horizontal → accept/cycle,
    // swipe-up → expand-all sheet, swipe-down → QWERTY/9-key toggle.
    // Vertical down was previously ignored, so nothing moves.
    var dragX by remember { mutableIntStateOf(0) }
    var dragY by remember { mutableIntStateOf(0) }
    var downT by remember { mutableLongStateOf(0L) }

    Column(modifier = modifier.fillMaxWidth()) {
        LazyRow(
            modifier = Modifier
                .fillMaxWidth()
                .pointerInput(candidates) {
                    detectDragGestures(
                        onDragStart = {
                            dragX = 0
                            dragY = 0
                            downT = SystemClock.uptimeMillis()
                        },
                        onDragEnd = {
                            val dt = (SystemClock.uptimeMillis() - downT).coerceAtLeast(1L)
                            val upThresholdPx = with(density) { 24.dp.toPx() }
                            if (abs(dragX) >= abs(dragY)) {
                                if (dragX > 0) {
                                    onFling("bar", "fling-right", "accept-#1")
                                    onAcceptFirst()
                                } else if (dragX < 0) {
                                    if (pageCount > 1) {
                                        page = (page + 1) % pageCount
                                    }
                                    onFling("bar", "fling-left", "cycle-overflow")
                                }
                            } else if (dragY < -upThresholdPx) {
                                onFling("bar", "swipe-up", "expand-all")
                                onExpandAll()
                            } else if (classifyModeSwitch(
                                    dragX.toFloat(), dragY.toFloat(), dt, density.density
                                )
                            ) {
                                onFling("bar", "swipe-down", "toggle-mode")
                                onToggleMode()
                            }
                            dragX = 0
                            dragY = 0
                        },
                        onDragCancel = { dragX = 0; dragY = 0 },
                        onDrag = { _, amount ->
                            // No consume(): the 3-item row fits without scrolling,
                            // so there is no scroll competitor for this stream.
                            dragX += amount.x.toInt()
                            dragY += amount.y.toInt()
                        }
                    )
                }
        ) {
            itemsIndexed(shown) { index, word ->
                // Top suggestion leads in primary; the rest sit tonal.
                // 12dp vertical padding keeps the ~48dp touch target.
                val container = if (index == 0) {
                    MaterialTheme.colorScheme.primaryContainer
                } else {
                    MaterialTheme.colorScheme.secondaryContainer
                }
                val content = if (index == 0) {
                    MaterialTheme.colorScheme.onPrimaryContainer
                } else {
                    MaterialTheme.colorScheme.onSecondaryContainer
                }
                Surface(
                    onClick = { onPick(word) },
                    shape = ExpressiveChipShape,
                    color = container,
                    tonalElevation = 1.dp,
                    modifier = Modifier.padding(horizontal = 4.dp, vertical = 4.dp)
                ) {
                    Text(
                        text = word,
                        color = content,
                        style = MaterialTheme.typography.bodyLarge,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)
                    )
                }
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
