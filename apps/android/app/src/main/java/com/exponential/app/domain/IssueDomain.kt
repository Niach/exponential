package com.exponential.app.domain

import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.ui.icons.ExpIcons
import java.time.LocalDate

enum class IssueStatus(val wire: String, val label: String) {
    Backlog("backlog", "Backlog"),
    Todo("todo", "Todo"),
    InProgress("in_progress", "In progress"),
    InReview("in_review", "In review"),
    Done("done", "Done"),
    Cancelled("cancelled", "Cancelled"),
    Duplicate("duplicate", "Duplicate");

    companion object {
        fun fromWire(value: String?): IssueStatus =
            entries.firstOrNull { it.wire == value } ?: Backlog

        /**
         * Human label for a wire status. Unlike [fromWire] (which falls back
         * to Backlog for UI that needs SOME status), an unknown wire value —
         * e.g. from a newer server — renders verbatim instead of mislabeling.
         */
        fun labelFor(wire: String): String =
            entries.firstOrNull { it.wire == wire }?.label ?: wire.replace('_', ' ')
    }
}

val issueStatusOrder: List<IssueStatus> = listOf(
    IssueStatus.InProgress,
    IssueStatus.InReview,
    IssueStatus.Todo,
    IssueStatus.Backlog,
    IssueStatus.Done,
    IssueStatus.Cancelled,
    IssueStatus.Duplicate,
)

fun statusIcon(status: IssueStatus): ImageVector = when (status) {
    IssueStatus.Backlog -> ExpIcons.statusBacklog
    IssueStatus.Todo -> ExpIcons.statusTodo
    IssueStatus.InProgress -> ExpIcons.statusInProgress
    IssueStatus.InReview -> ExpIcons.statusInReview
    IssueStatus.Done -> ExpIcons.statusDone
    IssueStatus.Cancelled -> ExpIcons.statusCancelled
    IssueStatus.Duplicate -> ExpIcons.statusDuplicate
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
    IssuePriority.None -> ExpIcons.priorityNone
    IssuePriority.Urgent -> ExpIcons.priorityUrgent
    IssuePriority.High -> ExpIcons.priorityHigh
    IssuePriority.Medium -> ExpIcons.priorityMedium
    IssuePriority.Low -> ExpIcons.priorityLow
}

// ---------------------------------------------------------------------------
// Canonical in-group issue ordering (EXP-38) — the SAME comparator ships on
// web, iOS, Android, and desktop; change it only in lockstep with the others.
// Group order itself is issueStatusOrder above; within a group:
// - backlog/todo/in_progress: overdue first (dueDate < today), then priority
//   rank urgent(0)→none(4), then dueDate ascending with null last, then issue
//   `number` ascending (numeric — never the identifier string, never sortOrder).
// - done: (completedAt ?? updatedAt) descending (latest completed first).
// - cancelled/duplicate: updatedAt descending.
// ---------------------------------------------------------------------------

/** Priority rank for sorting: urgent(0) < high(1) < medium(2) < low(3) < none(4). */
fun issuePriorityRank(priority: IssuePriority): Int = when (priority) {
    IssuePriority.Urgent -> 0
    IssuePriority.High -> 1
    IssuePriority.Medium -> 2
    IssuePriority.Low -> 3
    IssuePriority.None -> 4
}

/**
 * Normalize a stored timestamp into a fixed-width sortable string. Room holds
 * timestamps in TWO wire formats — Electric's `yyyy-MM-dd HH:mm:ss.ffffff+00`
 * and the optimistic tRPC upserts' ISO `yyyy-MM-ddTHH:mm:ss.SSSZ` — so a naive
 * string compare mis-orders mixed rows (space vs `T`, and shorter fractions
 * compare against the zone suffix). Swap the separator, drop the always-UTC
 * zone suffix, and pad the fraction to 6 digits.
 */
