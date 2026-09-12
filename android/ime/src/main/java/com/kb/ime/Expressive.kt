package com.kb.ime

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CardColors
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp

/**
 * Shared Material3 Expressive tokens for the IME surfaces (strip, popups,
 * coach, clipboard). One place for shape/color/elevation so the pad, strip,
 * and sheets stay consistent in light AND dark themes.
 *
 * Rules:
 * - No hardcoded content colors: every container comes from
 *   [MaterialTheme.colorScheme] (dark-mode correct by construction).
 * - Expressive radii: 20dp cards, 16dp chips (larger than M3 defaults).
 * - Tonal elevation (2dp) instead of shadows: cards lift via
 *   `surfaceContainerHigh` + tonal elevation, never a hardcoded scrim.
 *
 * The single sanctioned exception is incognito chrome ([DarkIncognito]):
 * plan/11 §2 requires the strip to render dark under ANY theme so
 * incognito is unmistakable — that hardcoded color is the design, not a
 * bug. Framework-View pad keys ([PadView]/[QwertyView] `android.widget`
 * buttons) are NOT covered here: they need a Material theme overlay in
 * the host theme, which is a separate (View-system) change.
 */
val ExpressiveCardShape = RoundedCornerShape(20.dp)
val ExpressiveChipShape = RoundedCornerShape(16.dp)

/** Tonal card colors shared by every IME popup/sheet. */
@Composable
fun expressiveCardColors(): CardColors = CardDefaults.cardColors(
    containerColor = MaterialTheme.colorScheme.surfaceContainerHigh
)
