import Foundation
import GRDB

// #EXP-1456: where an inbox row (and its push) opens ×4. An issue row opens
// its issue; an issue-less row naming a session opens that session (an
// agent's question or plan, a mention, a wall); `sessionTarget` is the S-3
// session-open rule. fixtures/inbox-route.json; mirrors: TS
// `src/inbox-route.ts`, desktop `domain::inbox_route`, Android
// `domain/InboxRoute.kt`.
public enum InboxRoute {
    public enum RowTarget: Equatable, Sendable {
        /// `resultsFace` = an `agent_message` about the issue (EXP-933).
        case issue(id: String, resultsFace: Bool)
        case session(id: String)
        case none
    }

    public enum SessionTarget: Equatable, Sendable {
        /// The agent's page (its channel).
        case agent(agentId: String)
        /// The thread; `sessionId` = the child page it opens on (nil = the
        /// Conversation).
        case thread(threadId: String, sessionId: String?)
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

    public static func inboxRowTarget(type: String, issueId: String?, sessionId: String?) -> RowTarget {
        if let issueId = nonEmpty(issueId) {
            return .issue(id: issueId, resultsFace: type == DomainContract.notificationTypeAgentMessage)
        }
        if sessionRowTypes.contains(type), let sessionId = nonEmpty(sessionId) {
            return .session(id: sessionId)
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
        return .thread(threadId: root, sessionId: root == id ? nil : id)
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
}
