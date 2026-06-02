import ExpCore
import SwiftUI

// SwiftUI color mapping for the status/priority enums, kept in the app module so
// `ExpCore` stays SwiftUI-free. `StatusColor`/`PriorityColor` live in GlassTheme.
// (Relocates to `ExpUI` in A2 when the first macOS view needs these colors.)
extension IssueStatus {
    var color: Color {
        switch self {
        case .backlog: StatusColor.backlog
        case .todo: StatusColor.todo
        case .inProgress: StatusColor.inProgress
        case .done: StatusColor.done
        case .cancelled: StatusColor.cancelled
        }
    }
}

extension IssuePriority {
    var color: Color {
        switch self {
        case .none: PriorityColor.none
        case .urgent: PriorityColor.urgent
        case .high: PriorityColor.high
        case .medium: PriorityColor.medium
        case .low: PriorityColor.low
        }
    }
}
