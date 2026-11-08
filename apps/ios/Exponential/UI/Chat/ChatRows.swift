import ExpCore
import ExpUI
import Foundation
import GRDB
import SwiftUI

/// #EXP-1732: a WAY INTO a session screen (a route's `child`, a landing):
/// a tab (`conversation` = Chat, `results`) or an item opened over its tab
/// (an issue: Issues, a session: Sessions, a plan over Chat), or a waiting
/// card the Chat scrolls to.
enum ChatPage: Hashable {
    case conversation
    /// #EXP-1662 (plan-page.json): the plan page as an opened item; nil =
    /// `PlanPage.planTabTarget`.
    case plan(String?)
    /// results-tab.json: the Results tab.
    case results
    /// #FEED-191 (needs-you.json): the Chat, scrolled to this waiting card.
    case card(String)
    /// The Issues tab with that issue opened in place.
    case issue(String)
    /// The Sessions tab with that session opened in place (its raw work +
    /// composer).
    case session(String)
}

/// #EXP-1732: what an app EDITOR puts into THE session screen hosting its
/// editor session (`ChatScreen.editor`, kind `app`): the header's lead and
/// title (glyph + name + version pill over #EXP-1729's `status · device`),
/// the active tab (the editor drives
/// it: a screen link, a Versions row's Preview), the Preview / Data /
/// Versions bodies, the gear's settings page and the header's `…` (Export,
/// Delete).
struct ChatEditorSlot {
    /// The title block over the TREE's status word (`ChatHeaderTitle.status`).
    let title: (String) -> AnyView
    let tab: Binding<SessionScreen.Tab>
    /// The app tab's body (preview, data, versions); nil = not the editor's.
    let pane: (SessionScreen.Tab) -> AnyView?
    /// The gear's opened item: the app's settings page.
    let settings: AnyView
    var trailing: AnyView? = nil
    /// #FEED-204: the composer's Device row and an ended root's move.
    var device: SessionComposerFooter.DeviceMenu? = nil
    var appMove: ChatComposerDraft.AppMove? = nil
    /// #EXP-1729: the composer's Account and Model rows.
    var picks: ChatEditorPicks? = nil
}

/// #EXP-1729 (composer-menu.json `account`/`model`, an app editor's
/// composer, always): THE launch composer's Account and Model rows while the
/// root is ended (a pick rides the next send); a live root reads its own
/// (the session switches), `liveAccount` standing in when its own switch
/// offers no row.
struct ChatEditorPicks {
    let account: SessionComposerFooter.AccountMenu
    let agent: String
    let model: String
    let onPickModel: (String) -> Void
    var liveAccount: SessionComposerFooter.AccountMenu?
}

/// EXP-1295: everything ONE root's screen reads, in one snapshot — the root
/// (a chat, or a team agent's channel), its tree, the sessions its cards
/// name outside the tree (the chats an agent started), its issues, its
/// conversation, its pull requests and the people who wrote in it. The pure
/// rules are ExpCore's (`SessionTree.buildSessionChat`, fixture-locked ×4).
struct ChatRows: Sendable {
    let root: SessionEntity
    /// The tree (root first) plus every session a card names.
    let sessions: [SessionEntity]
    let sessionIssues: [SessionIssueEntity]
    /// The root's linked issues in link order.
    let issues: [IssueEntity]
    /// #EXP-1730: keyed by `messages.session_id` (`SessionMessage`).
    let messages: [SessionMessage]
    let pullRequests: [PullRequestEntity]
    /// Per pull request id, the issues it closes (`pull_request_issues`).
    let pullRequestIssueIds: [String: [String]]
    let users: [UserEntity]
    /// The team's pull requests: the stack pool of the Guide's Merge
    /// (`PullRequestStack`, a stack may reach PRs outside this chat).
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
    /// chat, or a definition not synced yet (the defaults stand in).
    var agent: AgentDefinitionEntity?
    /// VAPP-110: an app EDITOR session's app (`apps`, the root's `app_id`):
    /// its chat is a CHANNEL like an agent's (the app's glyph + name on the
    /// editor's rows); nil = not an editor session, or not synced yet.
    var app: AppEntity?
    /// S-3: the root team's statuses: an issue chip's glyph resolves its
    /// `statusId` through them (the anchor only as their fallback).
    var issueStatuses: [ResolvedIssueStatus] = IssueStatusResolver.builtinFallbackTeam
    /// #EXP-1464 (chat-row-action.json): the question / plan messages of
    /// the sessions a card names OUTSIDE this tree — a Needs you card
    /// expands into its session's waiting card (`sessionWaitingCard`).
    /// Never rows of their own.
    var namedCards: [SessionMessage] = []
    /// #EXP-1611: the Guide PR's CI rows (`pull_request_checks`, synced):
    /// the header `…`'s Checks submenu and the failing Merge confirm.
    var guideChecks: [PrChecks.Check] = []

