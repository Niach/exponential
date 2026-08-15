import ExpUI
import ExpCore
import Foundation
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
    var filters = IssueFilters()
    /// EXP-314: the team's `issue_statuses` rows (every synced team's rows land
    /// in the pool; `teamStatuses` scopes them to this board's team).
    var statusRows: [IssueStatusEntity] = []
    /// Collapsed status GROUP KEYS (`ResolvedIssueStatus.id`), not enum values.
    var collapsedStatuses: Set<String> = []
    var permissions: TeamPermissions = .denied
    // True while a signed-in viewer looks like a non-member ONLY because the
    // team_members shape hasn't synced yet — drives a "Syncing team…"
    // banner instead of silently rendering everything as a permission denial.
    var permissionsPending = false
    /// True when the board's team has one human member: the selection bar's
    /// assignee button and the row assignee avatar are hidden (EXP-50 parity).
    var singleMemberTeam = false
    var error: String?

    private let accountId: String
    private let boardId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let boardsApi: BoardsApi
    private let labelsApi: LabelsApi
    private let auth: AuthRepository
    private let syncManager: SyncManager
    // Each observation loop is stored and cancelled individually — a single
    // wrapper task would NOT propagate cancellation into unstructured inner
    // `Task {}` loops, and the view re-arms on every appear, so leaked loops
    // would accumulate per push/pop.
    private var observationTasks: [Task<Void, Never>] = []

    init(accountId: String, boardId: String, db: DatabaseManager, issuesApi: IssuesApi, boardsApi: BoardsApi, labelsApi: LabelsApi, auth: AuthRepository, syncManager: SyncManager) {
        self.accountId = accountId
        self.boardId = boardId
        self.db = db
        self.issuesApi = issuesApi
        self.boardsApi = boardsApi
        self.labelsApi = labelsApi
        self.auth = auth
        self.syncManager = syncManager
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
                for try await board in boardObservation.values(in: pool) {
                    guard let self else { return }
                    self.board = board
                    self.refreshPermissions(for: board)
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
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues
                }
            } catch {}
        })

        // Observe labels (team-scoped, need board's team)
        let labelObservation = ValueObservation.tracking { db in
            try LabelEntity.fetchAll(db)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await labels in labelObservation.values(in: pool) {
                    self?.labels = labels
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
                for try await rows in statusObservation.values(in: pool) {
                    self?.statusRows = rows
                }
            } catch {}
        })

        // Observe issue labels
        let issueLabelObservation = ValueObservation.tracking { db in
            try IssueLabelEntity.fetchAll(db)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await issueLabels in issueLabelObservation.values(in: pool) {
                    self?.issueLabels = issueLabels
                }
            } catch {}
        })

        // Observe users
        let userObservation = ValueObservation.tracking { db in
            try UserEntity.fetchAll(db)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await users in userObservation.values(in: pool) {
                    self?.users = users
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
        let permsObservation = ValueObservation.tracking { db -> ([TeamMemberEntity], Bool) in
            let rows = try TeamMemberEntity.fetchAll(db)
            let live = try ElectricOffset.fetchOne(db, key: "team-members")?.isLive ?? false
            return (rows, live)
        }
        observationTasks.append(Task { [weak self] in
            do {
                for try await (rows, _) in permsObservation.values(in: pool) {
                    guard let self else { return }
                    // The rows also scope the bulk-assign picker to this
                    // board's team (EXP-487, `teamUsers`).
                    self.teamMembers = rows
                    self.refreshPermissions(for: self.board)
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
    var teamStatuses: [ResolvedIssueStatus] {
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

    var filteredIssues: [IssueEntity] {
        let team = teamStatuses
        return issues.filter { issue in
            let status = IssueStatusResolver.resolve(issue, team: team)
            let priority = IssuePriority.from(issue.priority)
            let issueLabelSet = Set(issueLabels.filter { $0.issueId == issue.id }.map(\.labelId))
            return matchesFilters(status: status, priority: priority, issueLabelIds: issueLabelSet, filters: filters)
        }
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
        for issue in filteredIssues {
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
            filteredIssues.filter { IssueStatusResolver.resolve($0, team: team).id == group.id },
            category: group.category
        )
    }

    func labelsFor(issueId: String) -> [LabelEntity] {
        let labelIds = issueLabels.filter { $0.issueId == issueId }.map(\.labelId)
        return labels.filter { labelIds.contains($0.id) }
    }

    func userFor(id: String?) -> UserEntity? {
        guard let id else { return nil }
        return users.first { $0.id == id }
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
        return users.filter { memberIds.contains($0.id) }
    }

    /// EXP-314: toggling goes through the resolved status so a stale
    /// `builtin:<key>` token (picked before the statuses shape synced) is
    /// recognized as the synced row it re-keyed into.
    func toggleStatus(_ status: ResolvedIssueStatus) {
        filters.toggleStatus(status)
    }

    func isStatusFiltered(_ status: ResolvedIssueStatus) -> Bool {
        filters.selectsStatus(status)
    }

    func togglePriority(_ priority: IssuePriority) {
        if filters.priorities.contains(priority) {
            filters.priorities.remove(priority)
        } else {
            filters.priorities.insert(priority)
        }
    }

    func toggleLabel(_ labelId: String) {
        if filters.labelIds.contains(labelId) {
            filters.labelIds.remove(labelId)
        } else {
            filters.labelIds.insert(labelId)
        }
    }

    func clearFilters() {
        filters = IssueFilters()
    }

    func toggleStatusCollapsed(_ groupId: String) {
        if collapsedStatuses.contains(groupId) {
            collapsedStatuses.remove(groupId)
        } else {
            collapsedStatuses.insert(groupId)
        }
    }

    // MARK: - Multi-select (EXP-239)

    /// Candidate issues for the selection bar's Start-coding sheet: this
    /// board's eligible issues — repo-backed board only, non-terminal, not
    /// merged — recency-ordered. Mirrors AgentsViewModel.startCandidates but
    /// board-scoped (the bar lives on one board, which also guarantees the
    /// one-repository-per-run rule).
    func startCodingCandidates() -> [StartCodingSheet.IssueOption] {
        guard let board, let repoId = board.repositoryId else { return [] }
        // ANCHOR set (EXP-314): a custom status anchors to one of these, so the
        // enum check keeps gating custom terminal statuses correctly.
        let terminal: Set<String> = [
            IssueStatus.done.rawValue,
            IssueStatus.cancelled.rawValue,
            IssueStatus.duplicate.rawValue,
        ]
        return issues
            .filter { row in
                if terminal.contains(row.status) { return false }
                if row.prState == DomainContract.prStateMerged { return false }
                return true
            }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map { row in
                StartCodingSheet.IssueOption(
                    id: row.id,
                    identifier: row.identifier,
                    title: row.title,
                    repositoryId: repoId,
                    status: row.status,
                    priority: row.priority
                )
            }
    }

    // MARK: - Mutations

    /// Enum-only convenience write (swipe actions). Deliberately keeps sending
    /// the ANCHOR: the server's derive trigger lands it on the team's builtin
    /// row for that enum value, which is exactly what "swipe to done" means
    /// (EXP-314).
    func setStatus(issueId: String, status: IssueStatus) async {
        do {
            try await issuesApi.update(accountId: accountId, UpdateIssueInput(id: issueId, status: status.rawValue))
        } catch {
            self.error = error.localizedDescription
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
            self.error = error.localizedDescription
        }
    }

    func setPriority(issueId: String, priority: IssuePriority) async {
        do {
            try await issuesApi.update(accountId: accountId, UpdateIssueInput(id: issueId, priority: priority.rawValue))
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// Set (or clear, when nil) an issue's assignee — mirrors the detail
    /// view's explicit-null path for "Unassigned".
    func setAssignee(issueId: String, assigneeId: String?) async {
        do {
            if let assigneeId {
                try await issuesApi.update(accountId: accountId, UpdateIssueInput(id: issueId, assigneeId: assigneeId))
            } else {
                var input = UpdateIssueInput(id: issueId)
                input.explicitNulls.insert("assigneeId")
                try await issuesApi.update(accountId: accountId, input)
            }
        } catch {
            self.error = error.localizedDescription
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

    /// nil ⇒ unassign, which only works when `assigneeId` reaches the server
    /// as an explicit JSON null — the same distinction `setAssignee` makes.
    func bulkSetAssignee(issueIds: [String], assigneeId: String?) async {
        await runBulk(issueIds) { ids in
            var input = BulkUpdateIssuesInput(ids: ids, assigneeId: assigneeId)
            if assigneeId == nil {
                input.explicitNulls.insert("assigneeId")
            }
            try await self.issuesApi.bulkUpdate(accountId: self.accountId, input)
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
                self.error = error.localizedDescription
                return
            }
            start = end
        }
    }

    /// Pull-to-refresh. Electric keeps the data live in the normal case, but
    /// the gesture exists precisely for the case where it doesn't — so cancel
    /// and relaunch this account's pipeline, which makes every parked
    /// long-poll re-request immediately instead of waiting out its backoff (up
    /// to 30s). The list repaints itself through ValueObservation as rows
    /// land; the short sleep just keeps the spinner from snapping back before
    /// the first batch arrives. `restartPipeline`'s resyncing guard turns an
    /// overlapping pull into a no-op.
    func refresh() async {
        await syncManager.restartPipeline(accountId: accountId, reason: "pull-to-refresh")
        try? await Task.sleep(for: .milliseconds(800))
    }

    // MARK: - Permissions

    private func refreshPermissions(for board: BoardEntity?) {
        guard let board else {
            permissions = .denied
            permissionsPending = false
            singleMemberTeam = false
            return
        }
        guard let pool = try? db.pool(forAccountId: accountId) else {
            permissions = .denied
            permissionsPending = false
            singleMemberTeam = false
            return
        }
        let (team, membersLive, humanCount): (TeamEntity?, Bool, Int) = (try? pool.read { db in
            let ws = try TeamEntity.fetchOne(db, key: board.teamId)
            let live = try ElectricOffset.fetchOne(db, key: "team-members")?.isLive ?? false
            let count = try humanTeamMemberIds(teamId: board.teamId, db: db).count
            return (ws, live, count)
        }) ?? (nil, false, 0)
        permissions = TeamPermissions.resolve(
            team: team,
            currentUserId: auth.userId,
            isAdmin: auth.isAdmin,
            dbPool: pool
        )
        // Only "pending" while membership genuinely hasn't landed — a live
        // members shape with no matching row means the viewer really isn't a
        // member, which is a real read-only state.
        permissionsPending = permissions.isAuthed && !permissions.isMember && !membersLive
        // Solo team ⇒ nothing to reassign to; the assignee affordances hide.
        // EXACTLY one, not `<= 1`: a count of 0 means the member list hasn't
        // synced yet (never a genuine empty team — the viewer is a member),
        // and treating that as solo made the assignee affordance vanish and
        // reappear on a real multi-member team. Same contract as web's
        // issue-list.tsx / bulk-action-bar.tsx.
        singleMemberTeam = humanCount == 1
    }
}
