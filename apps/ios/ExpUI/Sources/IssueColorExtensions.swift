import ExpCore
import SwiftUI

// SwiftUI color mapping for the status/priority enums, kept in ExpUI so ExpCore
// stays SwiftUI-free.
extension IssueStatus {
    public var color: Color {
        switch self {
        case .backlog: StatusColor.backlog
        case .todo: StatusColor.todo
        case .inProgress: StatusColor.inProgress
        case .done: StatusColor.done
        case .cancelled: StatusColor.cancelled
        case .duplicate: StatusColor.duplicate
        }
    }
}

extension IssuePriority {
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
