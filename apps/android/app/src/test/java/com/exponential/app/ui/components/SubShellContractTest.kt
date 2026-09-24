package com.exponential.app.ui.components

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1029 contract — sub-shell navigation on Android (web, IDE and iOS carry
 * the same case names), implemented by EXP-1043.
 *
 * The composable is a drawing of [SubShellNavigation]: the host renders the
 * level the stack names, the row pushes and the header pops. The rules live
 * in that class, so the contract is driven here without a compose rule.
 */
class SubShellContractTest {

    @Test
    fun tappingTheRowSlidesTheChildPageInPlaceOfTheWholeCard() {
        val nav = SubShellNavigation()
        assertFalse(nav.isOpen)
        assertNull(nav.current)

        nav.push("Workflow settings")

        // The host now draws that page INSTEAD of its card: one level deep,
        // and it is the row's page that is on top.
        assertTrue(nav.isOpen)
        assertEquals(1, nav.depth)
        assertEquals("Workflow settings", nav.current)
        // A deeper level slides in from the end edge.
        assertTrue(nav.forward)
    }

    @Test
    fun theChildPageCarriesABackButtonOnTopThatReturnsToTheCard() {
        val nav = SubShellNavigation()
        nav.push("Workflow settings")

        nav.back()

        assertFalse(nav.isOpen)
        assertEquals(0, nav.depth)
        assertNull(nav.current)
        // Coming back slides from the start edge.
        assertFalse(nav.forward)
        // Back at the card, there is nothing left to return from.
        nav.back()
        assertEquals(0, nav.depth)
    }

    @Test
    fun aSubShellInsideTheChildPageSlidesOneLevelDeeper() {
        val nav = SubShellNavigation()
        nav.push("Workflow settings")
        nav.push("Model")

        assertEquals(2, nav.depth)
        assertEquals("Model", nav.current)

        // Back returns ONE level: the page underneath, not the card.
        nav.back()
        assertTrue(nav.isOpen)
        assertEquals(1, nav.depth)
        assertEquals("Workflow settings", nav.current)
    }

    @Test
    fun aDisabledRowNeverOpens() {
        val nav = SubShellNavigation()

        nav.push("Workflow settings", enabled = false)

        assertFalse(nav.isOpen)
        assertEquals(0, nav.depth)
        assertNull(nav.current)
        // And an enabled row on the same stack still opens.
        nav.push("Workflow settings")
        assertTrue(nav.isOpen)
    }

    /** Each row owns its own page slot: reopening one is its page again. */
    @Test
    fun aRowKnowsWhenItsOwnPageIsTheOpenOne() {
        val nav = SubShellNavigation()
        val row = Any()
        val other = Any()

        nav.push("Workflow settings", id = row)

        assertTrue(nav.isOpen(row))
        assertFalse(nav.isOpen(other))
        nav.back()
        assertFalse(nav.isOpen(row))
    }
}
