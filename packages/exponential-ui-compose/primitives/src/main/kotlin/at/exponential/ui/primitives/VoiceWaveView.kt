package at.exponential.ui.primitives

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp

/**
 * One closed wave of a [VoiceWaveView] (NEW-77, catalog/style.json
 * `voiceWave`): its points in dp from the box's top-left (the last repeats
 * the first), filled with [color] at [fillAlpha] and stroked at
 * [strokeAlpha], [strokeWidth] dp wide.
 */
@Immutable
class VoiceWaveRing(
    /** x0, y0, x1, y1, … in dp. */
    val points: FloatArray,
    val color: Color,
    val fillAlpha: Float,
    val strokeAlpha: Float,
    val strokeWidth: Float,
)

/**
 * The rings of a voice wave drawn in order (outer, mid, inner), each a
 * straight-segment closed path through its points, offset by [origin] px.
 */
fun DrawScope.drawVoiceWave(rings: List<VoiceWaveRing>, origin: Offset = Offset.Zero) {
    val d = density
    for (ring in rings) {
        val pts = ring.points
        if (pts.size < 4) continue
        val path = Path()
        path.moveTo(origin.x + pts[0] * d, origin.y + pts[1] * d)
        var i = 2
        while (i + 1 < pts.size) {
            path.lineTo(origin.x + pts[i] * d, origin.y + pts[i + 1] * d)
            i += 2
        }
        path.close()
        val a = ring.color.alpha
        if (ring.fillAlpha > 0f) drawPath(path, ring.color.copy(alpha = a * ring.fillAlpha.coerceIn(0f, 1f)), style = Fill)
        if (ring.strokeAlpha > 0f && ring.strokeWidth > 0f) {
            drawPath(path, ring.color.copy(alpha = a * ring.strokeAlpha.coerceIn(0f, 1f)), style = Stroke(width = ring.strokeWidth * d, join = StrokeJoin.Round))
        }
    }
}

/** A voice wave of [size] dp: the [rings] the core computed for one frame. */
@Composable
fun VoiceWaveView(rings: List<VoiceWaveRing>, size: Dp, modifier: Modifier = Modifier) {
    Canvas(modifier.size(size)) { drawVoiceWave(rings) }
}
