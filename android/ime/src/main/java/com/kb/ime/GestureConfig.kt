package com.kb.ime

import kotlin.math.abs
import kotlin.math.max

/**
 * Plan 01 gesture detection defaults (ship values, tunable in
 * Settings → Gesture Tuning; persisted by [GestureTuningStore]).
 *
 * - fling: displacement > [flingThresholdDp] (24dp), velocity > [flingVelocityDpS]
 *   (200dp/s), dominant-axis ratio > [axisRatio] (1.4:1)
 * - word-step 20dp/word, char-step 10dp (delete-slide granularity)
 * - long-press [longPressMs] (400ms, tunable 300–600), slop [touchSlopDp] (8dp)
 *   cancels to tap; tap wins if < [tapMaxMs] (200ms) + < slop
 * - edge [edgeDeadDp] (8dp) dead-zone: gestures starting inside the edge margin
 *   of the Pad bounds are ignored ( evaluated, logged as rejected)
 * - evaluation on ACTION_UP; live preview only for delete-selection
 * - 12-key opt-in scale [twelveKeyScale] (+30% thresholds) when the user
 *   enables pad flings in 12-key mode (default off)
 */
data class GestureThresholds(
    val flingThresholdDp: Float = 24f,
    val flingVelocityDpS: Float = 200f,
    val axisRatio: Float = 1.4f,
    val wordStepDp: Float = 20f,
    val charStepDp: Float = 10f,
    val longPressMs: Long = 400L,
    val touchSlopDp: Float = 8f,
    val edgeDeadDp: Float = 8f,
    val tapMaxMs: Long = 200L,
    val twelveKeyScale: Float = 1.3f
) {
    init {
        require(flingThresholdDp > 0) { "flingThresholdDp must be > 0" }
        require(flingVelocityDpS > 0) { "flingVelocityDpS must be > 0" }
        require(axisRatio > 1f) { "axisRatio must be > 1" }
        require(wordStepDp > 0) { "wordStepDp must be > 0" }
        require(charStepDp > 0) { "charStepDp must be > 0" }
        require(longPressMs in 300L..600L) { "longPressMs must be 300-600" }
        require(touchSlopDp > 0) { "touchSlopDp must be > 0" }
        require(edgeDeadDp >= 0) { "edgeDeadDp must be >= 0" }
    }

    /** 12-key opt-in scales distance/velocity thresholds +30%. */
    fun scaledForTwelveKey(): GestureThresholds = copy(
        flingThresholdDp = flingThresholdDp * twelveKeyScale,
        flingVelocityDpS = flingVelocityDpS * twelveKeyScale
    )
}

/** Classified pad fling direction. */
enum class FlingDirection { LEFT, UP, RIGHT, DOWN }

/** Long-press behavior per category (plan 01 per-category deltas). */
enum class LongPressKind {
    /** Symbols sheet from the key's alt table (SPEC §1). */
    SYMBOLS_SHEET,
    /** Emoji tone/variant picker. */
    TONE_PICKER,
    /** Code/math symbol variant preview (`{}` vs `function`, `\frac{}{}`). */
    VARIANT_PREVIEW
}

/** Fling-up commit behavior per category. */
enum class FlingUpAction {
    /** Space + commit top candidate; plain space when not composing. */
    SPACE_COMMIT_TOP,
    /** Numbers pad: commit digit zero (the Space/0 position). */
    ZERO,
    /** Code/math packs: commit top + space or `;` per pack setting. */
    COMMIT_PLUS_TERMINATOR
}

/**
 * Effective gesture policy for one (category, layout) pair.
 * `flingLeft/flingUp` are force-disabled on `t9-12` unless the user opts in
 * via Gesture Tuning (bottom row `[Sym|Space|⌫]` is authoritative there);
 * `flingRight` (accept) stays enabled everywhere predictions exist.
 */
data class CategoryGesturePolicy(
    val predictionEnabled: Boolean,
    val tapCommitsImmediately: Boolean,
    val flingLeftEnabled: Boolean,
    val flingUpEnabled: Boolean,
    val flingRightEnabled: Boolean,
    val flingDownEnabled: Boolean,
    val flingUpAction: FlingUpAction,
    val longPressKind: LongPressKind,
    val disabledReason: String? = null
)

/**
 * Resolves the gesture policy for [categoryId] (canonical asset id, e.g.
 * `words`, `ne`, `numbers`, `emoji`, `js`) on [layoutId]
 * (`t9-9` / `t9-12` / `t9-16`, see plan 02 ids).
 *
 * Never throws for unknown categories: unknown ids resolve to the EN/NE
 * default policy and the fallback is recorded in [CategoryGesturePolicy.disabledReason]
 * only when a gesture is actually disabled, so callers can surface it.
 */
