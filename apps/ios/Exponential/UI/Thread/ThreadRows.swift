import ExpCore
import ExpUI
import Foundation
import GRDB
import SwiftUI

/// EXP-1295: a thread screen's page — the strip's tabs as the phone's
/// pager (Conversation · one per linked issue, EXP-1334: the folded
/// sub-issues too · Guide; the "+" is a button).
enum ThreadPage: Hashable {
    case conversation
    case issue(String)
    /// EXP-1343: a session chip (or a nested session) as a page — its raw
    /// work + composer under the thread's header.
    case session(String)
    case guide
}

/// VAPP-110: what an app EDITOR puts into the thread screen hosting its
/// chat (`ThreadScreen.editor`): the header title (glyph + name + version
/// pill), the band under it (the Chat | Preview | Versions control) and the
/// pane covering the Conversation (nil = the Chat tab).
struct ThreadEditorSlot {
    let title: AnyView
    let band: AnyView
    let pane: AnyView?
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
    /// NEW-8: keyed through their conversations (`SessionMessage`).
    let messages: [SessionMessage]
    let pullRequests: [PullRequestEntity]
    /// Per pull request id, the issues it closes (`pull_request_issues`).
    let pullRequestIssueIds: [String: [String]]
    let users: [UserEntity]
    /// The team's pull requests: the stack pool of the Guide's Merge
    /// (`PullRequestStack`, a stack may reach PRs outside this thread).
    var teamPullRequests: [PullRequestEntity] = []
    /// EXP-1334: each issue's parent (`issue_relations` kind parent) for the
    /// linked issues AND their ancestors — a sub-issue folds under its
    /// nearest LINKED ancestor, walking past unlinked ones.
    var issueParents: [String: String] = [:]
    /// EXP-1334: the root's device row (the composer's Computer use row
    /// shows only when it advertises the per-run cap).
    var rootDevice: DeviceEntity?
    /// NEW-2: an agent conversation's DEFINITION row (`agents`, the root's
    /// `agent_id`): its name, icon and colour head the screen; nil = a
    /// thread, or a definition not synced yet (the defaults stand in).
    var agent: AgentDefinitionEntity?
    /// VAPP-110: an app EDITOR session's app (`apps`, the root's `app_id`):
    /// its chat is a CHANNEL like an agent's (the app's glyph + name on the
    /// editor's rows); nil = not an editor session, or not synced yet.
    var app: AppEntity?

    /// A CHANNEL root: an agent's conversation, or (VAPP-110) an app's
    /// editor session — the Conversation alone, Slack-like rows.
    var isAgent: Bool { root.isAgent || root.appId != nil }

    /// The channel's name / icon / colour: the app's (an editor session),
    /// the agent definition's, else the defaults.
    var agentName: String { app?.name ?? agent?.name ?? "" }
    var agentIcon: String {
        BoardTypeDisplay.iconName(for: app?.icon ?? agent?.icon) ?? SessionTree.ListCopy.agentDefaultIcon
    }
    var agentColor: String { app?.color ?? agent?.color ?? SessionTree.ListCopy.agentDefaultColor }

    /// #EXP-1380: the per-row lookups, indexed ONCE per load (a row's
    /// session / author / time was a linear scan per row per body pass).
    var sessionsById: [String: SessionEntity] = [:]
    var usersById: [String: UserEntity] = [:]
    var messageCreatedAt: [String: String] = [:]

