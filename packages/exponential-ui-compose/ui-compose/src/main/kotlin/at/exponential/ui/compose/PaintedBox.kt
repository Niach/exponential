package at.exponential.ui.compose

import android.graphics.BlurMaskFilter
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.toArgb
import at.exponential.ui.theme.PaintStyle
import at.exponential.ui.theme.ShadowLayer
import kotlin.math.max
import kotlin.math.min

/**
 * A box painted from a resolved [PaintStyle] at a frame of [size] (dp):
 * the shadows behind (blurred rounded rects, spread and offset honoured),
 * the background as a rounded rect (radius clamped to half the shorter
 * side), the border drawn INSIDE the box over the content (the stroke inset
 * by half its width, so children, whose frames already include the border,
 * are never offset) and the opacity of the whole node. The SwiftUI
 * painter's `paintedBox`.
 */
fun Modifier.paintedBox(style: PaintStyle, size: Size): Modifier {
    val radiusDp = min(style.radius, min(size.width, size.height) / 2f).coerceAtLeast(0f)
    var m: Modifier = this
    val opacity = style.opacity
    if (opacity != null && opacity < 1f) {
        // Modulate instead of an offscreen layer: the shadows outside the
        // bounds stay visible (an offscreen buffer is clipped to the box).
        m = m.graphicsLayer {
            alpha = opacity.coerceIn(0f, 1f)
            compositingStrategy = CompositingStrategy.ModulateAlpha
        }
    }
    if (style.shadows.isNotEmpty() || style.background != null) {
        m = m.drawBehind {
            val r = radiusDp * density
            for (layer in style.shadows) drawShadowLayer(layer, r)
            style.background?.let { bg ->
                drawRoundRect(color = bg, cornerRadius = CornerRadius(r, r))
            }
        }
    }
    if (style.borderWidth > 0f && style.borderColor != null) {
        val borderColor = style.borderColor!!
        m = m.drawWithContent {
            drawContent()
            val w = style.borderWidth * density
            val half = w / 2f
            val r = max(0f, radiusDp * density - half)
            drawRoundRect(
                color = borderColor,
                topLeft = Offset(half, half),
                size = Size(max(0f, this.size.width - w), max(0f, this.size.height - w)),
                cornerRadius = CornerRadius(r, r),
                style = Stroke(width = w),
            )
        }
    }
    return m
}

/**
 * One CSS `box-shadow` layer: the box grown by `spread`, moved by
 * (`x`, `y`) and blurred with a Gaussian of sigma `blur / 2` (CSS), drawn
 * through a platform paint with a [BlurMaskFilter].
 */
private fun DrawScope.drawShadowLayer(layer: ShadowLayer, radiusPx: Float) {
    val spread = layer.spread * density
    val left = layer.x * density - spread
    val top = layer.y * density - spread
    val right = size.width + layer.x * density + spread
    val bottom = size.height + layer.y * density + spread
    if (right <= left || bottom <= top) return
    val r = max(0f, radiusPx + spread)
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