fun policyForCategory(
    categoryId: String,
    layoutId: String,
    allowTwelveKeyFlings: Boolean
): CategoryGesturePolicy {
    val normalized = categoryId.lowercase()
    val isTwelveKey = layoutId == "t9-12"
    val twelveKeyLockout = isTwelveKey && !allowTwelveKeyFlings
    val lockReason = if (twelveKeyLockout) {
        "Pad ←/↑ flings disabled in 12-key: bottom row is authoritative " +
            "(Settings → Gesture Tuning → Allow pad flings in 12-key to opt in)"
    } else {
        null
    }
    return when (normalized) {
        "numbers" -> CategoryGesturePolicy(
            predictionEnabled = false,
            tapCommitsImmediately = true,
            flingLeftEnabled = !twelveKeyLockout,
            flingUpEnabled = !twelveKeyLockout,
            // No prediction: nothing to accept; right fling is an explicit noop.
            flingRightEnabled = false,
            flingDownEnabled = true,
            flingUpAction = FlingUpAction.ZERO,
            longPressKind = LongPressKind.SYMBOLS_SHEET,
            disabledReason = lockReason
        )
        "emoji" -> CategoryGesturePolicy(
            predictionEnabled = true,
            tapCommitsImmediately = true,
            flingLeftEnabled = !twelveKeyLockout,
            flingUpEnabled = !twelveKeyLockout,
            flingRightEnabled = true,
            flingDownEnabled = true,
            flingUpAction = FlingUpAction.SPACE_COMMIT_TOP,
            longPressKind = LongPressKind.TONE_PICKER,
            disabledReason = lockReason
        )
        "js", "rust", "html", "math" -> CategoryGesturePolicy(
            predictionEnabled = true,
            tapCommitsImmediately = false,
            flingLeftEnabled = !twelveKeyLockout,
            flingUpEnabled = !twelveKeyLockout,
            flingRightEnabled = true,
            flingDownEnabled = true,
            flingUpAction = FlingUpAction.COMMIT_PLUS_TERMINATOR,
            longPressKind = LongPressKind.VARIANT_PREVIEW,
            disabledReason = lockReason
        )
        // words, ne, personal (★personal behaves as EN/NE), and any unknown id.
        else -> CategoryGesturePolicy(
            predictionEnabled = true,
            tapCommitsImmediately = false,
            flingLeftEnabled = !twelveKeyLockout,
            flingUpEnabled = !twelveKeyLockout,
            flingRightEnabled = true,
            flingDownEnabled = true,
            flingUpAction = FlingUpAction.SPACE_COMMIT_TOP,
            longPressKind = LongPressKind.SYMBOLS_SHEET,
            disabledReason = lockReason
        )
    }
}

/**
 * Pure fling classifier over a finished touch (ACTION_UP evaluation).
 *
 * @param dxPx horizontal displacement in px (positive = right)
 * @param dyPx vertical displacement in px (positive = down)
 * @param dtMs gesture duration in ms (must be > 0)
 * @param density display density (dp → px)
 * @return direction, or null when below distance/velocity/axis thresholds.
 *   Null carries no reason by design — callers log the raw metrics instead
 *   of inventing one (see [GestureLog]).
 */
fun classifyFling(
    dxPx: Float,
    dyPx: Float,
    dtMs: Long,
    density: Float,
    thresholds: GestureThresholds = GestureThresholds()
): FlingDirection? {
    require(density > 0) { "density must be > 0" }
    if (dtMs <= 0) return null
    val dxDp = dxPx / density
    val dyDp = dyPx / density
    val adx = abs(dxDp)
    val ady = abs(dyDp)
    val dominant = max(adx, ady)
    if (dominant < thresholds.flingThresholdDp) return null
    val velocity = dominant / (dtMs / 1000f)
    if (velocity < thresholds.flingVelocityDpS) return null
    val minor = if (dominant == adx) ady else adx
    if (minor > 0 && dominant / minor < thresholds.axisRatio) return null
    return if (dominant == adx) {
        if (dxDp < 0) FlingDirection.LEFT else FlingDirection.RIGHT
    } else {
        if (dyDp < 0) FlingDirection.UP else FlingDirection.DOWN
    }
}

/** True when the gesture qualifies as a tap (< 200ms and < 8dp slop). */
fun isTap(
    dxPx: Float,
    dyPx: Float,
    dtMs: Long,
    density: Float,
    thresholds: GestureThresholds = GestureThresholds()
): Boolean {
    require(density > 0) { "density must be > 0" }
    if (dtMs > thresholds.tapMaxMs) return false
    return max(abs(dxPx), abs(dyPx)) / density < thresholds.touchSlopDp
}

/** Delete-slide: extra leftward distance beyond the fling threshold, in words. */
fun deleteSlideWords(
    dxPx: Float,
    density: Float,
    thresholds: GestureThresholds = GestureThresholds()
): Int {
    require(density > 0) { "density must be > 0" }
    if (dxPx >= 0) return 0
    val extraDp = abs(dxPx) / density - thresholds.flingThresholdDp
    if (extraDp < 0) return 0
    // 1 word for crossing the fling threshold + one per word-step.
    return 1 + (extraDp / thresholds.wordStepDp).toInt()
}

/** True when the touch starts inside the 8dp edge dead-zone (gesture ignored). */
fun inEdgeDeadZone(
    xPx: Float,
    yPx: Float,
    widthPx: Float,
    heightPx: Float,
    density: Float,
    thresholds: GestureThresholds = GestureThresholds()
): Boolean {
    require(density > 0) { "density must be > 0" }
    val edgePx = thresholds.edgeDeadDp * density
    return xPx < edgePx || yPx < edgePx ||
        xPx > widthPx - edgePx || yPx > heightPx - edgePx
}
