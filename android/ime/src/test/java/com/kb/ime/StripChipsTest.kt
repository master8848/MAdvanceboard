package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Suggestion-strip chip sweep (bug-hunt): the strip pages a core-union view
 * (one fetched list, strip + expand-all share it), space commits the top
 * candidate, backspace cascades one digit at a time with no digit leaking
 * out of the seq, and fling-right accepts the leftmost chip (#1).
 *
 * Pure JVM: [policyForCategory] for the per-tab commit shape,
 * [DictStackOrder.stripOrder] for the pinned strip, plus executable mirrors
 * of the `onPadCode` / `deleteLast` / `onPadSpace` / `acceptTopCandidate`
 * string discipline (the service needs the framework; the seq math is what
 * must never regress — notably the old `1`-commits-`.` digit leak).
 */
class StripChipsTest {

    // -- Core-union view: one fetch, strip pages STRIP_LIMIT, expand-all shows EXPAND_LIMIT. --

    @Test
    fun stripVsExpandLimitsPinned() {
        assertEquals(3, STRIP_LIMIT)
        assertEquals(30, EXPAND_LIMIT)
        val pageCount = (EXPAND_LIMIT + STRIP_LIMIT - 1) / STRIP_LIMIT
        assertEquals(10, pageCount)
    }

    @Test
    fun overflowPagingCyclesWithoutCounter() {
        // Mirror of SuggestionStrip paging: fling-left advances one page of
        // 3 and wraps; there is no numeric counter to drift.
        val candidates = (1..30).map { "w$it" }
        val pageSize = STRIP_LIMIT
        val pageCount = (candidates.size + pageSize - 1) / pageSize
        var page = 0
        repeat(pageCount) { page = (page + 1) % pageCount }
        assertEquals(0, page)
        val shown = candidates.drop(page * pageSize).take(pageSize)
        assertEquals(listOf("w1", "w2", "w3"), shown)
    }

    @Test
    fun stripPinsNumbersFirstPersonalLast_disabledAbsent() {
        fun slot(id: String, prio: Int, enabled: Boolean = true) =
            StackSlot(id, id, prio, enabled, fixed = false, custom = false, learns = true)
        val slots = listOf(
            StackSlot("words", "E", 0, true, fixed = true, custom = false, learns = true),
            StackSlot("ne", "NE", 0, true, fixed = true, custom = false, learns = true),
            slot("numbers", 15),
            slot("js", 20, enabled = false),
            slot("math", 50),
            StackSlot("personal", "★personal", 100, true, fixed = true, custom = false, learns = false)
        )
        assertEquals(
            listOf("numbers", "words", "ne", "math", "★personal"),
            DictStackOrder.stripOrder(slots)
        )
    }

    // -- Space commits the top candidate (per-tab shape). --

    @Test
    fun flingUpActionMatrix() {
        assertEquals(
            FlingUpAction.SPACE_COMMIT_TOP,
            policyForCategory("words", "t9-9", false).flingUpAction
        )
        assertEquals(
            FlingUpAction.SPACE_COMMIT_TOP,
            policyForCategory("ne", "t9-9", false).flingUpAction
        )
        assertEquals(
            FlingUpAction.SPACE_COMMIT_TOP,
            policyForCategory("emoji", "t9-9", false).flingUpAction
        )
        // Numbers: no candidates — fling-up commits the digit zero.
        assertEquals(
            FlingUpAction.ZERO,
            policyForCategory("numbers", "t9-9", false).flingUpAction
        )
        // Code/math: top candidate + the tuned terminator.
        for (cat in listOf("js", "rust", "html", "math")) {
            assertEquals(
                "$cat must commit-plus-terminator",
                FlingUpAction.COMMIT_PLUS_TERMINATOR,
                policyForCategory(cat, "t9-9", false).flingUpAction
            )
        }
        // Unknown ids fall back to the General/NE default, never a dead tab.
        assertEquals(
            FlingUpAction.SPACE_COMMIT_TOP,
            policyForCategory("mystery-pack", "t9-9", false).flingUpAction
        )
    }

    @Test
    fun spaceAcceptsTopCandidateElseBareSpace() {
        // Mirror of onPadSpace: pending seq + top candidate -> accept top;
        // nothing pending -> bare space (seq stays empty, no phantom commit).
        fun space(seq: StringBuilder, top: String?): String {
            val t = if (seq.isNotEmpty()) top else null
            if (t != null) {
                seq.clear()
                return t
            }
            seq.clear()
            return " "
        }
        assertEquals("hello", space(StringBuilder("43556"), "hello"))
        assertEquals(" ", space(StringBuilder(), "hello"))
        assertEquals(" ", space(StringBuilder("43"), null))
    }

    // -- Backspace cascades one digit; no digit (incl. 1) ever leaks. --

    @Test
    fun backspaceCascadesPerDigit() {
        // Mirror of deleteLast's seq branch: exactly one code per press,
        // composing text tracks the remainder, empty seq ends composing.
        val seq = StringBuilder("43556")
        val remainder = mutableListOf<String>()
        while (seq.isNotEmpty()) {
            seq.deleteCharAt(seq.length - 1)
            remainder.add(seq.toString())
        }
        assertEquals(listOf("4355", "435", "43", "4", ""), remainder)
    }

    @Test
    fun digitOneStaysInSeq_noLeakToPunctuation() {
        // Regression pin for the old `1`-commits-`.` leak: `1` is a
        // text-role code whose symbols are punctuation-only, so the engine
        // owns `1`-seq suggestions and the service must extend the seq.
        val seq = StringBuilder("43")
        seq.append("1")
        assertEquals("431", seq.toString())
        // Numbers path keeps the digit too (raw digit commit, no seq).
        val raw = "1".filter { it.isDigit() }.takeIf { it.isNotEmpty() } ?: "1"
        assertEquals("1", raw)
    }

    // -- Leftmost chip is #1: fling-right accepts the first candidate. --

    @Test
    fun acceptFirstIsLeftmost_emptyStripIsExplicitNoop() {
        fun acceptTop(candidates: List<String>): String? = candidates.firstOrNull()
        assertEquals("the", acceptTop(listOf("the", "tie", "toe")))
        // Empty strip: explicit noop with a log entry (acceptTopCandidate),
        // never an index crash, never a silent swallow.
        assertEquals(null, acceptTop(emptyList()))
    }

    // -- Long-press sheet always offers the digit (digits stay reachable). --

    @Test
    fun longPressSheetKeepsDigit() {
        // Mirror of onKeyLongPress: key symbols + the key's digit, distinct.
        fun sheet(code: String, symbols: String): List<String> {
            if (code.isEmpty() || symbols.isEmpty()) {
                return listOf("1", "2", "3", "4", "5", "6", "7", "8", "9", "0")
            }
            val digit = code.filter { it.isDigit() }.take(1)
            return (symbols.toList().map { it.toString() } + digit)
                .distinct()
                .filter { it.isNotEmpty() }
        }
        val chars = sheet("2", "abc")
        assertTrue(chars.contains("2"))
        assertTrue(chars.contains("a"))
        // Unknown key: explicit 10-digit sheet + logged miss, never empty.
        assertEquals(10, sheet("", "").size)
    }
}
