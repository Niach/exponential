package com.exponential.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.vector.VectorPainter
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens

// THE square checkbox (owner rule: every check / selection indicator on every
// platform is this one look). Reference = web CHECKBOX_CLASSES
// (packages/exponential-ui/src/web/primitives/checkbox.ts), desktop `checkbox`
// (exponential-ui-gpui controls/inputs.rs); geometry = control-geometry.json
// Checkbox: 16×16, 1px border, 4px corner. Unchecked = the glass ROW fill
// under the glass STROKE-STRONG hairline; checked / indeterminate = primary
// fill + border with `ui-check` / `ui-minus` at 14 in primary-foreground;
// disabled = 50% alpha. The composable and the draw-phase painter (the
// editor's task-list margin) share [drawExpCheckbox], so both paint the same
// pixels.

enum class ExpCheckState { Unchecked, Checked, Indeterminate }

object ExpCheckboxDefaults {
    val Size: Dp = 16.dp
    val Corner: Dp = 4.dp
    val Border: Dp = 1.dp
    val Glyph: Dp = 14.dp
    const val DisabledAlpha: Float = 0.5f
}

/** The colours + glyph painters [drawExpCheckbox] paints with. */
@Immutable
class ExpCheckboxPaint(
    val fill: Color,
    val stroke: Color,
    val primary: Color,
    val onPrimary: Color,
    val check: VectorPainter,
    val minus: VectorPainter,
)

@Composable
fun rememberExpCheckboxPaint(): ExpCheckboxPaint = ExpCheckboxPaint(
    fill = GlassTokens.RowFill,
    stroke = GlassTokens.StrokeStrong,
    primary = MaterialTheme.colorScheme.primary,
    onPrimary = MaterialTheme.colorScheme.onPrimary,
    check = rememberVectorPainter(ExpIcons.uiCheck),
    minus = rememberVectorPainter(ExpIcons.uiMinus),
)

/** Paints THE checkbox with its top-left corner at [topLeft]. */
fun DrawScope.drawExpCheckbox(
    topLeft: Offset,
    state: ExpCheckState,
    paint: ExpCheckboxPaint,
    enabled: Boolean = true,
) {
    val box = ExpCheckboxDefaults.Size.toPx()
    val corner = ExpCheckboxDefaults.Corner.toPx()
    val border = ExpCheckboxDefaults.Border.toPx()
    val alpha = if (enabled) 1f else ExpCheckboxDefaults.DisabledAlpha
    if (state == ExpCheckState.Unchecked) {
        drawRoundRect(paint.fill, topLeft, Size(box, box), CornerRadius(corner, corner), alpha = alpha)
        val half = border / 2f
        drawRoundRect(
            paint.stroke,
            Offset(topLeft.x + half, topLeft.y + half),
            Size(box - border, box - border),
            CornerRadius(corner - half, corner - half),
            style = Stroke(width = border),
            alpha = alpha,
        )
        return
    }
    // Checked: fill and border are both primary, so one fill paints both.
    drawRoundRect(paint.primary, topLeft, Size(box, box), CornerRadius(corner, corner), alpha = alpha)
    val glyph = ExpCheckboxDefaults.Glyph.toPx()
    val inset = (box - glyph) / 2f
    val painter = if (state == ExpCheckState.Indeterminate) paint.minus else paint.check
    translate(topLeft.x + inset, topLeft.y + inset) {
        with(painter) { draw(Size(glyph, glyph), alpha = alpha, colorFilter = ColorFilter.tint(paint.onPrimary)) }
    }
}

/** THE square checkbox, 16dp. Taps belong to the caller's row. */
@Composable
fun ExpCheckbox(
    state: ExpCheckState,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    contentDescription: String? = null,
) {
    val paint = rememberExpCheckboxPaint()
    Canvas(
        modifier
            .size(ExpCheckboxDefaults.Size)
            .then(if (contentDescription != null) Modifier.semantics { this.contentDescription = contentDescription } else Modifier),
    ) {
        drawExpCheckbox(Offset.Zero, state, paint, enabled)
    }
}

@Composable
fun ExpCheckbox(
    checked: Boolean,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    contentDescription: String? = null,
) = ExpCheckbox(
    if (checked) ExpCheckState.Checked else ExpCheckState.Unchecked,
    modifier,
    enabled,
    contentDescription,
)
