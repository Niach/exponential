import ExpUI
import ExpCore
import Foundation
import SwiftUI
import GRDB

@MainActor @Observable
final class IssueListViewModel {
    var issues: [IssueEntity] = []
    var labels: [LabelEntity] = []
    var issueLabels: [IssueLabelEntity] = []
    var users: [UserEntity] = []
    /// Every synced `team_members` row; scoped to this board's team by
    /// `teamUsers` (EXP-487).
    var teamMembers: [TeamMemberEntity] = []
    var board: BoardEntity?
    /// EXP-314: the team's `issue_statuses` rows (every synced team's rows land
    /// in the pool; `teamStatuses` scopes them to this board's team).
    var statusRows: [IssueStatusEntity] = []
    /// EXP-980: every synced `issue_relations` row — `parent` rows nest the
    /// list, `blocks` rows badge it.
    var relations: [IssueRelationEntity] = []
    /// The issues at either end of a `blocks` row. A blocker on another board
    /// still counts, so the badge reads a pool WIDER than this board's rows.
    var relationIssues: [IssueEntity] = []
    /// Collapsed status GROUP KEYS (`ResolvedIssueStatus.id`), not enum values.
    var collapsedStatuses: Set<String> = []
    var permissions: TeamPermissions = .denied
    // True while a signed-in viewer looks like a non-member ONLY because the
    // team_members shape hasn't synced yet — drives a "Syncing team…"
    // banner instead of silently rendering everything as a permission denial.
    var permissionsPending = false
    /// EXP-1115: true once the issues shape has reached up-to-date at least
    /// once for this account (a zero-row snapshot counts). Until then an
    /// empty group list means "still syncing", not "empty board" — the view
    /// shows a spinner instead of the empty state + getting-started cards,
    /// which a full resync otherwise flashes while the snapshot lands.
    var issuesSynced = false
    /// True when the board's team has one human member: the selection bar's
    /// assignee button and the row assignee avatar are hidden (EXP-50 parity).
    var singleMemberTeam = false
    /// #EXP-1383: a solo team hides its assignee affordances only while it
    /// has no agents.
    var hidesAssignee = false
    /// #EXP-1383: the board's team's agents in the assignee picker's order.
    var teamAgents: [AgentDefinitionEntity] = []
    var error: String?

    private let accountId: String
    private let boardId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let boardsApi: BoardsApi
    private let labelsApi: LabelsApi
    private let auth: AuthRepository
    // Each observation loop is stored and cancelled individually — a single
    // wrapper task would NOT propagate cancellation into unstructured inner
    // `Task {}` loops, and the view re-arms on every appear, so leaked loops
    // would accumulate per push/pop.
    private var observationTasks: [Task<Void, Never>] = []

