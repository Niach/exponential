package com.exponential.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ContextBarSlice
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens

// EXP-1051: the STACKED sibling of [UsageTrack] — one track, many slices, and
// the tick marks that say where the compaction floor and the two usage
// thresholds sit. Same height and same track colour as the single-fill bar, so
// the context-window block and the rate-limit windows above it read as one
// instrument. (Web `ContextBar`, desktop `ui::context_layout`, iOS
// `ContextSegmentedTrack`.)

/**
 * The tone vocabulary the contract's layers name ([ContextBarSlice.tone]) in
 * mobile's palette: the avatar hues (the app's ONE generated colour wheel),
 * `neutral` = the foreground at a whisper, and `track` = the track itself —
 * `free` is never a slice, so a `track` tone draws nothing.
 */
fun contextToneColor(tone: String, foreground: Color): Color = when (tone) {
    "green" -> DesignTokens.Avatar.Green
    "yellow" -> DesignTokens.Avatar.Yellow
    "orange" -> DesignTokens.Avatar.Orange
    "violet" -> DesignTokens.Avatar.Violet
    "pink" -> DesignTokens.Avatar.Pink
    "blue" -> DesignTokens.Avatar.Blue
    "red" -> DesignTokens.Avatar.Red
    "teal" -> DesignTokens.Avatar.Teal
    "track" -> GlassTokens.StrokeStrong
    // `neutral` and anything a newer contract names: the foreground, dimmed.
    else -> foreground.copy(alpha = 0.30f)
}

/**
 * A stacked meter: [slices] left to right as percentages OF THE WHOLE track,
 * CLIPPED at 100% (the device's estimates may overshoot what it measured, and
 * a silently rescaled bar would lie about the layer sizes), with a hairline
 * mark at each of [ticks].
 */
@Composable
fun SegmentedTrack(
    slices: List<Pair<Color, Double>>,
    ticks: List<Int>,
    modifier: Modifier = Modifier,
    height: Dp = UsageTrackHeight,
    tickColor: Color = Color.White.copy(alpha = 0.35f),
) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(height)
            .clip(RoundedCornerShape(height / 2)),
    ) {
        Canvas(modifier = Modifier.fillMaxWidth().height(height)) {
            drawRect(color = GlassTokens.StrokeStrong, size = size)
            var x = 0f
            for ((color, percent) in slices) {
                if (x >= size.width) break
                val width = (percent / 100.0).coerceAtLeast(0.0).toFloat() * size.width
                if (width <= 0f) continue
                val drawn = width.coerceAtMost(size.width - x)
                drawRect(color = color, topLeft = Offset(x, 0f), size = Size(drawn, size.height))
                x += drawn
            }
            // The marks last, so a full bar never buries its own thresholds.
            val tickWidth = 1.dp.toPx()
            for (tick in ticks) {
                val at = (tick / 100f).coerceIn(0f, 1f) * size.width - tickWidth / 2f
                drawRect(
                    color = tickColor,
                    topLeft = Offset(at.coerceIn(0f, size.width - tickWidth), 0f),
                    size = Size(tickWidth, size.height),
                )
            }
        }
    }
}
