package com.exponential.app.ui.markdown

import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Placement of the markdown rail's attach/list menus (EXP-607). The rail sits
 * directly on the IME, so a menu placed BELOW its button lands in the keyboard
 * band — M3's own provider cannot see that band in an edge-to-edge window,
 * which is why the placement is hand-rolled and pinned by these tests.
 */
class RailMenuOffsetTest {

    private val window = IntSize(1080, 2400)
    private val menu = IntSize(320, 200)

    private fun place(
        anchor: IntRect = IntRect(120, 1400, 156, 1436),
        popup: IntSize = menu,
    ) = railMenuPopupOffset(
        anchorBounds = anchor,
        popupSize = popup,
        windowSize = window,
        marginPx = 8,
        gapPx = 4,
    )

    @Test
    fun theMenuSitsAboveTheAnchorLeftAligned() {
        assertEquals(IntOffset(120, 1400 - 4 - menu.height), place())
    }

    @Test
    fun theMenuClampsToTheLeftMargin() {
        assertEquals(8, place(anchor = IntRect(0, 1400, 36, 1436)).x)
    }

    @Test
    fun theMenuClampsToTheRightEdge() {
        assertEquals(
            window.width - menu.width - 8,
            place(anchor = IntRect(1040, 1400, 1076, 1436)).x,
        )
    }

    /** A menu taller than the space above it pins at the top margin rather than off-screen. */
    @Test
    fun theMenuNeverGoesAboveTheTopMargin() {
        assertEquals(8, place(anchor = IntRect(120, 40, 156, 76)).y)
        assertEquals(8, place(anchor = IntRect(120, 1400, 156, 1436), popup = IntSize(320, 2000)).y)
    }

    /**
     * Anchor-relative placement is what keeps the menu glued to its button as
     * the rail moves with the IME: shifting the anchor shifts the result by
     * exactly the same delta.
     */
    @Test
    fun placementShiftsExactlyWithTheAnchor() {
        val a = place(anchor = IntRect(120, 1400, 156, 1436))
        val b = place(anchor = IntRect(180, 1300, 216, 1336))
        assertEquals(a.x + 60, b.x)
        assertEquals(a.y - 100, b.y)
    }

    /** The regression: the menu must never reach into the keyboard band. */
    @Test
    fun theMenuNeverEntersTheKeyboardBand() {
        val imeHeight = 900
        val railHeight = 44
        val anchorTop = window.height - imeHeight - railHeight
        val offset = place(anchor = IntRect(120, anchorTop, 156, anchorTop + 36))
        assertTrue(
            "menu bottom ${offset.y + menu.height} must stay above the rail at $anchorTop",
            offset.y + menu.height <= anchorTop,
        )
    }
}
