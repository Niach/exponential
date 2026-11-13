import Foundation

// EXP-933: where a notification TAP lands — the push payload and the inbox
// row read the same rules. An `agent_message` that names an issue opens that
// issue; an issue-less one stays an inbox row (EXP-801). A blocked session
// opens that session; #EXP-1456: so does any issue-less push naming one.

public enum NotificationRouting {
    /// Where a push tap goes.
    public enum PushTarget: Equatable, Sendable {
        /// A session (EXP-980 wall, #EXP-1456 any push carrying `sessionId`);
        /// `card` = the question / plan it is about (waiting-cards.json
        /// DEEP LINKS).
        case session(id: String, card: Card? = nil)
        /// My Work → Inbox (an issue-less agent message, or a blocked session
        /// whose row was pruned).
        case inbox
        case issue(id: String)
        /// #EXP-1534: a device's settings (an `agent_signed_out` push;
        /// `id` = the devices row id).
        case device(id: String)
        /// Nothing to open.
        case none
    }

    /// waiting-cards.json DEEP LINKS: the card a session push is about (its
    /// `messageId`), the tree's `rootId`; `plan` = a plan card (its page).
    public struct Card: Equatable, Sendable {
        public let messageId: String
        public let rootId: String?
        public let plan: Bool

        public init(messageId: String, rootId: String?, plan: Bool) {
            self.messageId = messageId
            self.rootId = rootId
            self.plan = plan
        }
    }

    /// The push data's card (notification-actions.json `dataKeys` +
    /// question-push.json `rootId`), else nil.
    public static func card(_ userInfo: [AnyHashable: Any]) -> Card? {
        guard let messageId = nonEmpty(userInfo["messageId"]) else { return nil }
        return Card(
            messageId: messageId,
            rootId: nonEmpty(userInfo["rootId"]),
            plan: userInfo["cardKind"] as? String == "plan"
        )
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
        if let issueId = nonEmpty(userInfo["issueId"]) { return .issue(id: issueId) }
        // #EXP-1534 (inbox-route.json): a signed-out login opens that
        // device's settings; a removed device (no `deviceId`) opens nothing.
        if type == DomainContract.notificationTypeAgentSignedOut {
            guard let deviceId = nonEmpty(userInfo["deviceId"]) else { return .none }
            return .device(id: deviceId)
        }
        // #EXP-1456 (inbox-route.json): a push naming a session opens it
        // whatever its type (an agent's decision, a mention, a wall); the
        // navigator resolves where (`InboxRoute.sessionTarget`).
        if let sessionId = nonEmpty(userInfo["sessionId"]) { return .session(id: sessionId, card: card(userInfo)) }
        if type == DomainContract.notificationTypeSessionBlocked { return .inbox }
        // An issue-less agent message renders in the inbox alone.
        if type == DomainContract.notificationTypeAgentMessage { return .inbox }
        return .none
    }
}
