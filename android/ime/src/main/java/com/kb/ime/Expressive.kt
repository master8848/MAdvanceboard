package com.kb.ime

import android.content.Context
import android.widget.Button
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CardColors
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
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

/**
 * Gboard-inspired dark palette (colors ONLY — layout/shapes untouched).
 * Light theme values below are unchanged (contrast already passes).
 */
const val KbDarkBackground = 0xFF1C1C21
const val KbDarkKey = 0xFF2E2E33
const val KbDarkKeyText = 0xFFE6E6E6
/** Functional keys (space/delete/sym/control/globe): slightly lighter slab. */
const val KbDarkFunctional = 0xFF3F3F47
const val KbDarkFunctionalText = 0xFFE6E6E6
/** Accent enter: pink slab with dark glyph (both themes, light is deeper). */
const val KbDarkEnter = 0xFFF0B8D2
const val KbDarkEnterText = 0xFF1C1C21
const val KbLightEnter = 0xFFE3A9C4
const val KbLightEnterText = 0xFF1C1C21
/** Muted toolbar/strip grey (dark). */
const val KbDarkMuted = 0xFF9E9EA8

/** Tonal card colors shared by every IME popup/sheet. */
@Composable
fun expressiveCardColors(): CardColors = CardDefaults.cardColors(
    containerColor = MaterialTheme.colorScheme.surfaceContainerHigh
)

/**
 * Single theme entry point for every Compose surface (settings `:app` UI +
 * IME strip/popups). `darkTheme` comes from [ThemeStore.isDarkEffective] so
 * the System/Light/Dark setting is honored everywhere; defaults to the
 * framework value when no stored mode is consulted.
 */
@Composable
fun KbImeTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit
) {
    MaterialTheme(
        colorScheme = if (darkTheme) darkColorScheme() else lightColorScheme(),
        content = content
    )
}

/**
 * Dark-aware styling for framework-View pad keys ([PadView]/[QwertyView]
 * `android.widget` buttons). Compose theming cannot reach the View system,
 * so keys are tinted from the effective night mode instead: dark slate with
 * near-white text under a dark theme, light grey with black text otherwise.
 * Called at construction and via `refreshKeyTheme()` on config/start-input
 * so a theme change applies without reinstalling the keyboard.
 */
fun styleKeyButton(btn: Button, context: Context) {
    val dark = ThemeStore.isDarkEffective(context)
    btn.backgroundTintList = null
    if (dark) {
        btn.setBackgroundColor(KbDarkKey.toInt())
        btn.setTextColor(KbDarkKeyText.toInt())
    } else {
        btn.setBackgroundColor(0xFFE4E4E9.toInt())
        btn.setTextColor(0xFF101014.toInt())
    }
}

/**
 * Functional keys (space/delete/sym/control/globe): distinct slab from
 * letter keys — slightly lighter than [KbDarkKey] in dark, unchanged
 * light tint otherwise.
 */
fun styleFunctionalKey(btn: Button, context: Context) {
    val dark = ThemeStore.isDarkEffective(context)
    if (dark) {
        btn.backgroundTintList =
            android.content.res.ColorStateList.valueOf(KbDarkFunctional.toInt())
        btn.setTextColor(KbDarkFunctionalText.toInt())
    } else {
        btn.backgroundTintList =
            android.content.res.ColorStateList.valueOf(0xFF9AA5B1.toInt())
        btn.setTextColor(0xFF102027.toInt())
    }
}

/**
 * Accent enter key: circular pink slab with a dark glyph (both themes —
 * light uses a slightly deeper pink so the slab reads on a light pad).
 * The ONLY key allowed a circular shape; every other key keeps its
 * existing rectangle. Callers should give enter a square target so the
 * oval renders as a circle (QWERTY does); grid pads render an ellipse
 * from the same drawable without changing their layout.
 */
fun styleEnterKey(btn: Button, context: Context) {
    val dark = ThemeStore.isDarkEffective(context)
    val fill = if (dark) KbDarkEnter else KbLightEnter
    val glyph = if (dark) KbDarkEnterText else KbLightEnterText
    val oval = android.graphics.drawable.GradientDrawable().apply {
        shape = android.graphics.drawable.GradientDrawable.OVAL
        setColor(fill.toInt())
    }
    btn.background = oval
    btn.backgroundTintList = null
    btn.setTextColor(glyph.toInt())
}
