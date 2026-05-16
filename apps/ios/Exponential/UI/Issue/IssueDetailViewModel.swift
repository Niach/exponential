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
    var saving = false
    var error: String?

    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let labelsApi: LabelsApi
    private var observationTask: Task<Void, Never>?

    init(issueId: String, db: DatabaseManager, issuesApi: IssuesApi, labelsApi: LabelsApi) {
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.labelsApi = labelsApi
    }

    func startObserving() {
        observationTask = Task { [weak self] in
            guard let self else { return }

            let issueObs = ValueObservation.tracking { db in
                try IssueEntity.fetchOne(db, key: self.issueId)
            }
            Task {
                for try await issue in issueObs.values(in: self.db.dbPool) {
                    if let issue {
                        let isFirstLoad = self.issue == nil
                        self.issue = issue
                        if isFirstLoad {
                            self.editingTitle = issue.title
                            self.editingDescription = issue.description ?? ""
                        }
                    }
                }
            }

            let labelObs = ValueObservation.tracking { db in try LabelEntity.fetchAll(db) }
            Task {
                for try await labels in labelObs.values(in: self.db.dbPool) {
                    self.labels = labels
                }
            }

            let issueLabelObs = ValueObservation.tracking { db in
                try IssueLabelEntity.filter(Column("issue_id") == self.issueId).fetchAll(db)
            }
            Task {
                for try await il in issueLabelObs.values(in: self.db.dbPool) {
                    self.issueLabels = il
                }
            }

            let userObs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
            Task {
                for try await users in userObs.values(in: self.db.dbPool) {
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
        let currentDesc = issue.description ?? ""
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
        await update(UpdateIssueInput(id: issue.id, assigneeId: userId))
    }

    func setDueDate(_ date: Date?) async {
        guard let issue else { return }
        let dateStr = date.map { formatDate($0) }
        await update(UpdateIssueInput(id: issue.id, dueDate: dateStr))
    }

    func toggleLabel(_ labelId: String) async {
        guard let issue else { return }
        do {
            if assignedLabelIds.contains(labelId) {
                try await labelsApi.removeFromIssue(issueId: issue.id, labelId: labelId)
            } else {
                try await labelsApi.addToIssue(issueId: issue.id, labelId: labelId)
            }
        } catch {
            self.error = error.localizedDescription
        }
    }

    func deleteIssue() async -> Bool {
        guard let issue else { return false }
        do {
            try await issuesApi.delete(id: issue.id)
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    private func update(_ input: UpdateIssueInput) async {
        do {
            try await issuesApi.update(input)
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }
}
