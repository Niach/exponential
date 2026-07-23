import Foundation

// Mirrored from apps/web/src/lib/filters.ts. The filter shape and matching
// semantics must match the web mapping; if you change one, change the others
// (apps/web/src/lib/filters.ts, apps/android/.../domain/IssueFilters.kt).
public struct IssueFilters: Equatable, Sendable {
    public var statuses: Set<IssueStatus> = []
    public var priorities: Set<IssuePriority> = []
    public var labelIds: Set<String> = []

    public init(statuses: Set<IssueStatus> = [], priorities: Set<IssuePriority> = [], labelIds: Set<String> = []) {
        self.statuses = statuses
        self.priorities = priorities
        self.labelIds = labelIds
    }

    public var isEmpty: Bool { statuses.isEmpty && priorities.isEmpty && labelIds.isEmpty }
    public var count: Int { statuses.count + priorities.count + labelIds.count }
}

public func matchesFilters(
    status: IssueStatus,
    priority: IssuePriority,
    issueLabelIds: Set<String>,
    filters: IssueFilters
) -> Bool {
    if !filters.statuses.isEmpty && !filters.statuses.contains(status) { return false }
    if !filters.priorities.isEmpty && !filters.priorities.contains(priority) { return false }
    if !filters.labelIds.isEmpty && filters.labelIds.isDisjoint(with: issueLabelIds) { return false }
    return true
}
