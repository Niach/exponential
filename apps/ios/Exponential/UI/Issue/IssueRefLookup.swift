import ExpCore
import ExpUI
import Foundation
import GRDB

/// Team-scoped `#IDENTIFIER` issue-ref lookups against the local GRDB
/// store — pill resolution and the #-autocomplete (mirrors the web
/// `IssueRefProvider`): refs only resolve inside the SAME team, so a
/// same-prefix identifier from another team never leaks in.
enum IssueRefLookup {
    /// The team an editor's refs resolve against: the team of the
    /// issue being viewed/commented on, or of the board an issue is being
    /// created in.
    enum Scope {
        case issue(id: String)
        case board(id: String)
    }

    /// identifier (e.g. `VER-12`) → local issue id when it resolves inside the
    /// scope's team; nil otherwise (the token stays plain text).
    static func resolve(
        _ identifier: String,
        scope: Scope,
        db: DatabaseManager,
        accountId: String
    ) -> String? {
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        return (try? pool.read { db -> String? in
            guard let teamId = try teamId(for: scope, db: db) else { return nil }
            return try String.fetchOne(
                db,
                sql: """
                SELECT i.id FROM issues i
                JOIN boards p ON p.id = i.board_id
                WHERE upper(i.identifier) = ? AND i.archived_at IS NULL AND p.team_id = ?
                """,
                arguments: [identifier, teamId]
            )
        }) ?? nil
    }

    /// Universal-link resolution (EXP-92): team SLUG + identifier → local
    /// issue id. Unlike the #-ref resolve above: no archived filter (an emailed
    /// link to an archived issue should still open) and no board-slug
    /// predicate (identifiers are team-unique, and the board slug in an
    /// old link goes stale when an issue moves — the web route also keys on the
    /// identifier alone).
    static func resolve(
        identifier: String,
        teamSlug: String,
        db: DatabaseManager,
        accountId: String
    ) -> String? {
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        return (try? pool.read { db -> String? in
            try String.fetchOne(
                db,
                sql: """
                SELECT i.id FROM issues i
                JOIN boards p ON p.id = i.board_id
                JOIN teams w ON w.id = p.team_id
                WHERE upper(i.identifier) = upper(?) AND w.slug = ?
                """,
                arguments: [identifier, teamSlug]
            )
        }) ?? nil
    }

    /// Issues offered by the #-autocomplete: identifier/title substring match
    /// (case-insensitive), newest first, empty query = most recent (parity
    /// with the web `IssueRefProvider.search`). The issue being edited never
    /// offers itself.
    static func search(
        _ query: String,
        scope: Scope,
        db: DatabaseManager,
        accountId: String,
        limit: Int = 6
    ) -> [IssueRefCandidate] {
        guard let pool = try? db.pool(forAccountId: accountId) else { return [] }
        // Escape LIKE metacharacters so a literal `%`/`_` in the query can't
        // widen the match ("" stays a match-everything pattern by design).
        let escaped = query
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "%", with: "\\%")
            .replacingOccurrences(of: "_", with: "\\_")
        let pattern = "%\(escaped)%"
        let selfIssueId: String? = {
            if case .issue(let id) = scope { return id }
            return nil
        }()
        let rows: [Row] = (try? pool.read { db -> [Row] in
            guard let teamId = try teamId(for: scope, db: db) else { return [] }
            return try Row.fetchAll(
                db,
                sql: """
                SELECT i.identifier, i.title FROM issues i
                JOIN boards p ON p.id = i.board_id
                WHERE p.team_id = ? AND i.archived_at IS NULL
                  AND i.id IS NOT ?
                  AND (i.identifier LIKE ? ESCAPE '\\' OR i.title LIKE ? ESCAPE '\\')
                ORDER BY i.created_at DESC
                LIMIT ?
                """,
                arguments: [teamId, selfIssueId, pattern, pattern, limit]
            )
        }) ?? []
        return rows.map { IssueRefCandidate(identifier: $0["identifier"], title: $0["title"]) }
    }

    private static func teamId(for scope: Scope, db: Database) throws -> String? {
        switch scope {
        case .issue(let id):
            return try String.fetchOne(
                db,
                sql: """
                SELECT p.team_id FROM issues i
                JOIN boards p ON p.id = i.board_id
                WHERE i.id = ?
                """,
                arguments: [id]
            )
        case .board(let id):
            return try String.fetchOne(
                db,
                sql: "SELECT team_id FROM boards WHERE id = ?",
                arguments: [id]
            )
        }
    }
}
