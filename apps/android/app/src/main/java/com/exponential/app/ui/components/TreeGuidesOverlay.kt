package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.TreeGuide
import com.exponential.app.domain.TreeGuides
import com.exponential.app.ui.theme.GlassTokens

// EXP-965: ONE drawing of the nested-list connector, applied by every list
// that nests (the Agent page's Running band and its Recent sheet, the stack
// overlay's Runs and Pull requests, the Reviews stack members). The SHAPE is
// pure (`domain/TreeGuides.kt`, mirrored ×4); this file only paints it.

/** The connector's stroke — the shared hairline, never a literal colour. */
private val GuideWidth = 1.dp

/** The elbow's turn radius — the ×4 number. */
private val GuideRadius = 5.dp

/**
 * Draw [guide] in the gutter bands to the LEFT of this row's content. Apply it
 * BEFORE the indent padding, so the draw area still spans the gutters the
 * lines live in ([TreeGuidesRow] does exactly that).
 */
fun Modifier.treeGuides(guide: TreeGuide?): Modifier {
    if (guide == null || guide.isEmpty) return this
    return drawBehind {
        val indent = TreeGuides.INDENT_DP.dp.toPx()
        val stroke = Stroke(width = GuideWidth.toPx())
        val radius = GuideRadius.toPx()
        val midY = size.height / 2f
        fun centreOf(level: Int) = indent * level + indent / 2f

        // An ancestor whose subtree carries on: a straight full-height line.
        guide.passThrough.forEach { level ->
            val path = Path().apply {
                moveTo(centreOf(level), 0f)
                lineTo(centreOf(level), size.height)
            }
            drawPath(path, GlassTokens.StrokeStrong, style = stroke)
        }

        val elbow = guide.elbowAt ?: return@drawBehind
        val x = centreOf(elbow)
        val right = indent * (elbow + 1)
        val path = Path().apply {
            if (guide.tee) {
                // A later sibling follows: the vertical runs the whole height
                // and the stub branches off it square.
                moveTo(x, 0f)
                lineTo(x, size.height)
                moveTo(x, midY)
                lineTo(right, midY)
            } else {
                moveTo(x, 0f)
                lineTo(x, midY - radius)
                arcTo(
                    rect = Rect(x, midY - 2f * radius, x + 2f * radius, midY),
                    startAngleDegrees = 180f,
                    sweepAngleDegrees = -90f,
                    forceMoveTo = false,
                )
                lineTo(right, midY)
            }
        }
        drawPath(path, GlassTokens.StrokeStrong, style = stroke)
    }
}

/**
 * One nested row: [TreeGuides.INDENT_DP] of indent per level with [guide]
 * drawn in the gutters it leaves behind. Every nesting list wraps its row in
 * this instead of a bare indent padding.
 */
@Composable
fun TreeGuidesRow(
    depth: Int,
    guide: TreeGuide?,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .treeGuides(guide)
            .padding(start = (TreeGuides.INDENT_DP * depth).dp),
    ) {
        content()
    }
}
