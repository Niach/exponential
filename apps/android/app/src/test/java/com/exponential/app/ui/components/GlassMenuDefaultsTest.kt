package com.exponential.app.ui.components

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-332: every popup menu in the app renders from [GlassMenuDefaults], and the
 * container is DERIVED (a white glass tint composited over the opaque popover
 * base) rather than written down. That makes it silently editable from two
 * unrelated places — `GlassTokens.CardFill` and `DesignTokens.Palette.Popover` —
 * so pin the resulting look here.
 */
class GlassMenuDefaultsTest {

    @Test
    fun containerIsTheOpaqueGlassCardFillOverThePopoverBase() {
        // white .06 over #171717 == #252525. Opaque on purpose: Compose can't
        // blur, and a translucent menu ghosts the text underneath (EXP-165).
        assertEquals(Color(0xFF252525), GlassMenuDefaults.ContainerColor)
        assertEquals(1f, GlassMenuDefaults.ContainerColor.alpha)
    }

    /**
     * EXP-357: menus and `glassCard(opaque = true)` (the Review bar's failure
     * banner) render the SAME opaque fill, from one token — a second recipe
     * would drift the moment one of them is retuned.
     */
    @Test
    fun theOpaqueFillIsTheSharedGlassToken() {
        assertEquals(GlassTokens.OpaqueCardFill, GlassMenuDefaults.ContainerColor)
        assertEquals(Color(0xFF252525), GlassTokens.OpaqueCardFill)
        assertEquals(1f, GlassTokens.OpaqueCardFill.alpha)
    }

    @Test
    fun strokeIsTheGlassHairline() {
        assertEquals(GlassTokens.Hairline, GlassMenuDefaults.Border.width)
        assertEquals(0.5.dp, GlassMenuDefaults.Border.width)
    }

    @Test
    fun radiusIsTheSectionRungNotM3ExtraSmall() {
        assertEquals(RoundedCornerShape(GlassTokens.SectionRadius), GlassMenuDefaults.Shape)
    }

    @Test
    fun separationComesFromFillAndStrokeNotElevation() {
        assertEquals(0.dp, GlassMenuDefaults.ShadowElevation)
        assertEquals(0.dp, GlassMenuDefaults.TonalElevation)
    }

    @Test
    fun theOpaqueBaseIsTheDesignSystemPopoverToken() {
        assertEquals(DesignTokens.Palette.Popover, DesignTokens.Palette.Card)
        assertEquals(Color(0xFF171717), DesignTokens.Palette.Popover)
    }
}
