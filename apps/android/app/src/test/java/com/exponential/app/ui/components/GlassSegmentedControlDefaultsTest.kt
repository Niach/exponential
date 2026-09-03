package com.exponential.app.ui.components

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-698: the segmented capsule (Inbox/My issues, the agent strip, the
 * Start-coding sheet's subject tabs) is the one control all four clients draw
 * identically, and it used to carry its chrome as literals inside the
 * composable — a `white.12` hairline, a `white.12` active pill, 4dp
 * paddings and spacing, a 7dp segment inset. iOS pins the twins in `GlassSegmentedControlTokenTests`; these are
 * the numbers Android must not drift from.
 */
class GlassSegmentedControlDefaultsTest {

    /**
     * A strip is a CONTAINER of choices, so it sits on the section rung — not
     * the row rung, where it read as one more list row with tabs in it.
     */
    @Test
    fun theContainerIsTheSectionRung() {
        assertEquals(GlassTokens.SectionFill, GlassSegmentedControlDefaults.ContainerFill)
        assertEquals(DesignTokens.Glass.FillSection, GlassSegmentedControlDefaults.ContainerFill)
        assertEquals(GlassTokens.StrokeSection, GlassSegmentedControlDefaults.Hairline)
        assertEquals(DesignTokens.Glass.StrokeSection, GlassSegmentedControlDefaults.Hairline)
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

    /** A track, not a gutter: 3dp of inset around the active pill. */
    @Test
    fun theContainerInsetsItsSegmentsByThreeDp() {
        assertEquals(3.dp, GlassSegmentedControlDefaults.ContainerPadding)
    }

    /**
     * Segments TOUCH. The active fill is the only thing that separates two of
     * them, so the strip reads as one control rather than a row of chips.
     */
    @Test
    fun segmentsAreNotSpaced() {
        assertEquals(0.dp, GlassSegmentedControlDefaults.SegmentSpacing)
    }

    /** A standalone strip is the large control rung — the 36dp web/desktop height. */
    @Test
    fun aStandaloneStripIsTheLargeControlRung() {
        assertEquals(DesignTokens.Size.ControlLg, GlassSegmentedControlDefaults.Height)
        assertEquals(36.dp, GlassSegmentedControlDefaults.Height)
    }

    /**
     * The EMBEDDED strip only. It has no container and no pinned height, so
     * this padding is what gives it one; a STANDALONE strip's segments fill
     * [GlassSegmentedControlDefaults.Height] and pad nothing, because 36 minus
     * two container insets minus two of these left 18dp for a 20sp line and
     * clipped every label.
     */
    @Test
    fun anEmbeddedSegmentPadsSixDpVertically() {
        assertEquals(6.dp, GlassSegmentedControlDefaults.SegmentVerticalPadding)
    }

    /**
     * A standalone segment must be able to draw a full line inside the strip:
     * the height left after both container insets has to clear the 20sp
     * (== 20dp at default density) `labelLarge` line box.
     */
    @Test
    fun aStandaloneSegmentHasRoomForItsLine() {
        val inner = GlassSegmentedControlDefaults.Height -
            GlassSegmentedControlDefaults.ContainerPadding * 2
        assertTrue("segment box $inner must clear a 20dp line", inner >= 20.dp)
    }

    /**
     * Selection changes ALPHA, never weight: a SemiBold/Normal swap re-measures
     * the label and shifted the whole strip on every tap.
     */
    @Test
    fun theLabelWeightIsConstant() {
        assertEquals(FontWeight.Medium, GlassSegmentedControlDefaults.LabelWeight)
    }

    @Test
    fun containerAndSegmentsAreCapsules() {
        assertEquals(RoundedCornerShape(percent = 50), GlassSegmentedControlDefaults.Shape)
    }
}
