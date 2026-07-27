package com.exponential.app.domain

// Mirrored from apps/web/src/lib/filters.ts. The filter shape and matching
// semantics must match the web mapping; if you change one, change the others
// (apps/web/src/lib/filters.ts, apps/ios/.../Domain/IssueFilters.swift).
//
// EXP-314: the status filter is a set of GROUP KEYS — a team status row id, or
// `builtin:<key>` for a constructed fallback the shape hasn't synced yet — not
// the anchor enum, exactly like labelIds.
data class IssueFilters(
    val statusIds: Set<String> = emptySet(),
    val priorities: Set<IssuePriority> = emptySet(),
    val labelIds: Set<String> = emptySet(),
) {
    val isEmpty: Boolean get() =
        statusIds.isEmpty() && priorities.isEmpty() && labelIds.isEmpty()
    val count: Int get() =
        statusIds.size + priorities.size + labelIds.size
}

fun matchesFilters(
    statusGroupKey: String,
    priority: IssuePriority,
    issueLabelIds: Collection<String>,
    filters: IssueFilters,
): Boolean {
    if (filters.statusIds.isNotEmpty() && statusGroupKey !in filters.statusIds) return false
    if (filters.priorities.isNotEmpty() && priority !in filters.priorities) return false
    if (filters.labelIds.isNotEmpty() && filters.labelIds.none { it in issueLabelIds }) return false
    return true
}
