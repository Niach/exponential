package com.exponential.app.ui.issue

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.Diff
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

// EXP-895 — the patch BODY of the one diff view ×4 (web
// `packages/ui/src/file-diff-card.tsx`, desktop `crates/ui/src/diff`, iOS
// `DiffRendering.swift`). It renders a parsed [Diff.Hunk] list and nothing
// else: no raw patch strings, no per-line sign sniffing — the ONE parser
// (`domain/Diff.kt`) already decided what every line is.
//
// Unified layout only, four columns: old gutter · new gutter · sign · text.
// `compact` (a transcript card) drops the OLD gutter — the column a 320dp-wide
// tool row can least afford — and keeps everything else identical.
//
// Every colour comes from `DesignTokens.Diff.*` (generated from
// `packages/design-tokens/tokens.json`), never from a hand-picked emerald.

/**
 * What a file with no hunks says instead of rows — a binary blob, a pure
 * rename, an empty new file and a patch GitHub refused to send all land here,
 * and the reader has to be told WHICH. Byte-identical to web `noHunksNote`.
 */
fun noHunksNote(file: Diff.File): String = when {
    file.binary -> "Binary file"
    file.status == Diff.Status.ADDED -> "Empty file added"
    file.status == Diff.Status.REMOVED -> "File removed"
    file.status == Diff.Status.RENAMED -> "Renamed without content changes"
    file.status == Diff.Status.COPIED -> "Copied without content changes"
    else -> "No textual diff (binary or too large)"
}

/**
 * Rendered-line cap per file — every line composes a Text under
 * IntrinsicSize.Max intrinsic measurement, so an uncapped multi-thousand-line
 * patch (lockfile PRs, raw worktree diffs) freezes the frame. The web expands
 * in `LINE_CHUNK` steps instead; a phone card simply stops and says so.
 */
const val DIFF_MAX_RENDERED_LINES = 600

/**
 * Rows per fade band. The trailing fade is an alpha MASK, so the masked pixels
 * have to live in an offscreen layer — and a layer spanning a whole 600-line
 * patch is viewport-wide by ~23k px tall, past the GPU max texture size, where
 * HWUI silently drops the layer and the DstIn rect punches a hole through the
 * window instead. Banding the rows keeps every layer a few hundred px tall.
 */
private const val DIFF_FADE_BAND_ROWS = 40

/** One display row of a file's patch (web `Row`, same three kinds). */
sealed interface DiffRow {
    /** A `N unchanged lines` divider — context the patch never carried. */
    data class Gap(val text: String) : DiffRow

    /** The verbatim `@@ … @@` header, section heading and all. */
    data class Header(val text: String) : DiffRow

    data class Body(val line: Diff.Line) : DiffRow
}

/** [buildDiffRows]' answer: the rows to draw, and the lines that did not fit. */
data class DiffRows(val rows: List<DiffRow>, val hidden: Int)

/**
 * A file's hunks flattened into display rows: a gap divider before each hunk
 * that skipped context (including the file's own head, `unchangedBefore`), the
 * `@@` header, then the hunk's lines — stopping dead at [maxLines] BODY rows,
 * exactly where the web's reveal cap stops.
 *
 * Pure, so `DiffRowsTest` can hold the shape without a device.
 */
fun buildDiffRows(hunks: List<Diff.Hunk>, maxLines: Int = DIFF_MAX_RENDERED_LINES): DiffRows {
    var total = 0
    for (hunk in hunks) total += hunk.lines.size
    val out = mutableListOf<DiffRow>()
    var shown = 0
    outer@ for ((index, hunk) in hunks.withIndex()) {
        val skipped = if (index == 0) {
            Diff.unchangedBefore(hunk)
        } else {
            Diff.unchangedBetween(hunks[index - 1], hunk)
        }
        // A PLAIN row, never a button: the skipped context is not on the wire,
        // so there is nothing to expand to (EXP-895).
        if (skipped > 0) out.add(DiffRow.Gap(Diff.unchangedLabel(skipped)))
        out.add(DiffRow.Header(hunk.header))
        for (line in hunk.lines) {
            if (shown >= maxLines) break@outer
            out.add(DiffRow.Body(line))
            shown += 1
        }
    }
    return DiffRows(out, maxOf(0, total - shown))
}

private val DiffFontSize = 11.sp
private val DiffLineHeight = 15.sp

/** The two number columns; the compact card keeps only the new one. */
private val GutterWidth: Dp = 30.dp
private val CompactGutterWidth: Dp = 26.dp

