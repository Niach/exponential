import Foundation

// EXP-933: where a notification TAP lands — the push payload and the inbox
// row read the same rules. An `agent_message` that names an issue opens that
// issue; an issue-less one stays an inbox row (EXP-801). A blocked session
// opens that session.

public enum NotificationRouting {
    /// Where a push tap goes.
    public enum PushTarget: Equatable, Sendable {
        /// A blocked session (EXP-980).
        case session(id: String)
        /// My Work → Inbox (an issue-less agent message, or a blocked session
        /// whose row was pruned).
        case inbox
        case issue(id: String)
        /// Nothing to open.
        case none
    }

    private static func nonEmpty(_ value: Any?) -> String? {
        guard let string = value as? String, !string.isEmpty else { return nil }
        return string
    }

    /// The push payload (`userInfo`) → its destination.
    public static func pushTarget(_ userInfo: [AnyHashable: Any]) -> PushTarget {
        // SLOP-4: a `reporter_reply` carries `issueId` like `issue_comment`
        // and lands on the issue through the generic branch below.
        let type = userInfo["type"] as? String
        if type == DomainContract.notificationTypeSessionBlocked {
            if let sessionId = nonEmpty(userInfo["sessionId"]) { return .session(id: sessionId) }
            return .inbox
        }
        if let issueId = nonEmpty(userInfo["issueId"]) { return .issue(id: issueId) }
        // An issue-less agent message renders in the inbox alone.
        if type == DomainContract.notificationTypeAgentMessage { return .inbox }
        return .none
    }
}
