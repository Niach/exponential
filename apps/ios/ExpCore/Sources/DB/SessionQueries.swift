import Foundation
import GRDB

/// EXP-1291: reads over the session tree (`sessions`, `session_issues`,
/// `messages`, `pull_requests`, `pull_request_issues`). Every row here is one
/// the user may see (the shapes filter visibility and the board trash); a
/// root's `title` may be nil — deriving the display name is the screen's job.
public enum SessionQueries {
    // MARK: - Sessions

    /// A team's roots of one kind (`DomainContract.sessionKindThread` |
    /// `sessionKindAgent`); nil team = every team (phones list ALL teams).
    /// NEW-2: the kind derives from `agent_id` (`sessionKindOf`): agent = an
    /// agent's conversation session. Chats: most recent activity first.
    /// Conversations: their agent DEFINITION's order (`agents.sort_order`,
    /// then its name), like boards (NEW-8: never the session's own mirror).
    public static func roots(db: Database, teamId: String?, kind: String) throws -> [SessionEntity] {
        if kind == DomainContract.sessionKindAgent {
            var sql = """
                SELECT "sessions".* FROM "sessions"
                LEFT JOIN "agents" ON "agents"."id" = "sessions"."agent_id"
                WHERE "sessions"."parent_id" IS NULL
                  AND "sessions"."agent_id" IS NOT NULL
                  AND "sessions"."app_id" IS NULL
                """
            var arguments: StatementArguments = []
            if let teamId {
                sql += "\n  AND \"sessions\".\"team_id\" = ?"
                arguments += [teamId]
            }
            sql += """

                ORDER BY COALESCE("agents"."sort_order", 0) ASC, COALESCE("agents"."name", '') ASC,
                    "sessions"."id" ASC
                """
            return try SessionEntity.fetchAll(db, sql: sql, arguments: arguments)
        }
        var request = SessionEntity
            .filter(Column("parent_id") == nil)
            .filter(Column("agent_id") == nil)
            // VAPP-110: an app's editor session is neither a chat nor an agent.
            .filter(Column("app_id") == nil)
        if let teamId {
            request = request.filter(Column("team_id") == teamId)
        }
        return try request.order(Column("updated_at").desc, Column("id").asc).fetchAll(db)
    }

    /// NEW-2: a team's agent DEFINITIONS (nil team = every team), in their
    /// manual order (`sort_order`, then name, then id), like boards.
    public static func agents(db: Database, teamId: String?) throws -> [AgentDefinitionEntity] {
        var request = AgentDefinitionEntity.all()
        if let teamId {
            request = request.filter(Column("team_id") == teamId)
        }
        return try request
            .order(Column("sort_order").asc, Column("name").asc, Column("id").asc)
            .fetchAll(db)
    }

    /// #EXP-1714 (agent-settings.json `assistant.teamAgentsCases`): the
    /// agents a team lists: its own rows plus the viewer's global Assistant
    /// (stored in a personal team that never syncs) while the team holds no
    /// Assistant of theirs. The synced teams = this account's `teams` rows.
    public static func teamAgents(db: Database, teamId: String, me: String?) throws -> [AgentDefinitionEntity] {
        let teamIds = try String.fetchAll(db, sql: "SELECT id FROM teams")
        return AgentSettings.agentsInTeam(
            try agents(db: db, teamId: nil), teamId: teamId, teamIds: teamIds, me: me
        )
    }

    /// NEW-2: my conversation session with an agent (a root with its
    /// `agent_id` that I started); nil = none yet (the first message makes it).
    public static func conversation(db: Database, agentId: String, userId: String) throws -> SessionEntity? {
        try SessionEntity
            .filter(Column("parent_id") == nil)
            .filter(Column("agent_id") == agentId)
            .filter(Column("user_id") == userId)
            .fetchOne(db)
    }

    /// One session by id.
    public static func session(db: Database, id: String) throws -> SessionEntity? {
        try SessionEntity.fetchOne(db, key: id)
    }

