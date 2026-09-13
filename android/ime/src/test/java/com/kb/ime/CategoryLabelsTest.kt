package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * JVM tests for the compact badge strip: short badges render, engine ids
 * don't change, and every badge plus every legacy/visible spelling still
 * resolves to its engine id (notably `😀` → `emoji`: the strip shows the
 * badge, so the reverse trip is load-bearing for the category boost).
 */
class CategoryLabelsTest {

    @Test
    fun badgesRenderCompact_everythingElsePassesThrough() {
        assertEquals("E", CategoryLabels.label("words"))
        assertEquals("ने", CategoryLabels.label("ne"))
        assertEquals("123", CategoryLabels.label("numbers"))
        assertEquals("JS", CategoryLabels.label("js"))
        assertEquals("Rs", CategoryLabels.label("rust"))
        assertEquals("HTML", CategoryLabels.label("html"))
        assertEquals("\uD83D\uDE00", CategoryLabels.label("emoji"))
        assertEquals("∑", CategoryLabels.label("math"))
        assertEquals("⚕", CategoryLabels.label("medical"))
        assertEquals("★", CategoryLabels.label("★personal"))
        assertEquals("★", CategoryLabels.label("personal"))
        assertEquals("names", CategoryLabels.label("names"))
    }

    @Test
    fun canonicalIdResolvesBadgeAndLegacySpellingsToWords() {
        assertEquals("words", CategoryLabels.canonicalId("E"))
        assertEquals("words", CategoryLabels.canonicalId("e"))
        assertEquals("words", CategoryLabels.canonicalId("General"))
        assertEquals("words", CategoryLabels.canonicalId("general"))
        assertEquals("words", CategoryLabels.canonicalId("EN"))
        assertEquals("words", CategoryLabels.canonicalId("en"))
        assertEquals("words", CategoryLabels.canonicalId("words"))
    }

    @Test
    fun canonicalIdKeepsOtherTabs() {
        assertEquals("ne", CategoryLabels.canonicalId("ने"))
        assertEquals("ne", CategoryLabels.canonicalId("NE"))
        assertEquals("ne", CategoryLabels.canonicalId("ne"))
        assertEquals("ne", CategoryLabels.canonicalId("nepali"))
        assertEquals("numbers", CategoryLabels.canonicalId("123"))
        assertEquals("numbers", CategoryLabels.canonicalId("numbers"))
        assertEquals("js", CategoryLabels.canonicalId("JS"))
        assertEquals("rust", CategoryLabels.canonicalId("Rs"))
        assertEquals("rust", CategoryLabels.canonicalId("rust"))
        assertEquals("html", CategoryLabels.canonicalId("HTML"))
        assertEquals("emoji", CategoryLabels.canonicalId("\uD83D\uDE00"))
        assertEquals("emoji", CategoryLabels.canonicalId("emoji"))
        assertEquals("math", CategoryLabels.canonicalId("∑"))
        assertEquals("math", CategoryLabels.canonicalId("math"))
        assertEquals("medical", CategoryLabels.canonicalId("⚕"))
        assertEquals("medical", CategoryLabels.canonicalId("medical"))
        assertEquals("personal", CategoryLabels.canonicalId("★"))
        assertEquals("personal", CategoryLabels.canonicalId("★personal"))
        assertEquals("names", CategoryLabels.canonicalId("Names"))
    }

    @Test
    fun labelThenCanonicalIdRoundTrips() {
        for (id in listOf("words", "ne", "numbers", "js", "rust", "html", "emoji", "math", "medical", "personal")) {
            val stripId = if (id == "personal") "★personal" else id
            // personal's visible key differs; its strip id maps via ★personal.
            val back = CategoryLabels.canonicalId(CategoryLabels.label(stripId))
            assertEquals(id, back)
        }
    }
}
