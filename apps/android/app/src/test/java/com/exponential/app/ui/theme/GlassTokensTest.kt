package com.exponential.app.ui.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-698: [GlassTokens] used to WRITE DOWN the glass palette as literals
 * (white .05 / .06 / .08 / .10 …) beside the generated `DesignTokens.Glass`
 * values it was supposed to mirror, and the two had drifted a percent or two
 * apart on nearly every rung. Every member is an alias now, and these are the
 * assertions that keep it that way — a re-typed literal fails here rather than
 * a year later in a screenshot diff.
 */
class GlassTokensTest {

    @Test
    fun fillsAliasTheGeneratedGlassTokens() {
        assertEquals(DesignTokens.Glass.FillRow, GlassTokens.RowFill)
        assertEquals(DesignTokens.Glass.FillActive, GlassTokens.RowFillActive)
        assertEquals(DesignTokens.Glass.FillSection, GlassTokens.SectionFill)
        assertEquals(DesignTokens.Glass.FillCard, GlassTokens.CardFill)
    }

    @Test
    fun strokesAliasTheGeneratedGlassTokens() {
        assertEquals(DesignTokens.Glass.StrokeRow, GlassTokens.StrokeRow)
        assertEquals(DesignTokens.Glass.StrokeSection, GlassTokens.StrokeSection)
        assertEquals(DesignTokens.Glass.StrokeCard, GlassTokens.StrokeCard)
        assertEquals(DesignTokens.Glass.StrokeStrong, GlassTokens.StrokeStrong)
        assertEquals(DesignTokens.Glass.StrokeActive, GlassTokens.StrokeActive)
    }

    @Test
    fun theBackgroundGradientAliasesTheGeneratedStops() {
        assertEquals(DesignTokens.Glass.BackgroundTop, GlassTokens.BackgroundTop)
        assertEquals(DesignTokens.Glass.BackgroundBottom, GlassTokens.BackgroundBottom)
    }

    @Test
    fun radiiAliasTheGeneratedScale() {
        assertEquals(DesignTokens.Radius.Md, GlassTokens.RowRadius)
        assertEquals(DesignTokens.Radius.Lg, GlassTokens.GroupRadius)
        assertEquals(DesignTokens.Radius.Xl, GlassTokens.CardRadius)
        assertEquals(DesignTokens.Radius.Sm, GlassTokens.ChipRadius)
    }

    /** The one control diameter — circle buttons, the 32dp control rung. */
    @Test
    fun theControlSizeAliasesTheGeneratedControlRung() {
        assertEquals(DesignTokens.Size.ControlMd, GlassTokens.ControlSize)
        assertEquals(32.dp, GlassTokens.ControlSize)
    }

    /**
     * The hairline has no generated twin: Compose draws a border at device
     * pixels, and 0.5dp is the thinnest line that survives every density.
     */
    @Test
    fun theHairlineIsHalfADp() {
        assertEquals(0.5.dp, GlassTokens.Hairline)
    }

    @Test
    fun theOpaqueCardFillIsTheCardTintOverThePopoverBase() {
        assertEquals(GlassTokens.CardFill.compositeOver(DesignTokens.Palette.Card), GlassTokens.OpaqueCardFill)
        assertEquals(1f, GlassTokens.OpaqueCardFill.alpha)
        assertEquals(Color(0xFF252525), GlassTokens.OpaqueCardFill)
    }
}
