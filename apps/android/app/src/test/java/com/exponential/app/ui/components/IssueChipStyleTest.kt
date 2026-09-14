package com.exponential.app.ui.components

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.markdown.MdStyle
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-885: ONE issue badge, ONE style. [IssueChip] is the composable half and
 * `ui/markdown/IssueRefChips.kt` the painted half (a chip inside a paragraph
 * cannot be a composable), so the recipe lives in exactly one place —
 * [MdStyle]'s chip tokens — and both halves read it. This pins the tokens
 * themselves plus the ONE number [IssueChip] cannot read off the painter (its
 * hairline is a `Stroke` width, not a token), so changing the chip means
 * changing this file too, deliberately, for every surface at once.
 */
class IssueChipStyleTest {

    /** A rounded RECT at 5dp — never a capsule; that is what [GlassPill] is. */
    @Test
    fun theChipIsTheFiveDpRoundedRect() {
        assertEquals(5.dp, MdStyle.chipCornerRadius)
    }

    /** White 10% fill behind a white 16% hairline, on both halves. */
    @Test
    fun theChipChromeIsTheTenAndSixteenPercentPair() {
        assertEquals(Color.White.copy(alpha = 0.10f), MdStyle.IssueRefBg)
        assertEquals(Color.White.copy(alpha = 0.16f), MdStyle.IssueRefBorder)
        // `drawChip` strokes the painted chip at 1dp; the composable draws the
        // same hairline as a border.
        assertEquals(1.dp, IssueChipDefaults.BorderWidth)
    }

    /** The status glyph rung, and the muted identifier beside it. */
    @Test
    fun theGlyphIsThirteenDpAndTheTokenIsFiftyFivePercent() {
        assertEquals(13.dp, MdStyle.chipIconSize)
        assertEquals(Color.White.copy(alpha = 0.55f), MdStyle.ChipToken)
        assertEquals(Color.White.copy(alpha = 0.9f), MdStyle.Text)
    }

    /** The removable variant's ✕ — the composer's rung, inside the chip. */
    @Test
    fun theRemoveControlIsATwentyDpHitAreaAroundATenDpGlyph() {
        assertEquals(20.dp, IssueChipDefaults.RemoveHitArea)
        assertEquals(10.dp, IssueChipDefaults.RemoveGlyph)
    }
}
