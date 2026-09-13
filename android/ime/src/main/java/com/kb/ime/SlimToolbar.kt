package com.kb.ime

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * Slim toolbar row (Gboard-inspired, NOT a clone): a single compact row
 * above the suggestion strip with icon-text glyphs (no new deps).
 *
 * Layout budget: [Row] is fixed at 40dp tall (36-40dp spec). Buttons keep
 * a 48dp-minimum horizontal touch target ([widthIn]) for finger tapping;
 * vertical stays compact (Gboard-style slim row). The full 48x48dp
 * accessible path is the [FallbackActionBar] (shown when an AT service
 * disables flings) — this toolbar is a convenience row, never the sole
 * accessible path.
 *
 * Buttons (all text glyphs):
 * - `…` overflow → expand-all candidates sheet.
 * - `🙂` emoji → visual emoji picker ([EmojiPickerSheet]: grid + recents).
 * - `📋` clipboard → toggle [ClipboardPanel] (hidden while incognito).
 * - `🎭` incognito → manual mask toggle (moved here to declutter strip).
 * - `↩` undo → delete last word (see [KbInputMethodService.toolbarUndo]).
 * - `↪` redo → re-commit last toolbar-deleted word, disabled when empty.
 * - `✎` symbols → generic symbols sheet (same as fallback Sym).
 */
@Composable
fun SlimToolbar(
    onOverflow: () -> Unit = {},
    onEmoji: () -> Unit = {},
    onClipboard: () -> Unit = {},
    onToggleIncognito: () -> Unit = {},
    onUndo: () -> Unit = {},
    onRedo: () -> Unit = {},
    onSymbols: () -> Unit = {},
    clipboardActive: Boolean = false,
    incognito: Boolean = false,
    redoAvailable: Boolean = false,
    modifier: Modifier = Modifier
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(40.dp),
        horizontalArrangement = Arrangement.Start,
        verticalAlignment = Alignment.CenterVertically
    ) {
        ToolbarGlyphButton(glyph = "…", contentDesc = "More candidates", onClick = onOverflow)
        ToolbarGlyphButton(glyph = "🙂", contentDesc = "Emoji picker", onClick = onEmoji)
        ToolbarGlyphButton(
            glyph = if (clipboardActive) "[📋]" else "📋",
            contentDesc = "Clipboard history",
            onClick = onClipboard
        )
        ToolbarGlyphButton(
            glyph = if (incognito) "[🎭]" else "🎭",
            contentDesc = "Incognito toggle",
            onClick = onToggleIncognito
        )
        ToolbarGlyphButton(glyph = "↩", contentDesc = "Undo — delete last word", onClick = onUndo)
        ToolbarGlyphButton(
            glyph = "↪",
            contentDesc = "Redo — re-commit deleted word",
            onClick = onRedo,
            enabled = redoAvailable
        )
        ToolbarGlyphButton(glyph = "✎", contentDesc = "Symbols", onClick = onSymbols)
    }
}

@Composable
private fun ToolbarGlyphButton(
    glyph: String,
    contentDesc: String,
    onClick: () -> Unit,
    enabled: Boolean = true,
    modifier: Modifier = Modifier
) {
    TextButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier
            .widthIn(min = 48.dp)
            .heightIn(min = 36.dp, max = 40.dp),
        contentPadding = PaddingValues(horizontal = 4.dp, vertical = 2.dp)
    ) {
        Text(
            text = glyph,
            style = MaterialTheme.typography.bodyLarge,
            color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant
            else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.38f)
        )
    }
}
