package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** JVM tests for Tier-1 palettes: js/rust chains, HTML skeletons, math flat baseline. */
class SnippetPalettesTest {

    @Test
    fun jsIfChainsTwoToThreeTokens() {
        val tokens = SnippetPalettes.followTokens("js", "if")
        assertEquals(listOf("if (", "{", "}"), tokens)
    }

    @Test
    fun rustFnChainsBraces() {
        val tokens = SnippetPalettes.followTokens("rust", "fn")
        assertTrue(tokens.size in 2..3)
        assertTrue(tokens.contains("{"))
    }

    @Test
    fun followTokensOnlyJsRust() {
        assertTrue(SnippetPalettes.followTokens("html", "div").isEmpty())
        assertTrue(SnippetPalettes.followTokens("math", "frac").isEmpty())
        assertTrue(SnippetPalettes.followTokens("words", "the").isEmpty())
        assertTrue(SnippetPalettes.followTokens("numbers", "1").isEmpty())
    }

    @Test
    fun htmlDivSkeletonParksCaretBetweenTags() {
        val item = SnippetPalettes.skeletonFor(SnippetPalettes.HtmlTag("div"))
        assertEquals("<div></div>", item.body)
        assertEquals("</div>".length, item.cursorBack)
        assertEquals(SnippetCommit.HTML_SKELETON, item.commit)
    }

    @Test
    fun htmlAnchorCarriesHrefContextAttr() {
        val item = SnippetPalettes.skeletonFor(SnippetPalettes.HtmlTag("a", "href=\"\""))
        assertEquals("<a href=\"\"></a>", item.body)
        assertEquals("</a>".length, item.cursorBack)
    }

    @Test
    fun htmlImgIsVoidNoCloseCaretAtEnd() {
        val item = SnippetPalettes.skeletonFor(
            SnippetPalettes.HtmlTag("img", "src=\"\" alt=\"\"", void = true)
        )
        assertEquals("<img src=\"\" alt=\"\">", item.body)
        assertEquals(0, item.cursorBack)
        assertEquals(SnippetCommit.HTML_VOID, item.commit)
    }

    @Test
    fun autoCloseClosesDivNotImg() {
        assertEquals("</div>", SnippetPalettes.autoClose("div"))
        assertNull(SnippetPalettes.autoClose("img"))
        assertNull(SnippetPalettes.autoClose("br"))
        assertNull(SnippetPalettes.autoClose(""))
        assertNull(SnippetPalettes.autoClose("nosuchtag"))
    }

    @Test
    fun htmlStaticPaletteHasSkeletonsAndEntities() {
        val all = SnippetPalettes.staticSnippets("html", "")
        assertTrue(all.any { it.body == "<div></div>" })
        assertTrue(all.any { it.body == "&nbsp;" })
        assertTrue(all.any { it.body == "&amp;" })
        val filtered = SnippetPalettes.staticSnippets("html", "di")
        assertTrue(filtered.any { it.body == "<div></div>" })
        assertTrue(filtered.none { it.body == "<span></span>" })
    }

    @Test
    fun mathFlatHasFracCaretInsideNoTabstops() {
        // Baseline is verbatim bodies only: no tabstop field exists on
        // SnippetItem by design (deferred set needs the 09#5 test first).
        val items = SnippetPalettes.staticSnippets("math", "\\fr")
        val frac = items.firstOrNull { it.body == "\\frac{}{}" }
        assertTrue("expected \\frac{}{} for prefix \\fr", frac != null)
        assertEquals(3, frac!!.cursorBack)
        assertEquals(SnippetCommit.MATH_FLAT, frac.commit)
        assertTrue(items.all { it.commit == SnippetCommit.MATH_FLAT })
    }

    @Test
    fun staticSnippetsEmptyOutsideHtmlMath() {
        assertTrue(SnippetPalettes.staticSnippets("js", "").isEmpty())
        assertTrue(SnippetPalettes.staticSnippets("rust", "").isEmpty())
        assertTrue(SnippetPalettes.staticSnippets("words", "").isEmpty())
        assertTrue(SnippetPalettes.staticSnippets("numbers", "").isEmpty())
    }

    @Test(expected = IllegalArgumentException::class)
    fun snippetItemRejectsEmptyBody() {
        SnippetItem(display = "x", body = "")
    }

    @Test(expected = IllegalArgumentException::class)
    fun snippetItemRejectsCursorOutOfRange() {
        SnippetItem(display = "x", body = "ab", cursorBack = 5)
    }
}
