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
    var board: BoardEntity?
    var filters = IssueFilters()
    var activeTab: FilterTab = .all
    var collapsedStatuses: Set<IssueStatus> = []
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
    private var observationTask: Task<Void, Never>?

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
        observationTask = Task { [weak self] in
            guard let self else { return }
            guard let pool = try? self.db.pool(forAccountId: self.accountId) else { return }

            // Observe board
            let boardObservation = ValueObservation.tracking { db in
                try BoardEntity.fetchOne(db, key: self.boardId)
            }
            let boardTask = Task {
                do {
                    for try await board in boardObservation.values(in: pool) {
                        self.board = board
                        self.refreshPermissions(for: board)
                    }
                } catch {}
            }

            // Observe issues
            let issueObservation = ValueObservation.tracking { db in
                try IssueEntity
                    .filter(Column("board_id") == self.boardId)
                    .fetchAll(db)
            }
            let issueTask = Task {
                do {
                    for try await issues in issueObservation.values(in: pool) {
                        self.issues = issues
                    }
                } catch {}
            }

            // Observe labels (team-scoped, need board's team)
            let labelObservation = ValueObservation.tracking { db in
                try LabelEntity.fetchAll(db)
            }
            let labelTask = Task {
                do {
                    for try await labels in labelObservation.values(in: pool) {
                        self.labels = labels
                    }
                } catch {}
            }

            // Observe issue labels
            let issueLabelObservation = ValueObservation.tracking { db in
                try IssueLabelEntity.fetchAll(db)
            }
            let issueLabelTask = Task {
                do {
                    for try await issueLabels in issueLabelObservation.values(in: pool) {
                        self.issueLabels = issueLabels
                    }
                } catch {}
            }

            // Observe users
            let userObservation = ValueObservation.tracking { db in
                try UserEntity.fetchAll(db)
            }
            let userTask = Task {
                do {
                    for try await users in userObservation.values(in: pool) {
                        self.users = users
                    }
                } catch {}
            }

            // Recompute permissions when membership or the members-shape sync
            // state changes. The board row observed above may never change
            // again after the members shape snapshots in, so without this the
            // "Syncing team…" banner would stick and controls would stay
            // read-only until the view is remounted. Tracks the two regions the
            // computation reads: the team_members table and the
            // "team-members" offset row (isLive).
            let permsObservation = ValueObservation.tracking { db -> (Int, Bool) in
                let count = try TeamMemberEntity.fetchCount(db)
                let live = try ElectricOffset.fetchOne(db, key: "team-members")?.isLive ?? false
                return (count, live)
            }
            let permsTask = Task {
                do {
                    for try await _ in permsObservation.values(in: pool) {
                        self.refreshPermissions(for: self.board)
                    }
                } catch {}
            }

            // Wait for cancellation
            _ = await (boardTask.value, issueTask.value, labelTask.value, issueLabelTask.value, userTask.value, permsTask.value)
        }
    }

    func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
    }

    // MARK: - Computed

    var filteredIssues: [IssueEntity] {
        issues.filter { issue in
            let status = IssueStatus.from(issue.status)
            let priority = IssuePriority.from(issue.priority)
            let issueLabelSet = Set(issueLabels.filter { $0.issueId == issue.id }.map(\.labelId))
            return matchesFilters(status: status, priority: priority, issueLabelIds: issueLabelSet, filters: filters)
        }
    }

    func issuesForStatus(_ status: IssueStatus) -> [IssueEntity] {
        // Canonical in-group ordering (EXP-38, cross-platform contract):
        // overdue → priority → due date → number for the non-terminal groups,
        // resolution recency for done/cancelled/duplicate.
        IssueSorting.sorted(
            filteredIssues.filter { IssueStatus.from($0.status) == status },
            status: status
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

    func setTab(_ tab: FilterTab) {
        activeTab = tab
        filters.statuses = tab.statuses
    }

    func toggleStatus(_ status: IssueStatus) {
        if filters.statuses.contains(status) {
            filters.statuses.remove(status)
        } else {
            filters.statuses.insert(status)
        }
        // Keep the tab pills in sync when a manual status mix matches a preset.
        activeTab = deriveTab(from: filters.statuses)
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
        activeTab = .all
    }

    func toggleStatusCollapsed(_ status: IssueStatus) {
        if collapsedStatuses.contains(status) {
            collapsedStatuses.remove(status)
        } else {
            collapsedStatuses.insert(status)
        }
    }

    // MARK: - Multi-select (EXP-239)

    /// Candidate issues for the selection bar's Start-coding sheet: this
    /// board's eligible issues — repo-backed board only, non-archived,
    /// non-terminal, not merged — recency-ordered. Mirrors
    /// AgentsViewModel.startCandidates but board-scoped (the bar lives on one
    /// board, which also guarantees the one-repository-per-run rule).
    func startCodingCandidates() -> [StartCodingSheet.IssueOption] {
        guard let board, board.archivedAt == nil, let repoId = board.repositoryId else { return [] }
        let terminal: Set<String> = [
            IssueStatus.done.rawValue,
            IssueStatus.cancelled.rawValue,
            IssueStatus.duplicate.rawValue,
        ]
        return issues
            .filter { row in
                if row.archivedAt != nil { return false }
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

    func setStatus(issueId: String, status: IssueStatus) async {
        do {
            try await issuesApi.update(accountId: accountId, UpdateIssueInput(id: issueId, status: status.rawValue))
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

    /// Bulk status change from the selection bar (EXP-239). Sequential — bar
    /// selections are small and each update is an independent server write.
    func bulkSetStatus(issueIds: [String], status: IssueStatus) async {
        for id in issueIds {
            await setStatus(issueId: id, status: status)
        }
    }

    func bulkSetPriority(issueIds: [String], priority: IssuePriority) async {
        for id in issueIds {
            await setPriority(issueId: id, priority: priority)
        }
    }

    func bulkSetAssignee(issueIds: [String], assigneeId: String?) async {
        for id in issueIds {
            await setAssignee(issueId: id, assigneeId: assigneeId)
        }
    }

    func bulkToggleLabel(issueIds: [String], labelId: String, add: Bool) async {
        for id in issueIds {
            do {
                if add {
                    try await labelsApi.addToIssue(accountId: accountId, issueId: id, labelId: labelId)
                } else {
                    try await labelsApi.removeFromIssue(accountId: accountId, issueId: id, labelId: labelId)
                }
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    /// Pull-to-refresh hook. Electric keeps the data live, so this only
    /// needs to give the spinner enough time to feel intentional.
    func refresh() async {
        try? await Task.sleep(nanoseconds: 200_000_000)
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
        singleMemberTeam = humanCount <= 1
    }
}
