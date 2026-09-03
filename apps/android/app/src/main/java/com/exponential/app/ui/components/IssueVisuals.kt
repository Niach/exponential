package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.priorityIcon
import com.exponential.app.domain.statusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.priorityColor
import com.exponential.app.ui.theme.resolvedStatusColor
import com.exponential.app.ui.theme.statusColor

/**
 * Status glyph tinted by its semantic color (iOS colored status icon), keyed on
 * the ANCHOR enum. Kept for the cross-team surfaces (My Issues, search, start
 * pickers) that group/render by anchor; per-board surfaces use the
 * [ResolvedIssueStatus] overload below.
 */
@Composable
fun StatusIcon(status: IssueStatus, modifier: Modifier = Modifier, size: Dp = 16.dp) {
    Icon(
        statusIcon(status),
        contentDescription = status.label,
        tint = statusColor(status),
        modifier = modifier.size(size),
    )
}

/**
 * Glyph for a RESOLVED team status row (EXP-314): the category glyph (started
 * rows get their position's pie clock) tinted by [resolvedStatusColor]. An
 * unknown registry name can never blank the row — it degrades to the neutral
 * backlog glyph.
 */
@Composable
fun StatusIcon(status: ResolvedIssueStatus, modifier: Modifier = Modifier, size: Dp = 16.dp) {
    Icon(
        ExpIcons.byName(status.iconName) ?: ExpIcons.statusBacklog,
        contentDescription = status.name,
        tint = resolvedStatusColor(status),
        modifier = modifier.size(size),
    )
}

/** Priority glyph tinted by its semantic color (iOS colored priority icon). */
@Composable
fun PriorityIcon(priority: IssuePriority, modifier: Modifier = Modifier, size: Dp = 16.dp) {
    Icon(
        priorityIcon(priority),
        contentDescription = priority.label,
        tint = priorityColor(priority),
        modifier = modifier.size(size),
    )
}

/** Bare colored label dot used in list rows (iOS shows up to three dots). */
@Composable
fun LabelDot(color: Color, modifier: Modifier = Modifier, size: Dp = 8.dp) {
    androidx.compose.foundation.layout.Box(modifier.size(size).background(color, CircleShape))
}

/**
 * Dot + name pill used in the filter sheet and issue detail — the shared small
 * READONLY pill (EXP-698). It used to be its own recipe: a 6dp-radius box on an
 * 18%-tint of the label's own colour, which made a label read as a coloured
 * badge while every other capsule on the same screen was glass. The colour
 * survives as the pill's dot.
 */
@Composable
fun LabelChip(label: LabelEntity, modifier: Modifier = Modifier) {
    GlassPill(
        label.name,
        modifier = modifier,
        size = PillSize.Sm,
        mode = PillMode.Readonly,
        dot = remember(label.color) { parseColor(label.color) },
    )
}
