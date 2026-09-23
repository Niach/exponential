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
import androidx.compose.ui.unit.Dp
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

/** The elbow's turn radius — a tight one (EXP-998: 5 read as a bulge on a
 *  28dp row), the ×4 number. */
private val GuideRadius = 3.dp

/**
 * Draw [guide] in the gutter bands to the LEFT of this row's content. Apply it
 * BEFORE the indent padding, so the draw area still spans the gutters the
 * lines live in ([TreeGuidesRow] does exactly that).
 *
 * [gap] is the LIST's row spacing: every vertical that starts at the row's top
 * edge starts that much ABOVE it (nothing clips the row, so the ink lands in
 * the list's own gap), while a pass-through or a tee still ends at the bottom
 * edge — the next row's extension is what covers the gap. Without it a spaced
 * list drew the branch as a dashed ladder. The ×4 rule; the pure shape
 * (`domain/TreeGuides.kt`) knows nothing about it.
 */
fun Modifier.treeGuides(guide: TreeGuide?, gap: Dp = 0.dp): Modifier {
    if (guide == null || guide.isEmpty) return this
    return drawBehind {
        val indent = TreeGuides.INDENT_DP.dp.toPx()
        val stroke = Stroke(width = GuideWidth.toPx())
        val radius = GuideRadius.toPx()
        val midY = size.height / 2f
        // The row above ends `gap` up there: start every downward line from it.
        val top = -gap.toPx()
        fun centreOf(level: Int) = indent * level + indent / 2f

        // An ancestor whose subtree carries on: a straight full-height line.
        guide.passThrough.forEach { level ->
            val path = Path().apply {
                moveTo(centreOf(level), top)
                lineTo(centreOf(level), size.height)
            }
            drawPath(path, GlassTokens.StrokeStrong, style = stroke)
        }

        val elbow = guide.elbowAt ?: return@drawBehind
        val x = centreOf(elbow)
        val right = indent * (elbow + 1)
        val path = Path().apply {
            if (guide.tee) {
                // A later sibling follows: the vertical runs the whole height,
                // unbroken, and the corner below branches off it (EXP-998:
                // the same rounded branch as the last child, ×4).
                moveTo(x, top)
                lineTo(x, size.height)
                moveTo(x, midY - radius)
            } else {
                moveTo(x, top)
                lineTo(x, midY - radius)
            }
            arcTo(
                rect = Rect(x, midY - 2f * radius, x + 2f * radius, midY),
                startAngleDegrees = 180f,
                sweepAngleDegrees = -90f,
                forceMoveTo = false,
            )
            lineTo(right, midY)
        }
        drawPath(path, GlassTokens.StrokeStrong, style = stroke)
    }
}

/**
 * One nested row: [TreeGuides.INDENT_DP] of indent per level with [guide]
 * drawn in the gutters it leaves behind. Every nesting list wraps its row in
 * this instead of a bare indent padding. [gap] = the list's row spacing, so
 * the branch bridges it.
 */
@Composable
fun TreeGuidesRow(
    depth: Int,
    guide: TreeGuide?,
    modifier: Modifier = Modifier,
    gap: Dp = 0.dp,
    content: @Composable () -> Unit,
) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .treeGuides(guide, gap)
            .padding(start = (TreeGuides.INDENT_DP * depth).dp),
    ) {
        content()
    }
}
