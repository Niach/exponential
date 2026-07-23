package com.exponential.app.domain

// Mirrored from apps/web/src/lib/filters.ts. The filter shape and matching
// semantics must match the web mapping; if you change one, change the others
// (apps/web/src/lib/filters.ts, apps/ios/.../Domain/IssueFilters.swift).
data class IssueFilters(
    val statuses: Set<IssueStatus> = emptySet(),
    val priorities: Set<IssuePriority> = emptySet(),
    val labelIds: Set<String> = emptySet(),
) {
    val isEmpty: Boolean get() =
        statuses.isEmpty() && priorities.isEmpty() && labelIds.isEmpty()
    val count: Int get() =
        statuses.size + priorities.size + labelIds.size
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