    /// A conversation WITH someone: an agent's (its header names the agent,
    /// its `…` is the agent menu), or (VAPP-110) an app's editor session.
    var isAgent: Bool { root.isAgent || root.appId != nil }
    /// S-3 CHAT MODE (session-chat.json `chatModeCases`): an agent
    /// conversation is `bubbles`, a chat `transcript`; an app editor's chat
    /// (VAPP-110) is a conversation with its app, so bubbles too.
    var chatMode: SessionTree.ChatMode {
        root.appId != nil ? .bubbles : SessionTree.chatMode(agentId: root.agentId)
    }

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
    func indexed() -> ChatRows {
        var copy = self
        copy.sessionsById = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        copy.usersById = Dictionary(users.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        copy.messageCreatedAt = Dictionary(messages.map { ($0.id, $0.createdAt) }, uniquingKeysWith: { first, _ in first })
        return copy
    }

    /// #EXP-1736 (transcript-grouping.json): the grouping's input over
    /// this snapshot's people, agents and sessions; `chat` = the session
    /// whose transcript it is (the root, or a session's raw view);
    /// `originSessionId` = its S-3 origin from the synced rows, `fromStart`
    /// = the items begin at the conversation's first message (ONE origin
    /// rule: only then does the first message read as the origin's).
    func transcriptInput(
        items: [TranscriptGrouping.Item],
        viewerId: String?,
        chat: SessionEntity? = nil,
        originSessionId: String?,
        fromStart: Bool,
        extraSessions: [TranscriptGrouping.KnownSession] = []
    ) -> TranscriptGrouping.Input {
        let chat = chat ?? root
        let isRoot = chat.id == root.id
        let title = isRoot && !agentName.isEmpty ? agentName : (chat.title ?? SessionTitle.Copy.untitled)
        let known = sessions.map { session in
            TranscriptGrouping.KnownSession(
                id: session.id,
                title: session.title ?? SessionTitle.Copy.untitled,
                agentId: session.agentId
            )
        }
        return TranscriptGrouping.Input(
            viewerId: viewerId,
            chat: TranscriptGrouping.Chat(
                sessionId: chat.id,
                title: title,
                agentId: chat.agentId,
                originSessionId: originSessionId,
                fromStart: fromStart
            ),
            members: users.map { TranscriptGrouping.Named(id: $0.id, name: $0.name ?? $0.email) },
            agents: (xrefAgents + [agent].compactMap { $0 }).map { TranscriptGrouping.Named(id: $0.id, name: $0.name) },
            sessions: known + extraSessions.filter { extra in !known.contains { $0.id == extra.id } },
            items: items
        )
    }

    /// #EXP-1736: a Conversation row as a grouping item: a card-less
    /// message = `message`, a child's report = a message from its session,
    /// a subagent line = `trace`, every card `full`.
    static func transcriptItem(_ row: SessionChatRow) -> TranscriptGrouping.Item {
        switch row {
        case let .message(id, author, authorId, _, _, authorSessionId, _):
            TranscriptGrouping.Item(
                id: id, kind: .message, authorKind: author, authorId: authorId, authorSessionId: authorSessionId
            )
        case let .report(id, sessionId, _, _, _):
            TranscriptGrouping.Item(id: id, kind: .message, authorKind: DomainContract.messageAuthorKindAgent, authorSessionId: sessionId)
        case let .subagent(id, _, _):
            TranscriptGrouping.Item(id: id, kind: .trace)
        default:
            TranscriptGrouping.Item(id: row.id, kind: .full)
        }
    }

    /// #EXP-1736: a sender's avatar: a member's (their picture or
    /// initials in hue), an agent's mark (the defaults while its definition
    /// is not synced), a session's glyph.
    func senderAvatar(_ sender: TranscriptGrouping.Sender) -> SenderAvatarModel {
        switch sender.kind {
        case .member:
            return .member(user: user(id: sender.id), id: sender.id)
        case .agent:
            let definition = xrefAgents.first { $0.id == sender.id } ?? (agent?.id == sender.id ? agent : nil)
            if let definition { return .agent(UserAvatarAgent(definition)) }
            return .agent(UserAvatarAgent(icon: nil, color: nil))
        case .session:
            // VAPP-110: an app editor's own sender wears the app's mark.
            if sender.id == root.id, let app { return .agent(UserAvatarAgent(icon: app.icon, color: app.color)) }
            return .session
        }
    }

    /// S-3 / #EXP-1736: a cross-session sender's press PUSHES its session
    /// (Back returns): a root opens as its chat, a child as its raw work.
    func sessionRoute(_ sessionId: String, accountId: String) -> AppRoute {
        if let target = session(id: sessionId), target.parentId == nil {
            return .chat(accountId: accountId, chatId: target.id)
        }
        return .agentSession(accountId: accountId, sessionId: sessionId)
    }

    func session(id: String) -> SessionEntity? {
        sessionsById[id] ?? sessions.first { $0.id == id }
    }

    func issue(id: String) -> IssueEntity? {
        issues.first { $0.id == id }
    }

    /// S-3: an issue chip's status through the team's statuses (statusId
    /// first, like Android's `IssueStatusResolver`); nil = the issue is not
    /// in this snapshot, which draws NO glyph (never Backlog).
    func issueStatus(id: String) -> ResolvedIssueStatus? {
        issue(id: id).map { IssueStatusResolver.resolve($0, team: issueStatuses) }
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
    func ticked(_ view: SessionChatView, now: Date) -> SessionChatView? {
        var changed = false
        func refreshed(_ card: SessionCard) -> SessionCard? {
            guard let session = session(id: card.sessionId), session.isLive else { return nil }
            let next = SessionTree.sessionCard(TreeSession(sessionRow: session), title: card.name, now: now)
            return next == card ? nil : next
        }
        let rows = view.rows.map { row -> SessionChatRow in
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

    /// Whether a card's session is ANOTHER root (a chat an agent or a
    /// chat started): its card opens that chat, not the raw session.
    func isOtherRoot(_ sessionId: String) -> Bool {
        guard sessionId != root.id, let session = session(id: sessionId) else { return false }
        return session.parentId == nil
    }

    /// The chat view (fixture `session-chat.json`).
    func view(now: Date, focus: String? = nil) -> SessionChatView {
        SessionTree.buildSessionChat(
            root: TreeSession(sessionRow: root),
            sessions: sessions.map { TreeSession(sessionRow: $0) },
            sessionIssues: sessionIssues.map { TreeSessionIssue(sessionIssueRow: $0) },
            issues: issues.map { row in
                TreeIssue(id: row.id, identifier: row.identifier ?? "", title: row.title, parentId: issueParents[row.id])
            } + ancestorIssues,
            messages: messages.map { TreeMessage(messageRow: $0) },
            pullRequests: pullRequests.map { TreePullRequest(pullRequestRow: $0) },
            now: now,
            focus: focus
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

    /// The composer's placeholder: the chat's own (`Type / for commands`),
    /// an agent channel's conversation included — web's chat screen reads
    /// `view.composer.placeholder` there too (EXP-1649); `Message <name>`
    /// stays the EMPTY channel's (no conversation yet).
    func composerPlaceholder(_ view: SessionChatView) -> String {
        // VAPP-110: an app's editor chat (`copy.composerPlaceholder`).
        if root.appId != nil { return Apps.Copy.composerPlaceholder }
        return view.composer.placeholder
    }

    /// EXP-1306: the stack rail's names — per PR id of the Guide PR's open
    /// stack the issues it closes, and those issues (a member may sit in
    /// another chat, so `pullRequestIssueIds`/`issues` miss it).
    var stackIssueIds: [String: [String]] = [:]
    var stackIssues: [IssueEntity] = []
    /// The agents the tree's sessions name (`agentId`, `startedByAgentId`):
    /// the cross-session senders' names and marks.
    var xrefAgents: [AgentDefinitionEntity] = []

    /// The PR the Guide counts and merges (`SessionTree.guidePullRequest`,
    /// fixture-locked ×4): the newest open one, else the newest merged, else
    /// none.
    var guidePullRequest: PullRequestEntity? {
        SessionTree.guidePullRequest(pullRequests)
    }

    /// The open-stack walk's pool: the team's PRs plus this chat's.
    private var stackPool: [PullRequestEntity] {
        Self.pool(team: teamPullRequests, chat: pullRequests)
    }

    private static func pool(team: [PullRequestEntity], chat: [PullRequestEntity]) -> [PullRequestEntity] {
        team + chat.filter { row in !team.contains { $0.id == row.id } }
    }

    /// THE `Merge stack` confirm while the Guide's open PR sits in an open
    /// stack (Android `PullRequestStack.stackConfirm`), else nil.
    var guideStackConfirm: PullRequestStack.Confirm? {
        guard let pullRequest = guidePullRequest, pullRequest.isOpen else { return nil }
        return PullRequestStack.stackConfirm(pullRequest, in: stackPool)
    }

    /// #EXP-1710: the `…` menu's Merge now alert over the Guide's open PR
    /// (THE rule the Needs you stuck row shares: `titleStack` in an open stack).
    var guideMergeNowTarget: MergeNowTarget? {
        guard let pullRequest = guidePullRequest, pullRequest.isOpen else { return nil }
        return MergeNowTarget(pullRequest, pool: stackPool)
    }

    /// #EXP-1710 `Testing with N`: the synced PRs sharing the Guide PR's
    /// testing run (`merge_train_id`), itself included; nil = no run.
    var guideBatchSize: Int? {
        guard let pullRequest = guidePullRequest, let train = pullRequest.mergeTrainId, !train.isEmpty else { return nil }
        return max(stackPool.filter { $0.mergeTrainId == train }.count, 1)
    }

    /// EXP-1306: the Guide's stack rail (desktop `chat_screen::stack_card`),
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

    static func load(db: Database, rootId: String) throws -> ChatRows? {
        guard let root = try SessionQueries.session(db: db, id: rootId) else { return nil }
        // A child id lands on its root.
        if root.rootId != root.id, !root.rootId.isEmpty {
            return try load(db: db, rootId: root.rootId)
        }
        var sessions = try SessionQueries.tree(db: db, rootId: root.id)
        if !sessions.contains(where: { $0.id == root.id }) { sessions.insert(root, at: 0) }
        var messages = try SessionQueries.messages(db: db, rootId: root.id)
        // #EXP-1399: an `ask` mirror from ANOTHER tree draws its ORIGIN card
        // (session-chat.json), which lives outside this root: load those
        // origins by id (an unsynced one stays out, and its mirror draws
        // nothing).
        let loaded = Set(messages.map(\.id))
        let origins = Set(messages.compactMap { message -> String? in
            guard let card = TreeMessageCard.parse(message.card), card.kind == "ask" else { return nil }
            return card.messageId
        }).subtracting(loaded)
        if !origins.isEmpty {
            messages += try SessionQueries.messages(db: db, ids: origins.sorted())
        }
        // S-3: the sessions a row links to (cards, other sessions' turns)
        // and the roots of the foreign ones (their agent's mark).
        let named = Set(messages.compactMap { TreeMessageCard.parse($0.card)?.sessionId })
            .union(messages.compactMap(\.authorSessionId).filter { !$0.isEmpty })
            .subtracting(sessions.map(\.id))
        if !named.isEmpty {
            sessions += try SessionEntity.filter(Array(named).contains(Column("id"))).fetchAll(db)
        }
        let namedCards = try named.isEmpty ? [] : SessionQueries.cardMessages(db: db, sessionIds: named.sorted())
        let foreignRoots = Set(sessions.map(\.rootId).filter { !$0.isEmpty }).subtracting(sessions.map(\.id))
        if !foreignRoots.isEmpty {
            sessions += try SessionEntity.filter(Array(foreignRoots).contains(Column("id"))).fetchAll(db)
        }
        // S-3: the root's `origin` = the agent conversation that started it.
        if let agentId = root.startedByAgentId, !agentId.isEmpty,
           let conversation = try SessionEntity
               .filter(Column("agent_id") == agentId && Column("user_id") == root.userId && Column("id") == Column("root_id"))
               .fetchOne(db),
           !sessions.contains(where: { $0.id == conversation.id }) {
            sessions.append(conversation)
        }
        let xrefAgentIds = Set(sessions.flatMap { [$0.agentId, $0.startedByAgentId] }.compactMap { $0 }.filter { !$0.isEmpty })
        let xrefAgents = xrefAgentIds.isEmpty
            ? []
            : try AgentDefinitionEntity.filter(Array(xrefAgentIds).contains(Column("id"))).fetchAll(db)
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
        // chat without a PR reads (and tracks) none of them.
        let teamPullRequests = pullRequests.isEmpty
            ? []
            : try SessionQueries.pullRequests(db: db, teamId: root.teamId)
        // The stack rail names every member by its issue: read the links of
        // the Guide PR's open stack only, never the whole team's.
        var stackIssueIds: [String: [String]] = [:]
        var stackIssues: [IssueEntity] = []
        if let guide = SessionTree.guidePullRequest(pullRequests),
           let chain = PullRequestStack.openChain(guide, in: pool(team: teamPullRequests, chat: pullRequests)),
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
        let guideChecks = try SessionTree.guidePullRequest(pullRequests).map { guide in
            try PullRequestCheckEntity.filter(Column("pull_request_id") == guide.id).fetchAll(db).map(PrChecks.Check.init)
        } ?? []
        return ChatRows(
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
            issueStatuses: IssueStatusResolver.teamStatusesOrFallback(
                try IssueStatusEntity.filter(Column("team_id") == root.teamId).fetchAll(db)
            ),
            namedCards: namedCards,
            guideChecks: guideChecks,
            stackIssueIds: stackIssueIds,
            stackIssues: stackIssues,
            xrefAgents: xrefAgents
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
        // S-3: the sessions other sessions' turns link to.
        let authors = """
            SELECT "author_session_id" FROM "messages" WHERE "root_id" = ?1 AND "author_session_id" IS NOT NULL
            """
        var parts: [String] = [
            try digest(db, table: "sessions", where: "\"root_id\" = ?1 OR \"id\" = ?1 OR \"id\" IN (\(named)) OR \"id\" IN (\(authors))", id),
            try String.fetchOne(db, sql: """
                SELECT count(*) || ':' || ifnull(max("m"."updated_at"), '') || ':' || ifnull(max("m"."created_at"), '')
                    || ':' || ifnull(sum(length("m"."body")), 0) || ':' || ifnull(sum(length("m"."card")), 0)
                    || ':' || count("m"."answered_at") || ':' || count("m"."consumed_at")
                    || ':' || ifnull(group_concat(DISTINCT "m"."session_id"), '')
                FROM "messages" "m"
                WHERE "m"."root_id" = ?1 AND "m"."session_id" IS NOT NULL AND "m"."session_id" <> ''
                """, arguments: [id]) ?? "",
            try stamp(db, table: "users", where: "\"id\" IN (SELECT \"author_id\" FROM \"messages\" WHERE \"root_id\" = ?1)", id),
            try digest(db, table: "session_issues", where: "\"session_id\" = ?1", id),
            try stamp(db, table: "issues", where: "\"id\" IN (SELECT \"issue_id\" FROM \"session_issues\" WHERE \"session_id\" = ?1)", id),
            try stamp(db, table: "issue_relations", where: "\"type\" = 'parent'", nil),
            try stamp(db, table: "issue_statuses", where: "\"team_id\" = ?1", root.teamId),
            // #EXP-1710: every column (the train's state, position and run).
            try digest(db, table: "pull_requests", where: "\"root_session_id\" = ?1", id),
            // #EXP-1611: the chat PRs' CI rows (the Checks submenu).
            try stamp(db, table: "pull_request_checks", where: "\"pull_request_id\" IN (SELECT \"id\" FROM \"pull_requests\" WHERE \"root_session_id\" = ?1)", id),
            try digest(db, table: "devices", where: "\"device_id\" = ?1", root.deviceId),
        ]
        if let agentId = root.startedByAgentId, !agentId.isEmpty {
            parts.append(try stamp(db, table: "sessions", where: "\"agent_id\" = ?1 AND \"id\" = \"root_id\"", agentId))
        }
        // #EXP-1464: the question / plan cards of the sessions cards name.
        parts.append(try String.fetchOne(db, sql: """
            SELECT count(*) || ':' || ifnull(max("m"."updated_at"), '') || ':' || count("m"."answered_at")
            FROM "messages" "m"
            WHERE "m"."root_id" <> ?1 AND "m"."session_id" IN (\(named))
                AND json_valid("m"."card") AND json_extract("m"."card", '$.kind') IN ('question', 'plan')
            """, arguments: [id]) ?? "")
        // S-3: the team's agents (the marks of the links).
        parts.append(try stamp(db, table: "agents", where: "\"team_id\" = ?1", root.teamId))
        if let agentId = root.agentId { parts.append(try stamp(db, table: "agents", where: "\"id\" = ?1", agentId)) }
        if let appId = root.appId { parts.append(try stamp(db, table: "apps", where: "\"id\" = ?1", appId)) }
        if try Bool.fetchOne(db, sql: "SELECT EXISTS (SELECT 1 FROM \"pull_requests\" WHERE \"root_session_id\" = ?)", arguments: [id]) == true {
            // The Guide's stack walk: the team's PRs, their links and issues.
            let teamPrs = "SELECT \"id\" FROM \"pull_requests\" WHERE \"team_id\" = ?1"
            parts.append(try stamp(db, table: "pull_requests", where: "\"team_id\" = ?1", root.teamId))
            // #EXP-1710: who shares a testing run (`Testing with N`).
            parts.append(try String.fetchOne(db, sql: """
                SELECT ifnull(group_concat("id" || '=' || "merge_train_id", ','), '') FROM "pull_requests"
                WHERE "team_id" = ? AND "merge_train_id" IS NOT NULL
                """, arguments: [root.teamId]) ?? "")
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

/// EXP-1284 (review T11/T-N7/T-N9): asks the chat page already on the
/// stack to show one child — the navigator pops back to it instead of
/// pushing a second page of the same chat, then lands it here: a filed
/// issue → the new issue's tab. A reference box like `PushRouteAction` (ONE
/// instance in the navigator's state); `nonce` makes a repeated request for
/// the same page a new one.
@Observable
final class ChatLanding: @unchecked Sendable {
    struct Request: Equatable {
        let chatId: String
        let page: ChatPage
        let nonce: UUID
    }

    private(set) var request: Request?

    init() {}

    func land(chatId: String, page: ChatPage) {
        request = Request(chatId: chatId, page: page, nonce: UUID())
    }
}

// MARK: - #EXP-1662 a plan's ways in = the chat's tab

/// Inside a chat (its Conversation, an embedded session page) a plan opens in that chat's tab; `open` answers whether the chat
/// took it. The same reference box as `ChatIssueOutlet`.
final class ChatPlanOutlet: @unchecked Sendable {
    private var handler: ((String) -> Bool)?

    init() {}

    func setHandler(_ handler: @escaping (String) -> Bool) {
        self.handler = handler
    }

    @MainActor
    func open(_ messageId: String) -> Bool {
        handler?(messageId) ?? false
    }
}

private struct ChatPlanOutletKey: EnvironmentKey {
    static let defaultValue: ChatPlanOutlet? = nil
}

extension EnvironmentValues {
    var chatPlanOutlet: ChatPlanOutlet? {
        get { self[ChatPlanOutletKey.self] }
        set { self[ChatPlanOutletKey.self] = newValue }
    }
}

// MARK: - EXP-1347 chip navigation = the INNERMOST outlet

/// Inside a chat (its Conversation, the ask dock, an embedded session
/// page) an issue pill whose issue is one of THAT chat's issue tabs
/// selects the tab; `open` answers whether the chat took the issue.
/// A reference box like `PushRouteAction` (a bare closure cannot be an
/// `EnvironmentKey.defaultValue` under strict concurrency); the chat
/// screen holds ONE in `@State` and points it at its pages.
final class ChatIssueOutlet: @unchecked Sendable {
    private var handler: ((String) -> Bool)?

    init() {}

    func setHandler(_ handler: @escaping (String) -> Bool) {
        self.handler = handler
    }

    func open(_ issueId: String) -> Bool {
        handler?(issueId) ?? false
    }
}

private struct ChatIssueOutletKey: EnvironmentKey {
    static let defaultValue: ChatIssueOutlet? = nil
}

extension EnvironmentValues {
    var chatIssueOutlet: ChatIssueOutlet? {
        get { self[ChatIssueOutletKey.self] }
        set { self[ChatIssueOutletKey.self] = newValue }
    }
}

/// An issue pill's tap: the enclosing chat's tab when it holds the issue,
/// else the app's push (`deepLinkBus.navigateToIssue`).
func openIssueInnermost(
    _ issueId: String, outlet: ChatIssueOutlet?, deps: AppDependencies, accountId: String
) {
    if outlet?.open(issueId) == true { return }
    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
}
