package com.exponential.app.ui.theme

import androidx.compose.ui.graphics.Color
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.parseIsoDateOrNull
import java.time.LocalDate

// Semantic status / priority colors ported from iOS GlassTheme.swift so the
// issue list & detail are color-scannable and match the iOS look. iOS uses
// SwiftUI semantic colors; these are their closest fixed equivalents tuned for
// the zinc dark theme. The biggest single visual gap on Android was that every
// status/priority icon was tinted one muted zinc-400 — this restores the color.

private val Neutral = Color(0xFFA1A1AA) // zinc-400
private val Yellow = Color(0xFFFACC15)
private val Green = Color(0xFF22C55E)
private val Red = Color(0xFFEF4444)
private val Orange = Color(0xFFF97316)
private val Blue = Color(0xFF3B82F6)

fun statusColor(status: IssueStatus): Color = when (status) {
    IssueStatus.Backlog -> Neutral
    IssueStatus.Todo -> Neutral
    IssueStatus.InProgress -> Yellow
    IssueStatus.Done -> Green
    IssueStatus.Cancelled -> Red
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

/**
 * Agent-plan badge colors (iOS `PlanLabel`). Tokenized here to kill the
 * hardcoded hex previously duplicated across IssueDetailScreen and CommentThread.
 */
object PlanColors {
    val Drafting = Color(0xFFEAB308)
    val AwaitingAnswer = Color(0xFFB388F5)
    val AwaitingApproval = Color(0xFF60A5FA)
    val Approved = Color(0xFF34D399)
}
