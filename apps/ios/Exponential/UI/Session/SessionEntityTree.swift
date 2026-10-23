import ExpCore
import Foundation
import GRDB

// EXP-1295: the synced session-tree rows as the ×4 tree rule reads them
// (`Tree*`). ONE mapping each, so every surface (the session view, the ref
// previews, the thread and list screens, the issue's Threads row) derives
// off the same input; the jsonb text columns parse here, once.

extension TreeSession {
    /// A synced `sessions` row, its jsonb columns (`blocked`, `guide`) parsed
    /// tolerantly.
    init(sessionRow row: SessionEntity) {
        self.init(
            id: row.id,
            teamId: row.teamId,
            parentId: row.parentId,
            rootId: row.rootId,
            userId: row.userId,
            status: row.status,
            busy: row.busy,
            needsInput: row.needsInput,
            blocked: TreeSessionBlocked.parse(row.blocked),
            shared: row.shared,
            title: row.title,
            guide: TreeGuideEntry.parseList(row.guide),
            startedAt: row.startedAt,
            endedAt: row.endedAt,
            createdAt: row.createdAt,
            updatedAt: row.updatedAt,
            deviceId: row.deviceId.isEmpty ? nil : row.deviceId,
            agentId: row.agentId,
            appId: row.appId
        )
    }
}

extension TreeSessionIssue {
    init(sessionIssueRow row: SessionIssueEntity) {
        self.init(
            sessionId: row.sessionId,
            issueId: row.issueId,
            sortOrder: row.sortOrder
        )
    }
}

extension TreeIssue {
    init(issueRow row: IssueEntity) {
        self.init(id: row.id, identifier: row.identifier ?? "", title: row.title)
    }
}

extension TreeMessage {
    /// A synced `messages` row keyed to its conversation's session (NEW-8),
    /// its `card` jsonb parsed tolerantly.
    init(messageRow row: SessionMessage) {
        self.init(
            id: row.id,
            sessionId: row.sessionId,
            authorKind: row.authorKind,
            authorId: row.authorId,
            body: row.body,
            card: TreeMessageCard.parse(row.card),
            answeredAt: row.answeredAt,
            consumedAt: row.consumedAt,
            createdAt: row.createdAt,
            authorSessionId: row.authorSessionId
        )
    }
}

extension TreeSessionRead {
    init(sessionReadRow row: SessionReadEntity) {
        self.init(sessionId: row.sessionId, readAt: row.readAt)
    }
}

extension TreePullRequest {
    init(pullRequestRow row: PullRequestEntity) {
        self.init(id: row.id, number: row.number)
    }
}

extension SessionEntity {
    /// The ×4 state (`SessionTree.sessionState`): what a session's card,
    /// dot and mark all read.
    var sessionState: SessionState {
        SessionTree.sessionState(TreeSession(sessionRow: self))
    }

}

extension SessionTree {
    /// THE display title of any session (root, child or agent) off the
    /// synced rows (EXP-1341 `SessionTitle`: name/title → its issues (a
    /// root's links, a child's ASSIGNED ones) → `Run <action>` → its first
    /// prompt → `Untitled`; never `Session N`).
    static func sessionTitle(db: Database, session: SessionEntity) throws -> String {
        let links = try SessionQueries.sessionIssues(db: db, rootId: session.rootId)
        let issues = try SessionQueries.issues(db: db, rootId: session.rootId)
        let messages = try SessionQueries.messages(db: db, sessionId: session.id)
        return SessionTitle.title(
            TreeSession(sessionRow: session),
            sessionIssues: links.map { TreeSessionIssue(sessionIssueRow: $0) },
            issues: issues.map { TreeIssue(issueRow: $0) },
            messages: messages.map { TreeMessage(messageRow: $0) }
        )
    }
}