    /// A whole tree (the root included), shallowest first, then by creation.
    public static func tree(db: Database, rootId: String) throws -> [SessionEntity] {
        try SessionEntity
            .filter(Column("root_id") == rootId)
            .order(Column("depth").asc, Column("created_at").asc, Column("id").asc)
            .fetchAll(db)
    }

    // MARK: - Conversation

    /// #EXP-1730: every message keyed to its own session
    /// (`messages.session_id`): a dm/channel row (no session) keys nothing,
    /// and `conversations` is never read.
    public static let keyedMessagesSQL = """
        SELECT "messages".*
        FROM "messages"
        WHERE "messages"."session_id" IS NOT NULL AND "messages"."session_id" <> ''
        """

    /// A root's conversation (every session of its tree), oldest first.
    public static func messages(db: Database, rootId: String) throws -> [SessionMessage] {
        try SessionMessage.fetchAll(db, sql: """
            \(keyedMessagesSQL) AND "messages"."root_id" = ?
            ORDER BY "messages"."created_at" ASC, "messages"."id" ASC
            """, arguments: [rootId])
    }

    /// #EXP-1380 (TS `useNewestMessages`): a root's NEWEST `limit` messages
    /// (created_at desc, then id), returned oldest first — what an open chat
    /// loads; Load earlier widens `limit` (`Chats.MessageLimits`).
    public static func newestMessages(db: Database, rootId: String, limit: Int) throws -> [SessionMessage] {
        try SessionMessage.fetchAll(db, sql: """
            SELECT * FROM (
                \(keyedMessagesSQL) AND "messages"."root_id" = ?
                ORDER BY "messages"."created_at" DESC, "messages"."id" DESC
                LIMIT ?
            ) ORDER BY "created_at" ASC, "id" ASC
            """, arguments: [rootId, max(0, limit)])
    }

    /// results-tab.json: a root's result CANDIDATES = its tree's
    /// session-authored card rows (`root_id` + `author_session_id` + `card`),
    /// oldest first, raw (a sealed card opens where it is read). The Results
    /// tab and its count fold THESE, never the loaded window (#EXP-1380).
    public static func resultCardMessages(db: Database, rootId: String) throws -> [SessionMessage] {
        try SessionMessage.fetchAll(db, sql: """
            \(keyedMessagesSQL) AND "messages"."root_id" = ?
                AND "messages"."author_session_id" IS NOT NULL
                AND "messages"."card" IS NOT NULL AND "messages"."card" <> 'null'
            ORDER BY "messages"."created_at" ASC, "messages"."id" ASC
            """, arguments: [rootId])
    }

    /// #EXP-1399: messages by id (an `ask` mirror's ORIGIN card, which lives
    /// in another tree), oldest first; an unsynced id reads nothing.
    public static func messages(db: Database, ids: [String]) throws -> [SessionMessage] {
        guard !ids.isEmpty else { return [] }
        let marks = Array(repeating: "?", count: ids.count).joined(separator: ", ")
        return try SessionMessage.fetchAll(db, sql: """
            \(keyedMessagesSQL) AND "messages"."id" IN (\(marks))
            ORDER BY "messages"."created_at" ASC, "messages"."id" ASC
            """, arguments: StatementArguments(ids))
    }

    /// #EXP-1464: the question / plan card messages of `sessionIds`, oldest
    /// first.
    public static func cardMessages(db: Database, sessionIds: [String]) throws -> [SessionMessage] {
        guard !sessionIds.isEmpty else { return [] }
        let marks = Array(repeating: "?", count: sessionIds.count).joined(separator: ", ")
        return try SessionMessage.fetchAll(db, sql: """
            \(keyedMessagesSQL) AND "messages"."session_id" IN (\(marks))
                AND json_valid("messages"."card") AND json_extract("messages"."card", '$.kind') IN ('question', 'plan')
            ORDER BY "messages"."created_at" ASC, "messages"."id" ASC
            """, arguments: StatementArguments(sessionIds))
    }

