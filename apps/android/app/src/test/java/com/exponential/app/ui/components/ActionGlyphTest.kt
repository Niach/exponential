package com.exponential.app.ui.components

import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.builtinCreateAction
import com.exponential.app.ui.icons.ExpIcons
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test

/**
 * EXP-721: every surface that draws an action takes the SAME glyph — the
 * action's own curated registry icon, falling back to the generic action mark.
 * The Start-coding picker used to key off `isBuiltin` instead, so an action
 * with a `package` icon showed a bolt there and its own glyph in the Actions
 * list.
 */
class ActionGlyphTest {

    private fun action(icon: String?) = ActionDto(
        id = "a1",
        teamId = "t1",
        name = "Ship it",
        icon = icon,
    )

    @Test
    fun `a curated icon name resolves to its registry glyph`() {
        assertNotNull(ExpIcons.byName("package"))
        assertEquals(ExpIcons.byName("package"), actionGlyph(action("package")))
    }

    @Test
    fun `no icon falls back to the generic action mark`() {
        assertEquals(ExpIcons.actionDefault, actionGlyph(action(null)))
    }

    @Test
    fun `an unknown icon name falls back to the generic action mark`() {
        assertEquals(ExpIcons.actionDefault, actionGlyph(action("not-a-real-glyph")))
    }

    /** An automation or a session may name a row the shape hasn't synced yet. */
    @Test
    fun `a missing action falls back to the generic action mark`() {
        assertEquals(ExpIcons.actionDefault, actionGlyph(null))
    }

    /** A builtin carries a curated icon too — it is not a special case. */
    @Test
    fun `the builtin creator draws its own glyph`() {
        val builtin = builtinCreateAction("t1")
        assertEquals(ExpIcons.byName(builtin.icon!!), actionGlyph(builtin))
    }
}
