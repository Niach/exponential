package com.exponential.app.ui.markdown

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs

/**
 * The PAINTED per-paragraph decorations of one multi-line text run (EXP-534):
 * continuous quote bars, merged code-block backgrounds, and list glyphs
 * (bullet / number / checkbox) drawn into the indent margin — everything the
 * one-field-per-line editor previously laid out as sibling composables, now
 * derived from the field's [TextLayoutResult] exactly like the chip pills
 * ([drawIssueRefChips]). The text itself is inset via each line's
 * `ParagraphStyle.textIndent` ([ChipVisualTransformation]), so the margin the
 * glyphs occupy is empty text space in the SAME coordinate box as the layout.
 */
@Immutable
internal data class ParaBand(
    val attrs: ParagraphAttrs,
    /** Display offset of the line's first character. */
    val displayStart: Int,
    /** Display offset of the line's visual end (its '\n' / the text end). */
    val displayEnd: Int,
)

/** One band per source line, in the chip transform's DISPLAY coordinates. */
internal fun paraBands(
    source: String,
    paragraphs: List<ParagraphAttrs>,
    transform: IssueChipTransform,
): List<ParaBand> {
    val lines = if (source.isEmpty()) listOf("") else source.split("\n")
    val out = ArrayList<ParaBand>(lines.size)
    var start = 0
    for ((i, line) in lines.withIndex()) {
        val end = start + line.length
        val dStart = transform.originalToTransformed(start)
        val dEnd =
            if (i == lines.lastIndex) transform.display.length
            else (transform.originalToTransformed(end + 1) - 1).coerceAtLeast(dStart)
        out.add(ParaBand(paragraphs.getOrElse(i) { ParagraphAttrs.PLAIN }, dStart, dEnd))
        start = end + 1
    }
    return out
}

/**
 * Paint the run decorations behind the text. [layout] is read inside the draw
 * phase, so a fresh layout repaints without recomposing — and a layout that
 * lags the bands by a frame can only misplace a decoration for that frame,
 * never throw (offsets coerced, geometry guarded, exactly like `caretRect`).
 */
@Composable
internal fun Modifier.drawRunDecorations(
    bands: List<ParaBand>,
    layout: () -> TextLayoutResult?,
): Modifier {
    if (bands.none { it.attrs.kind != BlockKind.Paragraph }) return this
    val textMeasurer = rememberTextMeasurer()
    val density = LocalDensity.current
    val quoteBarWidth = with(density) { MdStyle.quoteBarWidth.toPx() }
    val indentBase = with(density) { MdStyle.listIndentBase.toPx() }
    val indentPerDepth = with(density) { MdStyle.listIndentPerDepth.toPx() }
    val corner = with(density) { 4.dp.toPx() }
    return this.drawBehind {
        val result = layout() ?: return@drawBehind
        val length = result.layoutInput.text.length
        fun lineOf(offset: Int) = result.getLineForOffset(offset.coerceIn(0, length))
        fun topOf(band: ParaBand) = result.getLineTop(lineOf(band.displayStart))
        fun bottomOf(band: ParaBand) = result.getLineBottom(lineOf(band.displayEnd))

        // Quote bars and code backgrounds cover MAXIMAL runs of same-kind
        // paragraphs so an N-line fence / quote reads as one connected block
        // (EXP-246 parity, now trivial — no sibling-row corner joining).
        var i = 0
        while (i < bands.size) {
            val kind = bands[i].attrs.kind
            if (kind != BlockKind.CodeBlock && kind != BlockKind.Blockquote) {
                i++
                continue
            }
            var j = i
            while (j + 1 < bands.size && bands[j + 1].attrs.kind == kind) j++
            runCatching {
                val top = topOf(bands[i])
                val bottom = bottomOf(bands[j])
                if (bottom > top) {
                    if (kind == BlockKind.CodeBlock) {
                        drawRoundRect(
                            color = MdStyle.CodeBlockBg,
                            topLeft = Offset(0f, top),
                            size = Size(size.width, bottom - top),
                            cornerRadius = CornerRadius(corner, corner),
                        )
                    } else {
                        drawRect(
                            color = MdStyle.QuoteBar,
                            topLeft = Offset(0f, top),
                            size = Size(quoteBarWidth, bottom - top),
                        )
                    }
                }
            }
            i = j + 1
        }

        for (band in bands) {
            val a = band.attrs
            if (a.kind != BlockKind.ListItem) continue
            val glyph = when (a.listType) {
                ListType.Checklist -> if (a.checked) "☑" else "☐"
                ListType.Ordered -> "${a.orderedIndex}."
                ListType.Bullet, null -> "•"
            }
            runCatching {
                val line = lineOf(band.displayStart)
                val top = result.getLineTop(line)
                val bottom = result.getLineBottom(line)
                val measured = textMeasurer.measure(glyph, MdStyle.body)
                drawText(
                    measured,
                    topLeft = Offset(
                        indentBase + indentPerDepth * a.listDepth.coerceAtLeast(0),
                        top + (bottom - top - measured.size.height) / 2f,
                    ),
                )
            }
        }
    }
}

/**
 * Checkbox tap targets in the glyph margin. Uses the INITIAL pass — the same
 * interception pattern as the chip-tap handler — because the field's own tap
 * handling lives on a descendant node; a consumed down+up toggles the item
 * instead of placing the caret. The margin sits left of the text indent, so
 * every other tap falls through untouched.
 */
@Composable
internal fun Modifier.checklistTapTargets(
    bands: List<ParaBand>,
    layout: () -> TextLayoutResult?,
    onToggle: (paraIndex: Int) -> Unit,
): Modifier {
    if (bands.none { it.attrs.listType == ListType.Checklist }) return this
    val density = LocalDensity.current
    val glyphWidth = with(density) { MdStyle.listGlyphWidth.toPx() }
    val indentBase = with(density) { MdStyle.listIndentBase.toPx() }
    val indentPerDepth = with(density) { MdStyle.listIndentPerDepth.toPx() }
    return this.pointerInput(bands) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            val result = layout() ?: return@awaitEachGesture
            val length = result.layoutInput.text.length
            val index = bands.indexOfFirst { band ->
                val a = band.attrs
                if (a.kind != BlockKind.ListItem || a.listType != ListType.Checklist) {
                    return@indexOfFirst false
                }
                val line = runCatching {
                    result.getLineForOffset(band.displayStart.coerceIn(0, length))
                }.getOrNull() ?: return@indexOfFirst false
                val x0 = indentBase + indentPerDepth * a.listDepth.coerceAtLeast(0)
                down.position.x >= x0 && down.position.x <= x0 + glyphWidth &&
                    down.position.y >= result.getLineTop(line) &&
                    down.position.y <= result.getLineBottom(line)
            }
            if (index < 0) return@awaitEachGesture
            down.consume()
            val up = waitForUpOrCancellation(PointerEventPass.Initial) ?: return@awaitEachGesture
            up.consume()
            onToggle(index)
        }
    }
}
