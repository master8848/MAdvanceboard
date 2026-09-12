package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * JVM tests for the `words` → "General" UI rename: labels change, engine
 * ids don't, and every legacy/visible spelling still resolves to `words`.
 */
class CategoryLabelsTest {

    @Test
    fun wordsRendersGeneral_everythingElsePassesThrough() {
        assertEquals("General", CategoryLabels.label("words"))
        assertEquals("ne", CategoryLabels.label("ne"))
        assertEquals("numbers", CategoryLabels.label("numbers"))
        assertEquals("★personal", CategoryLabels.label("★personal"))
        assertEquals("names", CategoryLabels.label("names"))
    }

    @Test
    fun canonicalIdResolvesGeneralAndLegacyEnToWords() {
        assertEquals("words", CategoryLabels.canonicalId("General"))
        assertEquals("words", CategoryLabels.canonicalId("general"))
        assertEquals("words", CategoryLabels.canonicalId("EN"))
        assertEquals("words", CategoryLabels.canonicalId("en"))
        assertEquals("words", CategoryLabels.canonicalId("words"))
    }

    @Test
    fun canonicalIdKeepsOtherTabs() {
        assertEquals("ne", CategoryLabels.canonicalId("NE"))
        assertEquals("ne", CategoryLabels.canonicalId("ne"))
        assertEquals("ne", CategoryLabels.canonicalId("nepali"))
        assertEquals("personal", CategoryLabels.canonicalId("★personal"))
        assertEquals("js", CategoryLabels.canonicalId("js"))
        assertEquals("names", CategoryLabels.canonicalId("Names"))
    }

    @Test
    fun labelThenCanonicalIdRoundTrips() {
        for (id in listOf("words", "ne", "numbers", "js", "personal")) {
            val engine = if (id == "personal") "★personal" else id
            // personal's visible key differs; its strip id maps via ★personal.
            val back = CategoryLabels.canonicalId(CategoryLabels.label(engine))
            if (id == "personal") {
                assertEquals("personal", back)
            } else {
                assertEquals(id, back)
            }
        }
    }
}
