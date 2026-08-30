package com.exponential.app.ui.theme

import androidx.compose.ui.graphics.Color
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.parseIsoDateOrNull
import java.time.LocalDate

// Semantic status / priority colors. The fixed values live in the shared
// packages/design-tokens/tokens.json (generated into DesignTokens.Semantic), so
// iOS/Android/Linux stop each carrying their own copy. The biggest single visual
// gap on Android was that every status/priority icon was tinted one muted
// zinc-400 — these restore the color.

private val Neutral = DesignTokens.Semantic.Neutral
private val Yellow = DesignTokens.Semantic.Yellow
private val Green = DesignTokens.Semantic.Green
private val Red = DesignTokens.Semantic.Red
private val Orange = DesignTokens.Semantic.Orange
private val Blue = DesignTokens.Semantic.Blue

fun statusColor(status: IssueStatus): Color = when (status) {
    IssueStatus.Backlog -> Neutral
    IssueStatus.InProgress -> Yellow
    // EXP-120: PR opened → in_review (green); merged → done (now blue).
    IssueStatus.InReview -> Green
    IssueStatus.Done -> Blue
    // Cancelled is a muted terminal RESOLUTION (like duplicate), not an error —
    // the web/desktop treatment, adopted everywhere by REV2-85.
    IssueStatus.Cancelled -> Neutral
    IssueStatus.Duplicate -> Neutral
}

/**
 * Color for a RESOLVED status row (EXP-314). Builtin rows and the constructed
 * fallbacks keep rendering the semantic design tokens above — byte-identical to
 * before custom statuses existed, and theme-safe (the seeded builtin hexes are
 * near-neutral and would read wrong). Only CUSTOM rows use their stored hex,
 * through the same [parseColor] path labels use; a missing hex stays neutral.
 */
fun resolvedStatusColor(status: ResolvedIssueStatus): Color {
    status.builtinKey?.let { return statusColor(it) }
    val hex = status.colorHex?.takeIf { it.isNotBlank() } ?: return Neutral
    return parseColor(hex)
}

fun priorityColor(priority: IssuePriority): Color = when (priority) {
    IssuePriority.None -> Neutral
    IssuePriority.Low -> Blue
    IssuePriority.Medium -> Yellow
    IssuePriority.High -> Orange
    IssuePriority.Urgent -> Red
}

/**
 * Color for a due-date string (`yyyy-MM-dd`): red if overdue, orange if today,
 * else a muted tertiary gray. Mirrors iOS `dueDateColor`.
 */
fun dueDateColor(dueDate: String?): Color {
    val due = parseIsoDateOrNull(dueDate) ?: return Neutral.copy(alpha = TextEmphasis.Tertiary)
    val today = LocalDate.now()
    return when {
        due.isBefore(today) -> Red
        due.isEqual(today) -> Orange
        else -> Neutral.copy(alpha = TextEmphasis.Tertiary)
    }
}