    /// One session's own messages, oldest first.
    public static func messages(db: Database, sessionId: String) throws -> [SessionMessage] {
        try SessionMessage.fetchAll(db, sql: """
            \(keyedMessagesSQL) AND "messages"."session_id" = ?
            ORDER BY "messages"."created_at" ASC, "messages"."id" ASC
            """, arguments: [sessionId])
    }

    // MARK: - Issues

    /// A root's issue links in display order (`sort_order`, creation, id).
    public static func sessionIssues(db: Database, rootId: String) throws -> [SessionIssueEntity] {
        // The whole tree's links (session-chat.json: a child's linked issue
        // is the chat's too), as web and Android read them.
        let treeIds = try SessionEntity
            .filter(Column("root_id") == rootId)
            .select(Column("id"), as: String.self)
            .fetchAll(db)
        return try SessionIssueEntity
            .filter((treeIds + [rootId]).contains(Column("session_id")))
            .order(Column("sort_order").asc, Column("created_at").asc, Column("id").asc)
            .fetchAll(db)
    }

    /// A root's issues in link order; a link whose issue has not synced is
    /// skipped.
    public static func issues(db: Database, rootId: String) throws -> [IssueEntity] {
        var seen = Set<String>()
        let ids = try sessionIssues(db: db, rootId: rootId).map(\.issueId).filter { seen.insert($0).inserted }
        guard !ids.isEmpty else { return [] }
        let byId = Dictionary(
            try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db).map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first }
        )
        return ids.compactMap { byId[$0] }
    }

    /// Every chat (root) holding the issue, most recent activity first —
    /// the first one is the issue's newest chat.
    public static func rootsHoldingIssue(db: Database, issueId: String) throws -> [SessionEntity] {
        let rootIds = try SessionIssueEntity
            .filter(Column("issue_id") == issueId)
            .select(Column("session_id"), as: String.self)
            .fetchAll(db)
        guard !rootIds.isEmpty else { return [] }
        return try SessionEntity
            .filter(rootIds.contains(Column("id")))
            .order(Column("updated_at").desc, Column("id").asc)
            .fetchAll(db)
    }

    // MARK: - Pull requests

    /// A chat's pull requests, oldest first.
    public static func pullRequests(db: Database, rootId: String) throws -> [PullRequestEntity] {
        try PullRequestEntity
            .filter(Column("root_session_id") == rootId)
            .order(Column("created_at").asc, Column("id").asc)
            .fetchAll(db)
    }

    /// A team's pull requests, oldest first: the open-stack walk behind a
    /// Guide's `Merge stack` (a stack may reach PRs outside the chat).
    public static func pullRequests(db: Database, teamId: String) throws -> [PullRequestEntity] {
        try PullRequestEntity
            .filter(Column("team_id") == teamId)
            .order(Column("created_at").asc, Column("id").asc)
            .fetchAll(db)
    }

    /// The pull requests that close an issue, newest first (the first one
    /// is the issue's PR badge).
    public static func pullRequests(db: Database, issueId: String) throws -> [PullRequestEntity] {
        let ids = try PullRequestIssueEntity
            .filter(Column("issue_id") == issueId)
            .select(Column("pull_request_id"), as: String.self)
            .fetchAll(db)
        guard !ids.isEmpty else { return [] }
        return try PullRequestEntity
            .filter(ids.contains(Column("id")))
            .order(Column("created_at").desc, Column("id").asc)
            .fetchAll(db)
    }

    /// The issue ids a pull request closes.
    public static func issueIds(db: Database, pullRequestId: String) throws -> [String] {
        try PullRequestIssueEntity
            .filter(Column("pull_request_id") == pullRequestId)
            .order(Column("created_at").asc, Column("id").asc)
            .select(Column("issue_id"), as: String.self)
            .fetchAll(db)
    }
}
