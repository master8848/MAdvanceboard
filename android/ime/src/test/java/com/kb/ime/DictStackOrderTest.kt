package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pure JVM tests for the dictionary-stack ordering (plan/08): reorder
 * math, priority bounds, deterministic ties, export `stack` section, and
 * the engine-capability probe that keeps missing FFI explicit.
 */
class DictStackOrderTest {

    private fun slot(id: String, prio: Int, enabled: Boolean = true) =
        StackSlot(id, id, prio, enabled, fixed = false, custom = false, learns = true)

    @Test
    fun priorityBounds() {
        assertNull(DictStackOrder.validatePriority(10))
        assertNull(DictStackOrder.validatePriority(90))
        assertNull(DictStackOrder.validatePriority(50))
        for (bad in listOf(0, 9, 91, 100, -5)) {
            val err = DictStackOrder.validatePriority(bad)
            assertTrue("priority $bad must fail", err != null && err.contains("out of range"))
        }
    }

    @Test
    fun sortMovableIsPriorityDescIdAsc() {
        val slots = listOf(slot("math", 50), slot("js", 20), slot("pets", 50, enabled = true))
        val ordered = DictStackOrder.sortMovable(slots).map { it.id }
        // math and pets tie at 50: id asc decides deterministically.
        assertEquals(listOf("math", "pets", "js"), ordered)
    }

    @Test
    fun sortMovableSkipsFixedAndDisabledStays() {
        val slots = listOf(
            StackSlot("words", "EN", 0, true, fixed = true, custom = false, learns = true),
            slot("math", 50, enabled = false),
            slot("js", 20)
        )
        // Disabled packs keep their rank (they reorder, not vanish, here —
        // the strip hides them separately via stripOrder).
        assertEquals(listOf("math", "js"), DictStackOrder.sortMovable(slots).map { it.id })
    }

    @Test
    fun respacePreservesOrderWithDistinctInRangePriorities() {
        val order = listOf("medical", "math", "emoji", "html", "rust", "js", "numbers")
        val spaced = DictStackOrder.respace(order)
        assertEquals(order, spaced.entries.sortedByDescending { it.value }.map { it.key })
        assertEquals(order.toSet(), spaced.keys)
        for (p in spaced.values) {
            assertTrue("priority $p in 10-90", p in 10..90)
        }
        assertEquals(order.size, spaced.values.toSet().size)
        assertEquals(90, spaced["medical"])
    }

    @Test
    fun respaceEdgeCases() {
        assertTrue(DictStackOrder.respace(emptyList()).isEmpty())
        assertEquals(mapOf("only" to 90), DictStackOrder.respace(listOf("only")))
        // Two slots take the extremes.
        assertEquals(mapOf("a" to 90, "b" to 10), DictStackOrder.respace(listOf("a", "b")))
        try {
            DictStackOrder.respace(listOf("a", "a"))
            throw AssertionError("duplicate ids must fail")
        } catch (e: IllegalArgumentException) {
            assertTrue(e.message!!.contains("duplicate"))
        }
    }

    @Test
    fun moveOrderStepsAndClamps() {
        val ids = listOf("medical", "math", "js")
        assertEquals(listOf("math", "medical", "js"), DictStackOrder.moveOrder(ids, "medical", 1))
        assertEquals(listOf("medical", "js", "math"), DictStackOrder.moveOrder(ids, "math", 5))
        assertEquals(listOf("math", "medical", "js"), DictStackOrder.moveOrder(ids, "math", -5))
        assertEquals(ids, DictStackOrder.moveOrder(ids, "medical", -1))
        assertEquals(ids, DictStackOrder.moveOrder(ids, "medical", 0))
        try {
            DictStackOrder.moveOrder(ids, "nope", 1)
            throw AssertionError("unknown id must fail")
        } catch (e: IllegalArgumentException) {
            assertTrue(e.message!!.contains("unknown category"))
        }
    }

    @Test
    fun stripOrderPinsNumbersAndPersonal() {
        val slots = listOf(
            StackSlot("words", "EN", 0, true, fixed = true, custom = false, learns = true),
            StackSlot("ne", "NE", 0, true, fixed = true, custom = false, learns = true),
            slot("numbers", 15),
            slot("math", 50),
            slot("js", 20, enabled = false),
            StackSlot("personal", "★personal", 100, true, fixed = true, custom = false, learns = false)
        )
        assertEquals(
            listOf("numbers", "words", "ne", "math", "★personal"),
            DictStackOrder.stripOrder(slots)
        )
    }

    @Test
    fun validateStackEntry() {
        assertNull(DictStackOrder.validateStackEntry("math", 50))
        assertTrue(DictStackOrder.validateStackEntry("nope", 50)!!.contains("unknown built-in"))
        assertTrue(DictStackOrder.validateStackEntry("math", 5)!!.contains("out of range"))
    }

    @Test
    fun exportCarriesStackSectionInOrder() {
        val packs = listOf(
            CustomPack("names", "Names", """{"id":"names"}""", 50, true)
        )
        val stack = listOf(
            StackEntry("medical", 90, true),
            StackEntry("math", 50, false)
        )
        val json = CustomPackValidation.exportJson(packs, stack)
        assertTrue(json.contains("\"stack\""))
        assertTrue(json.contains("\"id\":\"medical\""))
        assertTrue(json.contains("\"priority\":90"))
        assertTrue(json.contains("\"enabled\":false"))
        // Order preserved: medical before math.
        assertTrue(json.indexOf("\"medical\"") < json.indexOf("\"math\""))
        // Legacy shape (no stack) stays stack-free.
        assertFalse(CustomPackValidation.exportJson(packs).contains("\"stack\""))
    }

    @Test
    fun engineCapsExposeLiveFfi() {
        // The Rust methods ARE #[uniffi::export]ed, so the generated
        // bindings expose all three. These assertions pin the live
        // reality: if a regen ever drops them, the tests fail loudly and
        // the live-call paths must fall back to explicit-error notes.
        assertTrue(
            "setCatEnabled must be exported (popup toggle calls it live)",
            StackEngineCaps.setCatEnabledExported
        )
        assertTrue(
            "isCatEnabled must be exported (toggle verification + popup state read it live)",
            StackEngineCaps.isCatEnabledExported
        )
        assertTrue(
            "placement must be exported (popup reads word provenance live)",
            StackEngineCaps.placementExported
        )
        assertTrue(
            StackEngineCaps.liveToggleApplied("math", false).contains("verified")
        )
        assertTrue(
            StackEngineCaps.liveToggleFailure("math", "boom").contains("boom")
        )
        assertTrue(
            StackEngineCaps.placementUnknown("zxq").contains("no pack holds")
        )
    }
}
