package com.exponential.app.ui.issue

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
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// Shared unified-diff rendering primitives (iOS DiffRendering.swift parity):
// +/−/@@ line coloring + tinted line backgrounds, used by the issue Changes
// page and the agent-session "Latest changes" diff panel.

val DiffAddColor = Color(0xFF6EE7B7) // emerald-300
val DiffDelColor = Color(0xFFFDA4AF) // rose-300
val DiffHunkColor = Color(0xFFA5B4FC) // indigo-300

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
    Column(modifier = modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState()),
        ) {
            SelectionContainer {
                Column(modifier = Modifier.width(IntrinsicSize.Max)) {
                    shown.forEach { line ->
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
        if (truncated) {
            Text(
                text = "Diff truncated — showing the first $maxLines lines.",
                color = contextColor,
                fontSize = 11.sp,
                lineHeight = 15.sp,
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            )
        }
    }
}
