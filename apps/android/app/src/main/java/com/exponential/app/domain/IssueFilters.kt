package com.exponential.app.domain

data class IssueFilters(
    val statuses: Set<IssueStatus> = emptySet(),
    val priorities: Set<IssuePriority> = emptySet(),
    val labelIds: Set<String> = emptySet(),
) {
    val isEmpty: Boolean get() = statuses.isEmpty() && priorities.isEmpty() && labelIds.isEmpty()
    val count: Int get() = statuses.size + priorities.size + labelIds.size
}

enum class FilterTab(val label: String) {
    All("All issues"),
    Active("Active"),
    Backlog("Backlog");
}

private val activeStatuses = setOf(IssueStatus.InProgress, IssueStatus.Todo)
private val backlogStatuses = setOf(IssueStatus.Backlog)

fun FilterTab.statuses(): Set<IssueStatus> = when (this) {
    FilterTab.All -> emptySet()
    FilterTab.Active -> activeStatuses
    FilterTab.Backlog -> backlogStatuses
}

fun deriveTab(statuses: Set<IssueStatus>): FilterTab = when {
    statuses.isEmpty() -> FilterTab.All
    statuses == activeStatuses -> FilterTab.Active
    statuses == backlogStatuses -> FilterTab.Backlog
    else -> FilterTab.All
}

fun matchesFilters(
    status: IssueStatus,
    priority: IssuePriority,
    issueLabelIds: Collection<String>,
    filters: IssueFilters,
): Boolean {
    if (filters.statuses.isNotEmpty() && status !in filters.statuses) return false
    if (filters.priorities.isNotEmpty() && priority !in filters.priorities) return false
    if (filters.labelIds.isNotEmpty() && filters.labelIds.none { it in issueLabelIds }) return false
    return true
}