/** The sign column — one glyph wide, `+` / U+2212 / nothing. */
private val SignWidth: Dp = 12.dp

/**
 * The monospace patch body of ONE file: `@@` headers on the hunk band,
 * `N unchanged lines` dividers between hunks, and the lines themselves on
 * their add/del washes. Horizontal scrolling lives INSIDE this block — never
 * on the page — and the trailing edge fades while there is more to the right.
 */
@Composable
fun PatchLines(
    hunks: List<Diff.Hunk>,
    modifier: Modifier = Modifier,
    /** A transcript card: drops the old-side gutter, keeps the rest. */
    compact: Boolean = false,
    maxLines: Int = DIFF_MAX_RENDERED_LINES,
) {
    val built = remember(hunks, maxLines) { buildDiffRows(hunks, maxLines) }
    val scrollState = rememberScrollState()
    val fadeStrength = trailingFadeStrength(scrollState)
    val contextColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val gutter = if (compact) CompactGutterWidth else GutterWidth
    Column(modifier = modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(scrollState),
        ) {
            SelectionContainer {
                Column(modifier = Modifier.width(IntrinsicSize.Max)) {
                    built.rows.chunked(DIFF_FADE_BAND_ROWS).forEach { band ->
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .trailingScrollFade(scrollState, fadeStrength),
                        ) {
                            band.forEach { row ->
                                PatchRow(
                                    row = row,
                                    compact = compact,
                                    gutter = gutter,
                                    contextColor = contextColor,
                                )
                            }
                        }
                    }
                }
            }
        }
        if (built.hidden > 0) {
            Text(
                text = "Diff truncated. Showing the first $maxLines lines.",
                color = contextColor,
                fontSize = DiffFontSize,
                lineHeight = DiffLineHeight,
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            )
        }
    }
}

@Composable
private fun PatchRow(row: DiffRow, compact: Boolean, gutter: Dp, contextColor: Color) {
    when (row) {
        // The divider sits on a WEAKER wash than the `@@` header above it — it
        // is the absence of a hunk, not one.
        is DiffRow.Gap -> Text(
            text = row.text,
            color = DesignTokens.Diff.GutterFg,
            fontFamily = FontFamily.Monospace,
            fontSize = DiffFontSize,
            lineHeight = DiffLineHeight,
            maxLines = 1,
            softWrap = false,
            textAlign = TextAlign.Center,
            modifier = Modifier
                .fillMaxWidth()
                .background(GapFill)
                .padding(horizontal = 10.dp),
        )
        is DiffRow.Header -> Text(
            text = row.text,
            color = DesignTokens.Diff.HunkFg,
            fontFamily = FontFamily.Monospace,
            fontSize = DiffFontSize,
            lineHeight = DiffLineHeight,
            maxLines = 1,
            softWrap = false,
            modifier = Modifier
                .fillMaxWidth()
                .background(DesignTokens.Diff.HunkBg)
                .padding(horizontal = 10.dp),
        )
        is DiffRow.Body -> PatchBodyRow(row.line, compact, gutter, contextColor)
    }
}

/** `bg-diff-hunk-bg/60` — the gap divider's share of the hunk band. */
private val GapFill: Color =
    DesignTokens.Diff.HunkBg.copy(alpha = DesignTokens.Diff.HunkBg.alpha * 0.6f)

@Composable
private fun PatchBodyRow(line: Diff.Line, compact: Boolean, gutter: Dp, contextColor: Color) {
    // `\ No newline at end of file` — numbered on NEITHER side, so it gets no
    // gutters at all and leans italic to read as metadata rather than content.
    if (line.kind == Diff.LineKind.META) {
        Text(
            text = line.text.ifEmpty { " " },
            color = DesignTokens.Diff.GutterFg,
            fontFamily = FontFamily.Monospace,
            fontStyle = FontStyle.Italic,
            fontSize = DiffFontSize,
            lineHeight = DiffLineHeight,
            maxLines = 1,
            softWrap = false,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 10.dp),
        )
        return
    }
    val fill = when (line.kind) {
        Diff.LineKind.ADD -> DesignTokens.Diff.AddBg
        Diff.LineKind.DEL -> DesignTokens.Diff.DelBg
        else -> Color.Transparent
    }
    val ink = when (line.kind) {
        Diff.LineKind.ADD -> DesignTokens.Diff.AddFg
        Diff.LineKind.DEL -> DesignTokens.Diff.DelFg
        else -> contextColor
    }
    Row(modifier = Modifier.fillMaxWidth().background(fill)) {
        if (!compact) Gutter(line.oldNo, gutter)
        Gutter(line.newNo, gutter)
        Text(
            // U+2212 MINUS SIGN, never a hyphen — the same glyph the counts
            // use, locked ×4.
            text = when (line.kind) {
                Diff.LineKind.ADD -> "+"
                Diff.LineKind.DEL -> "−"
                else -> " "
            },
            color = ink,
            fontFamily = FontFamily.Monospace,
            fontSize = DiffFontSize,
            lineHeight = DiffLineHeight,
            maxLines = 1,
            softWrap = false,
            textAlign = TextAlign.Center,
            modifier = Modifier.width(SignWidth),
        )
        Text(
            text = line.text.ifEmpty { " " },
            color = ink,
            fontFamily = FontFamily.Monospace,
            fontSize = DiffFontSize,
            lineHeight = DiffLineHeight,
            maxLines = 1,
            softWrap = false,
            modifier = Modifier.padding(end = 10.dp),
        )
    }
}

