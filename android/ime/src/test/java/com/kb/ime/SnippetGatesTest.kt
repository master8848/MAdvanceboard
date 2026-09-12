package com.kb.ime

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Frozen truth table from plan 12:43-53, plus the clipboard gate. */
class SnippetGatesTest {

    private fun gate(
        password: Boolean = false,
        incognito: Boolean = false,
        textClass: Boolean = true,
        emailOrUri: Boolean = false,
        tab: String = "words",
        learnForTab: Boolean = true,
    ) = GateInput(password, incognito, textClass, emailOrUri, tab, learnForTab)

    @Test
    fun normalEnNeJsLearnAndSnippet() {
        for (tab in listOf("words", "ne", "js", "rust", "html", "emoji")) {
            val g = gate(tab = tab)
            assertTrue("$tab learn", SnippetGates.learnNow(g))
            assertTrue("$tab snippet", SnippetGates.snippetNow(g))
        }
    }

    @Test
    fun numbersTabNeitherGate() {
        val g = gate(tab = "numbers", learnForTab = false)
        assertFalse(SnippetGates.learnNow(g))
        assertFalse(SnippetGates.snippetNow(g))
    }

    @Test
    fun mathTabSnippetExemptFromNoLearn() {
        // The math-exempt rule: learn=false stays, snippet=true.
        val g = gate(tab = "math", learnForTab = false)
        assertFalse(SnippetGates.learnNow(g))
        assertTrue(SnippetGates.snippetNow(g))
    }

    @Test
    fun passwordFieldNeitherGate() {
        val g = gate(password = true, tab = "js")
        assertFalse(SnippetGates.learnNow(g))
        assertFalse(SnippetGates.snippetNow(g))
    }

    @Test
    fun incognitoNeitherGate() {
        val g = gate(incognito = true, tab = "js")
        assertFalse(SnippetGates.learnNow(g))
        assertFalse(SnippetGates.snippetNow(g))
    }

    @Test
    fun emailUriSuggestsAndSnippetsButNeverLearns() {
        val g = gate(emailOrUri = true, tab = "words")
        assertFalse(SnippetGates.learnNow(g))
        assertTrue(SnippetGates.snippetNow(g))
    }

    @Test
    fun nonTextInputClassNeitherGate() {
        val g = gate(textClass = false, tab = "js")
        assertFalse(SnippetGates.learnNow(g))
        assertFalse(SnippetGates.snippetNow(g))
    }

    @Test
    fun clipboardNeverInPasswordOrIncognito() {
        assertTrue(SnippetGates.mayCaptureClipboard(password = false, incognito = false))
        assertFalse(SnippetGates.mayCaptureClipboard(password = true, incognito = false))
        assertFalse(SnippetGates.mayCaptureClipboard(password = false, incognito = true))
        assertFalse(SnippetGates.mayCaptureClipboard(password = true, incognito = true))
    }
}
