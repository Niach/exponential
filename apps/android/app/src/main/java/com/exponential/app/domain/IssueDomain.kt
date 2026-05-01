package com.exponential.app.domain

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cancel
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material.icons.filled.HourglassTop
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.RadioButtonChecked
import androidx.compose.material.icons.filled.RadioButtonUnchecked
import androidx.compose.material.icons.filled.Remove
import androidx.compose.material.icons.filled.Warning
import androidx.compose.ui.graphics.vector.ImageVector

enum class IssueStatus(val wire: String, val label: String) {
    Backlog("backlog", "Backlog"),
    Todo("todo", "Todo"),
    InProgress("in_progress", "In progress"),
    Done("done", "Done"),
    Cancelled("cancelled", "Cancelled");

    companion object {
        fun fromWire(value: String?): IssueStatus =
            entries.firstOrNull { it.wire == value } ?: Backlog
    }
}

val issueStatusOrder: List<IssueStatus> = listOf(
    IssueStatus.InProgress,
    IssueStatus.Todo,
    IssueStatus.Backlog,
    IssueStatus.Done,
    IssueStatus.Cancelled,
)

fun statusIcon(status: IssueStatus): ImageVector = when (status) {
    IssueStatus.Backlog -> Icons.Filled.RadioButtonUnchecked
    IssueStatus.Todo -> Icons.Filled.RadioButtonUnchecked
    IssueStatus.InProgress -> Icons.Filled.HourglassTop
    IssueStatus.Done -> Icons.Filled.CheckCircle
    IssueStatus.Cancelled -> Icons.Filled.Cancel
}

enum class IssuePriority(val wire: String, val label: String) {
    None("none", "No priority"),
    Urgent("urgent", "Urgent"),
    High("high", "High"),
    Medium("medium", "Medium"),
    Low("low", "Low");

    companion object {
        fun fromWire(value: String?): IssuePriority =
            entries.firstOrNull { it.wire == value } ?: None
    }
}

val issuePriorityOrder: List<IssuePriority> = listOf(
    IssuePriority.Urgent,
    IssuePriority.High,
    IssuePriority.Medium,
    IssuePriority.Low,
    IssuePriority.None,
)

fun priorityIcon(priority: IssuePriority): ImageVector = when (priority) {
    IssuePriority.None -> Icons.Filled.Remove
    IssuePriority.Urgent -> Icons.Filled.Warning
    IssuePriority.High -> Icons.Filled.KeyboardArrowUp
    IssuePriority.Medium -> Icons.Filled.DragHandle
    IssuePriority.Low -> Icons.Filled.KeyboardArrowDown
}
