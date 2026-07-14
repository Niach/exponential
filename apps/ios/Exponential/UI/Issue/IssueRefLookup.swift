import ExpCore
import ExpUI
import Foundation
import GRDB

/// Workspace-scoped `#IDENTIFIER` issue-ref lookups against the local GRDB
/// store — pill resolution and the #-autocomplete (mirrors the web
/// `IssueRefProvider`): refs only resolve inside the SAME workspace, so a
/// same-prefix identifier from another workspace never leaks in.
enum IssueRefLookup {
    /// The workspace an editor's refs resolve against: the workspace of the
    /// issue being viewed/commented on, or of the project an issue is being
    /// created in.
    enum Scope {
        case issue(id: String)
        case project(id: String)
    }

    /// identifier (e.g. `VER-12`) → local issue id when it resolves inside the
    /// scope's workspace; nil otherwise (the token stays plain text).
    static func resolve(
        _ identifier: String,
        scope: Scope,
        db: DatabaseManager,
        accountId: String
    ) -> String? {
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        return (try? pool.read { db -> String? in
            guard let workspaceId = try workspaceId(for: scope, db: db) else { return nil }
            return try String.fetchOne(
                db,
                sql: """
                SELECT i.id FROM issues i
                JOIN projects p ON p.id = i.project_id
                WHERE upper(i.identifier) = ? AND i.archived_at IS NULL AND p.workspace_id = ?
                """,
                arguments: [identifier, workspaceId]
            )
        }) ?? nil
    }

    /// Universal-link resolution (EXP-92): workspace SLUG + identifier → local
    /// issue id. Unlike the #-ref resolve above: no archived filter (an emailed
    /// link to an archived issue should still open) and no project-slug
    /// predicate (identifiers are workspace-unique, and the project slug in an
    /// old link goes stale when an issue moves — the web route also keys on the
    /// identifier alone).
    static func resolve(
        identifier: String,
        workspaceSlug: String,
        db: DatabaseManager,
        accountId: String
    ) -> String? {
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        return (try? pool.read { db -> String? in
            try String.fetchOne(
                db,
                sql: """
                SELECT i.id FROM issues i
                JOIN projects p ON p.id = i.project_id
                JOIN workspaces w ON w.id = p.workspace_id
                WHERE upper(i.identifier) = upper(?) AND w.slug = ?
                """,
                arguments: [identifier, workspaceSlug]
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
            guard let workspaceId = try workspaceId(for: scope, db: db) else { return [] }
            return try Row.fetchAll(
                db,
                sql: """
                SELECT i.identifier, i.title FROM issues i
                JOIN projects p ON p.id = i.project_id
                WHERE p.workspace_id = ? AND i.archived_at IS NULL
                  AND i.id IS NOT ?
                  AND (i.identifier LIKE ? ESCAPE '\\' OR i.title LIKE ? ESCAPE '\\')
                ORDER BY i.created_at DESC
                LIMIT ?
                """,
                arguments: [workspaceId, selfIssueId, pattern, pattern, limit]
            )
        }) ?? []
        return rows.map { IssueRefCandidate(identifier: $0["identifier"], title: $0["title"]) }
    }

    private static func workspaceId(for scope: Scope, db: Database) throws -> String? {
        switch scope {
        case .issue(let id):
            return try String.fetchOne(
                db,
                sql: """
                SELECT p.workspace_id FROM issues i
                JOIN projects p ON p.id = i.project_id
                WHERE i.id = ?
                """,
                arguments: [id]
            )
        case .project(let id):
            return try String.fetchOne(
                db,
                sql: "SELECT workspace_id FROM projects WHERE id = ?",
                arguments: [id]
            )
        }
    }
}
