package at.exponential.ui.compose

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.graphics.graphicsLayer
import at.exponential.ui.catalog.CatalogConstants
import at.exponential.ui.model.SurfaceModel
import kotlin.math.max

/**
 * The chrome glyphs the controls need (check marks, chevrons, close,
 * search, calendar, send, attach, loader, play, image…), drawn from
 * Lucide-shaped 24-unit paths so a host with no icon registry still gets
 * working controls. Catalog icon NAMES go through `HostPlugin.icon`
 * ([IconView]); these never do.
 */
enum class Glyph(
    /** SVG path data on a 24 × 24 grid. */
    internal val paths: List<String>,
    /** Filled (play, stop) instead of stroked. */
    internal val filled: Boolean = false,
) {
    /** A check mark. */
    Check(listOf("M20 6 9 17 4 12")),

    /** A chevron pointing down. */
    ChevronDown(listOf("M6 9l6 6 6-6")),

    /** A chevron pointing up. */
    ChevronUp(listOf("M18 15l-6-6-6 6")),

    /** A chevron pointing right. */
    ChevronRight(listOf("M9 18l6-6-6-6")),

    /** A chevron pointing left. */
    ChevronLeft(listOf("M15 18l-6-6 6-6")),

    /** Up + down chevrons (a Select trigger). */
    ChevronsUpDown(listOf("M7 15l5 5 5-5", "M7 9l5-5 5 5")),

    /** An ×. */
    Close(listOf("M18 6 6 18", "M6 6l12 12")),

    /** A magnifying glass. */
    Search(listOf("M11 3a8 8 0 1 0 0 16a8 8 0 1 0 0-16z", "M21 21l-4.3-4.3")),

    /** A calendar page. */
    Calendar(listOf("M8 2v4", "M16 2v4", "M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z", "M3 10h18")),

    /** A clock face (a TimePicker trigger). */
    Clock(listOf("M12 2a10 10 0 1 0 0 20a10 10 0 1 0 0-20z", "M12 6v6l4 2")),

    /** An arrow up (send). */
    Send(listOf("M5 12l7-7 7 7", "M12 19V5")),

    /** A paperclip (attach). */
    Attach(listOf("M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48")),

    /** An open circle (a loader frame). */
    Loader(listOf("M21 12a9 9 0 1 1-6.219-8.56")),

    /** A play triangle (filled). */
    Play(listOf("M6 3l14 9-14 9z"), filled = true),

    /** A picture frame. */
    Image(listOf("M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z", "M9 7a2 2 0 1 0 0 4a2 2 0 1 0 0-4z", "M21 15l-3.086-3.086a2 2 0 0 0-2.828 0L6 21")),

    /** A rounded square (filled stop). */
    Stop(listOf("M8 6h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z"), filled = true),

    /** A circle outline. */
    Circle(listOf("M12 2a10 10 0 1 0 0 20a10 10 0 1 0 0-20z")),

    /** An arrow up and right (an external link). */
    ExternalLink(listOf("M7 7h10v10", "M7 17 17 7")),
    ;

    private val vectors = HashMap<Float, ImageVector>()

    /** The vector at a stroke width (cached per width). */
    internal fun vector(strokeWidth: Float): ImageVector = vectors.getOrPut(strokeWidth) {
        val b = ImageVector.Builder(name = name, defaultWidth = 24.dp, defaultHeight = 24.dp, viewportWidth = 24f, viewportHeight = 24f)
        for (d in paths) {
            if (filled) {
                b.addPath(pathData = addPathNodes(d), fill = SolidColor(Color.Black))
            } else {
                b.addPath(
                    pathData = addPathNodes(d),
                    stroke = SolidColor(Color.Black),
                    strokeLineWidth = strokeWidth,
                    strokeLineCap = StrokeCap.Round,
                    strokeLineJoin = StrokeJoin.Round,
                )
            }
        }
        b.build()
    }
}

/** The stroke weights a [GlyphView] draws with (Lucide's 2 = medium). */
enum class GlyphWeight(internal val stroke: Float) {
    /** 2 units on the 24 grid. */
    Medium(2f),

    /** 2.25 units. */
    Semibold(2.25f),

    /** 2.75 units (the Checkbox check). */
    Bold(2.75f),
}

/** A chrome [glyph] in a `size` dp square, tinted [color]; decorative (no semantics). */
@Composable
fun GlyphView(glyph: Glyph, size: Float, color: Color, modifier: Modifier = Modifier, weight: GlyphWeight = GlyphWeight.Medium) {
    val painter = rememberVectorPainter(glyph.vector(weight.stroke))
    Image(
        painter = painter,
        contentDescription = null,
        modifier = modifier.size(max(size, 0f).dp).clearAndSetSemantics {},
        colorFilter = ColorFilter.tint(color),
    )
}

/**
 * A catalog icon by NAME through the host (`HostPlugin.icon`, drawn in
 * [color] as the `LocalContentColor`); the placeholder circle (a 1.5 dp
 * ring at 80 %, half alpha) when the host has none.
 */
@Composable
fun IconView(name: String, size: Float, color: Color, model: SurfaceModel, modifier: Modifier = Modifier, rtl: Boolean = false) {
    val s = max(size, 0f)
    val icon = model.host.icon(name, s)
    // Round 1 §4: under rtl ONLY the directional glyphs mirror, outermost.
    val mirror = rtl && name in CatalogConstants.rtlMirroredIcons
    Box(modifier.then(if (mirror) Modifier.graphicsLayer { scaleX = -1f } else Modifier).size(s.dp), contentAlignment = Alignment.Center) {
        if (icon != null) {
            CompositionLocalProvider(LocalContentColor provides color) { icon() }
        } else {
            Canvas(Modifier.size((s * 0.8f).dp)) {
                val w = 1.5.dp.toPx()
                drawCircle(color.copy(alpha = color.alpha * 0.5f), radius = (this.size.minDimension - w) / 2f, style = Stroke(w))
            }
        }
    }
}

/** The rotating loader: an 85 % arc spinning once per 0.8 s, decorative. */
@Composable
fun SpinnerView(size: Float, color: Color, modifier: Modifier = Modifier) {
    val transition = rememberInfiniteTransition(label = "spinner")
    val angle = transition.animateFloat(0f, 360f, infiniteRepeatable(tween(800, easing = LinearEasing), RepeatMode.Restart), label = "angle")
    val s = max(size, 0f)
    Canvas(modifier.size(s.dp).clearAndSetSemantics {}) {
        val w = max(1.5f, s / 10f).dp.toPx()
        val inset = 1.dp.toPx() + w / 2f
        rotate(angle.value) {
            drawArc(
                color = color,
                startAngle = 0.15f * 360f - 90f,
                sweepAngle = 0.85f * 360f,
                useCenter = false,
                topLeft = Offset(inset, inset),
                size = Size(this.size.width - 2 * inset, this.size.height - 2 * inset),
                style = Stroke(width = w, cap = StrokeCap.Round),
            )
        }
    }
}
