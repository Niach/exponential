package at.exponential.ui.primitives

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicText
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * A disclosure header (the catalog `Collapsible` / `Accordion` triggers;
 * the app's tool-group rows): a leading chevron that turns 0 → 90 degrees
 * as it [expanded], an optional [leading] glyph, the [title], an optional
 * [count] and a [trailing] slot; the whole row toggles via [onToggle].
 * [chevron] replaces the drawn chevron (a host glyph pointing RIGHT; it is
 * rotated the same way) and carries [expandLabel]/[collapseLabel].
 * [height] = `Dp.Unspecified` hugs the content.
 */
@Composable
fun DisclosureHeader(
    title: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
    count: Int? = null,
    titleStyle: TextStyle = TextStyle(fontSize = 14.sp, fontWeight = FontWeight.Medium),
    titleColor: Color = LocalPrimitiveTokens.current.foreground,
    maxLines: Int = 1,
    chevronSize: Dp = 12.dp,
    chevronColor: Color = LocalPrimitiveTokens.current.mutedForeground,
    spacing: Dp = 8.dp,
    height: Dp = LocalPrimitiveTokens.current.rowHeight,
    expandLabel: String = "Expand",
    collapseLabel: String = "Collapse",
    animationMillis: Int = 150,
    chevron: (@Composable () -> Unit)? = null,
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val turn by animateFloatAsState(if (expanded) 90f else 0f, tween(animationMillis), label = "chevron")
    val label = if (expanded) collapseLabel else expandLabel
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onToggle)
            .then(if (height != Dp.Unspecified) Modifier.height(height) else Modifier),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(spacing),
    ) {
        CompositionLocalProvider(LocalContentColor provides chevronColor) {
            Box(
                Modifier
                    .size(chevronSize)
                    .rotate(turn)
                    .semantics { contentDescription = label },
                contentAlignment = Alignment.Center,
            ) {
                if (chevron != null) chevron() else DrawnChevron(chevronColor)
            }
            leading?.invoke()
        }
        BasicText(
            title,
            style = titleStyle.merge(TextStyle(color = titleColor)),
            maxLines = maxLines,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        if (count != null) {
            BasicText(
                count.toString(),
                style = TextStyle(fontSize = 12.sp, color = LocalPrimitiveTokens.current.mutedForeground),
            )
        }
        trailing?.invoke()
    }
}

/** A right-pointing chevron stroked in [color], filling its box. */
@Composable
private fun DrawnChevron(color: Color) {
    Canvas(Modifier.fillMaxSize()) {
        val w = size.width
        val h = size.height
        val path = Path().apply {
            moveTo(w * 0.375f, h * 0.25f)
            lineTo(w * 0.625f, h * 0.5f)
            lineTo(w * 0.375f, h * 0.75f)
        }
        drawPath(path, color, style = Stroke(width = w / 8f, cap = StrokeCap.Round, join = StrokeJoin.Round))
    }
}
