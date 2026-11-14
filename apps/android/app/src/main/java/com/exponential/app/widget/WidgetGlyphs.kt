package com.exponential.app.widget

import android.graphics.Bitmap
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.VectorGroup
import androidx.compose.ui.graphics.vector.VectorPath
import androidx.compose.ui.graphics.vector.toPath
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection

/**
 * #EXP-1830: a registry glyph (ExpIcons) as a tinted bitmap: RemoteViews take
 * bitmaps, never an ImageVector (the same drawing DirectShareShortcuts uses
 * for an agent's shortcut icon). Cached per (glyph, tint, size).
 */
internal object WidgetGlyphs {
    private val cache = HashMap<Triple<ImageVector, Color, Int>, Bitmap>()

    @Synchronized
    fun bitmap(vector: ImageVector, tint: Color, px: Int): Bitmap = cache.getOrPut(Triple(vector, tint, px)) {
        val bitmap = Bitmap.createBitmap(px, px, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap.asImageBitmap())
        CanvasDrawScope().draw(Density(1f), LayoutDirection.Ltr, canvas, Size(px.toFloat(), px.toFloat())) {
            val factor = px / vector.viewportWidth
            scale(factor, factor, pivot = Offset.Zero) { drawGroup(vector.root, tint) }
        }
        bitmap
    }

    private fun DrawScope.drawGroup(group: VectorGroup, tint: Color) {
        for (node in group) {
            when (node) {
                is VectorGroup -> drawGroup(node, tint)
                is VectorPath -> {
                    val path = node.pathData.toPath()
                    if (node.fill != null) drawPath(path, SolidColor(tint))
                    if (node.stroke != null) {
                        drawPath(path, SolidColor(tint), style = Stroke(node.strokeLineWidth, cap = node.strokeLineCap, join = node.strokeLineJoin))
                    }
                }
            }
        }
    }
}
