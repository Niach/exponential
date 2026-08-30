import Foundation

public enum IssueStatus: String, CaseIterable, Codable, Identifiable, Sendable {
    case backlog
    // EXP-685: the builtin `todo` status is retired. The PG enum keeps `todo`
    // as an orphan label but no server sends it any more, so an old wire value
    // decodes through the forward-compat fallback below (backlog).
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
        case .inProgress: "In Progress"
        case .inReview: "In Review"
        case .done: "Done"
        case .cancelled: "Cancelled"
        case .duplicate: "Duplicate"
        }
    }

    // Lifecycle order (EXP-448 — the same order the statuses settings page
    // lays out); duplicate is a terminal resolution like cancelled, grouped
    // after it.
    public static let displayOrder: [IssueStatus] = [.backlog, .inProgress, .inReview, .done, .cancelled, .duplicate]

    public static func from(_ wire: String?) -> IssueStatus {
        guard let wire else { return .backlog }
        return IssueStatus(rawValue: wire) ?? .backlog
    }
}
