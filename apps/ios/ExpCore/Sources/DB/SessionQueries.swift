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
    /// agent's conversation session. Threads: most recent activity first.
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
            // VAPP-110: an app's editor session is neither a thread nor an agent.
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

    /// NEW-8: every message keyed to the session of its conversation
    /// (contract `message-conversation.json`): the INNER join holds back a
    /// message whose conversation has not synced, a dm/channel conversation
    /// (`session_id` NULL) keys nothing, and the row's own legacy session is
    /// never read. Observations tracking it wait on BOTH shapes.
    public static let keyedMessagesSQL = """
        SELECT "messages".*, "conversations"."session_id" AS "conversation_session_id"
        FROM "messages"
        JOIN "conversations" ON "conversations"."id" = "messages"."conversation_id"
        WHERE "conversations"."session_id" IS NOT NULL AND "conversations"."session_id" <> ''
        """

    /// A root's conversation (every session of its tree), oldest first.
    public static func messages(db: Database, rootId: String) throws -> [SessionMessage] {
        try SessionMessage.fetchAll(db, sql: """
            \(keyedMessagesSQL) AND "messages"."root_id" = ?
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

    /// One session's own messages (its conversation's), oldest first.
    public static func messages(db: Database, sessionId: String) throws -> [SessionMessage] {
        try SessionMessage.fetchAll(db, sql: """
            \(keyedMessagesSQL) AND "conversations"."session_id" = ?
            ORDER BY "messages"."created_at" ASC, "messages"."id" ASC
            """, arguments: [sessionId])
    }

    // MARK: - Issues

    /// A root's issue links in display order (`sort_order`, creation, id).
    public static func sessionIssues(db: Database, rootId: String) throws -> [SessionIssueEntity] {
        try SessionIssueEntity
            .filter(Column("session_id") == rootId)
            .order(Column("sort_order").asc, Column("created_at").asc, Column("id").asc)
            .fetchAll(db)
    }

    /// A root's issues in link order; a link whose issue has not synced is
    /// skipped.
    public static func issues(db: Database, rootId: String) throws -> [IssueEntity] {
        let ids = try sessionIssues(db: db, rootId: rootId).map(\.issueId)
        guard !ids.isEmpty else { return [] }
        let byId = Dictionary(
            try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db).map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first }
        )
        return ids.compactMap { byId[$0] }
    }

    /// Every thread (root) holding the issue, most recent activity first —
    /// the first one is the issue's newest thread.
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

    /// A thread's pull requests, oldest first.
    public static func pullRequests(db: Database, rootId: String) throws -> [PullRequestEntity] {
        try PullRequestEntity
            .filter(Column("root_session_id") == rootId)
            .order(Column("created_at").asc, Column("id").asc)
            .fetchAll(db)
    }

    /// A team's pull requests, oldest first: the open-stack walk behind a
    /// Guide's `Merge stack` (a stack may reach PRs outside the thread).
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
