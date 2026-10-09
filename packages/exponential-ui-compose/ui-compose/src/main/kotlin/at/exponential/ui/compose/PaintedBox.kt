package at.exponential.ui.compose

import android.graphics.BlurMaskFilter
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import at.exponential.ui.theme.GradientPaint
import at.exponential.ui.theme.PaintStyle
import at.exponential.ui.theme.ShadowLayer
import at.exponential.ui.theme.TransformOp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/**
 * The radii (dp) `[topLeft, topRight, bottomRight, bottomLeft]` a box of
 * [size] paints with: CSS clamping (when two radii on one side overflow it,
 * every radius scales down by the same factor).
 */
internal fun clampedCorners(style: PaintStyle, size: Size): List<Float> {
    val r = style.corners.map { max(0f, it) }
    if (r.all { it == 0f }) return r
    val w = size.width
    val h = size.height
    var f = 1f
    fun fit(a: Float, b: Float, side: Float) {
        if (a + b > side && a + b > 0f) f = min(f, side / (a + b))
    }
    fit(r[0], r[1], w)
    fit(r[3], r[2], w)
    fit(r[0], r[3], h)
    fit(r[1], r[2], h)
    return r.map { max(0f, it * f) }
}

/** The rounded rect of a box (px) with its clamped corners (dp radii × [density]). */
internal fun boxRoundRect(style: PaintStyle, sizeDp: Size, px: Size, density: Float, inset: Float = 0f): RoundRect {
    val c = clampedCorners(style, sizeDp).map { max(0f, it * density - inset) }
    return RoundRect(
        left = inset, top = inset, right = max(inset, px.width - inset), bottom = max(inset, px.height - inset),
        topLeftCornerRadius = CornerRadius(c[0], c[0]),
        topRightCornerRadius = CornerRadius(c[1], c[1]),
        bottomRightCornerRadius = CornerRadius(c[2], c[2]),
        bottomLeftCornerRadius = CornerRadius(c[3], c[3]),
    )
}

/** The node's clip shape: its rounded box (per-corner radii, CSS clamping). */
internal class BoxShape(private val style: PaintStyle, private val sizeDp: Size) : Shape {
    override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density): Outline =
        Outline.Rounded(boxRoundRect(style, sizeDp, size, density.density))
}

/** The 2D matrix of the node's `transform` ops about the box centre (px), source order like CSS. */
internal fun transformMatrix(ops: List<TransformOp>, size: Size, density: Float): Matrix? {
    if (ops.isEmpty()) return null
    val m = Matrix()
    val cx = size.width / 2f
    val cy = size.height / 2f
    m.translate(cx, cy)
    for (op in ops) when (op) {
        is TransformOp.Translate -> m.translate(op.x * density, op.y * density)
        is TransformOp.Scale -> m.scale(op.factor, op.factor)
        is TransformOp.Rotate -> m.rotateZ(op.degrees)
    }
    m.translate(-cx, -cy)
    return m
}

/** The gradient brush over a box of [size] px for a CSS angle (0 = up, 90 = right). */
internal fun gradientBrush(g: GradientPaint, size: Size): Brush {
    val rad = g.angle * PI.toFloat() / 180f
    val dx = sin(rad)
    val dy = -cos(rad)
    // CSS: the gradient line's length = |w·sin| + |h·cos|, through the centre.
    val half = (abs(size.width * dx) + abs(size.height * dy)) / 2f
    val c = Offset(size.width / 2f, size.height / 2f)
    val start = Offset(c.x - dx * half, c.y - dy * half)
    val end = Offset(c.x + dx * half, c.y + dy * half)
    return Brush.linearGradient(colorStops = g.stops.map { it.first to it.second }.toTypedArray(), start = start, end = end)
}

/**
 * A box painted from a resolved [PaintStyle] at a frame of [size] (dp):
 * the shadows behind (blurred rounded rects, spread and offset honoured),
 * the background (+ its gradient) as a rounded rect (per-corner radii, CSS
 * clamping), the border drawn INSIDE the box over the content (per-side
 * widths in one colour, `dashed` / `dotted`; children, whose frames already
 * include the border, are never offset), the opacity of the whole node
 * (`ModulateAlpha`, so shadows survive), the paint-only `transform` about
 * the centre and `visibility: hidden` (nothing paints). `backdropBlur` paints
 * the translucent background alone (see the README).
 */
