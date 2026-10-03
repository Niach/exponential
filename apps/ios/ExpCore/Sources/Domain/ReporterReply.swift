import Foundation

/// SLOP-4 — a widget submission IS an issue, and the conversation with its
/// reporter is COMMENTS. The copy of the reporter comment card and the
/// "Reply to reporter" composer toggle, mirrored ×4 (web imports
/// `fixtures/reporter-reply.json`, desktop `domain::reporter_reply`, Android
/// `domain/ReporterReply.kt`) and locked by `ReporterReplyTests` against the
/// contract fixture.
public enum ReporterReply {
    /// The composer's leading-row pill.
    public static let toggleLabel = "Reply to reporter"
    /// The field placeholder while the pill is ON; `{name}` = the reporter.
    public static let placeholderOn = "Reply to {name}… (emailed to them)"
    /// A reporter with no name (and the chip of a reporter comment).
    public static let anonymousName = "Anonymous visitor"
    /// A comment whose author row is gone and whose source is not reporter.
    public static let formerMemberName = "Former member"
    /// After the time on a `source == reporter` card — where "via MCP" sits.
    public static let reporterCaption = "reporter"
    /// Same slot on a member's `audience == reporter` card.
    public static let toReporterCaption = "to reporter"
    public static let sentToast = "Reply emailed to the reporter."
    public static let notSentToast = "Reply saved. No email was sent: this server has no mail transport."
    /// The `reporter_reply` inbox row.
    public static let notificationLabel = "Reporter replied"

    /// `placeholderOn` with the reporter's name filled in.
    public static func placeholder(name: String?) -> String {
        placeholderOn.replacingOccurrences(of: "{name}", with: reporterName(name))
    }

    /// The name a reporter comment (and the toggle) reads: the submission's
    /// `reporterName`, trimmed, else `anonymousName`.
    public static func reporterName(_ name: String?) -> String {
        let trimmed = name?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? anonymousName : trimmed
    }
}
