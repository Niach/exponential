package com.exponential.app.ui.components

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1212/EXP-1215: the ONE prompt card's button row. The answers sit in one
 * row while their natural widths fit; otherwise they stack at their natural
 * widths on the trailing edge, the primary on top and a leading destructive
 * answer at the bottom (`prompts.json` LAYOUT).
 */
class GlassAlertDefaultsTest {

    @Test
    fun trailingPillsFitWithTheirGaps() {
        // Cancel 80 + gap 8 + Delete 80 = 168.
        assertTrue(GlassAlertDefaults.rowFits(listOf(80, 80), hasLeading = false, gap = 8, width = 168))
        assertFalse(GlassAlertDefaults.rowFits(listOf(80, 80), hasLeading = false, gap = 8, width = 167))
    }

    @Test
    fun aLeadingAnswerNeedsTwoGapsToStandApart() {
        // Discard 60 + 2 gaps + Create 100 + gap + Save 90 = 274.
        assertTrue(GlassAlertDefaults.rowFits(listOf(60, 100, 90), hasLeading = true, gap = 8, width = 274))
        assertFalse(GlassAlertDefaults.rowFits(listOf(60, 100, 90), hasLeading = true, gap = 8, width = 273))
    }

    @Test
    fun aSingleAnswerFitsAtItsOwnWidth() {
        assertTrue(GlassAlertDefaults.rowFits(listOf(50), hasLeading = false, gap = 8, width = 50))
    }

    @Test
    fun theStackPutsThePrimaryOnTop() {
        // Cancel · Merge this pull request · Merge stack (primary).
        assertEquals(listOf(2, 1, 0), GlassAlertDefaults.stackOrder(3, hasLeading = false))
    }

    @Test
    fun theStackPutsTheLeadingDestructiveAnswerLast() {
        // Discard (leading) · Create issue · Save draft (primary).
        // The leading child is index 0, so it lands at the bottom.
        assertEquals(listOf(2, 1, 0), GlassAlertDefaults.stackOrder(3, hasLeading = true))
        assertEquals(listOf(3, 2, 1, 0), GlassAlertDefaults.stackOrder(4, hasLeading = true))
        assertEquals(listOf(0), GlassAlertDefaults.stackOrder(1, hasLeading = true))
    }

    @Test
    fun aStackedPillKeepsItsNaturalWidth() {
        // Never a full-width block: Cancel stays 80 wide on a 300 card,
        // placed at x = 300 - 80 on the trailing edge.
        assertEquals(80, GlassAlertDefaults.stackedWidth(80, width = 300))
        assertEquals(300, GlassAlertDefaults.stackedWidth(80, width = 300) + (300 - 80))
    }

    @Test
    fun aStackedPillNeverOutgrowsTheCard() {
        assertEquals(240, GlassAlertDefaults.stackedWidth(400, width = 240))
    }
}
