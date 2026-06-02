import Foundation

public enum IssueStatus: String, CaseIterable, Codable, Identifiable, Sendable {
    case backlog
    case todo
    case inProgress = "in_progress"
    case done
    case cancelled

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .backlog: "Backlog"
        case .todo: "Todo"
        case .inProgress: "In Progress"
        case .done: "Done"
        case .cancelled: "Cancelled"
        }
    }

    public var sfSymbol: String {
        switch self {
        case .backlog: "circle.dashed"
        case .todo: "circle"
        case .inProgress: "hourglass"
        case .done: "checkmark.circle.fill"
        case .cancelled: "xmark.circle.fill"
        }
    }

    public static let displayOrder: [IssueStatus] = [.inProgress, .todo, .backlog, .done, .cancelled]

    public static func from(_ wire: String?) -> IssueStatus {
        guard let wire else { return .backlog }
        return IssueStatus(rawValue: wire) ?? .backlog
    }
}
