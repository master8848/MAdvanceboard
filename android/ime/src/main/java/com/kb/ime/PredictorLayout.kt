package com.kb.ime

import com.kb.bridge.Predictor
import com.kb.bridge.ScoredCandidate

/**
 * Layout-aware call sites for the shared [Predictor] path. `layoutId` is an
 * encoder flag: it records which pad (`t9-9` / `t9-12` / `t9-16`) produced
 * `seq`, so a 12/16-key seq is never confused with a 9-key seq. The Rust
 * core already resolves per layout (`suggest_with_layout` /
 * `suggest_for_cat`, per-layout FST + neighbor graph, plan/02); the
 * Kotlin [Predictor] interface (owned by `:core-bridge`) has no `layoutId`
 * parameter yet, so these helpers still forward seq/prev/limit unchanged
 * and scoring semantics are unchanged. When the UniFFI regen adds
 * `suggest(seq, prev, layoutId, limit)`, only these two bodies change —
 * no call-site churn. [StubPredictor] keeps working untouched.
 *
 * Unknown ids are an explicit [IllegalArgumentException], never a silent
 * wrong-pad forward.
 */
suspend fun suggestWithLayout(
    predictor: Predictor,
    seq: String,
    prev: String?,
    layoutId: String,
    limit: Int = 30
): List<ScoredCandidate> {
    require(layoutId in SUPPORTED_LAYOUT_IDS) { "unknown layout_id: $layoutId" }
    // Encoder flag only today: forward seq/prev/limit unchanged.
    return predictor.suggest(seq, prev, limit)
}

suspend fun learnWithLayout(
    predictor: Predictor,
    word: String,
    prev: String?,
    layoutId: String
) {
    require(layoutId in SUPPORTED_LAYOUT_IDS) { "unknown layout_id: $layoutId" }
    // Encoder flag only today: personal-dict entry is layout-agnostic.
    predictor.learn(word, prev)
}