    /// Fills the indexes (`load` calls it; a hand-built snapshot, as the
    /// styleguide's, gets them through `indexed()`).
    func indexed() -> ThreadRows {
        var copy = self
        copy.sessionsById = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        copy.usersById = Dictionary(users.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        copy.messageCreatedAt = Dictionary(messages.map { ($0.id, $0.createdAt) }, uniquingKeysWith: { first, _ in first })
        return copy
    }

    func session(id: String) -> SessionEntity? {
        sessionsById[id] ?? sessions.first { $0.id == id }
    }

    func issue(id: String) -> IssueEntity? {
        issues.first { $0.id == id }
    }

    func user(id: String?) -> UserEntity? {
        guard let id else { return nil }
        return usersById[id] ?? users.first { $0.id == id }
    }

    /// A message's wire `createdAt` (nil = not in this snapshot).
    func createdAt(messageId: String) -> String? {
        messageCreatedAt[messageId] ?? messages.first { $0.id == messageId }?.createdAt
    }

    /// #EXP-1380 PERF: the clock's tick re-reads ONLY the elapsed of the
    /// cards naming a live session (`SessionTree.sessionCard`): the session
    /// cards and the started items of a suggested tasks card. Every other
    /// row is untouched, and nil = no card moved (the view is not
    /// re-published at all).
    func ticked(_ view: SessionThreadView, now: Date) -> SessionThreadView? {
        var changed = false
        func refreshed(_ card: SessionCard) -> SessionCard? {
            guard let session = session(id: card.sessionId), session.isLive else { return nil }
            let next = SessionTree.sessionCard(TreeSession(sessionRow: session), title: card.name, now: now)
            return next == card ? nil : next
        }
        let rows = view.rows.map { row -> ThreadRow in
            switch row {
            case let .session(id, card):
                guard let next = refreshed(card) else { return row }
                changed = true
                return .session(id: id, card: next)
            case let .suggestions(id, sessionId, header, startAll, items):
                var moved = false
                let nextItems = items.map { item -> SuggestionRow in
                    guard let card = item.card, let next = refreshed(card) else { return item }
                    moved = true
                    return SuggestionRow(
                        id: item.id, title: item.title, text: item.text, state: item.state,
                        card: next, issueId: item.issueId, identifier: item.identifier
                    )
                }
                guard moved else { return row }
                changed = true
                return .suggestions(id: id, sessionId: sessionId, header: header, startAll: startAll, items: nextItems)
            default:
                return row
            }
        }
        return changed ? view.replacingRows(rows) : nil
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
            issues: issues.map { row in
                TreeIssue(id: row.id, identifier: row.identifier ?? "", title: row.title, parentId: issueParents[row.id])
            } + ancestorIssues,
            messages: messages.map { TreeMessage(messageRow: $0) },
            pullRequests: pullRequests.map { TreePullRequest(pullRequestRow: $0) },
            now: now
        )
    }

    /// The unlinked ancestors the fold walks past (id + parent only).
    private var ancestorIssues: [TreeIssue] {
        let linked = Set(issues.map(\.id))
        return Set(issueParents.values).subtracting(linked).sorted().map { id in
            TreeIssue(id: id, identifier: "", title: "", parentId: issueParents[id])
        }
    }

    /// EXP-1334: the parent links above `issueIds`, walked up to a fixed
    /// depth (a cycle stops at the first repeat). Several parents: the
    /// LOWEST parent issue id wins and a self-relation is skipped, as web
    /// `linkedIssueParents` and Android `SessionTree.issueParents` do.
    static func issueParents(db: Database, issueIds: [String]) throws -> [String: String] {
        var parents: [String: String] = [:]
        var seen = Set(issueIds)
        var frontier = issueIds
        for _ in 0..<8 where !frontier.isEmpty {
            let relations = try IssueRelationEntity
                .filter(Column("type") == IssueRelationType.parent.rawValue)
                .filter(frontier.contains(Column("related_issue_id")))
                .filter(Column("issue_id") != Column("related_issue_id"))
                .order(Column("issue_id").asc, Column("id").asc)
                .fetchAll(db)
            frontier = []
            for relation in relations
            where relation.issueId != relation.relatedIssueId && parents[relation.relatedIssueId] == nil {
                parents[relation.relatedIssueId] = relation.issueId
                if seen.insert(relation.issueId).inserted { frontier.append(relation.issueId) }
            }
        }
        return parents
    }

    /// The composer's placeholder: `Message <name>` on an agent channel
    /// (web `agents/$agentId`, Android `ThreadUiCopy.messageAgent`), else the
    /// thread's own.
    func composerPlaceholder(_ view: SessionThreadView) -> String {
        // VAPP-110: an app's editor chat (`copy.composerPlaceholder`).
        if root.appId != nil { return Apps.Copy.composerPlaceholder }
        guard isAgent, !agentName.isEmpty else { return view.composer.placeholder }
        return "Message \(agentName)"
    }

    /// EXP-1306: the stack rail's names — per PR id of the Guide PR's open
    /// stack the issues it closes, and those issues (a member may sit in
    /// another thread, so `pullRequestIssueIds`/`issues` miss it).
    var stackIssueIds: [String: [String]] = [:]
    var stackIssues: [IssueEntity] = []

    /// The PR the Guide counts and merges (`SessionTree.guidePullRequest`,
    /// fixture-locked ×4): the newest open one, else the newest merged, else
    /// none.
    var guidePullRequest: PullRequestEntity? {
        SessionTree.guidePullRequest(pullRequests)
    }

    /// The open-stack walk's pool: the team's PRs plus this thread's.
    private var stackPool: [PullRequestEntity] {
        Self.pool(team: teamPullRequests, thread: pullRequests)
    }

    private static func pool(team: [PullRequestEntity], thread: [PullRequestEntity]) -> [PullRequestEntity] {
        team + thread.filter { row in !team.contains { $0.id == row.id } }
    }

    /// THE `Merge stack` confirm while the Guide's open PR sits in an open
    /// stack (Android `PullRequestStack.stackConfirm`), else nil.
    var guideStackConfirm: PullRequestStack.Confirm? {
        guard let pullRequest = guidePullRequest, pullRequest.isOpen else { return nil }
        return PullRequestStack.stackConfirm(pullRequest, in: stackPool)
    }

