import ExpCore
import Foundation
import GRDB

/// EXP-1295: a thread screen's page — the strip's tabs as the phone's
/// pager (Conversation · one per linked issue · Guide; the "+" is a button).
enum ThreadPage: Hashable {
    case conversation
    case issue(String)
    case guide
}

/// EXP-1295: everything ONE root's screen reads, in one snapshot — the root
/// (a thread, or a team agent's channel), its tree, the sessions its cards
/// name outside the tree (the threads an agent started), its issues, its
/// conversation, its pull requests and the people who wrote in it. The pure
/// rules are ExpCore's (`SessionTree.buildSessionThread`, fixture-locked ×4).
struct ThreadRows: Sendable {
    let root: SessionEntity
    /// The tree (root first) plus every session a card names.
    let sessions: [SessionEntity]
    let sessionIssues: [SessionIssueEntity]
    /// The root's linked issues in link order.
    let issues: [IssueEntity]
    let messages: [MessageEntity]
    let pullRequests: [PullRequestEntity]
    /// Per pull request id, the issues it closes (`pull_request_issues`).
    let pullRequestIssueIds: [String: [String]]
    let users: [UserEntity]
    /// The team's pull requests: the stack pool of the Guide's Merge
    /// (`PullRequestStack`, a stack may reach PRs outside this thread).
    var teamPullRequests: [PullRequestEntity] = []

    var isAgent: Bool { root.isAgent }

    func session(id: String) -> SessionEntity? {
        sessions.first { $0.id == id }
    }

    func issue(id: String) -> IssueEntity? {
        issues.first { $0.id == id }
    }

    func user(id: String?) -> UserEntity? {
        guard let id else { return nil }
        return users.first { $0.id == id }
    }

    /// Whether a card's session is ANOTHER root (a thread an agent or a
    /// thread started): its card opens that thread, not the raw session.
    func isOtherRoot(_ sessionId: String) -> Bool {
        guard sessionId != root.id, let session = session(id: sessionId) else { return false }
        return session.parentId == nil
    }

    /// The thread view (fixture `session-thread.json`).
    func view(now: Date) -> SessionThreadView {
        SessionTree.buildSessionThread(
            root: TreeSession(sessionRow: root),
            sessions: sessions.map { TreeSession(sessionRow: $0) },
            sessionIssues: sessionIssues.map { TreeSessionIssue(sessionIssueRow: $0) },
            issues: issues.map { TreeIssue(issueRow: $0) },
            messages: messages.map { TreeMessage(messageRow: $0) },
            pullRequests: pullRequests.map { TreePullRequest(pullRequestRow: $0) },
            now: now
        )
    }

    /// The composer's placeholder: `Message <name>` on an agent channel
    /// (web `agents/$agentId`, Android `ThreadUiCopy.messageAgent`), else the
    /// thread's own.
    func composerPlaceholder(_ view: SessionThreadView) -> String {
        guard isAgent, let name = root.name, !name.isEmpty else { return view.composer.placeholder }
        return "Message \(name)"
    }

    /// The PR the Guide counts and merges: the newest open one, else the
    /// newest merged, else none.
    var guidePullRequest: PullRequestEntity? {
        pullRequests.last { $0.isOpen }
            ?? pullRequests.last { $0.state == DomainContract.prStateMerged }
    }

    /// THE `Merge stack` confirm while the Guide's open PR sits in an open
    /// stack (Android `PullRequestStack.stackConfirm`), else nil.
    var guideStackConfirm: PullRequestStack.Confirm? {
        guard let pullRequest = guidePullRequest, pullRequest.isOpen else { return nil }
        let pool = teamPullRequests + pullRequests.filter { row in !teamPullRequests.contains { $0.id == row.id } }
        return PullRequestStack.stackConfirm(pullRequest, in: pool)
    }

    static func load(db: Database, rootId: String) throws -> ThreadRows? {
        guard let root = try SessionQueries.session(db: db, id: rootId) else { return nil }
        // A child id lands on its root.
        if root.rootId != root.id, !root.rootId.isEmpty {
            return try load(db: db, rootId: root.rootId)
        }
        var sessions = try SessionQueries.tree(db: db, rootId: root.id)
        if !sessions.contains(where: { $0.id == root.id }) { sessions.insert(root, at: 0) }
        let messages = try SessionQueries.messages(db: db, rootId: root.id)
        let known = Set(sessions.map(\.id))
        let named = Set(messages.compactMap { TreeMessageCard.parse($0.card)?.sessionId })
            .subtracting(known)
        if !named.isEmpty {
            sessions += try SessionEntity.filter(Array(named).contains(Column("id"))).fetchAll(db)
        }
        let authorIds = Set(messages.compactMap(\.authorId))
        let users = authorIds.isEmpty
            ? []
            : try UserEntity.filter(Array(authorIds).contains(Column("id"))).fetchAll(db)
        let pullRequests = try SessionQueries.pullRequests(db: db, rootId: root.id)
        var pullRequestIssueIds: [String: [String]] = [:]
        for pullRequest in pullRequests {
            pullRequestIssueIds[pullRequest.id] = try SessionQueries.issueIds(db: db, pullRequestId: pullRequest.id)
        }
        return ThreadRows(
            root: root,
            sessions: sessions,
            sessionIssues: try SessionQueries.sessionIssues(db: db, rootId: root.id),
            issues: try SessionQueries.issues(db: db, rootId: root.id),
            messages: messages,
            pullRequests: pullRequests,
            pullRequestIssueIds: pullRequestIssueIds,
            users: users,
            teamPullRequests: try SessionQueries.pullRequests(db: db, teamId: root.teamId)
        )
    }
}

/// EXP-1284 (review T11/T-N7/T-N9): asks the thread page already on the
/// stack to show one child — the navigator pops back to it instead of
/// pushing a second page of the same thread, then lands it here: a filed
/// issue → the new issue's tab. A reference box like `PushRouteAction` (ONE
/// instance in the navigator's state); `nonce` makes a repeated request for
/// the same page a new one.
@Observable
final class ThreadLanding: @unchecked Sendable {
    struct Request: Equatable {
        let threadId: String
        let page: ThreadPage
        let nonce: UUID
    }

    private(set) var request: Request?

    init() {}

    func land(threadId: String, page: ThreadPage) {
        request = Request(threadId: threadId, page: page, nonce: UUID())
    }
}
