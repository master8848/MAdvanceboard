package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pure JVM tests for the custom-category import flow (names task).
 * Only [CustomPackValidation] (no Android imports) is exercised:
 * bad rows fail loudly with exact errors, never silently dropped;
 * disable hides the tab via [CustomPackValidation.resolveTabs].
 */
class CustomPacksTest {

    @Test
    fun wordlistParsesWordFreqLines() {
        val out = CustomPackValidation.parseWordlist("# contacts\nava 9000\nbob\n")
        assertTrue(out.errors.isEmpty())
        assertEquals(2, out.words.size)
        assertEquals(CustomWord("ava", 9000L), out.words[0])
        assertEquals(CustomWord("bob", 100L), out.words[1])
    }

    @Test
    fun badRowsFailLoudlyNeverSilently() {
        val out = CustomPackValidation.parseWordlist(
            "ava 9000\nava 100\nbob 0\n*\nbob 100 extra\n"
        )
        assertEquals("only the first row is valid", 1, out.words.size)
        assertEquals(4, out.errors.size)
        assertTrue(out.errors.any { it.contains("duplicate") })
        assertTrue(out.errors.any { it.contains("out of range") })
        assertTrue(out.errors.any { it.contains("empty seq") })
        assertTrue(out.errors.any { it.contains("trailing text") })
        assertTrue(out.errors.all { it.startsWith("line ") })
    }

    @Test
    fun emptyWordlistIsAnError() {
        val out = CustomPackValidation.parseWordlist("  \n# nothing here\n")
        assertTrue(out.words.isEmpty())
        assertEquals(1, out.errors.size)
        assertTrue(out.errors[0].contains("empty"))
    }

    @Test
    fun idAndPriorityRules() {
        assertNull(CustomPackValidation.validateId("names"))
        assertNull(CustomPackValidation.validateId("my_names2"))
        assertNull(CustomPackValidation.validatePriority(10))
        assertNull(CustomPackValidation.validatePriority(90))
        for (bad in listOf("", "N", "names!", "1names", "a", "UPPER", "has space")) {
            assertTrue("id $bad must fail", CustomPackValidation.validateId(bad) != null)
        }
        for (reserved in listOf("words", "ne", "personal", "medical")) {
            assertTrue(
                CustomPackValidation.validateId(reserved)!!.contains("reserved")
            )
        }
        for (bad in listOf(0, 9, 91, 100)) {
            assertTrue(CustomPackValidation.validatePriority(bad)!!.contains("out of range"))
        }
    }

    @Test
    fun packEnvelopeCarriesCatAndFreq() {
        val json = CustomPackValidation.buildPackJson(
            "names", "Names", listOf(CustomWord("ava", 9000L), CustomWord("bob", 100L))
        )
        assertTrue(json.contains("\"id\":\"names\""))
        assertTrue(json.contains("\"w\":\"ava\""))
        assertTrue(json.contains("\"freq\":9000"))
        assertTrue(json.contains("\"cat\":\"names\""))
    }

    @Test
    fun resolveTabsPinsPersonalLastAndHidesDisabled() {
        // Enabled customs appear before pinned ★personal.
        assertEquals(
            listOf("numbers", "words", "ne", "js", "rust", "html", "emoji", "math", "names", "★personal"),
            CustomPackValidation.resolveTabs(listOf("names"))
        )
        // Disable hides the tab: a disabled id is simply not passed in.
        assertEquals(DEFAULT_CATEGORIES, CustomPackValidation.resolveTabs(emptyList()))
        // Built-in ids and duplicates never double-add.
        assertEquals(
            CustomPackValidation.resolveTabs(listOf("names")),
            CustomPackValidation.resolveTabs(listOf("names", "names", "words"))
        )
    }

    @Test
    fun exportCarriesPriorityAndEnabled() {
        val packs = listOf(
            CustomPack("names", "Names", """{"id":"names"}""", 50, true),
            CustomPack("pets", "Pets", """{"id":"pets"}""", 30, false)
        )
        val json = CustomPackValidation.exportJson(packs)
        assertTrue(json.contains("\"priority\":50"))
        assertTrue(json.contains("\"enabled\":false"))
        // Stack order: higher priority first.
        assertTrue(json.indexOf("\"names\"") < json.indexOf("\"pets\""))
    }
}
