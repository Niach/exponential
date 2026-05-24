import Foundation

// Mirrored from apps/web/src/lib/filters.ts. The active/backlog tab presets
// must match the web mapping; if you change one, change the others
// (apps/web/src/lib/filters.ts, apps/android/.../domain/IssueFilters.kt).
struct IssueFilters: Equatable, Sendable {
    var statuses: Set<IssueStatus> = []
    var priorities: Set<IssuePriority> = []
    var labelIds: Set<String> = []

    var isEmpty: Bool { statuses.isEmpty && priorities.isEmpty && labelIds.isEmpty }
    var count: Int { statuses.count + priorities.count + labelIds.count }
}

enum FilterTab: String, CaseIterable, Identifiable, Sendable {
    case all
    case active
    case backlog

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: "All Issues"
        case .active: "Active"
        case .backlog: "Backlog"
        }
    }

    var statuses: Set<IssueStatus> {
        switch self {
        case .all: []
        case .active: [.inProgress, .todo]
        case .backlog: [.backlog]
        }
    }
}

func deriveTab(from statuses: Set<IssueStatus>) -> FilterTab {
    if statuses.isEmpty { return .all }
    if statuses == Set([IssueStatus.inProgress, .todo]) { return .active }
    if statuses == Set([IssueStatus.backlog]) { return .backlog }
    return .all
}

func matchesFilters(
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
