package com.exponential.app.ui.markdown

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.key
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons

/**
 * The PAINTED half of a resolved `#IDENTIFIER` chip (EXP-423, Linear parity):
 * the rounded fill + hairline behind the token, and the status pie-clock glyph
 * inside the `#` cell. Neither is expressible as a `SpanStyle`, so both are
 * drawn from the [TextLayoutResult] the hosting `Text` / `BasicTextField`
 * reports — which is why every chip-hosting text hands its layout to
 * [drawIssueRefChips].
 *
 * The `#` itself is hidden by a TRANSPARENT span rather than substituted away:
 * that keeps the read renderer's offset map and — far more importantly — the
 * editor's "pure suffix" chip transform untouched, so the stored markdown can
 * never be affected. Web and the desktop editor deliberately diverge and keep
 * their `#` visible next to the icon (edit affordance + offset-map safety).
 */
@Immutable
internal data class IssueRefChipSpec(
    /** Display-coordinate range of the whole chip (token + injected title). */
    val start: Int,
    val end: Int,
    /** Display offset of the `#`, whose glyph cell holds the status icon. */
    val tokenStart: Int,
    /** Registry glyph name; null ⇒ no icon, and the `#` stays visible. */
    val iconName: String? = null,
    val color: Color? = null,
)

/**
 * Paint [chips] behind the text this modifier is applied to. [layout] is read
 * inside the draw phase, so a fresh layout repaints without recomposing — and a
 * layout that lags the text by a frame can only skip a chip, never throw (the
 * offsets are coerced and the geometry guarded, exactly like `caretRect`).
 */
@Composable
internal fun Modifier.drawIssueRefChips(
    chips: List<IssueRefChipSpec>,
    layout: () -> TextLayoutResult?,
): Modifier {
    if (chips.isEmpty()) return this
    val painters = HashMap<String, VectorPainter>()
    for (name in chips.mapNotNull { it.iconName }.distinct().sorted()) {
        // Keyed by name so a changed icon set can't shift one chip's remembered
        // painter onto another's glyph.
        painters[name] = key(name) {
            rememberVectorPainter(ExpIcons.byName(name) ?: ExpIcons.statusBacklog)
        }
    }
    val density = LocalDensity.current
    val radius = with(density) { MdStyle.chipCornerRadius.toPx() }
    val padX = with(density) { MdStyle.chipPadX.toPx() }
    val iconSide = with(density) { MdStyle.chipIconSize.toPx() }
    val hairline = with(density) { 1.dp.toPx() }
    return this.drawBehind {
        val result = layout() ?: return@drawBehind
        for (chip in chips) {
            runCatching {
                drawChip(chip, result, radius, padX, hairline)
                val painter = painters[chip.iconName]
                if (painter != null) drawChipIcon(chip, result, painter, iconSide)
            }
        }
    }
}

/** One rounded pill per WRAPPED line, so a chip that breaks gets two pills. */
private fun DrawScope.drawChip(
    chip: IssueRefChipSpec,
    result: TextLayoutResult,
    radius: Float,
    padX: Float,
    hairline: Float,
) {
    val length = result.layoutInput.text.length
    val start = chip.start.coerceIn(0, length)
    val end = chip.end.coerceIn(start, length)
    if (end <= start) return
    for (line in result.getLineForOffset(start)..result.getLineForOffset(end - 1)) {
        val segStart = maxOf(start, result.getLineStart(line))
        val segEnd = minOf(end, result.getLineEnd(line, visibleEnd = true))
        if (segEnd <= segStart) continue
        val first = result.getBoundingBox(segStart)
        val last = result.getBoundingBox(segEnd - 1)
        val lineTop = result.getLineTop(line)
        val lineBottom = result.getLineBottom(line)
        // The line box carries the paragraph's leading; inset proportionally so
        // the pill hugs the text at any text size / heading level.
        val inset = (lineBottom - lineTop) * 0.08f
        val left = minOf(first.left, last.left) - padX
        val right = maxOf(first.right, last.right) + padX
        val top = lineTop + inset
        val bottom = lineBottom - inset
        if (right <= left || bottom <= top) continue
        val topLeft = Offset(left, top)
        val size = Size(right - left, bottom - top)
        val corner = CornerRadius(radius, radius)
        drawRoundRect(color = MdStyle.IssueRefBg, topLeft = topLeft, size = size, cornerRadius = corner)
        drawRoundRect(
            color = MdStyle.IssueRefBorder,
            topLeft = topLeft,
            size = size,
            cornerRadius = corner,
            style = Stroke(width = hairline),
        )
    }
}

/** The status glyph, centered in the (transparent) `#` cell. */
private fun DrawScope.drawChipIcon(
    chip: IssueRefChipSpec,
    result: TextLayoutResult,
    painter: VectorPainter,
    side: Float,
) {
    val length = result.layoutInput.text.length
    if (length == 0) return
    val cell = result.getBoundingBox(chip.tokenStart.coerceIn(0, length - 1))
    translate(left = cell.center.x - side / 2, top = cell.center.y - side / 2) {
        with(painter) {
            draw(size = Size(side, side), colorFilter = chip.color?.let { ColorFilter.tint(it) })
        }
    }
}
