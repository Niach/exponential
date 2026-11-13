import Foundation
import GRDB

// #EXP-1456: where an inbox row (and its push) opens ×4. An issue row opens
// its issue; an issue-less row naming a session opens that session (an
// agent's question or plan, a mention, a wall); #EXP-1534: an
// `agent_signed_out` row opens its device's settings; `sessionTarget` is the S-3
// session-open rule. fixtures/inbox-route.json; mirrors: TS
// `src/inbox-route.ts`, desktop `domain::inbox_route`, Android
// `domain/InboxRoute.kt`.
public enum InboxRoute {
    public enum RowTarget: Equatable, Sendable {
        /// `resultsFace` = an `agent_message` about the issue (EXP-933).
        case issue(id: String, resultsFace: Bool)
        case session(id: String)
        /// #EXP-1534: the Devices page with this device's settings open
        /// (`id` = the devices row id).
        case device(id: String)
        case none
    }

    public enum SessionTarget: Equatable, Sendable {
        /// The agent's page (its channel).
        case agent(agentId: String)
        /// The chat; `sessionId` = the child page it opens on (nil = the
        /// Conversation).
        case chat(chatId: String, sessionId: String?)
        /// The session on its own screen.
        case session(id: String)
    }

    /// The issue-less types an inbox lists.
    static let sessionRowTypes: Set<String> = [
        DomainContract.notificationTypeAgentMessage,
        DomainContract.notificationTypeSessionBlocked,
    ]

    private static func nonEmpty(_ value: String?) -> String? {
        guard let value, !value.isEmpty else { return nil }
        return value
    }

    public static func inboxRowTarget(
        type: String, issueId: String?, sessionId: String?, deviceId: String? = nil
    ) -> RowTarget {
        if let issueId = nonEmpty(issueId) {
            return .issue(id: issueId, resultsFace: type == DomainContract.notificationTypeAgentMessage)
        }
        if sessionRowTypes.contains(type), let sessionId = nonEmpty(sessionId) {
            return .session(id: sessionId)
        }
        if type == DomainContract.notificationTypeAgentSignedOut, let deviceId = nonEmpty(deviceId) {
            return .device(id: deviceId)
        }
        return .none
    }

    /// `agentId` + `userId` are the ROOT's (a child resolves through its root).
    public static func sessionTarget(
        id: String, rootId: String?, agentId: String?, userId: String?, viewerId: String?
    ) -> SessionTarget {
        let root = rootId ?? id
        if let agentId {
            return root == id && userId == viewerId
                ? .agent(agentId: agentId)
                : .session(id: id)
        }
        return .chat(chatId: root, sessionId: root == id ? nil : id)
    }
}

extension InboxRoute {
    /// `sessionTarget` over the local store: the row, then its root's
    /// `agentId` + `userId`. A session this client has not synced opens as a
    /// session (the session route's not-found state), never nothing.
    public static func sessionTarget(db: Database, sessionId: String, viewerId: String?) throws -> SessionTarget {
        guard let session = try SessionQueries.session(db: db, id: sessionId) else {
            return .session(id: sessionId)
        }
        let root = session.rootId == session.id
            ? session
            : try SessionQueries.session(db: db, id: session.rootId)
        guard let root else { return .session(id: sessionId) }
        return sessionTarget(
            id: session.id, rootId: session.rootId,
            agentId: root.agentId, userId: root.userId, viewerId: viewerId
        )
    }

    /// waiting-cards.json DEEP LINKS: where a jump INTO a session's message lands.
    public enum Landing: Equatable, Sendable {
        /// THE plan page of the chat `rootId`, on the plan `messageId`.
        case plan(rootId: String, messageId: String)
        /// The chat `rootId`'s Conversation, the pending deep link on `messageId`.
        case card(rootId: String, messageId: String)
        /// No card to reach: the session by `sessionTarget`.
        case session
    }

    /// An inbox row's or a push's jump into `messageId` (the push's
    /// `messageId`, else the session's waiting card): a plan opens THE plan
    /// page, a question its chat at the card (the chat lands a descendant's
    /// card in that session, `waitingTarget`). `rootId` = the session's root
    /// (synced, else the push's `rootId`); none = the session's own route. A
    /// session opening on its own screen (`.session`) keeps it.
    public static func landing(target: SessionTarget?, rootId: String?, messageId: String?, plan: Bool) -> Landing {
        guard let rootId, !rootId.isEmpty, let messageId, !messageId.isEmpty else { return .session }
        if plan { return .plan(rootId: rootId, messageId: messageId) }
        if case .session = target { return .session }
        return .card(rootId: rootId, messageId: messageId)
    }
}
