import ExpCore
import ExpUI
import Foundation
import GRDB
import SwiftUI

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

        var cacheKey: String {
            switch self {
            case .issue(let id): return "i:\(id)"
            case .board(let id): return "b:\(id)"
            }
        }
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
                WHERE upper(i.identifier) = ? AND p.team_id = ?
                """,
                arguments: [identifier, teamId]
            )
        }) ?? nil
    }

    /// identifier → the issue's TITLE inside the scope's team (EXP-307: the
    /// read-only chip shows `#ID <title>`); nil when the identifier does not
    /// resolve.
    static func resolveTitle(
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
                SELECT i.title FROM issues i
                JOIN boards p ON p.id = i.board_id
                WHERE upper(i.identifier) = ? AND p.team_id = ?
                """,
                arguments: [identifier, teamId]
            )
        }) ?? nil
    }

    /// Universal-link resolution (EXP-92): team SLUG + identifier → local
    /// issue id. Unlike the #-ref resolve above: no board-slug predicate
    /// (identifiers are team-unique, and the board slug in an old link goes
    /// stale when an issue moves — the web route also keys on the identifier
    /// alone).
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
        limit: Int = 8
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
        return (try? pool.read { db -> [IssueRefCandidate] in
            guard let teamId = try teamId(for: scope, db: db) else { return [] }
            let rows = try Row.fetchAll(
                db,
                sql: """
                SELECT i.identifier, i.title, i.status, i.status_id FROM issues i
                JOIN boards p ON p.id = i.board_id
                WHERE p.team_id = ?
                  AND i.id IS NOT ?
                  AND (i.identifier LIKE ? ESCAPE '\\' OR i.title LIKE ? ESCAPE '\\')
                ORDER BY i.created_at DESC
                LIMIT ?
                """,
                arguments: [teamId, selfIssueId, pattern, pattern, limit]
            )
            guard !rows.isEmpty else { return [] }
            // EXP-581: the candidate row leads with the issue's status glyph
            // (web/Android parity), resolved in the SAME read as the search.
            let team = IssueStatusResolver.teamStatusesOrFallback(
                try IssueStatusEntity.filter(Column("team_id") == teamId).fetchAll(db)
            )
            return rows.map { row in
                let statusId: String? = row["status_id"]
                let anchor: String? = row["status"]
                return IssueRefCandidate(
                    identifier: row["identifier"],
                    title: row["title"],
                    status: IssueStatusResolver.resolve(statusId: statusId, anchor: anchor, team: team)
                )
            }
        }) ?? []
    }

    /// Both halves of a chip in ONE read. The chip decoration pass runs on
    /// every keystroke, so the old `resolve` + `resolveTitle` pair meant two
    /// synchronous SQLite round trips per token per character typed.
    static func resolveChip(
        _ identifier: String,
        scope: Scope,
        db: DatabaseManager,
        accountId: String
    ) -> Chip? {
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        return (try? pool.read { db -> Chip? in
            guard let teamId = try teamId(for: scope, db: db) else { return nil }
            guard let row = try Row.fetchOne(
                db,
                sql: """
                SELECT i.id, i.title, i.status, i.status_id FROM issues i
                JOIN boards p ON p.id = i.board_id
                WHERE upper(i.identifier) = ? AND p.team_id = ?
                """,
                arguments: [identifier, teamId]
            ) else { return nil }
            // The chip paints the issue's status glyph over its `#` (EXP-423),
            // so the team's statuses come out of the SAME read — one round trip
            // per token, as before.
            let statusRows = try IssueStatusEntity
                .filter(Column("team_id") == teamId)
                .fetchAll(db)
            let statusId: String? = row["status_id"]
            let anchor: String? = row["status"]
            let status = IssueStatusResolver.resolve(
                statusId: statusId,
                anchor: anchor,
                team: IssueStatusResolver.teamStatusesOrFallback(statusRows)
            )
            return Chip(issueId: row["id"], title: row["title"], status: status)
        }) ?? nil
    }

    struct Chip {
        let issueId: String
        let title: String
        let status: ResolvedIssueStatus
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

/// Short-TTL memo in front of `IssueRefLookup.resolveChip`, because the chip
/// decoration pass re-resolves every token on every keystroke (EXP-322).
/// Caching MISSES is what matters most: typing `#E`, `#EX`, `#EXP` is a run of
/// unresolvable lookups. The TTL keeps a newly-synced issue at most a few
/// seconds away from chipping.
@MainActor
enum IssueRefChipCache {
    private struct Key: Hashable {
        let scope: String
        let identifier: String
    }

    private static var entries: [Key: (value: IssueRefLookup.Chip?, at: Date)] = [:]
    private static let ttl: TimeInterval = 5
    private static let capacity = 512

    static func chip(
        _ identifier: String,
        scope: IssueRefLookup.Scope,
        db: DatabaseManager,
        accountId: String
    ) -> IssueRefLookup.Chip? {
        let key = Key(scope: scope.cacheKey, identifier: identifier.uppercased())
        let now = Date()
        if let hit = entries[key], now.timeIntervalSince(hit.at) < ttl { return hit.value }
        let value = IssueRefLookup.resolveChip(identifier, scope: scope, db: db, accountId: accountId)
        if entries.count >= capacity { entries.removeAll(keepingCapacity: true) }
        entries[key] = (value, now)
        return value
    }

    /// The chip's status as the render info `IssueRefs` decorates with, so every
    /// editor site stays a one-liner and shares this memo with the id/title
    /// resolvers. The 5s TTL bounds only how stale a RE-RUN decoration pass can
    /// read — no pass is TRIGGERED by a referenced issue's status change, so a
    /// painted glyph stays as-is until the user edits the text or the view
    /// reloads (unlike desktop, whose EXP-423 collection observers re-decorate
    /// live; the iOS half is a follow-up needing Mac-side verification).
    ///
    /// One info instance per distinct status, deliberately: the decoration
    /// pass's `changed` flag compares attribute values, and while
    /// `IssueRefStatusInfo` compares by field, its `PlatformColor` came out of a
    /// SwiftUI `Color` bridge whose equality is not something to bet the "does
    /// the editor rewrite its storage on every keystroke" question on.
    static func statusInfo(
        _ identifier: String,
        scope: IssueRefLookup.Scope,
        db: DatabaseManager,
        accountId: String
    ) -> IssueRefStatusInfo? {
        guard let status = chip(identifier, scope: scope, db: db, accountId: accountId)?.status else {
            return nil
        }
        if let hit = statusInfos[status] { return hit }
        let info = IssueRefStatusInfo(iconName: status.iconName, color: PlatformColor(status.color))
        // A team has 7 builtins plus its customs; the cap is a leak guard, not
        // an eviction policy.
        if statusInfos.count >= 128 { statusInfos.removeAll(keepingCapacity: true) }
        statusInfos[status] = info
        return info
    }

    private static var statusInfos: [ResolvedIssueStatus: IssueRefStatusInfo] = [:]
}
