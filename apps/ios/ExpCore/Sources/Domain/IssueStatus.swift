import Foundation

public enum IssueStatus: String, CaseIterable, Codable, Identifiable, Sendable {
    case backlog
    case todo
    case inProgress = "in_progress"
    // Opening a PR moves a linked issue here (EXP-120); merging it completes to
    // `done`. Sits between in_progress and done in the display order.
    case inReview = "in_review"
    case done
    case cancelled
    case duplicate

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .backlog: "Backlog"
        case .todo: "Todo"
        case .inProgress: "In Progress"
        case .inReview: "In Review"
        case .done: "Done"
        case .cancelled: "Cancelled"
        case .duplicate: "Duplicate"
        }
    }

    public var sfSymbol: String {
        switch self {
        case .backlog: "circle.dashed"
        case .todo: "circle"
        case .inProgress: "hourglass"
        // Mirrors web's GitPullRequest glyph — the issue is up for review.
        case .inReview: "arrow.triangle.pull"
        case .done: "checkmark.circle.fill"
        case .cancelled: "xmark.circle.fill"
        case .duplicate: "doc.on.doc"
        }
    }

    // duplicate is a terminal resolution like cancelled — grouped after it.
    public static let displayOrder: [IssueStatus] = [.inProgress, .inReview, .todo, .backlog, .done, .cancelled, .duplicate]

    public static func from(_ wire: String?) -> IssueStatus {
        guard let wire else { return .backlog }
        return IssueStatus(rawValue: wire) ?? .backlog
    }
}
