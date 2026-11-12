import Foundation

/// #EXP-1797: the team's issue trash ×4, mirrored from the contract fixture
/// `domain-contract/fixtures/issue-lifecycle.json` → `trash` (TS
/// `issueTrashCaption` / `issueTrashActions` / `issueTrashRowText` /
/// `isIssuePurgeDue`) and locked by `IssueTrashTests`. A deleted issue sits
/// in the trash `retentionDays`, restorable by any member; owners may also
/// Delete forever (the `purge-issue` prompt).
public enum IssueTrash {
    /// Days a deleted issue stays restorable before the sweep purges it.
    public static let retentionDays = 30

    /// The band's copy (fixture `copy`).
    public enum Copy {
        public static let band = "Deleted issues"
        public static let row = "{identifier} {title}"
        public static let restore = "Restore"
        public static let purge = "Delete forever"
    }

    /// A trash row's pill (fixture `actionCases`), display order.
    public enum Action: String, Equatable, Sendable {
        case restore
        case purge
    }

    /// The role that may Delete forever (fixture `access.purge`).
    public static let purgeRole = "owner"

    private static let hourMs: Double = 3_600_000

    /// The purge countdown: hours inside the last two days, else whole days
    /// (both rounded up); "Purging soon" once due, "Purges soon" without a
    /// timestamp.
    public static func caption(purgeAt: String?, now: Date = Date()) -> String {
        guard let date = parse(purgeAt) else { return "Purges soon" }
        let ms = (date.timeIntervalSince(now) * 1000).rounded()
        if ms <= 0 { return "Purging soon" }
        if ms <= 48 * hourMs { return "Purges in ~\(Int((ms / hourMs).rounded(.up)))h" }
        return "Purges in ~\(Int((ms / (24 * hourMs)).rounded(.up)))d"
    }

    /// The row's pills for a member of `role`.
    public static func actions(role: String) -> [Action] {
        role == purgeRole ? [.restore, .purge] : [.restore]
    }

    /// The row text: `{identifier} {title}`.
    public static func rowText(identifier: String, title: String) -> String {
        Prompts.fill(Copy.row, ["identifier": identifier, "title": title])
    }

    /// The sweep's rule: due once `deletedAt` + retention has passed.
    public static func isPurgeDue(deletedAt: String?, now: Date = Date()) -> Bool {
        guard let date = parse(deletedAt) else { return false }
        return date.addingTimeInterval(Double(retentionDays) * 24 * 3600) <= now
    }

    static func parse(_ raw: String?) -> Date? {
        guard let raw, !raw.isEmpty else { return nil }
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: raw) ?? ISO8601DateFormatter().date(from: raw)
    }
}
