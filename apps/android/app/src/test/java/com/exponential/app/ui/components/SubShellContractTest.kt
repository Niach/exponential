package com.exponential.app.ui.components

import androidx.compose.runtime.Composable
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1029 contract — sub-shell navigation on Android (web, IDE and iOS carry
 * the same cases), implemented by EXP-1043.
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

    /**
     * The parity case (web `keeps the open page in sync with the card's live
     * props`, iOS `testAnOpenPageFollowsItsLiveBindings`): an open page draws
     * the surface's CURRENT state, never a snapshot frozen at tap time.
     *
     * Android reaches that a different way than the two siblings. There the
     * page is a closure the host re-invokes, so the test calls it twice and
     * reads both renders. Here it is a `@Composable` lambda, which a plain
     * JVM test cannot invoke at all (no composer), and this module has
     * neither Robolectric nor a compose rule — so the recomposition half is
     * NOT observable here and is not claimed to be: it rides `KeptAlive` in
     * [SubShellHost], which keeps the row composing behind the open page so
     * the compose compiler updates that lambda's captures in place.
     *
     * What the stack owns IS observable, and it is the half that would break
     * the rule on its own: the entry holds the ROW's lambda by reference
     * (never a copy, never a rendered snapshot), under the row's own slot id
     * so the level is not re-keyed while it is open.
     */
    @Test
    fun anOpenPageFollowsItsLiveBindings() {
        val nav = SubShellNavigation()
        val row = Any()
        // A CAPTURING page, the shape the sheet hands over (its pickers read
        // the surface's drafts) — the captures are what must stay live.
        val model = "opus"
        val page: @Composable () -> Unit = { check(model.isNotEmpty()) }

        nav.push("Workflow settings", id = row, content = page)

        val entry = nav.pages.last()
        assertSame(page, entry.content)
        assertSame(row, entry.id)

        // Going deeper leaves the page underneath holding its own lambda —
        // a parent level stays composed, so its row's captures stay live.
        val deeper: @Composable () -> Unit = { check(model.isNotEmpty()) }
        nav.push("Model", content = deeper)
        assertSame(page, nav.pages.first().content)
        assertSame(deeper, nav.pages.last().content)
    }
}
