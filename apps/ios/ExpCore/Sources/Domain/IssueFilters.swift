import Foundation

// Mirrored from apps/web/src/lib/filters.ts. The filter shape and matching
// semantics must match the web mapping; if you change one, change the others
// (apps/web/src/lib/filters.ts, apps/android/.../domain/IssueFilters.kt).
public struct IssueFilters: Equatable, Sendable {
    /// EXP-314: status GROUP KEYS — an `issue_statuses` row id, or the
    /// synthetic `builtin:<key>` of a constructed default while the statuses
    /// shape hasn't synced. Same semantics as `labelIds`.
    public var statusIds: Set<String> = []
    public var priorities: Set<IssuePriority> = []
    public var labelIds: Set<String> = []

    public init(statusIds: Set<String> = [], priorities: Set<IssuePriority> = [], labelIds: Set<String> = []) {
        self.statusIds = statusIds
        self.priorities = priorities
        self.labelIds = labelIds
    }

    public var isEmpty: Bool { statusIds.isEmpty && priorities.isEmpty && labelIds.isEmpty }
    public var count: Int { statusIds.count + priorities.count + labelIds.count }
}

/// `statusId` is the issue's RESOLVED group key (`ResolvedIssueStatus.id`), not
/// the raw `issues.status_id` column — a pre-backfill row still matches the
/// builtin group it resolves into.
public func matchesFilters(
    statusId: String,
    priority: IssuePriority,
    issueLabelIds: Set<String>,
    filters: IssueFilters
) -> Bool {
    if !filters.statusIds.isEmpty && !filters.statusIds.contains(statusId) { return false }
    if !filters.priorities.isEmpty && !filters.priorities.contains(priority) { return false }
    if !filters.labelIds.isEmpty && filters.labelIds.isDisjoint(with: issueLabelIds) { return false }
    return true
}