/** One line-number column: right-aligned, dimmed, never selectable noise. */
@Composable
private fun Gutter(number: Int?, width: Dp) {
    Text(
        text = number?.toString().orEmpty(),
        color = DesignTokens.Diff.GutterFg,
        fontFamily = FontFamily.Monospace,
        fontSize = DiffFontSize,
        lineHeight = DiffLineHeight,
        maxLines = 1,
        softWrap = false,
        textAlign = TextAlign.End,
        modifier = Modifier.width(width).padding(end = 4.dp),
    )
}

/**
 * Wide enough to dissolve a few characters, narrow enough to leave the line
 * readable — the horizontal twin of the steer feed's top fade (iOS
 * `DiffPatchBlock.trailingFadeWidth` parity).
 */
private val TrailingFadeWidth: Dp = 36.dp

/**
 * EXP-722: a scroller with no scrollbar has NO affordance at rest — a line cut
 * flush at the block's edge reads as truncated, not as "more to the right". So
 * the trailing edge FADES while [state] can still scroll forward and turns
 * crisp once the reader has panned to the end. Reduce Motion snaps the edge
 * instead of dissolving it. Hoisted out of [trailingScrollFade] so one patch
 * runs ONE animation, not one per band.
 */
@Composable
private fun trailingFadeStrength(state: ScrollState): Float {
    val strength by animateFloatAsState(
        targetValue = if (state.canScrollForward) 1f else 0f,
        animationSpec = Motion.fast(),
        label = "diff-trailing-fade",
    )
    return strength
}

/**
 * Paints the trailing fade over one band of scrolled patch rows.
 *
 * It is an alpha MASK, not a painted gradient: the block sits on translucent
 * glass over the page gradient, so no single colour would match what is behind
 * it. A DstIn rect only masks the patch when the pixels it multiplies live in
 * an offscreen layer, so the fade strip — and ONLY the strip — is wrapped in a
 * `saveLayer`: fade-width by band-height, small enough to always be a real
 * layer, versus the viewport-by-whole-patch layer that a `graphicsLayer` on
 * the scroller would demand (and that HWUI drops past the max texture size,
 * leaving DstIn to cut a hole through the window behind the patch). Nothing is
 * layered at all while [strength] is 0, i.e. while the band does not overflow.
 *
 * This modifier sits INSIDE [horizontalScroll], so the strip is placed in
 * content coordinates: the viewport's trailing edge is `value + viewportSize`.
 */
private fun Modifier.trailingScrollFade(state: ScrollState, strength: Float): Modifier =
    this.drawWithContent {
        val content = this
        val fadeWidth = TrailingFadeWidth.toPx()
        val right = (state.value + state.viewportSize).toFloat()
        val left = right - fadeWidth
        if (strength <= 0f || state.viewportSize == 0 || left >= size.width || left < 0f) {
            content.drawContent()
            return@drawWithContent
        }
        // Everything left of the strip draws straight to the canvas, unlayered.
        clipRect(right = left) { content.drawContent() }
        val strip = Rect(left, 0f, right.coerceAtMost(size.width), size.height)
        drawContext.canvas.saveLayer(strip, Paint())
        clipRect(left = left) { content.drawContent() }
        drawRect(
            brush = Brush.horizontalGradient(
                colors = listOf(Color.Black, Color.Black.copy(alpha = 1f - strength)),
                startX = left,
                endX = right,
            ),
            topLeft = Offset(left, 0f),
            size = Size(fadeWidth, size.height),
            blendMode = BlendMode.DstIn,
        )
        drawContext.canvas.restore()
    }
