package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * JVM tests for the friend-ready audience filter (plan/19, 20, 24):
 * code tabs hide for friends, order is otherwise preserved, the strip is
 * never stranded empty.
 */
class FriendDefaultsTest {

    private val full = listOf(
        "numbers", "words", "ne", "js", "rust", "html", "emoji", "math", "medical", "★personal"
    )

    @Test
    fun showCodeKeepsEverythingInOrder() {
        assertEquals(full, FriendDefaults.filterTabs(full, showCode = true))
    }

    @Test
    fun friendsSeeNoCodeTabs() {
        val shown = FriendDefaults.filterTabs(full, showCode = false)
        assertTrue(shown.containsAll(listOf("numbers", "words", "ne", "emoji", "★personal")))
        for (code in FriendDefaults.CODE_TABS) {
            assertTrue("code tab leaked: $code", code !in shown)
        }
        // Order preserved among the kept tabs.
        assertEquals(shown.sortedBy { full.indexOf(it) }, shown)
    }

    @Test
    fun medicalIsACodeTab() {
        assertTrue("medical" in FriendDefaults.CODE_TABS)
        assertTrue("medical" !in FriendDefaults.filterTabs(full, showCode = false))
        assertTrue("medical" in FriendDefaults.filterTabs(full, showCode = true))
    }

    @Test
    fun emptyAfterFilterFallsBackToFriendSet() {
        val onlyCode = listOf("js", "rust")
        val shown = FriendDefaults.filterTabs(onlyCode, showCode = false)
        assertTrue(shown.isNotEmpty())
    }

    @Test
    fun friendTabsAreAllNonCode() {
        for (tab in FriendDefaults.FRIEND_TABS) {
            assertTrue("$tab must not be a code tab", tab !in FriendDefaults.CODE_TABS)
        }
    }
}
