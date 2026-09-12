package com.kb.ime

import com.kb.bridge.Predictor
import com.kb.bridge.ScoredCandidate

/**
 * Fetch discipline for the shared [Predictor] path (batch-string FFI):
 *
 * - ONE `suggest*` call per keystroke fetching [EXPAND_LIMIT] candidates.
 * - The inline strip pages [STRIP_LIMIT] at a time from that same list
 *   ([SuggestionStrip] `pageSize`); expand-all shows all [EXPAND_LIMIT].
 * - Never fetch twice per keystroke (3 + 30) — that would double FFI
 *   traffic against the plan/04 no-per-word-chatter invariant.
 */
const val STRIP_LIMIT = 3
const val EXPAND_LIMIT = 30

/**
 * Layout-aware call sites for the shared [Predictor] path. `layoutId` names
 * the pad (`t9-9` / `t9-12` / `t9-16`) that produced `seq`, so a 12/16-key
 * seq is never confused with a 9-key seq; `activeTab` (canonical asset id:
 * `words`, `ne`, …) selects the engine category boost. Both travel every
 * call — the core resolves `suggest_with_layout` per layout (per-layout
 * FST + neighbor graph, plan/02).
 *
 * Unknown ids are an explicit [IllegalArgumentException], never a silent
 * wrong-pad forward.
 *
 * Nullability contract: [prev] is null under no-learn (password, incognito,
 * email/URI — see `prevWord`, which harvests no context there) and maps to
 * `ctx = ""` (no bigram) — an explicit empty context, never the literal
 * string "null" and never a harvested word that must not be read.
 */
suspend fun suggestWithLayout(
    predictor: Predictor,
    seq: String,
    prev: String?,
    layoutId: String,
    activeTab: String,
    limit: Int = EXPAND_LIMIT
): List<ScoredCandidate> {
    require(layoutId in SUPPORTED_LAYOUT_IDS) { "unknown layout_id: $layoutId" }
    require(limit >= 0) { "suggestWithLayout: negative limit $limit" }
    return predictor.suggestWithLayout(
        ctx = prev.orEmpty(),
        digits = seq,
        layoutId = layoutId,
        activeTab = activeTab,
        limit = limit
    )
}

/**
 * Learn path: `category` is the pack id ([activeAssetId]-style `words` /
 * `ne`), NOT the ctx-prev word; `shown` is the caller-observed last
 * suggest result, forwarded to `learn_with_shown` so the engine stays an
 * O(1) personal op with no suggest-in-lock (plan/05 #5).
 */
suspend fun learnWithLayout(
    predictor: Predictor,
    word: String,
    category: String,
    shown: List<String>,
    layoutId: String
) {
    require(layoutId in SUPPORTED_LAYOUT_IDS) { "unknown layout_id: $layoutId" }
    predictor.learnWithShown(word, category, shown)
}