    /// EXP-1306: the Guide's stack rail (desktop `thread_screen::stack_card`),
    /// nil unless its open PR sits in a linear open stack of 2+.
    var guideStackView: PullRequestStack.StackView? {
        guard let pullRequest = guidePullRequest else { return nil }
        var issueIds = stackIssueIds
        issueIds.merge(pullRequestIssueIds) { stack, _ in stack }
        return PullRequestStack.stackView(
            pullRequest, in: stackPool, issueIds: issueIds,
            issues: stackIssues + issues.filter { row in !stackIssues.contains { $0.id == row.id } }
        )
    }

    /// A rail member's long press: Merge through here (the stack's ONE
    /// confirm, landing that member and everything beneath it).
    func mergeThroughConfirm(pullRequestId: String) -> PullRequestStack.Confirm? {
        guard let pullRequest = guidePullRequest,
              let member = stackPool.first(where: { $0.id == pullRequestId })
        else { return nil }
        return PullRequestStack.stackConfirm(pullRequest, in: stackPool, through: member)
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
        // #EXP-1380: the team's PRs only feed the Guide PR's stack walk: a
        // thread without a PR reads (and tracks) none of them.
        let teamPullRequests = pullRequests.isEmpty
            ? []
            : try SessionQueries.pullRequests(db: db, teamId: root.teamId)
        // The stack rail names every member by its issue: read the links of
        // the Guide PR's open stack only, never the whole team's.
        var stackIssueIds: [String: [String]] = [:]
        var stackIssues: [IssueEntity] = []
        if let guide = SessionTree.guidePullRequest(pullRequests),
           let chain = PullRequestStack.openChain(guide, in: pool(team: teamPullRequests, thread: pullRequests)),
           chain.count >= 2 {
            for member in chain {
                stackIssueIds[member.id] = try SessionQueries.issueIds(db: db, pullRequestId: member.id)
            }
            let ids = Array(Set(stackIssueIds.values.joined()))
            if !ids.isEmpty {
                stackIssues = try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db)
            }
        }
        let issues = try SessionQueries.issues(db: db, rootId: root.id)
        return ThreadRows(
            root: root,
            sessions: sessions,
            sessionIssues: try SessionQueries.sessionIssues(db: db, rootId: root.id),
            issues: issues,
            messages: messages,
            pullRequests: pullRequests,
            pullRequestIssueIds: pullRequestIssueIds,
            users: users,
            teamPullRequests: teamPullRequests,
            issueParents: try issueParents(db: db, issueIds: issues.map(\.id)),
            rootDevice: try DeviceEntity.filter(Column("device_id") == root.deviceId).fetchOne(db),
            agent: try root.agentId.flatMap { try AgentDefinitionEntity.fetchOne(db, key: $0) },
            app: try root.appId.flatMap { try AppEntity.fetchOne(db, key: $0) },
            stackIssueIds: stackIssueIds,
            stackIssues: stackIssues
        ).indexed()
    }

    /// #EXP-1380: what the screen's observation tracks instead of the whole
    /// snapshot — one cheap digest per input of `load`, scoped to THIS root
    /// (its tree and the sessions its cards name, its keyed messages and
    /// their authors, its issue links, issues and parent relations, its PRs
    /// and, only while it has one, the team's PR pool, its device, agent and
    /// app). A write anywhere else in the database (another chat's
    /// messages, another team's sessions) leaves the digest equal, so the
    /// snapshot is neither re-read nor re-built.
    static func signature(db: Database, rootId: String) throws -> String? {
        guard let root = try SessionQueries.session(db: db, id: rootId) else { return nil }
        if root.rootId != root.id, !root.rootId.isEmpty {
            return try signature(db: db, rootId: root.rootId)
        }
        let id = root.id
        let named = """
            SELECT json_extract("card", '$.sessionId') FROM "messages"
            WHERE "root_id" = ?1 AND "card" IS NOT NULL AND json_valid("card")
            """
        var parts: [String] = [
            try digest(db, table: "sessions", where: "\"root_id\" = ?1 OR \"id\" = ?1 OR \"id\" IN (\(named))", id),
            try String.fetchOne(db, sql: """
                SELECT count(*) || ':' || ifnull(max("m"."updated_at"), '') || ':' || ifnull(max("m"."created_at"), '')
                    || ':' || ifnull(sum(length("m"."body")), 0) || ':' || ifnull(sum(length("m"."card")), 0)
                    || ':' || count("m"."answered_at") || ':' || count("m"."consumed_at")
                    || ':' || ifnull(group_concat(DISTINCT "c"."session_id"), '')
                FROM "messages" "m" JOIN "conversations" "c" ON "c"."id" = "m"."conversation_id"
                WHERE "m"."root_id" = ?1 AND "c"."session_id" IS NOT NULL AND "c"."session_id" <> ''
                """, arguments: [id]) ?? "",
            try stamp(db, table: "users", where: "\"id\" IN (SELECT \"author_id\" FROM \"messages\" WHERE \"root_id\" = ?1)", id),
            try digest(db, table: "session_issues", where: "\"session_id\" = ?1", id),
            try stamp(db, table: "issues", where: "\"id\" IN (SELECT \"issue_id\" FROM \"session_issues\" WHERE \"session_id\" = ?1)", id),
            try stamp(db, table: "issue_relations", where: "\"type\" = 'parent'", nil),
            try stamp(db, table: "pull_requests", where: "\"root_session_id\" = ?1", id),
            try digest(db, table: "devices", where: "\"device_id\" = ?1", root.deviceId),
        ]
        if let agentId = root.agentId { parts.append(try stamp(db, table: "agents", where: "\"id\" = ?1", agentId)) }
        if let appId = root.appId { parts.append(try stamp(db, table: "apps", where: "\"id\" = ?1", appId)) }
        if try Bool.fetchOne(db, sql: "SELECT EXISTS (SELECT 1 FROM \"pull_requests\" WHERE \"root_session_id\" = ?)", arguments: [id]) == true {
            // The Guide's stack walk: the team's PRs, their links and issues.
            let teamPrs = "SELECT \"id\" FROM \"pull_requests\" WHERE \"team_id\" = ?1"
            parts.append(try stamp(db, table: "pull_requests", where: "\"team_id\" = ?1", root.teamId))
            parts.append(try stamp(db, table: "pull_request_issues", where: "\"pull_request_id\" IN (\(teamPrs))", root.teamId))
            parts.append(try stamp(db, table: "issues", where: "\"id\" IN (SELECT \"issue_id\" FROM \"pull_request_issues\" WHERE \"pull_request_id\" IN (\(teamPrs)))", root.teamId))
        }
        return parts.joined(separator: "|")
    }

    /// Count + newest `updated_at` of the matching rows (tables that stamp
    /// every write).
    private static func stamp(_ db: Database, table: String, where clause: String, _ argument: String?) throws -> String {
        try String.fetchOne(db, sql: """
            SELECT count(*) || ':' || ifnull(max("updated_at"), '') FROM "\(table)" WHERE \(clause)
            """, arguments: StatementArguments(argument.map { [$0] } ?? [])) ?? ""
    }

    /// Every column of the matching (few) rows, quoted: for tables without
    /// an `updated_at` stamp (`devices`) and for the tree, whose live flags
    /// must never be missed.
    private static func digest(_ db: Database, table: String, where clause: String, _ argument: String) throws -> String {
        let columns = try db.columns(in: table).map { "quote(\"\($0.name)\")" }.joined(separator: " || ',' || ")
        return try String.fetchOne(db, sql: """
            SELECT count(*) || ':' || ifnull(group_concat(\(columns), ';'), '')
            FROM (SELECT * FROM "\(table)" WHERE \(clause) ORDER BY rowid)
            """, arguments: [argument]) ?? ""
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

// MARK: - EXP-1347 chip navigation = the INNERMOST outlet

/// Inside a thread (its Conversation, the ask dock, an embedded session
/// page) an issue pill whose issue is one of THAT thread's issue tabs
/// selects the tab; `open` answers whether the thread took the issue.
/// A reference box like `PushRouteAction` (a bare closure cannot be an
/// `EnvironmentKey.defaultValue` under strict concurrency); the thread
/// screen holds ONE in `@State` and points it at its pages.
final class ThreadIssueOutlet: @unchecked Sendable {
    private var handler: ((String) -> Bool)?

    init() {}

    func setHandler(_ handler: @escaping (String) -> Bool) {
        self.handler = handler
    }

    func open(_ issueId: String) -> Bool {
        handler?(issueId) ?? false
    }
}

private struct ThreadIssueOutletKey: EnvironmentKey {
    static let defaultValue: ThreadIssueOutlet? = nil
}

extension EnvironmentValues {
    var threadIssueOutlet: ThreadIssueOutlet? {
        get { self[ThreadIssueOutletKey.self] }
        set { self[ThreadIssueOutletKey.self] = newValue }
    }
}

/// An issue pill's tap: the enclosing thread's tab when it holds the issue,
/// else the app's push (`deepLinkBus.navigateToIssue`).
func openIssueInnermost(
    _ issueId: String, outlet: ThreadIssueOutlet?, deps: AppDependencies, accountId: String
) {
    if outlet?.open(issueId) == true { return }
    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
}
