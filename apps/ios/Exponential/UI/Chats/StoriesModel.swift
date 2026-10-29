import ExpCore
import ExpUI
import Foundation
import GRDB
import SwiftUI

/// S-3 STORIES (stories.json): the fold's input off the synced rows — ONE
/// observation of `sessions` (named by THE session title, the chat list's
/// rung: stored title → issues → first prompt over the conversations'
/// digests), the `agents` definitions, the story-card `messages` (a
/// session's own plan / question / surface cards: the native shape syncs
/// every visible message, so no per-thread subset is needed) and the
/// viewer's `session_reads` (`story_seen_at` included). The rings derive in
/// the view (`rings(now:teamId:)`) so their ages follow the clock; the cards
/// stay keyed by message id for the viewer's body and footer.
@MainActor @Observable
final class StoriesModel {
    struct Input: Equatable, Sendable {
        var sessions: [Stories.SessionInput] = []
        /// Session id → team id (the active team narrows the fold).
        var teams: [String: String] = [:]
        var agents: [Stories.AgentInput] = []
        var agentAvatars: [String: StoryRingAvatar] = [:]
        /// Session id → its CLI (a chat ring's brand mark).
        var clis: [String: String] = [:]
        var messages: [Stories.MessageInput] = []
        var reads: [Stories.ReadInput] = []
        var cards: [String: TreeMessageCard] = [:]
    }

    private(set) var input = Input()

    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var boundAccountId: String?

    nonisolated init() {}

    /// THE fold over the active team's rows.
    func rings(now: Date, teamId: String?) -> [Stories.StoryRing] {
        let sessions = input.sessions.filter { teamId == nil || input.teams[$0.id] == teamId }
        let ids = Set(sessions.map(\.id))
        return Stories.rings(
            now: now.timeIntervalSince1970 * 1000,
            sessions: sessions,
            agents: input.agents,
            messages: input.messages.filter { ids.contains($0.sessionId) },
            reads: input.reads
        )
    }

    /// stories.json STORIES TAB: every story `rootId`'s tree ever posted
    /// (`Stories.storyTab`), no time window; nil = the empty state.
    func tab(now: Date, rootId: String) -> Stories.StoryRing? {
        Stories.storyTab(
            now: now.timeIntervalSince1970 * 1000,
            sessions: input.sessions,
            agents: input.agents,
            messages: input.messages,
            reads: input.reads,
            rootId: rootId
        )
    }

    func card(_ storyId: String) -> TreeMessageCard? {
        input.cards[storyId]
    }

    /// A ring's avatar: its agent's mark, else the chat row's bare brand
    /// mark of the root's CLI.
    func avatar(_ ring: Stories.StoryRing) -> StoryRingAvatar {
        if let agentId = ring.agentId, let avatar = input.agentAvatars[agentId] { return avatar }
        if ring.agentId != nil {
            return .agent(
                icon: BoardTypeDisplay.iconName(for: SessionTree.ListCopy.agentDefaultIcon)
                    ?? SessionTree.ListCopy.agentDefaultIcon,
                color: SessionTree.ListCopy.agentDefaultColor
            )
        }
        return .chat(cli: ring.rootId.flatMap { input.clis[$0] })
    }

    /// (Re)binds to an account's pool; a no-op while already bound to it.
    func bind(accountId: String, db: DatabaseManager) {
        guard boundAccountId != accountId || task == nil else { return }
        task?.cancel()
        boundAccountId = accountId
        input = Input()
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try Self.read(db)
        }.removeDuplicates()
        task = Task { [weak self] in
            do {
                for try await input in observation.values(in: pool) {
                    self?.input = input
                }
            } catch {}
        }
    }

    func stop() {
        task?.cancel()
        task = nil
        boundAccountId = nil
    }

    nonisolated private static func read(_ db: Database) throws -> Input {
        let sessions = try SessionEntity.fetchAll(db)
        let links = try SessionIssueEntity.order(Column("sort_order")).fetchAll(db)
        let issueIds = Array(Set(links.map(\.issueId)))
        let issues = issueIds.isEmpty ? [] : try IssueEntity.filter(issueIds.contains(Column("id"))).fetchAll(db)
        // The chat list's title rung reads the digests, never a transcript.
        let digests = try Row.fetchAll(db, sql: """
            SELECT "session_id", "digest" FROM "conversations"
            WHERE "session_id" IS NOT NULL AND "session_id" <> ''
            """)
            .flatMap { row -> [TreeMessage] in
                Chats.digestMessages(sessionId: row["session_id"], digest: row["digest"] as String?)
            }
        let trees = sessions.map { TreeSession(sessionRow: $0) }
        let titles = SessionTitle.titles(
            trees,
            sessionIssues: links.map { TreeSessionIssue(sessionIssueRow: $0) },
            issues: issues.map { TreeIssue(issueRow: $0) },
            messages: digests
        )
        // A session's OWN show cards (the fold drops everything else).
        let rows = try SessionMessage.fetchAll(db, sql: """
            \(SessionQueries.keyedMessagesSQL)
            AND "messages"."card" IS NOT NULL
            AND "messages"."author_session_id" = "conversations"."session_id"
            AND json_extract("messages"."card", '$.kind') IN ('plan', 'question', 'surface')
            """)
        var cards: [String: TreeMessageCard] = [:]
        let messages: [Stories.MessageInput] = rows.compactMap { row in
            guard let card = TreeMessageCard.parse(row.card), let createdAt = Stories.ms(row.createdAt) else {
                return nil
            }
            cards[row.id] = card
            return Stories.MessageInput(
                id: row.id,
                sessionId: row.sessionId,
                authorSessionId: row.authorSessionId,
                createdAt: createdAt,
                answeredAt: Stories.ms(row.answeredAt),
                card: card
            )
        }
        let agents = try AgentDefinitionEntity.fetchAll(db)
        return Input(
            sessions: sessions.map { session in
                Stories.SessionInput(
                    id: session.id,
                    rootId: session.rootId,
                    name: titles[session.id] ?? SessionTitle.Copy.untitled,
                    agentId: session.agentId,
                    startedByAgentId: session.startedByAgentId
                )
            },
            teams: Dictionary(sessions.map { ($0.id, $0.teamId) }, uniquingKeysWith: { a, _ in a }),
            // The synced definitions carry no role column: no Assistant ring
            // leads until they do.
            agents: agents.map { Stories.AgentInput(id: $0.id, name: $0.name, assistant: false) },
            agentAvatars: Dictionary(
                agents.map { agent in
                    (agent.id, StoryRingAvatar.agent(
                        icon: BoardTypeDisplay.iconName(for: agent.icon ?? SessionTree.ListCopy.agentDefaultIcon)
                            ?? SessionTree.ListCopy.agentDefaultIcon,
                        color: agent.color ?? SessionTree.ListCopy.agentDefaultColor
                    ))
                },
                uniquingKeysWith: { a, _ in a }
            ),
            clis: Dictionary(
                sessions.compactMap { session in session.cli.map { (session.id, $0) } },
                uniquingKeysWith: { a, _ in a }
            ),
            messages: messages,
            reads: try SessionReadEntity.fetchAll(db).map { read in
                Stories.ReadInput(
                    sessionId: read.sessionId,
                    readAt: Stories.ms(read.readAt),
                    storySeenAt: Stories.ms(read.storySeenAt)
                )
            },
            cards: cards
        )
    }
}
