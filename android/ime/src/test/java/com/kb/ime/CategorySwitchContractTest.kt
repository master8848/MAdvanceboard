package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Category-switch pending-input contract (bug-hunt sweep for
 * `KbInputMethodService.onCategoryChanged`).
 *
 * The service itself needs the framework (`InputMethodService`), so this
 * pins the exact decision rule it implements, branch by branch:
 * - same resolved layout + in-progress seq -> `refreshSuggestions` with the
 *   SAME seq under the NEW tab (digits preserved, strip re-queried with the
 *   new tab's category boost);
 * - resolved layout differs -> buffers cleared, strip emptied (codes belong
 *   to the previous pad and must not leak across layouts);
 * - snippet scratch always dies on switch; the new tab's static palette is
 *   the only helper that may appear.
 */
class CategorySwitchContractTest {

    /**
     * Mirror of the pending-input branch in `onCategoryChanged`: returns the
     * seq that must be re-queried, or null when the switch must clear.
     */
    private fun pendingAfterSwitch(
        resolvedLayout: String,
        activeLayout: String,
        pendingSeq: String?
    ): String? = if (resolvedLayout != activeLayout) null else pendingSeq

    @Test
    fun sameLayoutPreservesDigits_newTabRequeries() {
        // words(t9-9) -> ne(t9-9): identical seq, new tab boost.
        assertEquals("43556", pendingAfterSwitch("t9-9", "t9-9", "43556"))
        assertEquals("2", pendingAfterSwitch("t9-12", "t9-12", "2"))
    }

    @Test
    fun layoutChangeClearsPending_noCodeLeakAcrossPads() {
        // words(t9-9) -> js(t9-12): t9-9 codes are meaningless on t9-12.
        assertNull(pendingAfterSwitch("t9-12", "t9-9", "43556"))
        assertNull(pendingAfterSwitch("t9-9", "t9-16", "22"))
    }

    @Test
    fun idleSwitchEmptiesStripEitherWay() {
        assertNull(pendingAfterSwitch("t9-9", "t9-9", null))
        assertNull(pendingAfterSwitch("t9-12", "t9-9", null))
    }

    @Test
    fun switchChangesPredictionPolicy_perTabIsolation() {
        // The strip must re-query because the policy travels with the tab:
        // numbers commits digits raw (no prediction), words predicts.
        val numbers = policyForCategory("numbers", "t9-9", allowTwelveKeyFlings = false)
        val words = policyForCategory("words", "t9-9", allowTwelveKeyFlings = false)
        assertFalse(numbers.predictionEnabled)
        assertTrue(words.predictionEnabled)
        // Emoji taps commit immediately; words wait for an explicit accept.
        val emoji = policyForCategory("emoji", "t9-9", allowTwelveKeyFlings = false)
        assertTrue(emoji.tapCommitsImmediately)
        assertFalse(words.tapCommitsImmediately)
    }

    @Test
    fun switchWipesSnippetScratch_newTabShowsOnlyStaticPalette() {
        // Buffer dies on tab switch: trigger armed in js must not surface in html.
        val buffer = SnippetBuffer(clock = { 0L })
        buffer.offer("if", listOf("if (", "{", "}"), now = 0L)
        assertEquals(1, buffer.size(now = 0L))
        buffer.clear()
        assertTrue(buffer.isEmpty(now = 0L))
        // The new tab's static palette is still available immediately.
        assertTrue(SnippetPalettes.staticSnippets("html", "").isNotEmpty())
        assertTrue(SnippetPalettes.staticSnippets("math", "").isNotEmpty())
        // ...while js has no static palette (its Tier-1 was the wiped buffer).
        assertTrue(SnippetPalettes.staticSnippets("js", "").isEmpty())
    }

    @Test
    fun spaceLabelFollowsTab_withoutRebuildingPad() {
        assertEquals("English", CategoryLabels.spaceLabel("words"))
        assertEquals("नेपाली", CategoryLabels.spaceLabel("ne"))
        assertEquals("JS", CategoryLabels.spaceLabel("js"))
        assertEquals("123", CategoryLabels.spaceLabel("numbers"))
    }

    @Test
    fun legacyBadgesResolveBeforeSwitch() {
        // The tap strip shows badges/legacy labels; the switch must resolve
        // them to engine ids first (canonicalTabId), never forward a badge.
        assertEquals("words", CategoryLabels.canonicalId("EN"))
        assertEquals("words", CategoryLabels.canonicalId("General"))
        assertEquals("ne", CategoryLabels.canonicalId("NE"))
        assertEquals("emoji", CategoryLabels.canonicalId("😀"))
    }
}
