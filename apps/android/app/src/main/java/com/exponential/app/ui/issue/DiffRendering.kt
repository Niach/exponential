package com.exponential.app.ui.issue

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
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
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.ui.theme.Motion

// Shared unified-diff rendering primitives (iOS DiffRendering.swift parity):
// +/−/@@ line coloring + tinted line backgrounds, used by the issue Changes
// page and the agent-session "Latest changes" diff panel.

val DiffAddColor = Color(0xFF6EE7B7) // emerald-300
val DiffDelColor = Color(0xFFFDA4AF) // rose-300
val DiffHunkColor = Color.White.copy(alpha = 0.5f) // muted (EXP-594, iOS parity)

/** Foreground color for one unified-diff line. `+++`/`---` file headers are meta, not changes. */
fun diffLineColor(line: String, context: Color): Color = when {
    line.startsWith("@@") -> DiffHunkColor
    line.startsWith("+++") || line.startsWith("---") -> context
    line.startsWith("+") -> DiffAddColor
    line.startsWith("-") -> DiffDelColor
    else -> context
}

/** Faint green/red row tint behind added/deleted lines (iOS DiffFilesView parity). */
fun diffLineBackground(line: String): Color = when {
    line.startsWith("+++") || line.startsWith("---") || line.startsWith("@@") -> Color.Transparent
    line.startsWith("+") -> DiffAddColor.copy(alpha = 0.08f)
    line.startsWith("-") -> DiffDelColor.copy(alpha = 0.08f)
    else -> Color.Transparent
}

data class DiffStats(val additions: Int, val deletions: Int)

/** Count +/− lines of a unified diff, excluding the `+++`/`---` file headers. */
fun unifiedDiffStats(diff: String): DiffStats {
    var add = 0
    var del = 0
    diff.split("\n").forEach { line ->
        when {
            line.startsWith("+++") || line.startsWith("---") -> Unit
            line.startsWith("+") -> add++
            line.startsWith("-") -> del++
        }
    }
    return DiffStats(add, del)
}

/** One file's chunk of a multi-file unified diff (split on `diff --git`). */
data class DiffFileSection(val filename: String, val lines: List<String>)

/**
 * Split raw `git diff` output into per-file sections. Diffs without a
 * `diff --git` header (e.g. a bare hunk) come back as one unnamed section.
 */
fun splitUnifiedDiff(diff: String): List<DiffFileSection> {
    val sections = mutableListOf<DiffFileSection>()
    var filename = ""
    var lines = mutableListOf<String>()
    fun flush() {
        if (filename.isNotEmpty() || lines.any { it.isNotBlank() }) {
            sections.add(DiffFileSection(filename, lines))
        }
    }
    diff.split("\n").forEach { line ->
        if (line.startsWith("diff --git ")) {
            flush()
            // `diff --git a/path b/path` — the b/ side is the current name.
            filename = line.substringAfterLast(" b/", missingDelimiterValue = "")
                .ifEmpty { line.removePrefix("diff --git ").trim() }
            lines = mutableListOf()
        } else {
            lines.add(line)
        }
    }
    flush()
    return sections
}

/**
 * Rendered-line cap per patch (iOS DiffRendering `maxLines: Int = 600` parity)
 * — every line composes a Text under IntrinsicSize.Max intrinsic measurement,
 * so an uncapped multi-thousand-line patch (lockfile PRs, raw worktree diffs)
 * freezes the frame.
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

/**
 * The monospace patch body: colored +/−/@@ lines with faint row tints.
 * Horizontal scrolling lives INSIDE this block — never on the page.
 * Capped at [maxLines] with a truncation footer (iOS DiffPatchBlock parity).
 */
@Composable
fun PatchLines(
    lines: List<String>,
    contextColor: Color,
    modifier: Modifier = Modifier,
    maxLines: Int = DIFF_MAX_RENDERED_LINES,
) {
    val truncated = lines.size > maxLines
    val shown = if (truncated) lines.subList(0, maxLines) else lines
    val scrollState = rememberScrollState()
    val fadeStrength = trailingFadeStrength(scrollState)
    Column(modifier = modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(scrollState),
        ) {
            SelectionContainer {
                Column(modifier = Modifier.width(IntrinsicSize.Max)) {
                    shown.chunked(DIFF_FADE_BAND_ROWS).forEach { band ->
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .trailingScrollFade(scrollState, fadeStrength),
                        ) {
                            band.forEach { line ->
                                Text(
                                    text = line.ifEmpty { " " },
                                    color = diffLineColor(line, contextColor),
                                    fontFamily = FontFamily.Monospace,
                                    fontSize = 11.sp,
                                    lineHeight = 15.sp,
                                    maxLines = 1,
                                    softWrap = false,
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .background(diffLineBackground(line))
                                        .padding(horizontal = 10.dp),
                                )
                            }
                        }
                    }
                }
            }
        }
        if (truncated) {
            Text(
                text = "Diff truncated. Showing the first $maxLines lines.",
                color = contextColor,
                fontSize = 11.sp,
                lineHeight = 15.sp,
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            )
        }
    }
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