fun Modifier.paintedBox(style: PaintStyle, size: Size, alphaScale: Float = 1f): Modifier {
    var m: Modifier = this
    val opacity = (style.opacity ?: 1f) * alphaScale * (if (style.visibilityHidden) 0f else 1f)
    if (opacity < 1f) {
        m = m.graphicsLayer {
            alpha = opacity.coerceIn(0f, 1f)
            compositingStrategy = CompositingStrategy.ModulateAlpha
        }
    }
    if (style.transform.isNotEmpty()) {
        m = m.drawWithContent {
            val matrix = transformMatrix(style.transform, this.size, density)
            if (matrix == null) drawContent() else withTransform({ transform(matrix) }) { this@drawWithContent.drawContent() }
        }
    }
    if (style.shadows.isNotEmpty() || style.background != null || style.gradient != null) {
        m = m.drawBehind {
            val rr = boxRoundRect(style, size, this.size, density)
            for (layer in style.shadows) drawShadowLayer(layer, rr)
            val path = Path().apply { addRoundRect(rr) }
            style.background?.let { bg -> drawPath(path, bg) }
            style.gradient?.let { g -> drawPath(path, gradientBrush(g, this.size)) }
        }
    }
    if (style.hasBorder) {
        m = m.drawWithContent {
            drawContent()
            drawBorder(style, size)
        }
    }
    return m
}

/** The border inside the box: a uniform rounded stroke, else each side as a band (one colour). */
private fun DrawScope.drawBorder(style: PaintStyle, sizeDp: Size) {
    val color = style.borderColor ?: return
    val sides = style.sides.map { it * density }
    val dash = style.borderStyle
    val uniform = sides.all { it == sides[0] }
    if (uniform) {
        val w = sides[0]
        if (w <= 0f) return
        val rr = boxRoundRect(style, sizeDp, size, density, inset = w / 2f)
        val path = Path().apply { addRoundRect(rr) }
        drawPath(path, color, style = Stroke(width = w, pathEffect = dashEffect(dash, w), cap = if (dash == "dotted") StrokeCap.Round else StrokeCap.Butt))
        return
    }
    // Per-side widths: clip to the rounded box, stroke each side's band.
    val outer = Path().apply { addRoundRect(boxRoundRect(style, sizeDp, size, density)) }
    clipPath(outer) {
        val (t, r, b, l) = sides
        fun side(x0: Float, y0: Float, x1: Float, y1: Float, w: Float) {
            if (w <= 0f) return
            drawLine(color, Offset(x0, y0), Offset(x1, y1), strokeWidth = w, pathEffect = dashEffect(dash, w), cap = if (dash == "dotted") StrokeCap.Round else StrokeCap.Butt)
        }
        side(0f, t / 2f, size.width, t / 2f, t)
        side(size.width - r / 2f, 0f, size.width - r / 2f, size.height, r)
        side(0f, size.height - b / 2f, size.width, size.height - b / 2f, b)
        side(l / 2f, 0f, l / 2f, size.height, l)
    }
}

private fun dashEffect(style: String?, width: Float): PathEffect? = when (style) {
    "dashed" -> PathEffect.dashPathEffect(floatArrayOf(max(3f * width, 3f), max(3f * width, 3f)))
    // A round cap on a zero-length dash = a dot.
    "dotted" -> PathEffect.dashPathEffect(floatArrayOf(0.01f, max(2f * width, 2f)))
    else -> null
}

/**
 * One CSS `box-shadow` layer: the box grown by `spread`, moved by
 * (`x`, `y`) and blurred with a Gaussian of sigma `blur / 2` (CSS), drawn
 * through a platform paint with a [BlurMaskFilter].
 */
private fun DrawScope.drawShadowLayer(layer: ShadowLayer, box: RoundRect) {
    val spread = layer.spread * density
    val left = layer.x * density - spread
    val top = layer.y * density - spread
    val right = size.width + layer.x * density + spread
    val bottom = size.height + layer.y * density + spread
    if (right <= left || bottom <= top) return
    val r = max(0f, max(max(box.topLeftCornerRadius.x, box.topRightCornerRadius.x), max(box.bottomLeftCornerRadius.x, box.bottomRightCornerRadius.x)) + spread)
    val blurPx = layer.blur * density
    if (blurPx <= 0f) {
        drawRoundRect(color = layer.color, topLeft = Offset(left, top), size = Size(right - left, bottom - top), cornerRadius = CornerRadius(r, r))
        return
    }
    drawIntoCanvas { canvas ->
        val paint = Paint()
        val frameworkPaint = paint.asFrameworkPaint()
        frameworkPaint.isAntiAlias = true
        frameworkPaint.color = layer.color.toArgb()
        // Skia's blur radius → sigma = 0.57735·r + 0.5; CSS sigma = blur / 2.
        val sigma = blurPx / 2f
        val skRadius = max(0.5f, (sigma - 0.5f) / 0.57735f)
        frameworkPaint.maskFilter = BlurMaskFilter(skRadius, BlurMaskFilter.Blur.NORMAL)
        canvas.drawRoundRect(left, top, right, bottom, r, r, paint)
    }
}

/** Colour with [alpha] multiplied in. */
internal fun Color.times(alpha: Float): Color = copy(alpha = this.alpha * alpha)
