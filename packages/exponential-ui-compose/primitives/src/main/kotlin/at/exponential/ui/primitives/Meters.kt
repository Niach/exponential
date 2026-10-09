package at.exponential.ui.primitives

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** One segment of a [MeterTrack]: a fraction of the whole width in [color]. */
@Immutable
data class MeterSegment(
    /** The share of the track's width (clamped 0..1). */
    val fraction: Float,
    /** The segment's colour. */
    val color: Color,
)

/**
 * A single-value capsule bar (the catalog `Progress`; the app's usage
 * track): [track] across the full width, [fill] over [fraction] of it with
 * its own round ends. Takes the full width.
 */
@Composable
fun MeterTrack(
    fraction: Float,
    modifier: Modifier = Modifier,
    height: Dp = 8.dp,
    track: Color = LocalPrimitiveTokens.current.muted,
    fill: Color = LocalPrimitiveTokens.current.primary,
) {
    MeterTrack(
        segments = listOf(MeterSegment(fraction, fill)),
        modifier = modifier,
        height = height,
        track = track,
        capsuleSegments = true,
    )
}

/**
 * A horizontal track of stacked [segments] (the catalog `Meter`).
 * [capsuleSegments] = each segment its own capsule, nothing clipped; else
 * the segments are clipped to the track's [radius] (default: a capsule).
 * [ticks] mark fractions of the width over the fill. Takes the full width.
 */
@Composable
fun MeterTrack(
    segments: List<MeterSegment>,
    modifier: Modifier = Modifier,
    height: Dp = 8.dp,
    track: Color = LocalPrimitiveTokens.current.muted,
    radius: Dp? = null,
    capsuleSegments: Boolean = false,
    ticks: List<Float> = emptyList(),
    tickColor: Color = Color.Black.copy(alpha = 0.45f),
    tickWidth: Dp = 1.dp,
) {
    Canvas(modifier.fillMaxWidth().height(height)) {
        val w = size.width
        val h = size.height
        val r = radius?.toPx() ?: (h / 2f)
        val corner = CornerRadius(r, r)
        drawRoundRect(track, size = size, cornerRadius = CornerRadius(h / 2f, h / 2f))
        if (capsuleSegments) {
            var x = 0f
            segments.forEach { s ->
                val sw = widthOf(s.fraction, w)
                if (sw > 0f) {
                    drawRoundRect(s.color, topLeft = Offset(x, 0f), size = Size(sw, h), cornerRadius = CornerRadius(h / 2f, h / 2f))
                }
                x += sw
            }
        } else {
            val clip = Path().apply { addRoundRect(RoundRect(0f, 0f, w, h, corner)) }
            clipPath(clip) {
                var x = 0f
                segments.forEach { s ->
                    val sw = widthOf(s.fraction, w)
                    drawRect(s.color, topLeft = Offset(x, 0f), size = Size(sw, h))
                    x += sw
                }
            }
        }
        val tw = tickWidth.toPx()
        ticks.forEach { t ->
            drawRect(tickColor, topLeft = Offset(widthOf(t, w), 0f), size = Size(tw, h))
        }
    }
}

/** A fraction of [total], clamped to the track. */
fun widthOf(fraction: Float, total: Float): Float =
    if (fraction.isNaN()) 0f else (total * fraction.coerceIn(0f, 1f)).coerceAtLeast(0f)

/**
 * A ring: a [track] circle and a [fill] arc from twelve o'clock, clockwise
 * to [fraction] (the catalog `Ring`; the app's context ring). [fillStroke]
 * defaults to [stroke]; an empty arc draws nothing (a round cap would leave
 * a dot).
 */
@Composable
fun RingView(
    fraction: Float,
    modifier: Modifier = Modifier,
    size: Dp = 16.dp,
    stroke: Dp = 2.dp,
    track: Color = LocalPrimitiveTokens.current.muted,
    fill: Color = LocalPrimitiveTokens.current.primary,
    fillStroke: Dp? = null,
    cap: StrokeCap = StrokeCap.Round,
) {
    val arc = if (fraction.isNaN()) 0f else fraction.coerceIn(0f, 1f)
    Canvas(modifier.size(size)) {
        val tw = stroke.toPx()
        val fw = (fillStroke ?: stroke).toPx()
        drawArc(
            color = track,
            startAngle = -90f,
            sweepAngle = 360f,
            useCenter = false,
            topLeft = Offset(tw / 2f, tw / 2f),
            size = Size(this.size.width - tw, this.size.height - tw),
            style = Stroke(width = tw, cap = cap),
        )
        if (arc > 0f) {
            drawArc(
                color = fill,
                startAngle = -90f,
                sweepAngle = 360f * arc,
                useCenter = false,
                topLeft = Offset(fw / 2f, fw / 2f),
                size = Size(this.size.width - fw, this.size.height - fw),
                style = Stroke(width = fw, cap = cap),
            )
        }
    }
}
