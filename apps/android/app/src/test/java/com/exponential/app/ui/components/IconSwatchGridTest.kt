package com.exponential.app.ui.components

import com.exponential.app.data.api.builtinCreateAction
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.icons.ExpIcons
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-273: `icon`-typed action inputs (the "Create action" builtin's optional
 * Icon) render the curated swatch grid, NOT a text field where a raw glyph name
 * had to be typed. The value is a `pickable` registry NAME and the input is
 * optional, so "no icon" must be a real state — this locks the resolver both
 * the grid and the sheet's summary row read.
 */
class IconSwatchGridTest {

    @Test
    fun `an unfilled optional input picks no icon`() {
        assertNull(pickableIconName(null))
        assertNull(pickableIconName(""))
        assertNull(pickableIconName("   "))
    }

    @Test
    fun `a name outside the pickable set picks no icon`() {
        // A newer server (or a hand-typed leftover from the old text field)
        // must read as unset rather than highlight a phantom swatch.
        assertNull(pickableIconName("not-a-real-glyph"))
        // A registry glyph that is not user-pickable is not offered either.
        assertNotNull(ExpIcons.byName("bell-off"))
        assertTrue("bell-off" !in ExpIcons.pickable)
        assertNull(pickableIconName("bell-off"))
    }

    @Test
    fun `a pickable name resolves to itself`() {
        assertEquals("rocket", pickableIconName("rocket"))
        assertEquals("square-kanban", pickableIconName("square-kanban"))
        assertEquals(
            ExpIcons.pickable.first(),
            pickableIconName(ExpIcons.pickable.first()),
        )
    }

    @Test
    fun `every pickable name draws a glyph`() {
        // The grid skips a name the registry can't resolve, which would
        // silently shrink the picker; nothing may be droppable.
        val missing = ExpIcons.pickable.filter { ExpIcons.byName(it) == null }
        assertEquals(emptyList<String>(), missing)
    }

    @Test
    fun `the create-action builtin declares an optional icon input`() {
        val icon = builtinCreateAction("team-1").inputs.orEmpty()
            .single { it.key == "icon" }
        assertEquals("icon", icon.type)
        // Known to the contract, so the sheet never blocks the run as an
        // unknown type — it must reach the grid branch instead.
        assertTrue(icon.type in DomainContract.actionInputTypeValues)
        assertTrue("the Icon input is optional", !icon.required)
    }
}
