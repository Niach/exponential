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
    var project: ProjectEntity?
    var filters = IssueFilters()
    var activeTab: FilterTab = .all
    var collapsedStatuses: Set<IssueStatus> = []
    var permissions: WorkspacePermissions = .denied
    var error: String?

    private let accountId: String
    private let projectId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let projectsApi: ProjectsApi
    private let auth: AuthRepository
    private var observationTask: Task<Void, Never>?

    init(accountId: String, projectId: String, db: DatabaseManager, issuesApi: IssuesApi, projectsApi: ProjectsApi, auth: AuthRepository) {
        self.accountId = accountId
        self.projectId = projectId
        self.db = db
        self.issuesApi = issuesApi
        self.projectsApi = projectsApi
        self.auth = auth
    }

    func startObserving() {
        observationTask = Task { [weak self] in
            guard let self else { return }
            guard let pool = try? self.db.pool(forAccountId: self.accountId) else { return }

            // Observe project
            let projectObservation = ValueObservation.tracking { db in
                try ProjectEntity.fetchOne(db, key: self.projectId)
            }
            let projectTask = Task {
                do {
                    for try await project in projectObservation.values(in: pool) {
                        self.project = project
                        self.refreshPermissions(for: project)
                    }
                } catch {}
            }

            // Observe issues
            let issueObservation = ValueObservation.tracking { db in
                try IssueEntity
                    .filter(Column("project_id") == self.projectId)
                    .fetchAll(db)
            }
            let issueTask = Task {
                do {
                    for try await issues in issueObservation.values(in: pool) {
                        self.issues = issues
                    }
                } catch {}
            }

            // Observe labels (workspace-scoped, need project's workspace)
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

            // Wait for cancellation
            _ = await (projectTask.value, issueTask.value, labelTask.value, issueLabelTask.value, userTask.value)
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
        filteredIssues
            .filter { IssueStatus.from($0.status) == status }
            .sorted { a, b in
                (a.sortOrder ?? 0) < (b.sortOrder ?? 0)
            }
    }

    func labelsFor(issueId: String) -> [LabelEntity] {
        let labelIds = issueLabels.filter { $0.issueId == issueId }.map(\.labelId)
        return labels.filter { labelIds.contains($0.id) }
    }

    func userFor(id: String?) -> UserEntity? {
        guard let id else { return nil }
        return users.first { $0.id == id }
    }

    /// Labels belonging to this project's workspace (the pool holds every
    /// synced workspace's labels).
    var workspaceLabels: [LabelEntity] {
        guard let workspaceId = project?.workspaceId else { return [] }
        return labels.filter { $0.workspaceId == workspaceId }
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

    // MARK: - Mutations

    func setStatus(issueId: String, status: IssueStatus) async {
        do {
            try await issuesApi.update(accountId: accountId, UpdateIssueInput(id: issueId, status: status.rawValue))
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// Pull-to-refresh hook. Electric keeps the data live, so this only
    /// needs to give the spinner enough time to feel intentional.
    func refresh() async {
        try? await Task.sleep(nanoseconds: 200_000_000)
    }

    // MARK: - Permissions

    private func refreshPermissions(for project: ProjectEntity?) {
        guard let project else {
            permissions = .denied
            return
        }
        guard let pool = try? db.pool(forAccountId: accountId) else {
            permissions = .denied
            return
        }
        let workspace: WorkspaceEntity? = (try? pool.read { db -> WorkspaceEntity? in
            try WorkspaceEntity.fetchOne(db, key: project.workspaceId)
        }) ?? nil
        permissions = WorkspacePermissions.resolve(
            workspace: workspace,
            currentUserId: auth.userId,
            isAdmin: auth.isAdmin,
            dbPool: pool
        )
    }
}
