package at.exponential.ui.primitives

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.clearAndSetSemantics
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

/**
 * THE CHILD RULE (catalog/style.json `voiceWave`) on a child laid out at
 * its own size: drawn scaled by [scale] (the core's `child.scale` =
 * min(1, childSize / its longer edge), never up) about [pivot] (the wave
 * box's centre in the child's own fractions; the centre for a centred
 * child). At scale 0 (`sm`) it stays composed but paints nothing, takes no
 * pointer (a zero-size layer) and is hidden from assistive tech.
 */
fun Modifier.voiceWaveChild(scale: Float, pivot: TransformOrigin = TransformOrigin.Center): Modifier {
    val s = scale.coerceIn(0f, 1f)
    if (s == 1f) return this
    val layer = graphicsLayer {
        scaleX = s
        scaleY = s
        transformOrigin = pivot
        if (s == 0f) alpha = 0f
    }
    return if (s == 0f) layer.clearAndSetSemantics { } else layer
}

/** A voice wave of [size] dp: the [rings] the core computed for one frame. */
@Composable
fun VoiceWaveView(rings: List<VoiceWaveRing>, size: Dp, modifier: Modifier = Modifier) {
    Canvas(modifier.size(size)) { drawVoiceWave(rings) }
}

/**
 * A voice wave of [size] dp with its ONE [child] centred at its own size
 * and drawn at [childScale] about the centre (the core's `child.scale` for
 * the child's size, [voiceWaveChild]).
 */
@Composable
fun VoiceWaveView(rings: List<VoiceWaveRing>, size: Dp, childScale: Float, modifier: Modifier = Modifier, child: @Composable () -> Unit) {
    Box(modifier.size(size), contentAlignment = Alignment.Center) {
        Canvas(Modifier.size(size)) { drawVoiceWave(rings) }
        Box(Modifier.voiceWaveChild(childScale)) { child() }
    }
}
