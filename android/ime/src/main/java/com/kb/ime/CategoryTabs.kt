package com.kb.ime

import android.os.SystemClock
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp

val DEFAULT_CATEGORIES = listOf(
    "numbers", "words", "ne", "js", "rust", "html", "emoji", "math", "medical", "★personal"
)

/**
 * Zone separation (plan 01): this row owns horizontal scroll/swipe ONLY.
 * Tap / h-scroll / h-fling switches category (sets dict filter + layout
 * resolve); no delete, no accept, no hide here. The Pad must never switch
 * categories (rejected: conflicts with delete/accept flings) — see
 * [classifyCategoryTabsSwipe] for the full disambiguation rule.
 *
 * Compact single row (FlorisBoard-style): fixed 48dp height, badge labels
 * ([CategoryLabels]: `123` / `E` / `ने` / `JS` / `Rs` / `HTML` / `😀` /
 * `∑` / `⚕` / `★`), horizontally scrollable, no debug text.
 *
 * Swipe handling mirrors [SuggestionStrip]: the drag observer accumulates
 * without consuming, so taps still reach Tab onClick and the row's own
 * scroll keeps working. On drag end a fling qualifying per
 * [classifyCategoryTabsSwipe] (24dp / 200dp/s ship thresholds, shared with
 * Pad/Bar flings) steps selection via [onSelect] (clamped at the ends, no
 * wrap) and reports it via [onSwipe] for the gesture log; anything else
 * (tap, short/slow drag, vertical) is ignored here. While a horizontal
 * drag passes the distance floor the expand key previews the direction
 * (`‹`/`›`) as live swipe feedback. TalkBack users keep the focusable
 * tabs themselves plus the Next-Category fallback button — this swipe
 * never replaces those paths.
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
    val context = LocalContext.current
    // Compact single row: fixed 48dp (touch-target floor AND the 40-48dp
    // tab budget), badges scroll horizontally, no debug text.
    // Live swipe feedback: once the horizontal drag passes the 24dp floor
    // the expand key previews the step direction (‹/›).
    val swipeHint = when {
        dragX < -24f * density -> "‹"
        dragX > 24f * density -> "›"
        else -> "∨"
    }
    Row(
        modifier = modifier.fillMaxWidth().height(48.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        PrimaryScrollableTabRow(
            selectedTabIndex = selected,
            // Transparent bed: the strip floats on the keyboard background
            // instead of drawing an opaque surface band (dark-mode safe —
            // selected/unselected come from the scheme explicitly).
            containerColor = Color.Transparent,
            contentColor = MaterialTheme.colorScheme.primary,
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
                    // Engine id stays `words`; humans see the `E` badge.
                    // Compact label style so the 48dp row never clips.
                    text = {
                        Text(
                            CategoryLabels.label(cat),
                            style = MaterialTheme.typography.labelMedium,
                            maxLines = 1
                        )
                    },
                    selectedContentColor = MaterialTheme.colorScheme.primary,
                    unselectedContentColor =
                        if (ThemeStore.isDarkEffective(context)) Color(KbDarkMuted.toInt())
                        else MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.pointerInput(cat) {
                        detectTapGestures(
                            onLongPress = { latestOnTabLongPress(cat) }
                        )
                    }
                )
            }
        }
        // Expand-all: 48dp minimum touch target, doubles as the live
        // swipe-direction hint (‹/› while a tab-switch drag is armed).
        TextButton(
            onClick = onExpandAll,
            modifier = Modifier
                .heightIn(min = 48.dp)
                .widthIn(min = 48.dp)
        ) { Text(swipeHint, style = MaterialTheme.typography.labelMedium) }
    }
}
