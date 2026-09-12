package com.kb.ime

import kotlin.math.abs

/**
 * Swipe-down-to-switch-mode classifier for the SuggestionStrip (UX: a gesture
 * to change input MODE, pad 9-key ↔ QWERTY).
 *
 * Why swipe-DOWN on the strip (and not the alternatives):
 * - Pad `←`/`→`/`↑`/`↓` are ALL taken (delete/accept, space, hide) —
 *   any pad gesture collides with a commit path.
 * - Strip `←`/`→` are taken (accept-#1 / cycle-overflow) and strip `↑` is
 *   taken (expand-all). A long horizontal fling on the strip edge would be
 *   the SAME axis in the SAME zone as accept/cycle — indistinguishable.
 * - Tabs-strip `←`/`→` switch category; a vertical fling there fights the
 *   row's own horizontal scroll and the strip is too narrow to aim.
 * - A spacebar swipe collides with QWERTY typing (space is a key) and with
 *   the 12-key bottom-row authority rule.
 * - Strip `↓` is the only free axis in the whole pad+strip+tabs surface.
 *   Down reads as "pull down the other keyboard".
 *
 * Anti-accident design: the distance floor is [MODE_SWITCH_DISTANCE_DP]
 * (32dp, stricter than the standard 24dp fling) PLUS the shared velocity
 * (>200dp/s) and dominant-axis (>1.4:1) gates, so tap jitter, horizontal
 * paging, and the expand-all swipe-up can never satisfy it. The TalkBack
 * QWERTY FAB stays the authoritative accessible path; this gesture is a
 * convenience that duplicates it.
 */
const val MODE_SWITCH_DISTANCE_DP = 32f

/**
 * Pure swipe-down-vs-everything-else decision for the SuggestionStrip.
 *
 * @param dxPx horizontal displacement in px (positive = right)
 * @param dyPx vertical displacement in px (positive = down)
 * @param dtMs gesture duration in ms (must be > 0)
 * @param density display density (dp → px)
 * @return true only for a fast, vertically-dominant, downward fling that
 *   clears [MODE_SWITCH_DISTANCE_DP]; false for taps, short/slow drags,
 *   horizontal flings (accept/cycle), and swipe-up (expand-all).
 */
fun classifyModeSwitch(
    dxPx: Float,
    dyPx: Float,
    dtMs: Long,
    density: Float,
    thresholds: GestureThresholds = GestureThresholds()
): Boolean {
    require(density > 0) { "density must be > 0" }
    if (dtMs <= 0) return false
    // Tap wins — candidate onClick owns taps.
    if (isTap(dxPx, dyPx, dtMs, density, thresholds)) return false
    val dxDp = dxPx / density
    val dyDp = dyPx / density
    val adx = abs(dxDp)
    val ady = abs(dyDp)
    // Down-only: swipe-up is expand-all and must never toggle mode.
    if (dyDp <= 0) return false
    // Vertical-only zone: horizontal flings are accept-#1 / cycle-overflow.
    if (adx >= ady) return false
    if (adx > 0 && ady / adx < thresholds.axisRatio) return false
    // Stricter-than-fling distance floor: no accidental toggles.
    if (ady < MODE_SWITCH_DISTANCE_DP) return false
    val velocity = ady / (dtMs / 1000f)
    if (velocity < thresholds.flingVelocityDpS) return false
    return true
}
