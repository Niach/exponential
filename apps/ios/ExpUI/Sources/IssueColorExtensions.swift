import ExpCore
import SwiftUI

// SwiftUI color + shared-registry icon mapping for the status/priority enums,
// kept in ExpUI so ExpCore stays SwiftUI-free (and registry-free — the generated
// `AppIcons` names live here).
extension IssueStatus {
    /// Shared-registry icon name (EXP-273) — render through `AppIcon`.
    public var iconName: String {
        switch self {
        case .backlog: AppIcons.statusBacklog
        case .todo: AppIcons.statusTodo
        case .inProgress: AppIcons.statusInProgress
        case .inReview: AppIcons.statusInReview
        case .done: AppIcons.statusDone
        case .cancelled: AppIcons.statusCancelled
        case .duplicate: AppIcons.statusDuplicate
        }
    }

    public var color: Color {
        switch self {
        case .backlog: StatusColor.backlog
        case .todo: StatusColor.todo
        case .inProgress: StatusColor.inProgress
        case .inReview: StatusColor.inReview
        case .done: StatusColor.done
        case .cancelled: StatusColor.cancelled
        case .duplicate: StatusColor.duplicate
        }
    }
}

extension IssuePriority {
    /// Shared-registry icon name (EXP-273) — render through `AppIcon`.
    public var iconName: String {
        switch self {
        case .none: AppIcons.priorityNone
        case .urgent: AppIcons.priorityUrgent
        case .high: AppIcons.priorityHigh
        case .medium: AppIcons.priorityMedium
        case .low: AppIcons.priorityLow
        }
    }

    public var color: Color {
        switch self {
        case .none: PriorityColor.none
        case .urgent: PriorityColor.urgent
        case .high: PriorityColor.high
        case .medium: PriorityColor.medium
        case .low: PriorityColor.low
        }
    }
}
