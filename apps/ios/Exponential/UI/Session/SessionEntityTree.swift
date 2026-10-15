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
            kind: row.kind,
            userId: row.userId,
            status: row.status,
            busy: row.busy,
            needsInput: row.needsInput,
            blocked: TreeSessionBlocked.parse(row.blocked),
            shared: row.shared,
            title: row.title,
            guide: TreeGuideEntry.parseList(row.guide),
            actionName: row.actionName,
            name: row.name,
            icon: row.icon,
            color: row.color,
            sortOrder: row.sortOrder,
            startedAt: row.startedAt,
            endedAt: row.endedAt,
            createdAt: row.createdAt,
            updatedAt: row.updatedAt,
            deviceId: row.deviceId.isEmpty ? nil : row.deviceId
        )
    }
}

extension TreeSessionIssue {
    init(sessionIssueRow row: SessionIssueEntity) {
        self.init(sessionId: row.sessionId, issueId: row.issueId, sortOrder: row.sortOrder)
    }
}

extension TreeIssue {
    init(issueRow row: IssueEntity) {
        self.init(id: row.id, identifier: row.identifier ?? "", title: row.title)
    }
}

extension TreeMessage {
    /// A synced `messages` row, its `card` jsonb parsed tolerantly.
    init(messageRow row: MessageEntity) {
        self.init(
            id: row.id,
            sessionId: row.sessionId,
            authorKind: row.authorKind,
            authorId: row.authorId,
            body: row.body,
            card: TreeMessageCard.parse(row.card),
            answeredAt: row.answeredAt,
            consumedAt: row.consumedAt,
            createdAt: row.createdAt
        )
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

    /// The session's name as its card reads it: its own title, else
    /// `Session N` by its place in the tree (`SessionTree.sessionCard`).
    /// `tree` = every row of its tree (any order); empty = `Session`.
    func cardName(in tree: [SessionEntity], now: Date = Date()) -> String {
        let ordered = SessionTree.treeOrder(tree.map { TreeSession(sessionRow: $0) })
        guard let index = ordered.firstIndex(where: { $0.id == id }) else {
            let own = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            return own.isEmpty ? SessionTree.CardCopy.session : own
        }
        return SessionTree.sessionCard(ordered[index], ordinal: index + 1, now: now).name
    }
}

extension SessionTree {
    /// A ROOT row's thread title off the synced rows (`threadTitle`: agent name,
    /// action, issues, stored, first request line, `Untitled`).
    static func threadTitle(db: Database, root: SessionEntity) throws -> String {
        let issues = try SessionQueries.issues(db: db, rootId: root.id)
        let messages = try SessionQueries.messages(db: db, sessionId: root.id)
        return threadTitle(
            root: TreeSession(sessionRow: root),
            issues: issues.map { TreeIssue(issueRow: $0) },
            messages: messages.map { TreeMessage(messageRow: $0) }
        )
    }
}