fun sortableTimestamp(value: String): String {
    val t = value.replaceFirst(' ', 'T')
    // Zone designator can only start at/after the seconds field ("yyyy-MM-ddTHH:mm:ss"
    // is 19 chars) — earlier '-' are the date separators.
    var end = t.length
    for (i in 19 until t.length) {
        val c = t[i]
        if (c == 'Z' || c == '+' || c == '-') {
            end = i
            break
        }
    }
    val local = t.substring(0, end)
    val dot = local.indexOf('.')
    return if (dot >= 0) {
        local.substring(0, dot + 1) + local.substring(dot + 1).padEnd(6, '0').take(6)
    } else {
        "$local.000000"
    }
}

/**
 * Comparator for issues WITHIN one status group. [today] is the local ISO date
 * (`yyyy-MM-dd`, injectable for tests) the overdue boost compares `dueDate`
 * against — date-only ISO strings order correctly lexicographically.
 */
fun issueComparatorForGroup(
    status: IssueStatus,
    today: String = LocalDate.now().toString(),
): Comparator<IssueEntity> = when (status) {
    IssueStatus.Backlog, IssueStatus.Todo, IssueStatus.InProgress, IssueStatus.InReview ->
        activeIssueComparator(today)
    IssueStatus.Done -> completedIssueComparator()
    IssueStatus.Cancelled, IssueStatus.Duplicate -> updatedDescComparator()
}

/**
 * The same comparator keyed on a status CATEGORY (EXP-314) — the per-board list
 * groups by team status ROW now, and a custom row's ordering follows its
 * category. Branches are identical to [issueComparatorForGroup]: completed →
 * (completedAt ?? updatedAt) desc, cancelled/duplicate → updatedAt desc,
 * everything else (backlog/unstarted/started, and any unknown category) → the
 * active branch.
 */
fun issueComparatorForCategory(
    category: IssueStatusCategory,
    today: String = LocalDate.now().toString(),
): Comparator<IssueEntity> = when (category) {
    IssueStatusCategory.Completed -> completedIssueComparator()
    IssueStatusCategory.Cancelled, IssueStatusCategory.Duplicate -> updatedDescComparator()
    IssueStatusCategory.Backlog, IssueStatusCategory.Unstarted, IssueStatusCategory.Started ->
        activeIssueComparator(today)
}

private fun activeIssueComparator(today: String): Comparator<IssueEntity> =
    // false < true, so overdue rows (dueDate < today) come first.
    compareBy<IssueEntity> { !(it.dueDate != null && it.dueDate < today) }
        .thenBy { issuePriorityRank(IssuePriority.fromWire(it.priority)) }
        .thenBy { it.dueDate == null } // null due dates last
        .thenBy { it.dueDate ?: "" }
        .thenBy { it.number }

private fun completedIssueComparator(): Comparator<IssueEntity> =
    compareByDescending { sortableTimestamp(it.completedAt ?: it.updatedAt) }

private fun updatedDescComparator(): Comparator<IssueEntity> =
    compareByDescending { sortableTimestamp(it.updatedAt) }

/** Sort one status group's issues by the canonical in-group order (EXP-38). */
fun <T> sortIssuesForGroup(
    status: IssueStatus,
    issues: List<T>,
    today: String = LocalDate.now().toString(),
    issueOf: (T) -> IssueEntity,
): List<T> {
    val comparator = issueComparatorForGroup(status, today)
    return issues.sortedWith(Comparator { a, b -> comparator.compare(issueOf(a), issueOf(b)) })
}

/** [sortIssuesForGroup] for a status-ROW group, keyed on its category (EXP-314). */
fun <T> sortIssuesForCategory(
    category: IssueStatusCategory,
    issues: List<T>,
    today: String = LocalDate.now().toString(),
    issueOf: (T) -> IssueEntity,
): List<T> {
    val comparator = issueComparatorForCategory(category, today)
    return issues.sortedWith(Comparator { a, b -> comparator.compare(issueOf(a), issueOf(b)) })
}
