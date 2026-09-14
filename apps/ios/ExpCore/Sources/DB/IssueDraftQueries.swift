import Foundation
import GRDB

/// EXP-878: one draft, RESOLVED against the local store. The `issue_drafts`
/// shape is per-user and never team/trash-scoped, so a row may name a board on
/// a trashed team or one the user has since left — `IssueDraftQueries.resolved`
/// drops those, exactly like `PinQueries.resolved` drops an unresolvable pin,
/// and a draft renders ONLY when its board is here.
public struct IssueDraftRow: Identifiable, Sendable {
    public let draft: IssueDraftEntity
    public let board: BoardEntity
    /// The draft's status resolved against its OWN team's rows — a NULL
    /// `status_id` lands on that team's Backlog builtin (EXP-314).
    public let status: ResolvedIssueStatus

    public init(draft: IssueDraftEntity, board: BoardEntity, status: ResolvedIssueStatus) {
        self.draft = draft
        self.board = board
        self.status = status
    }

    public var id: String { draft.id }
}

public enum IssueDraftQueries {
    /// Every resolvable draft, newest edit first. `teamId` nil = ACCOUNT-WIDE
    /// (the My Work segment spans teams, showing each row's board name);
    /// passing one narrows to that team. Rows whose board is missing locally
    /// are omitted.
    public static func resolved(db: Database, teamId: String? = nil) throws -> [IssueDraftRow] {
        var request = IssueDraftEntity.all()
        if let teamId {
            request = request.filter(Column("team_id") == teamId)
        }
        let drafts = try request
            .order(Column("updated_at").desc, Column("id").asc)
            .fetchAll(db)
        guard !drafts.isEmpty else { return [] }

        var boards: [String: BoardEntity?] = [:]
        // One statuses read per TEAM, not per draft: a cross-team list would
        // otherwise re-resolve the same six builtins for every row.
        var teamStatuses: [String: [ResolvedIssueStatus]] = [:]

        var rows: [IssueDraftRow] = []
        rows.reserveCapacity(drafts.count)
        for draft in drafts {
            let board: BoardEntity?
            if let cached = boards[draft.boardId] {
                board = cached
            } else {
                board = try BoardEntity.fetchOne(db, key: draft.boardId)
                boards[draft.boardId] = board
            }
            guard let board else { continue }

            let statuses: [ResolvedIssueStatus]
            if let cached = teamStatuses[board.teamId] {
                statuses = cached
            } else {
                let stored = try IssueStatusEntity
                    .filter(Column("team_id") == board.teamId)
                    .fetchAll(db)
                statuses = IssueStatusResolver.teamStatusesOrFallback(stored)
                teamStatuses[board.teamId] = statuses
            }

            rows.append(IssueDraftRow(
                draft: draft,
                board: board,
                // No anchor column on a draft: a NULL `status_id` IS "Backlog",
                // which is what `resolve` falls back to for a nil anchor.
                status: IssueStatusResolver.resolve(
                    statusId: draft.statusId, anchor: nil, team: statuses
                )
            ))
        }
        return rows
    }

    /// One draft row by id, however it resolves (the compose page seeds from
    /// this and needs the row even before its board is on screen).
    public static func draft(db: Database, id: String) throws -> IssueDraftEntity? {
        try IssueDraftEntity.fetchOne(db, key: id)
    }
}
