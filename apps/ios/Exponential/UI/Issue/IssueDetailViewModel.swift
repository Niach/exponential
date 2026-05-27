import Foundation
import GRDB

@MainActor @Observable
final class IssueDetailViewModel {
    var issue: IssueEntity?
    var labels: [LabelEntity] = []
    var issueLabels: [IssueLabelEntity] = []
    var users: [UserEntity] = []
    var editingTitle: String = ""
    var editingDescription: String = ""
    var pendingImages: [String: PendingImage] = [:]
    var saving = false
    var error: String?
    var permissions: WorkspacePermissions = .denied

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let labelsApi: LabelsApi
    private let auth: AuthRepository
    private var observationTask: Task<Void, Never>?

    init(accountId: String, issueId: String, db: DatabaseManager, issuesApi: IssuesApi, labelsApi: LabelsApi, auth: AuthRepository) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.labelsApi = labelsApi
        self.auth = auth
    }

    func startObserving() {
        observationTask = Task { [weak self] in
            guard let self else { return }
            let pool = try! self.db.pool(forAccountId: self.accountId)

            let issueObs = ValueObservation.tracking { db in
                try IssueEntity.fetchOne(db, key: self.issueId)
            }
            Task {
                for try await issue in issueObs.values(in: pool) {
                    if let issue {
                        let isFirstLoad = self.issue == nil
                        self.issue = issue
                        if isFirstLoad {
                            self.editingTitle = issue.title
                            self.editingDescription = getIssueDescriptionText(issue.description)
                        }
                        self.refreshPermissions(for: issue)
                    }
                }
            }

            let labelObs = ValueObservation.tracking { db in try LabelEntity.fetchAll(db) }
            Task {
                for try await labels in labelObs.values(in: pool) {
                    self.labels = labels
                }
            }

            let issueLabelObs = ValueObservation.tracking { db in
                try IssueLabelEntity.filter(Column("issue_id") == self.issueId).fetchAll(db)
            }
            Task {
                for try await il in issueLabelObs.values(in: pool) {
                    self.issueLabels = il
                }
            }

            let userObs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
            Task {
                for try await users in userObs.values(in: pool) {
                    self.users = users
                }
            }
        }
    }

    func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
    }

    var assignedLabelIds: Set<String> {
        Set(issueLabels.map(\.labelId))
    }

    func assignee() -> UserEntity? {
        guard let id = issue?.assigneeId else { return nil }
        return users.first { $0.id == id }
    }

    // MARK: - Mutations

    func saveTitle() async {
        guard let issue, editingTitle != issue.title, !editingTitle.isEmpty else { return }
        await update(UpdateIssueInput(id: issue.id, title: editingTitle))
    }

    func saveDescription() async {
        guard let issue else { return }
        let currentDesc = getIssueDescriptionText(issue.description)
        guard editingDescription != currentDesc else { return }
        let desc = editingDescription.isEmpty ? nil : IssueDescription(text: editingDescription)
        await update(UpdateIssueInput(id: issue.id, description: desc))
    }

    func setStatus(_ status: IssueStatus) async {
        guard let issue else { return }
        await update(UpdateIssueInput(id: issue.id, status: status.rawValue))
    }

    func setPriority(_ priority: IssuePriority) async {
        guard let issue else { return }
        await update(UpdateIssueInput(id: issue.id, priority: priority.rawValue))
    }

    func setAssignee(_ userId: String?) async {
        guard let issue else { return }
        if let userId {
            await update(UpdateIssueInput(id: issue.id, assigneeId: userId))
        } else {
            var input = UpdateIssueInput(id: issue.id)
            input.explicitNulls.insert("assigneeId")
            await update(input)
        }
    }

    func setDueDate(_ date: Date?) async {
        guard let issue else { return }
        if let date {
            await update(UpdateIssueInput(id: issue.id, dueDate: formatDate(date)))
        } else {
            var input = UpdateIssueInput(id: issue.id)
            input.explicitNulls.insert("dueDate")
            await update(input)
        }
    }

    func setDueTime(_ time: String?) async {
        guard let issue else { return }
        if let time {
            await update(UpdateIssueInput(id: issue.id, dueTime: time))
        } else {
            var input = UpdateIssueInput(id: issue.id)
            input.explicitNulls.insert("dueTime")
            await update(input)
        }
    }

    func setEndTime(_ time: String?) async {
        guard let issue else { return }
        if let time {
            await update(UpdateIssueInput(id: issue.id, endTime: time))
        } else {
            var input = UpdateIssueInput(id: issue.id)
            input.explicitNulls.insert("endTime")
            await update(input)
        }
    }

    func setRecurrence(interval: Int?, unit: RecurrenceUnit?) async {
        guard let issue else { return }
        var input = UpdateIssueInput(id: issue.id, recurrenceInterval: interval, recurrenceUnit: unit?.rawValue)
        if interval == nil { input.explicitNulls.insert("recurrenceInterval") }
        if unit == nil { input.explicitNulls.insert("recurrenceUnit") }
        await update(input)
    }

    func toggleLabel(_ labelId: String) async {
        guard let issue else { return }
        do {
            if assignedLabelIds.contains(labelId) {
                try await labelsApi.removeFromIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
            } else {
                try await labelsApi.addToIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
            }
        } catch {
            self.error = error.localizedDescription
        }
    }

    func deleteIssue() async -> Bool {
        guard let issue else { return false }
        do {
            try await issuesApi.delete(accountId: accountId, id: issue.id)
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    private func isoNow() -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f.string(from: Date())
    }

    private func update(_ input: UpdateIssueInput) async {
        do {
            try await issuesApi.update(accountId: accountId, input)
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func refreshPermissions(for issue: IssueEntity) {
        let pool = try! db.pool(forAccountId: accountId)
        let workspace: WorkspaceEntity? = (try? pool.read { db -> WorkspaceEntity? in
            guard let project = try ProjectEntity.fetchOne(db, key: issue.projectId) else {
                return nil
            }
            return try WorkspaceEntity.fetchOne(db, key: project.workspaceId)
        }) ?? nil
        permissions = WorkspacePermissions.resolve(
            workspace: workspace,
            currentUserId: auth.userId,
            isAdmin: auth.isAdmin,
            dbPool: pool
        )
    }

    private func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }
}
