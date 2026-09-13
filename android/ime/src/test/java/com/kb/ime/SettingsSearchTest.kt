package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * JVM tests for the settings search filter (plan/19 step 2): blank query
 * returns every row; otherwise case-insensitive substring over title /
 * subtitle / keywords / id.
 */
class SettingsSearchTest {

    private val rows = listOf(
        SearchRow("typing-layout", "Layout / Pad", "9 / 12 / 16-key default", "t9 qwerty per-tab"),
        SearchRow("dict-manager", "Dictionaries", "tabs and packs", "words nepali stack"),
        SearchRow("appearance-theme", "Theme", "system light dark", "night mode")
    )

    @Test
    fun blankQueryReturnsEveryRow() {
        assertEquals(rows, filterSearchRows(rows, ""))
        assertEquals(rows, filterSearchRows(rows, "   "))
    }

    @Test
    fun titleSubstringMatchesCaseInsensitive() {
        val out = filterSearchRows(rows, "DICTION")
        assertEquals(listOf(rows[1]), out)
    }

    @Test
    fun keywordMatches() {
        assertEquals(listOf(rows[0]), filterSearchRows(rows, "qwerty"))
    }

    @Test
    fun idMatches() {
        assertEquals(listOf(rows[2]), filterSearchRows(rows, "appearance-theme"))
    }

    @Test
    fun noMatchReturnsEmpty() {
        assertTrue(filterSearchRows(rows, "zzzz").isEmpty())
    }
}
