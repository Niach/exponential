package com.exponential.app.ui.theme

import androidx.compose.ui.graphics.Color
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
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
    IssueStatus.Todo -> Neutral
    IssueStatus.InProgress -> Yellow
    IssueStatus.Done -> Green
    IssueStatus.Cancelled -> Red
    IssueStatus.Duplicate -> Neutral
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
