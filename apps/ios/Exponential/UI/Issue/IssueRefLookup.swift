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
        /// EXP-760 — the steering feed resolves against the SESSION's team
        /// directly: a run may be issue-less (batch, action, chat), so there
        /// is no issue or board to derive the team from.
        case team(id: String)

        var cacheKey: String {
            switch self {
            case .issue(let id): return "i:\(id)"
            case .board(let id): return "b:\(id)"
            case .team(let id): return "t:\(id)"
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

    /// Issues offered by the #-autocomplete, ranked by the shared
    /// `IssueSearch` engine (EXP-892 — the ONE algorithm web, iOS, Android and
    /// desktop run: identifier before title before description, recency
    /// breaking ties, an empty query = most recent). The issue being edited
    /// never offers itself.
    ///
    /// The pre-pass is BOUNDED, and it reads no descriptions: this runs on the
    /// main actor for every keystroke, so a team's whole issue table (bodies
    /// included) may never be materialized to answer one. SQL keeps the rows
    /// whose identifier or title holds every token, newest first, capped at
    /// [prePassCap]; `IssueSearch` then ranks that slice by the SHARED engine
    /// (EXP-892) with `description: nil` — so the local pass ranks on
    /// identifier and title alone, exactly what it filtered on, and body hits
    /// arrive through the server search (`issues.search`) that backs this menu.
    /// (SQLite's `LIKE` folds ASCII case only, so a capital OUTSIDE ASCII is
    /// answered by the server pass rather than this one.)
    static func search(
        _ query: String,
        scope: Scope,
        db: DatabaseManager,
        accountId: String,
        limit: Int = 8
    ) -> [IssueRefCandidate] {
        guard let pool = try? db.pool(forAccountId: accountId) else { return [] }
        let selfIssueId: String? = {
            if case .issue(let id) = scope { return id }
            return nil
        }()
        return (try? pool.read { db -> [IssueRefCandidate] in
            guard let teamId = try teamId(for: scope, db: db) else { return [] }
            // The SAME tokens the engine scores with, so the filter can never
            // drop a row the ranking would have kept on those two fields.
            let tokens = IssueSearch.tokens(query)
            var sql = """
                SELECT i.id, i.identifier, i.title, i.status, i.status_id,
                       i.created_at, i.updated_at
                FROM issues i
                JOIN boards p ON p.id = i.board_id
                WHERE p.team_id = ?
                """
            // Every bound value is a string, so the array stays concrete and
            // `StatementArguments` takes it without an existential dance.
            var arguments: [String] = [teamId]
            for token in tokens {
                sql += "\n  AND (i.identifier LIKE ? ESCAPE '\\' OR i.title LIKE ? ESCAPE '\\')"
                let like = "%\(escapedForLike(token))%"
                arguments.append(like)
                arguments.append(like)
            }
            // An empty query is the menu's "recent work" list, which the engine
            // orders by CREATION — cap the same column it will sort on.
            sql += tokens.isEmpty
                ? "\nORDER BY i.created_at DESC\nLIMIT \(prePassCap)"
                : "\nORDER BY i.updated_at DESC\nLIMIT \(prePassCap)"
            let rows = try Row.fetchAll(db, sql: sql, arguments: StatementArguments(arguments))
            guard !rows.isEmpty else { return [] }
            let ranked = IssueSearch.rank(
                rows,
                query: query,
                limit: limit,
                exclude: selfIssueId.map { Set([$0]) } ?? [],
                projection: { row in
                    let identifier: String? = row["identifier"]
                    let createdAt: String? = row["created_at"]
                    let updatedAt: String? = row["updated_at"]
                    return IssueSearch.Row(
                        id: row["id"],
                        identifier: identifier ?? "",
                        title: row["title"],
                        description: nil,
                        createdAt: createdAt,
                        updatedAt: updatedAt,
                        // EXP-922: the anchor the row already selects, so the
                        // `#` menu lists undone work first.
                        status: row["status"]
                    )
                }
            )
            guard !ranked.isEmpty else { return [] }
            // EXP-581: the candidate row leads with the issue's status glyph
            // (web/Android parity), resolved in the SAME read as the search.
            let team = IssueStatusResolver.teamStatusesOrFallback(
                try IssueStatusEntity.filter(Column("team_id") == teamId).fetchAll(db)
            )
            return ranked.compactMap { row in
                guard let identifier: String = row["identifier"] else { return nil }
                let statusId: String? = row["status_id"]
                let anchor: String? = row["status"]
                return IssueRefCandidate(
                    identifier: identifier,
                    title: row["title"],
                    status: IssueStatusResolver.resolve(statusId: statusId, anchor: anchor, team: team),
                    issueId: row["id"]
                )
            }
        }) ?? []
    }

    /// How many rows the local `#`-menu pass ever materializes.
    private static let prePassCap = 300

    /// A token as a SQL `LIKE` operand: `\`, `%` and `_` are literal here, so
    /// each one is escaped for the `ESCAPE '\'` the query declares.
    private static func escapedForLike(_ token: String) -> String {
        var out = ""
        for character in token {
            if character == "\\" || character == "%" || character == "_" { out.append("\\") }
            out.append(character)
        }
        return out
    }

    /// EXP-892: the server's full-text hits (`issues.search` — title +
    /// description + COMMENT bodies) turned into renderable `#`-menu rows, in
    /// ONE read: a hit whose id is already synced renders the local row (its
    /// live title + precise status), an unsynced one renders from the hit's own
    /// fields with its anchor status resolved against the same team.
    /// Keyed by issue id, which is what `IssueSearch.mergeServerHits` resolves.
    static func candidates(
        for hits: [SearchIssueHit],
        scope: Scope,
        db: DatabaseManager,
        accountId: String
    ) -> [String: IssueRefCandidate] {
        guard !hits.isEmpty, let pool = try? db.pool(forAccountId: accountId) else { return [:] }
        let ids = hits.map(\.id)
        return (try? pool.read { db -> [String: IssueRefCandidate] in
            guard let teamId = try teamId(for: scope, db: db) else { return [:] }
            let team = IssueStatusResolver.teamStatusesOrFallback(
                try IssueStatusEntity.filter(Column("team_id") == teamId).fetchAll(db)
            )
            // `issues.search` is team-scoped server-side, so a hit is in this
            // team by construction — the local lookup needs no board join.
            let local = try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db)
            let byId = Dictionary(local.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            var table: [String: IssueRefCandidate] = [:]
            for hit in hits {
                if let row = byId[hit.id], let identifier = row.identifier {
                    table[hit.id] = IssueRefCandidate(
                        identifier: identifier,
                        title: row.title,
                        status: IssueStatusResolver.resolve(
                            statusId: row.statusId, anchor: row.status, team: team),
                        issueId: row.id
                    )
                } else {
                    table[hit.id] = IssueRefCandidate(
                        identifier: hit.identifier,
                        title: hit.title,
                        status: IssueStatusResolver.resolve(
                            statusId: nil, anchor: hit.status, team: team),
                        issueId: hit.id
                    )
                }
            }
            return table
        }) ?? [:]
    }

    /// The team an editor's refs resolve against, as a one-shot read — what
    /// the team-scoped `issues.search` augmentation needs before it can ask.
    static func teamId(for scope: Scope, db: DatabaseManager, accountId: String) -> String? {
        if case .team(let id) = scope { return id }
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        return (try? pool.read { db in try teamId(for: scope, db: db) }) ?? nil
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
        case .team(let id):
            return id
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
