package com.exponential.app.ui.components

import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-698: fourteen hand-rolled capsules collapsed into one [GlassPill] with
 * two rungs. Every one of them used to carry its own height, padding, glyph
 * size and label alpha — a filter pill was 6dp tall in its padding, a property
 * chip 6dp with a 14dp glyph, a role badge 3dp, a suggestion chip 2dp — so the
 * same screen showed four capsule heights side by side. These are the two
 * rungs that replaced them; iOS pins the twins in its own token tests.
 */
class GlassPillDefaultsTest {

    /** Md is the shared 32dp control rung — the circle button's diameter. */
    @Test
    fun theMediumPillIsTheControlRung() {
        assertEquals(32.dp, GlassPillDefaults.MdHeight)
        assertEquals(GlassTokens.ControlSize, GlassPillDefaults.MdHeight)
        assertEquals(DesignTokens.Size.ControlMd, GlassPillDefaults.MdHeight)
        assertEquals(GlassPillDefaults.MdHeight, GlassPillDefaults.height(PillSize.Md))
    }

    /** Sm is the small control rung — an inline badge inside a list row. */
    @Test
    fun theSmallPillIsTheSmallControlRung() {
        assertEquals(24.dp, GlassPillDefaults.SmHeight)
        assertEquals(DesignTokens.Size.ControlSm, GlassPillDefaults.SmHeight)
        assertEquals(GlassPillDefaults.SmHeight, GlassPillDefaults.height(PillSize.Sm))
    }

    @Test
    fun horizontalPaddingIsTwelveAndEight() {
        assertEquals(12.dp, GlassPillDefaults.horizontalPadding(PillSize.Md))
        assertEquals(8.dp, GlassPillDefaults.horizontalPadding(PillSize.Sm))
    }

    @Test
    fun glyphAndLabelAreSpacedSixAndFour() {
        assertEquals(6.dp, GlassPillDefaults.spacing(PillSize.Md))
        assertEquals(4.dp, GlassPillDefaults.spacing(PillSize.Sm))
    }

    @Test
    fun glyphsAreSixteenAndTwelve() {
        assertEquals(16.dp, GlassPillDefaults.glyphSize(PillSize.Md))
        assertEquals(12.dp, GlassPillDefaults.glyphSize(PillSize.Sm))
    }

    /** The label pill's colour disc — the only place a label's colour shows. */
    @Test
    fun theColourDotIsSixDp() {
        assertEquals(6.dp, GlassPillDefaults.DotSize)
    }

    /**
     * Every Sm number is strictly under its Md twin: the two rungs may not
     * converge into one, and the small pill may never out-measure the large.
     */
    @Test
    fun theSmallRungIsStrictlySmallerThanTheLarge() {
        assertTrue(GlassPillDefaults.SmHeight < GlassPillDefaults.MdHeight)
        assertTrue(GlassPillDefaults.SmHorizontalPadding < GlassPillDefaults.MdHorizontalPadding)
        assertTrue(GlassPillDefaults.SmSpacing < GlassPillDefaults.MdSpacing)
        assertTrue(GlassPillDefaults.SmGlyphSize < GlassPillDefaults.MdGlyphSize)
    }

    /**
     * A glyph always fits inside its capsule with room to spare — a pill whose
     * icon touched both edges is what made the old 14dp-glyph chips read as
     * cramped.
     */
    @Test
    fun everyGlyphFitsInsideItsCapsule() {
        PillSize.entries.forEach { size ->
            assertTrue(
                "$size glyph must clear its pill height",
                GlassPillDefaults.glyphSize(size) < GlassPillDefaults.height(size),
            )
        }
    }

    /**
     * EXP-1822: every pill number is the SHARED token (`tokens.json` `size`),
     * the one web's `Pill` classes, iOS `GlassPillTokens` and the IDE's
     * `glass_pill` are pinned to — never a figure typed here.
     */
    @Test
    fun everyRungReadsTheSharedTokens() {
        assertEquals(DesignTokens.Size.PillPaddingSm, GlassPillDefaults.SmHorizontalPadding)
        assertEquals(DesignTokens.Size.PillPaddingMd, GlassPillDefaults.MdHorizontalPadding)
        assertEquals(DesignTokens.Size.PillGapSm, GlassPillDefaults.SmSpacing)
        assertEquals(DesignTokens.Size.PillGapMd, GlassPillDefaults.MdSpacing)
        assertEquals(DesignTokens.Size.PillGlyphSm, GlassPillDefaults.SmGlyphSize)
        assertEquals(DesignTokens.Size.PillGlyphMd, GlassPillDefaults.MdGlyphSize)
        assertEquals(DesignTokens.Size.PillDot, GlassPillDefaults.DotSize)
    }

    /**
     * EXP-1822: the assignee chip's face is 16dp ×4 (web `UserAvatar
     * size={16}`, iOS, the IDE's XSmall). Android alone drew 18dp, which left
     * 3dp above and below it in the 24dp capsule instead of 4.
     */
    @Test
    fun theAssigneeFaceIsSixteenAndClearsTheCapsuleByFour() {
        assertEquals(16.dp, GlassPillDefaults.AvatarSize)
        assertEquals(DesignTokens.Size.PillAvatar, GlassPillDefaults.AvatarSize)
        assertEquals(4.dp, (GlassPillDefaults.SmHeight - GlassPillDefaults.AvatarSize) / 2)
    }

    /**
     * EXP-1822: the labels chip's colour dots are web's `h-2 w-2` LAYOUT box
     * (the ring paints outside it). Android's were a 10dp box, so a labelled
     * chip came out 2dp wider per dot than on the other clients.
     */
    @Test
    fun theLabelsChipDotIsEightDp() {
        assertEquals(8.dp, GlassPillDefaults.LabelDotSize)
        assertEquals(DesignTokens.Size.PillLabelDot, GlassPillDefaults.LabelDotSize)
    }

    /** EXP-1822: the property chip box wraps at 6 and pads by 10 ×4. */
    @Test
    fun theChipBoxGapAndPaddingAreTheSharedOnes() {
        assertEquals(6.dp, GlassPillDefaults.ChipBoxGap)
        assertEquals(10.dp, GlassPillDefaults.ChipBoxPadding)
        assertEquals(DesignTokens.Size.ChipBoxGap, GlassPillDefaults.ChipBoxGap)
        assertEquals(DesignTokens.Size.ChipBoxPadding, GlassPillDefaults.ChipBoxPadding)
    }
}
