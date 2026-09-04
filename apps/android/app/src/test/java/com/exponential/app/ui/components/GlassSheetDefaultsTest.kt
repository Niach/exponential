package com.exponential.app.ui.components

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-687: every bottom sheet on every mobile client renders the SAME shell —
 * opaque #111114 (the gradient's darker EXP-723 bottom stop), a drag handle, a
 * left title, one pinned bottom button, and a fitted height capped at 85 % of
 * the screen. iOS pins the twin numbers in
 * `GlassSheetTokenTests`; these are the values Android must not drift from.
 */
class GlassSheetDefaultsTest {

    @Test
    fun containerIsTheOpaqueGradientEndColor() {
        // A sheet continues the app gradient where it ends, so it takes the
        // gradient's BOTTOM stop — opaque, because Compose can't blur.
        assertEquals(GlassTokens.BackgroundBottom, GlassSheetDefaults.ContainerColor)
        assertEquals(Color(0xFF111114), GlassSheetDefaults.ContainerColor)
        assertEquals(1f, GlassSheetDefaults.ContainerColor.alpha)
    }

    /** The generated design token is the source; Glass.kt must keep mirroring it. */
    @Test
    fun theContainerIsTheGeneratedDesignToken() {
        assertEquals(DesignTokens.Glass.BackgroundBottom, GlassSheetDefaults.ContainerColor)
    }

    @Test
    fun fittedSheetsAreCappedAt85PercentOfTheScreen() {
        assertEquals(0.85f, GlassSheetDefaults.FittedMaxHeightFraction)
    }

    @Test
    fun theHandleIsTheOnlyDismissAffordance() {
        // No ✕, no Cancel pill — so the handle has to read as one.
        assertEquals(Color.White.copy(alpha = TextEmphasis.Quaternary), GlassSheetDefaults.DragHandleColor)
    }

    @Test
    fun titleUsesTheSheetRowLabelColor() {
        assertEquals(Color.White.copy(alpha = 0.9f), GlassSheetDefaults.TitleColor)
        assertEquals(GlassMenuDefaults.TextColor, GlassSheetDefaults.TitleColor)
    }

    /** The title lines up with the gutter [GlassSheetRow] pads its rows to. */
    @Test
    fun headerSharesTheRowGutter() {
        assertEquals(20.dp, GlassSheetDefaults.HorizontalPadding)
    }
}