    init(accountId: String, boardId: String, db: DatabaseManager, issuesApi: IssuesApi, boardsApi: BoardsApi, labelsApi: LabelsApi, auth: AuthRepository) {
        self.accountId = accountId
        self.boardId = boardId
        self.db = db
        self.issuesApi = issuesApi
        self.boardsApi = boardsApi
        self.labelsApi = labelsApi
        self.auth = auth
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        // Captured locally so the tracking closures don't retain self.
        let boardId = boardId

        // Observe board
        let boardObservation = ValueObservation.tracking { db in
            try BoardEntity.fetchOne(db, key: boardId)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await board in boardObservation.removeDuplicates().values(in: pool) {
                    guard let self else { return }
                    self.board = board
                    self.rebuildRows()
                }
            } catch {}
        })

        // Observe issues
        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity
                .filter(Column("board_id") == boardId)
                .fetchAll(db)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await issues in issueObservation.removeDuplicates().values(in: pool) {
                    guard let self else { return }
                    self.issues = issues
                    self.rebuildRows()
                }
            } catch {}
        })

        // EXP-1115: the issues shape's "up-to-date seen" flag — the same
        // offset-row read the permissions banner makes for "team-members".
        // A missing row (fresh install, or a full resync's wipe) is not synced.
        let issuesLiveObservation = ValueObservation.tracking { db in
            try ElectricOffset.fetchOne(db, key: "issues")?.isLive ?? false
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await live in issuesLiveObservation.removeDuplicates().values(in: pool) {
                    guard let self else { return }
                    self.issuesSynced = live
                }
            } catch {}
        })

        // EXP-980: the relation rows the list nests (`parent`) and badges
        // (`blocks`) with, plus the issues at either end of a `blocks` row —
        // a blocker on ANOTHER board still counts, so the badge's pool is
        // wider than this board's. One tracked read.
        let relationObservation = ValueObservation.tracking {
            db -> ([IssueRelationEntity], [IssueEntity]) in
            let relations = try IssueRelationEntity.fetchAll(db)
            let ids = Array(Set(
                relations
                    .filter { $0.type == IssueRelationType.blocks.rawValue }
                    .flatMap { [$0.issueId, $0.relatedIssueId] }
            ))
            let issues = ids.isEmpty
                ? []
                : try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db)
            return (relations, issues)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await (relations, issues) in relationObservation.values(in: pool) {
                    guard let self else { return }
                    self.relations = relations
                    self.relationIssues = issues
                    self.rebuildRows()
                }
            } catch {}
        })

        // Observe labels (team-scoped, need board's team)
        let labelObservation = ValueObservation.tracking { db in
            try LabelEntity.fetchAll(db)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await labels in labelObservation.removeDuplicates().values(in: pool) {
                    self?.labels = labels
                    self?.rebuildLabelIndex()
                }
            } catch {}
        })

        // Observe issue statuses (team-scoped, EXP-314) — same pattern as
        // labels: fetch all, scope to the board's team in `teamStatuses`.
        let statusObservation = ValueObservation.tracking { db in
            try IssueStatusEntity.fetchAll(db)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await rows in statusObservation.removeDuplicates().values(in: pool) {
                    guard let self else { return }
                    self.statusRows = rows
                    self.rebuildRows()
                }
            } catch {}
        })

        // Observe issue labels
        let issueLabelObservation = ValueObservation.tracking { db in
            try IssueLabelEntity.fetchAll(db)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await issueLabels in issueLabelObservation.removeDuplicates().values(in: pool) {
                    self?.issueLabels = issueLabels
                    self?.rebuildLabelIndex()
                }
            } catch {}
        })

        // Observe users
        let userObservation = ValueObservation.tracking { db in
            try UserEntity.fetchAll(db)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await users in userObservation.removeDuplicates().values(in: pool) {
                    self?.users = users
                    self?.rebuildUserIndex()
                }
            } catch {}
        })

        // Recompute permissions when membership or the members-shape sync
        // state changes. The board row observed above may never change
        // again after the members shape snapshots in, so without this the
        // "Syncing team…" banner would stick and controls would stay
        // read-only until the view is remounted. Tracks the two regions the
        // computation reads: the team_members table and the
        // "team-members" offset row (isLive).
        // EXP-1358: ONE tracked read derives the whole permission state
        // (board → team → my role, the members-shape liveness, the human
        // count) off the main actor, and equal snapshots never re-render —
        // this used to re-fetch every member row and run two synchronous
        // pool reads on main on EVERY shape poll (the offsets row it tracks
        // was rewritten ~once a second while idle).
        let userId = auth.userId
        let isAdmin = auth.isAdmin
        let permsObservation = ValueObservation.tracking { db -> TeamPermissionsSnapshot in
            let teamId = try BoardEntity.fetchOne(db, key: boardId)?.teamId
            return try TeamPermissionsSnapshot.read(
                db: db, teamId: teamId, currentUserId: userId, isAdmin: isAdmin
            )
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await snapshot in permsObservation.removeDuplicates().values(in: pool) {
                    guard let self else { return }
                    // The rows also scope the bulk-assign picker to this
                    // board's team (EXP-487, `teamUsers`).
                    self.teamMembers = snapshot.members
                    self.permissions = snapshot.permissions
                    // EXP-1358 review: no team yet (board row unsynced) isn't "Syncing team…" — the old `refreshPermissions(for: nil)` forced the banner off.
                    self.permissionsPending = snapshot.team != nil && snapshot.permissionsPending
                    self.singleMemberTeam = snapshot.singleMemberTeam
                    self.hidesAssignee = snapshot.hidesAssignee
                    self.teamAgents = snapshot.agents
                }
            } catch {}
        })
    }

    func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
    }

    // MARK: - Computed

    /// This board's team's statuses in render order (EXP-314), degrading to the
    /// constructed builtin defaults while the shape is still syncing.
    /// EXP-1358: resolved ONCE per status/board emission (`rebuildRows`), not
    /// per row per render — every row's status glyph used to re-filter and
    /// re-sort the account's status rows.
    private(set) var teamStatuses: [ResolvedIssueStatus] = IssueStatusResolver.builtinFallbackTeam

    private func resolveTeamStatuses() -> [ResolvedIssueStatus] {
        guard let teamId = board?.teamId else { return IssueStatusResolver.builtinFallbackTeam }
        return IssueStatusResolver.teamStatusesOrFallback(statusRows.filter { $0.teamId == teamId })
    }

    /// Statuses a picker may write: every team status except the duplicate
    /// category — marking a duplicate needs the canonical-issue picker.
    var pickableStatuses: [ResolvedIssueStatus] {
        teamStatuses.filter { $0.category != .duplicate }
    }

    func resolved(_ issue: IssueEntity) -> ResolvedIssueStatus {
        IssueStatusResolver.resolve(issue, team: teamStatuses)
    }

    /// The groups the list renders: the team's statuses in resolver order,
    /// then an APPENDED group for any issue whose resolved status is OUTSIDE
    /// that vocabulary (a constructed default while the team's issue_statuses
    /// rows are still landing), in first-encounter order — an issue must never
    /// silently vanish from its board. Same contract as web
    /// (`buildVisibleIssueGroups`) and desktop (`build_status_groups`).
    var visibleGroups: [ResolvedIssueStatus] {
        let team = teamStatuses
        var groups = team
        var seen = Set(team.map(\.id))
        for issue in issues {
            let status = IssueStatusResolver.resolve(issue, team: team)
            if seen.insert(status.id).inserted { groups.append(status) }
        }
        return groups
    }

    /// One group's issues (EXP-314: grouped by resolved status row, not by the
    /// anchor enum). Canonical in-group ordering (EXP-38, cross-platform
    /// contract) now switches on the group's CATEGORY: overdue → priority →
    /// due date → number for the open categories, resolution recency for
    /// completed/cancelled/duplicate.
    func issues(forGroup group: ResolvedIssueStatus) -> [IssueEntity] {
        let team = teamStatuses
        return IssueSorting.sorted(
            issues.filter { IssueStatusResolver.resolve($0, team: team).id == group.id },
            category: group.category
        )
    }

    // MARK: - Rendered rows (EXP-980)

    /// One rendered group: a status row and the rows displayed under it (a
    /// group a nesting emptied is dropped, so every group here has rows).
    struct RenderGroup: Identifiable {
        let status: ResolvedIssueStatus
        let rows: [NestedIssueRow]
        var id: String { status.id }
    }

    /// The list, nested and ready to draw. Recomputed ONCE per incoming change
    /// (not per row) — the nesting rule runs over ALL groups at once, because
    /// a sub-issue follows its ROOT out of its own status group.
    private(set) var renderGroups: [RenderGroup] = []

    /// EXP-980: the per-row blocks badge numbers, computed once per list
    /// change over the synced relation rows and their issues.
    private(set) var blockCounts: [String: IssueGraph.BlockCounts] = [:]

    /// The rows in DISPLAY order, flattened — what anything that used to walk
    /// the flat list reads now (the selection bar's start payload).
    var displayOrderedIssues: [IssueEntity] {
        renderGroups.flatMap { $0.rows.map(\.issue) }
    }

    /// The mini-graph a row's badge opens: the issue's transitive blockers and
    /// blocked work over the same pool the counts came from.
    func blockGraph(forIssueId issueId: String) -> IssueGraph.Graph {
        IssueGraph.blockGraph(
            subjectIds: [issueId], relations: relations, issues: relationIssues
        )
    }

    private func rebuildRows() {
        teamStatuses = resolveTeamStatuses()
        let groups = visibleGroups
        let sorted = groups.map { issues(forGroup: $0) }
        let byId = Dictionary(
            sorted.flatMap { $0 }.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }
        )
        let nested = IssueNesting.nestIssueRows(
            groups: sorted.map { $0.map(\.id) },
            relations: relations,
            identifierOf: { byId[$0]?.identifier ?? $0 }
        )
        renderGroups = zip(groups, nested).compactMap { group, rows in
            let mapped = rows.compactMap { row in
                byId[row.id].map { NestedIssueRow(issue: $0, depth: row.depth) }
            }
            return mapped.isEmpty ? nil : RenderGroup(status: group, rows: mapped)
        }
        blockCounts = IssueGraph.blockCounts(relations: relations, issues: relationIssues)
    }

    /// EXP-523: a cheap value that changes exactly when the list's SHAPE does
    /// — which issues are visible, which group each one lands in and how deep
    /// it hangs. Bound to the List's `.animation(_:value:)` so a status change
    /// or an incoming sync MOVES rows instead of teleporting them. Read off the
    /// already-built rows: the nesting ran once, this is one O(n) pass over it.
    var layoutSignature: Int {
        var hasher = Hasher()
        for group in renderGroups {
            hasher.combine(group.status.id)
            for row in group.rows {
                hasher.combine(row.issue.id)
                hasher.combine(row.depth)
            }
        }
        return hasher.finalize()
    }

    // EXP-1358: the row lookups, indexed once per emission. A row's body
    // used to filter the account's whole `issue_labels` table and scan the
    // users array on every render.
    private(set) var labelsByIssue: [String: [LabelEntity]] = [:]
    private(set) var usersById: [String: UserEntity] = [:]
    /// A label's dot colour, parsed once per label row.
    private(set) var labelColors: [String: Color] = [:]

    private func rebuildLabelIndex() {
        let labelsById = Dictionary(labels.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var byIssue: [String: [LabelEntity]] = [:]
        // `labels` order (the team's label order) wins, like the filter did.
        var idsByIssue: [String: Set<String>] = [:]
        for link in issueLabels { idsByIssue[link.issueId, default: []].insert(link.labelId) }
        for (issueId, ids) in idsByIssue {
            byIssue[issueId] = labels.filter { ids.contains($0.id) }
        }
        labelsByIssue = byIssue
        labelColors = labelsById.compactMapValues { Color(hex: $0.color) }
    }

    private func rebuildUserIndex() {
        usersById = Dictionary(users.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    func labelsFor(issueId: String) -> [LabelEntity] {
        labelsByIssue[issueId] ?? []
    }

    func userFor(id: String?) -> UserEntity? {
        guard let id else { return nil }
        return usersById[id]
    }

    /// Labels belonging to this board's team (the pool holds every
    /// synced team's labels).
    var teamLabels: [LabelEntity] {
        guard let teamId = board?.teamId else { return [] }
        return labels.filter { $0.teamId == teamId }
    }

    /// The board's team's member users — the bulk-assign picker vocabulary.
    /// `users` stays account-wide so `userFor(id:)` still resolves an
    /// ex-member assignee's row avatar (EXP-487).
    var teamUsers: [UserEntity] {
        guard let teamId = board?.teamId else { return [] }
        let memberIds = Set(teamMembers.filter { $0.teamId == teamId }.map(\.userId))
        // Display-name order, at the source: `users` is a `fetchAll`, and the
        // assignee picker renders what it is handed (EXP-1021 review r2).
        return membersByDisplayName(users.filter { memberIds.contains($0.id) })
    }

    func toggleStatusCollapsed(_ groupId: String) {
        if collapsedStatuses.contains(groupId) {
            collapsedStatuses.remove(groupId)
        } else {
            collapsedStatuses.insert(groupId)
        }
    }

    // MARK: - Multi-select (EXP-239)

    // MARK: - Mutations

    /// Enum-only convenience write (swipe actions). Deliberately keeps sending
    /// the ANCHOR: the server's derive trigger lands it on the team's builtin
    /// row for that enum value, which is exactly what "swipe to done" means
    /// (EXP-314).
    func setStatus(issueId: String, status: IssueStatus) async {
        do {
            try await issuesApi.update(accountId: accountId, UpdateIssueInput(id: issueId, status: status.rawValue))
        } catch {
            self.error = error.userFacingMessage
        }
    }

    /// Status-picker write (EXP-314): sends the row id. A CONSTRUCTED default
    /// (the statuses shape hasn't synced) has no row id, so it falls back to
    /// the anchor enum — the server resolves that to the same builtin row.
    func setStatus(issueId: String, resolved: ResolvedIssueStatus) async {
        do {
            let input = resolved.rowId.map { UpdateIssueInput(id: issueId, statusId: $0) }
                ?? UpdateIssueInput(id: issueId, status: resolved.anchor.rawValue)
            try await issuesApi.update(accountId: accountId, input)
        } catch {
            self.error = error.userFacingMessage
        }
    }

    func setPriority(issueId: String, priority: IssuePriority) async {
        do {
            try await issuesApi.update(accountId: accountId, UpdateIssueInput(id: issueId, priority: priority.rawValue))
        } catch {
            self.error = error.userFacingMessage
        }
    }

    /// Set (or clear) an issue's assignee — #EXP-1383: both columns, the
    /// cleared one as an explicit null (the detail view's path).
    func setAssignee(issueId: String, assignee: IssueAssignee) async {
        do {
            try await issuesApi.update(accountId: accountId, UpdateIssueInput(id: issueId, assignee: assignee))
        } catch {
            self.error = error.userFacingMessage
        }
    }

    /// The server caps every bulk procedure's id array at 200; larger
    /// selections go out as sequential chunks, matching the web bar's
    /// BULK_CHUNK_SIZE.
    private static let bulkChunkSize = 200

    /// Bulk status change from the selection bar (EXP-239). One
    /// `issues.bulkUpdate` per chunk instead of a per-issue loop: the batch is
    /// transactional, and past 25 ids the server deliberately drops the
    /// per-issue notification fan-out — looping `issues.update` bypassed that
    /// cap entirely (60 issues ⇒ 60 pushes) and could half-apply.
    func bulkSetStatus(issueIds: [String], resolved: ResolvedIssueStatus) async {
        await runBulk(issueIds) { ids in
            // Row id when there is one; anchor enum for a constructed default
            // (EXP-314) — both land the selection on the same status.
            let input = resolved.rowId.map { BulkUpdateIssuesInput(ids: ids, statusId: $0) }
                ?? BulkUpdateIssuesInput(ids: ids, status: resolved.anchor.rawValue)
            try await self.issuesApi.bulkUpdate(accountId: self.accountId, input)
        }
    }

    func bulkSetPriority(issueIds: [String], priority: IssuePriority) async {
        await runBulk(issueIds) { ids in
            try await self.issuesApi.bulkUpdate(
                accountId: self.accountId,
                BulkUpdateIssuesInput(ids: ids, priority: priority.rawValue)
            )
        }
    }

    /// Unassigned only works when the columns reach the server as explicit
    /// JSON nulls — #EXP-1383: both columns, the cleared one null.
    func bulkSetAssignee(issueIds: [String], assignee: IssueAssignee) async {
        await runBulk(issueIds) { ids in
            try await self.issuesApi.bulkUpdate(
                accountId: self.accountId,
                BulkUpdateIssuesInput(ids: ids, assignee: assignee)
            )
        }
    }

    /// #EXP-1383: an agent assignee's row (nil while unsynced).
    func agentFor(id: String?) -> AgentDefinitionEntity? {
        guard let id else { return nil }
        return teamAgents.first { $0.id == id }
    }

    /// Delete the whole selection (EXP-698 r5 — the bulk bar's trash, gated on
    /// `isModerator` and confirmed by the caller). The rows leave the local
    /// store through Electric's delete messages, so there is nothing to write
    /// here.
    func bulkDelete(issueIds: [String]) async {
        await runBulk(issueIds) { ids in
            try await self.issuesApi.bulkDelete(
                accountId: self.accountId,
                BulkDeleteIssuesInput(issueIds: ids)
            )
        }
    }

    /// The bulk label procedures record a timeline event only for the rows
    /// they really inserted/deleted, so the caller can hand over the whole
    /// selection — issues that already carry the label are skipped silently
    /// instead of getting a duplicate `label_added` entry.
    func bulkToggleLabel(issueIds: [String], labelId: String, add: Bool) async {
        await runBulk(issueIds) { ids in
            if add {
                try await self.labelsApi.bulkAddToIssues(accountId: self.accountId, issueIds: ids, labelId: labelId)
            } else {
                try await self.labelsApi.bulkRemoveFromIssues(accountId: self.accountId, issueIds: ids, labelId: labelId)
            }
        }
    }

    /// Shared driver for the selection-bar writes: chunks at the server's
    /// 200-id cap and runs the chunks sequentially, stopping at the first
    /// failure. Electric replays transactions in commit order, so the rows
    /// land in chunk order too.
    private func runBulk(_ issueIds: [String], _ write: ([String]) async throws -> Void) async {
        guard !issueIds.isEmpty else { return }
        var start = issueIds.startIndex
        while start < issueIds.endIndex {
            let end = min(start + Self.bulkChunkSize, issueIds.endIndex)
            do {
                try await write(Array(issueIds[start..<end]))
            } catch {
                self.error = error.userFacingMessage
                return
            }
            start = end
        }
    }

}
