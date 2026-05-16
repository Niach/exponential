import SwiftUI

enum IssueStatus: String, CaseIterable, Codable, Identifiable, Sendable {
    case backlog
    case todo
    case inProgress = "in_progress"
    case done
    case cancelled

    var id: String { rawValue }

    var label: String {
        switch self {
        case .backlog: "Backlog"
        case .todo: "Todo"
        case .inProgress: "In Progress"
        case .done: "Done"
        case .cancelled: "Cancelled"
        }
    }

    var sfSymbol: String {
        switch self {
        case .backlog: "circle.dashed"
        case .todo: "circle"
        case .inProgress: "hourglass"
        case .done: "checkmark.circle.fill"
        case .cancelled: "xmark.circle.fill"
        }
    }

    var color: Color {
        switch self {
        case .backlog: StatusColor.backlog
        case .todo: StatusColor.todo
        case .inProgress: StatusColor.inProgress
        case .done: StatusColor.done
        case .cancelled: StatusColor.cancelled
        }
    }

    static let displayOrder: [IssueStatus] = [.inProgress, .todo, .backlog, .done, .cancelled]

    static func from(_ wire: String?) -> IssueStatus {
        guard let wire else { return .backlog }
        return IssueStatus(rawValue: wire) ?? .backlog
    }
}
