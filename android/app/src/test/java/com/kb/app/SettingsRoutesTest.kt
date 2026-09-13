package com.kb.app

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Settings 8→4 route sweep (bug-hunt): every legacy deep-link, doc link,
 * and stored section string must land on a live section — no dead routes,
 * no 404 home-stranding for known pages. Unknown routes fall back to home.
 *
 * [resolveSection] is pure (no framework calls), so it runs as plain JVM
 * JUnit4. Onboarding stays skippable by construction (every step renders
 * Skip + "Skip setup"); the <2min budget is a manual stopwatch check, not
 * a unit assertion.
 */
class SettingsRoutesTest {

    @Test
    fun legacyTypingRoutesResolveToTyping() {
        for (route in listOf("typing", "layout", "suggestions", "gestures")) {
            assertEquals("route $route must land on typing", Sections.TYPING, resolveSection(route))
        }
    }

    @Test
    fun legacyDictionaryRouteResolvesToDictionaries() {
        assertEquals(Sections.DICTIONARIES, resolveSection("dictionary"))
        assertEquals(Sections.DICTIONARIES, resolveSection(Sections.DICTIONARIES))
    }

    @Test
    fun appearanceSectionStable() {
        assertEquals(Sections.APPEARANCE, resolveSection(Sections.APPEARANCE))
        assertEquals(Sections.APPEARANCE, resolveSection("appearance"))
    }

    @Test
    fun legacyAdvancedRoutesResolveToAdvanced() {
        for (route in listOf("advanced", "clipboard", "tuning", "privacy")) {
            assertEquals("route $route must land on advanced", Sections.ADVANCED, resolveSection(route))
        }
    }

    @Test
    fun unknownRoutesFallBackToHome_neverCrash() {
        for (route in listOf("", "nope", "TYPING", "Layout", "home ", "settings/typing")) {
            assertEquals("route '$route' must land home", Sections.HOME, resolveSection(route))
        }
    }

    @Test
    fun fourSectionsAreDistinctFromHome() {
        val sections = setOf(Sections.TYPING, Sections.DICTIONARIES, Sections.APPEARANCE, Sections.ADVANCED)
        // Exactly 4 destinations: the whole IA fits on one screen each.
        assertEquals(4, sections.size)
        for (s in sections) {
            // Resolving a section id is the identity (no redirect loops).
            assertEquals(s, resolveSection(s))
        }
    }

    @Test
    fun imeQuickSheetDeepLinkLandsOnDictionaries() {
        // The IME ⚙ key opens Settings with section="dictionaries" (the
        // current-tab editor); it must resolve, not home-strand.
        assertEquals(Sections.DICTIONARIES, resolveSection("dictionaries"))
    }
}
