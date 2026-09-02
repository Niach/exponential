package com.exponential.app.ui.components

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-698: the segmented capsule (Inbox/My issues, the agent strip, the
 * Start-coding sheet's subject tabs) is the one control all four clients draw
 * identically, and it used to carry its chrome as literals inside the
 * composable — a `white.12` hairline, a `white.12` active pill, 4dp and 7dp
 * paddings. iOS pins the twins in `GlassSegmentedControlTokenTests`; these are
 * the numbers Android must not drift from.
 */
class GlassSegmentedControlDefaultsTest {

    @Test
    fun theContainerHairlineIsTheStrongStroke() {
        assertEquals(GlassTokens.StrokeStrong, GlassSegmentedControlDefaults.Hairline)
        assertEquals(DesignTokens.Glass.StrokeStrong, GlassSegmentedControlDefaults.Hairline)
    }

    /**
     * The selected segment is the shared ACTIVE fill — the same tint a pressed
     * glass row or an on-state circle button takes, not a second recipe that
     * happened to land near it.
     */
    @Test
    fun theActiveSegmentIsTheSharedActiveFill() {
        assertEquals(GlassTokens.RowFillActive, GlassSegmentedControlDefaults.ActiveFill)
        assertEquals(DesignTokens.Glass.FillActive, GlassSegmentedControlDefaults.ActiveFill)
    }

    @Test
    fun theContainerInsetsItsSegmentsByFourDp() {
        assertEquals(4.dp, GlassSegmentedControlDefaults.ContainerPadding)
    }

    /** The strip's height comes from this alone — no fixed height anywhere. */
    @Test
    fun aSegmentPadsSevenDpVertically() {
        assertEquals(7.dp, GlassSegmentedControlDefaults.SegmentVerticalPadding)
    }

    @Test
    fun containerAndSegmentsAreCapsules() {
        assertEquals(RoundedCornerShape(percent = 50), GlassSegmentedControlDefaults.Shape)
    }
}
