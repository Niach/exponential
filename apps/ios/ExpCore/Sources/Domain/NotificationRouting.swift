import Foundation

// EXP-933: where a notification TAP lands — the push payload and the inbox
// row read the same rules. An `agent_message` that names an issue is the
// agent's REPORT on that issue, so it opens the issue's Work screen on its
// Results face; an issue-less one stays an inbox row (EXP-801).

public enum NotificationRouting {
    /// The push payload's face hint for the Results face.
    public static let resultsFaceHint = "results"

    /// Where a push tap goes.
    public enum PushTarget: Equatable, Sendable {
        /// A blocked run (EXP-980).
        case session(id: String)
        /// My Work → Inbox (an issue-less agent message, or a blocked run
        /// whose row was pruned).
        case inbox
        /// An issue's Work screen, on `face`.
        case issue(id: String, face: WorkFaceKind)
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
        if type == DomainContract.notificationTypeAgentMessage {
            // EXP-933: a targeted message carries its issue (and
            // `face: results`); an issue-less one renders in the inbox alone.
            guard let issueId = nonEmpty(userInfo["issueId"]) else { return .inbox }
            return .issue(id: issueId, face: .results)
        }
        if let issueId = nonEmpty(userInfo["issueId"]) {
            let face: WorkFaceKind = userInfo["face"] as? String == resultsFaceHint ? .results : .issue
            return .issue(id: issueId, face: face)
        }
        return .none
    }

    /// The face an issue inbox row opens on, by its LATEST notification's
    /// type: an agent's message → Results, everything else → Issue.
    public static func issueFace(latestType: String?) -> WorkFaceKind {
        latestType == DomainContract.notificationTypeAgentMessage ? .results : .issue
    }
}
