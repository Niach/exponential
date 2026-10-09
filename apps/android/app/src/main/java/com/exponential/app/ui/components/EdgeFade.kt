package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DetailChrome
import com.exponential.app.ui.theme.GlassTokens
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.HazeTint
import dev.chrisbanes.haze.hazeEffect
import dev.chrisbanes.haze.hazeSource

// EXP-1162: the detail chrome's EDGE STRIPS (`DetailChrome`, the contract
// fixture `detail-chrome.json`; iOS `EdgeFade.swift`). Content scrolls UNDER
// the header band and the floating bottom bar; behind them sits the page
// background at `scrim` alpha over an `edgeBlur` blur, and a thin strip past
// each fades both to nothing — no hairline, no hard cut. The blur is Haze:
// what scrolls registers as a SOURCE ([detailHazeSource]) on the host's ONE
// [LocalDetailHaze] state, the chrome draws it blurred. Below API 31 Haze
// draws the scrim alone, which the fixture allows.

/** The Work screen's backdrop state; null (outside it) = the scrim alone. */
val LocalDetailHaze = compositionLocalOf<HazeState?> { null }

/**
 * Whether the page composing here is the one ON SCREEN. A pager composes its
 * neighbours too ([TabPager]'s `beyondViewportPageCount`), and every face body
 * registers as a source; only the shown page may feed the ONE state. Two
 * sources on it (the Issue face and the off-screen Run face, EXP-1175) kept
 * Haze re-running its pre-draw pass on every frame: a never-ending frame loop
 * that burned the battery and kept Compose from ever going idle.
 */
val LocalDetailHazeSourceActive = compositionLocalOf { true }

/** Marks scrolling content the chrome blurs. A no-op outside the Work screen
 *  and on a pager page that is not the shown one. */
@Composable
fun Modifier.detailHazeSource(): Modifier {
    val state = LocalDetailHaze.current ?: return this
    if (!LocalDetailHazeSourceActive.current) return this
    return this.hazeSource(state)
}

/**
 * The page [color] at `scrim` alpha over the blurred content behind, faded by
 * [mask] (null = solid). Without a [state] the scrim draws alone.
 */
private fun Modifier.edgeBackdrop(state: HazeState?, color: Color, mask: Brush?): Modifier {
    val scrim = color.copy(alpha = DetailChrome.SCRIM)
    if (state == null) return background(scrim)
    return hazeEffect(state) {
        backgroundColor = color
        tints = listOf(HazeTint(scrim))
        fallbackTint = HazeTint(scrim)
        blurRadius = DetailChrome.EDGE_BLUR.dp
        noiseFactor = 0f
        this.mask = mask
    }
}

/** Which edge a strip hangs from: full strength there, nothing opposite. */
enum class FadeEdge { Top, Bottom }

private fun fadeMask(edge: FadeEdge): Brush = when (edge) {
    FadeEdge.Top -> Brush.verticalGradient(listOf(Color.Black, Color.Transparent))
    FadeEdge.Bottom -> Brush.verticalGradient(listOf(Color.Transparent, Color.Black))
}

/**
 * The FADING strip: the backdrop from full strength at [edge] to nothing at
 * the opposite side. Takes no touch (no pointer handling at all).
 */
@Composable
fun EdgeFade(edge: FadeEdge, modifier: Modifier = Modifier) {
    val color = if (edge == FadeEdge.Top) GlassTokens.BackgroundTop else GlassTokens.BackgroundBottom
    val state = LocalDetailHaze.current
    val backdrop = if (state == null) {
        val scrim = color.copy(alpha = DetailChrome.SCRIM)
        val stops = if (edge == FadeEdge.Top) listOf(scrim, Color.Transparent) else listOf(Color.Transparent, scrim)
        Modifier.background(Brush.verticalGradient(stops))
    } else {
        Modifier.edgeBackdrop(state, color, fadeMask(edge))
    }
    Box(modifier.then(backdrop))
}

/**
 * The header band's chrome: the solid backdrop behind [content] (up through
 * the status bar, which the top bar already pads) and the `edgeTop` strip
 * hanging BELOW it over whatever scrolls there. The strip is placed outside
 * the reported size, so the band's height — and the Scaffold's top padding —
 * is untouched, and a touch there reaches the content.
 */
@Composable
fun HeaderEdgeChrome(content: @Composable () -> Unit) {
    val state = LocalDetailHaze.current
    Layout(
        contents = listOf(
            { EdgeFade(FadeEdge.Top) },
            { Box(Modifier.edgeBackdrop(state, GlassTokens.BackgroundTop, mask = null)) { content() } },
        ),
    ) { (stripMeasurables, bandMeasurables), constraints ->
        val band = bandMeasurables.first().measure(constraints)
        val strip = stripMeasurables.first().measure(
            Constraints.fixed(band.width, DetailChrome.EDGE_TOP.dp.roundToPx()),
        )
        layout(band.width, band.height) {
            strip.place(0, band.height)
            band.place(0, 0)
        }
    }
}

/**
 * The bottom strip behind a floating bottom bar ([content]): from the screen's
 * bottom edge ([bottomInset] below the bar — the navigation bar it sits on)
 * up to `edgeBottom` above the bar's top, fading upwards. Drawn BEHIND the bar
 * and outside its reported size, so it never shifts a slot or takes a touch.
 * [visible] hides it without touching the bar's identity (an expanded
 * composer card is its own surface).
 */
@Composable
fun FloatingBarEdge(
    bottomInset: Dp,
    modifier: Modifier = Modifier,
    visible: Boolean = true,
    content: @Composable () -> Unit,
) {
    Layout(
        contents = listOf(
            { if (visible) EdgeFade(FadeEdge.Bottom) },
            content,
        ),
        modifier = modifier,
    ) { (stripMeasurables, barMeasurables), constraints ->
        val bars = barMeasurables.map { it.measure(constraints.copy(minWidth = 0, minHeight = 0)) }
        val barWidth = bars.maxOfOrNull { it.width } ?: 0
        // Edge to edge: the strip spans the screen even under a narrower bar.
        val width = if (constraints.hasBoundedWidth) constraints.maxWidth else barWidth
        val height = bars.maxOfOrNull { it.height } ?: 0
        val above = DetailChrome.EDGE_BOTTOM.dp.roundToPx()
        val below = bottomInset.roundToPx()
        // A bar that draws nothing (faded out, no slots) wants no strip.
        val strip = if (height > 0) {
            stripMeasurables.firstOrNull()?.measure(Constraints.fixed(width, height + above + below))
        } else {
            null
        }
        layout(width, height) {
            strip?.place(0, -above)
            bars.forEach { it.place((width - it.width) / 2, 0) }
        }
    }
}
