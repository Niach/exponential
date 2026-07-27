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

/**
 * Every filter token that addresses [status]: its own group key, plus — for a
 * builtin row — the `builtin:<key>` token a filter TICKED BEFORE the
 * issue_statuses shape synced would have stored. A filter set on the
 * constructed fallback must survive the fallback→synced re-key (the row id
 * changes from `builtin:backlog` to a uuid the moment the rows land); a
 * row-uuid token still matches only that row.
 */
fun statusFilterTokens(status: ResolvedIssueStatus): Set<String> {
    val builtinToken = status.builtinKey?.let { IssueStatusResolver.BUILTIN_ID_PREFIX + it.wire }
    return if (builtinToken == null || builtinToken == status.id) setOf(status.id)
    else setOf(status.id, builtinToken)
}

/** Whether [status] is currently ticked in this filter's status set. */
fun IssueFilters.isStatusSelected(status: ResolvedIssueStatus): Boolean =
    statusFilterTokens(status).any { it in statusIds }

/**
 * Idempotent tick/untick of one status. Unticking drops EVERY token of that
 * status, so a stale pre-sync `builtin:<key>` token can never survive the
 * removal and keep the filter silently active.
 */
fun IssueFilters.toggleStatus(status: ResolvedIssueStatus): IssueFilters {
    val tokens = statusFilterTokens(status)
    val next = if (isStatusSelected(status)) statusIds - tokens else statusIds + status.id
    return copy(statusIds = next)
}

fun matchesFilters(
    status: ResolvedIssueStatus,
    priority: IssuePriority,
    issueLabelIds: Collection<String>,
    filters: IssueFilters,
): Boolean {
    if (filters.statusIds.isNotEmpty() && !filters.isStatusSelected(status)) return false
    if (filters.priorities.isNotEmpty() && priority !in filters.priorities) return false
    if (filters.labelIds.isNotEmpty() && filters.labelIds.none { it in issueLabelIds }) return false
    return true
}
