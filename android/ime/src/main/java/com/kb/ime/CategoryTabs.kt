package com.kb.ime

import android.os.SystemClock
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp

val DEFAULT_CATEGORIES = listOf(
    "numbers", "words", "ne", "js", "rust", "html", "emoji", "math", "★personal"
)

/**
 * Zone separation (plan 01): this row owns horizontal scroll/swipe ONLY.
 * Tap / h-scroll / h-fling switches category (sets dict filter + layout
 * resolve); no delete, no accept, no hide here. The Pad must never switch
 * categories (rejected: conflicts with delete/accept flings) — see
 * [classifyCategoryTabsSwipe] for the full disambiguation rule.
 *
 * Swipe handling mirrors [SuggestionStrip]: the drag observer accumulates
 * without consuming, so taps still reach Tab onClick and the row's own
 * scroll keeps working. On drag end a fling qualifying per
 * [classifyCategoryTabsSwipe] steps selection via [onSelect] (clamped at the
 * ends, no wrap) and reports it via [onSwipe] for the gesture log; anything
 * else (tap, short/slow drag, vertical) is ignored here. TalkBack users keep
 * the focusable tabs themselves plus the Next-Category fallback button —
 * this swipe never replaces those paths.
 */
@Composable
fun CategoryTabs(
    categories: List<String> = DEFAULT_CATEGORIES,
    selected: Int = 1,
    onSelect: (Int) -> Unit = {},
    onExpandAll: () -> Unit = {},
    onSwipe: (fromIndex: Int, toIndex: Int) -> Unit = { _, _ -> },
    /**
     * Long-press on a tab (plan/08): opens the placement popup for that
     * category (pack record + move up/down + enable/disable). Taps still
     * reach [Tab.onClick]: the detector below only handles long-press
     * (no `onTap`, so up-events pass through unconsumed).
     */
    onTabLongPress: (String) -> Unit = {},
    modifier: Modifier = Modifier
) {
    val density = LocalDensity.current.density
    // Fresh long-press sink without restarting detectors on recompose.
    val latestOnTabLongPress by rememberUpdatedState(onTabLongPress)
    val latestSelected by rememberUpdatedState(selected)
    val latestOnSelect by rememberUpdatedState(onSelect)
    val latestOnSwipe by rememberUpdatedState(onSwipe)
    var dragX by remember { mutableFloatStateOf(0f) }
    var dragY by remember { mutableFloatStateOf(0f) }
    var downT by remember { mutableLongStateOf(0L) }
    Row(modifier = modifier.fillMaxWidth()) {
        PrimaryScrollableTabRow(
            selectedTabIndex = selected,
            modifier = Modifier
                .weight(1f)
                .pointerInput(categories.size) {
                    detectDragGestures(
                        onDragStart = {
                            dragX = 0f
                            dragY = 0f
                            downT = SystemClock.uptimeMillis()
                        },
                        onDragEnd = {
                            val dt = (SystemClock.uptimeMillis() - downT).coerceAtLeast(1L)
                            val swipe = classifyCategoryTabsSwipe(dragX, dragY, dt, density)
                            if (swipe != null && categories.isNotEmpty()) {
                                val to = stepCategoryIndex(latestSelected, swipe, categories.size)
                                if (to != latestSelected) {
                                    latestOnSwipe(latestSelected, to)
                                    latestOnSelect(to)
                                }
                            }
                            dragX = 0f
                            dragY = 0f
                        },
                        onDragCancel = { dragX = 0f; dragY = 0f },
                        onDrag = { _, amount ->
                            // No consume(): taps must still reach Tab onClick
                            // and the row's own scroll keeps the stream.
                            dragX += amount.x
                            dragY += amount.y
                        }
                    )
                },
            edgePadding = 0.dp
        ) {
            categories.forEachIndexed { index, cat ->
                Tab(
                    selected = index == selected,
                    onClick = { onSelect(index) },
                    // Engine id stays `words`; humans see "General".
                    text = { Text(CategoryLabels.label(cat)) },
                    modifier = Modifier.pointerInput(cat) {
                        detectTapGestures(
                            onLongPress = { latestOnTabLongPress(cat) }
                        )
                    }
                )
            }
        }
        TextButton(onClick = onExpandAll) { Text("∨") }
    }
}
