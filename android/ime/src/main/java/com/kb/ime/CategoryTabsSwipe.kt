package com.kb.ime

import kotlin.math.abs

/**
 * Swipe-to-switch classifier for the CategoryTabs strip (plan 01).
 *
 * Disambiguation rule (zone ownership — plan 01 §Zone separation):
 * - ONLY a horizontal fling that starts inside the CategoryTabs strip switches
 *   the active category. Callers must attach this to the tabs row and to
 *   nothing else.
 * - Pad `←`/`→` are ALWAYS delete/accept and must NEVER switch tabs (rejected:
 *   conflicts with delete/accept flings). The Pad has no tab-switch path, so
 *   there is no gesture id to collide with — this classifier is consulted only
 *   for touches the tabs strip itself observed.
 * - SuggestionBar `←`/`→` are ALWAYS accept-#1/cycle-overflow ([SuggestionStrip]
 *   owns that stream). A bar fling never reaches this classifier because the
 *   bar and the tabs strip are disjoint composables with disjoint pointer
 *   streams.
 * - Tap wins: below slop + below [GestureThresholds.tapMaxMs] returns null so
 *   the Tab's own onClick handles selection (tap still works, unchanged).
 * - TalkBack/disabled-fling fallback (Next-Category button in [ImeScreen])
 *   stays the authoritative accessible path; this swipe is a scroll-equivalent
 *   convenience and never replaces it.
 *
 * Thresholds reuse the ship defaults ([GestureThresholds]: >24dp displacement,
 * >200dp/s velocity, dominant-axis ratio >1.4:1) so tab swipes feel identical
 * to Pad/Bar flings.
 */
enum class CategoryTabsSwipe { PREVIOUS, NEXT }

/**
 * Pure swipe-vs-tap decision for the CategoryTabs strip.
 *
 * @param dxPx horizontal displacement in px (positive = right)
 * @param dyPx vertical displacement in px (positive = down)
 * @param dtMs gesture duration in ms (must be > 0)
 * @param density display density (dp → px)
 * @return [CategoryTabsSwipe.NEXT] for a leftward fling (forward, ViewPager
 *   convention), [CategoryTabsSwipe.PREVIOUS] for a rightward fling, or null
 *   when the touch is a tap, too short/slow, or not horizontally dominant.
 */
fun classifyCategoryTabsSwipe(
    dxPx: Float,
    dyPx: Float,
    dtMs: Long,
    density: Float,
    thresholds: GestureThresholds = GestureThresholds()
): CategoryTabsSwipe? {
    require(density > 0) { "density must be > 0" }
    if (dtMs <= 0) return null
    // Tap wins — the Tab onClick owns selection for taps.
    if (isTap(dxPx, dyPx, dtMs, density, thresholds)) return null
    val dxDp = dxPx / density
    val dyDp = dyPx / density
    val adx = abs(dxDp)
    val ady = abs(dyDp)
    // Horizontal-only zone: vertical-dominant movement never switches tabs.
    if (ady > adx) return null
    if (ady > 0 && adx / ady < thresholds.axisRatio) return null
    if (adx < thresholds.flingThresholdDp) return null
    val velocity = adx / (dtMs / 1000f)
    if (velocity < thresholds.flingVelocityDpS) return null
    return if (dxPx < 0) CategoryTabsSwipe.NEXT else CategoryTabsSwipe.PREVIOUS
}

/**
 * Applies [swipe] to [current] over [count] tabs.
 *
 * Clamps at the ends (no wrap): overswiping past the first/last tab is a
 * no-op instead of a surprise jump. The Next-Category fallback button wraps
 * instead — it is an explicit button press, not an overshoot.
 */
fun stepCategoryIndex(current: Int, swipe: CategoryTabsSwipe, count: Int): Int {
    require(count > 0) { "count must be > 0" }
    val clamped = current.coerceIn(0, count - 1)
    return when (swipe) {
        CategoryTabsSwipe.PREVIOUS -> (clamped - 1).coerceAtLeast(0)
        CategoryTabsSwipe.NEXT -> (clamped + 1).coerceAtMost(count - 1)
    }
}
