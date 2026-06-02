import ExpCore
import Foundation
import GRDB

@MainActor @Observable
final class IssueDetailViewModel {
    var issue: IssueEntity?
    var labels: [LabelEntity] = []
    var issueLabels: [IssueLabelEntity] = []
    var users: [UserEntity] = []
    var editingTitle: String = ""
    /// Single source of truth for the description editor (blocks + pending images).
    let editor = IssueEditorModel()
    var saving = false
    var error: String?
    var permissions: WorkspacePermissions = .denied

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let issueImagesApi: IssueImagesApi
    private let labelsApi: LabelsApi
    private let auth: AuthRepository
    private let baseURL: URL?
    private var observationTask: Task<Void, Never>?
    private var autosaveTask: Task<Void, Never>?

    init(
        accountId: String,
        issueId: String,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        issueImagesApi: IssueImagesApi,
        labelsApi: LabelsApi,
        auth: AuthRepository
    ) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.issueImagesApi = issueImagesApi
        self.labelsApi = labelsApi
        self.auth = auth
        let instanceUrl = auth.accounts.first(where: { $0.id == accountId })?.instanceUrl ?? auth.instanceUrl
        self.baseURL = instanceUrl.flatMap { URL(string: $0) }
        editor.onEdit = { [weak self] in self?.scheduleAutosave() }
    }

    func startObserving() {
        observationTask = Task { [weak self] in
            guard let self else { return }
            guard let pool = try? self.db.pool(forAccountId: self.accountId) else {
                self.error = "Couldn't open local data store"
                return
            }

            let issueObs = ValueObservation.tracking { db in
                try IssueEntity.fetchOne(db, key: self.issueId)
            }
            Task {
                for try await issue in issueObs.values(in: pool) {
                    guard let issue else { continue }
                    let isFirstLoad = self.issue == nil
                    self.issue = issue
                    let remoteText = getIssueDescriptionText(issue.description)
                    if isFirstLoad {
                        self.editingTitle = issue.title
                        self.editor.load(markdown: remoteText, baseURL: self.baseURL)
                    } else {
                        // Live-apply remote edits when safe; otherwise stash for
                        // a user-driven reload (field-level last-write-wins).
                        self.editor.applyRemote(markdown: remoteText, baseURL: self.baseURL)
                    }
                    self.refreshPermissions(for: issue)
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
        autosaveTask?.cancel()
        autosaveTask = nil
    }

    var assignedLabelIds: Set<String> {
        Set(issueLabels.map(\.labelId))
    }

    func assignee() -> UserEntity? {
        guard let id = issue?.assigneeId else { return nil }
        return users.first { $0.id == id }
    }

    func reloadRemoteDescription() {
        editor.reloadPendingRemote(baseURL: baseURL)
    }

    // MARK: - Mutations

    func saveTitle() async {
        guard let issue, editingTitle != issue.title, !editingTitle.isEmpty else { return }
        await update(UpdateIssueInput(id: issue.id, title: editingTitle))
    }

    /// Upload any pending draft images, then persist the description — but only
    /// if every image resolved (all-or-nothing). On partial failure the failed
    /// drafts stay pending with a retry affordance and nothing is saved.
    func commitDescription() async {
        guard let issue else { return }

        let allUploaded = await editor.commitPendingImages(uploader: makeImageUploader(issueId: issue.id))
        guard allUploaded, !editor.hasUncommittedDrafts else {
            error = "Some images couldn't be uploaded. Tap an image to retry."
            return
        }
        error = nil

        let markdown = editor.currentMarkdown()
        guard markdown != editor.lastSavedMarkdown else { return }

        var input = UpdateIssueInput(id: issue.id)
        if markdown.isEmpty {
            input.explicitNulls.insert("description")
        } else {
            input.description = IssueDescription(text: markdown)
        }
        await update(input)
        editor.markSaved(markdown)
    }

    private func makeImageUploader(issueId: String) -> @Sendable (PendingImage) async throws -> String {
        let api = issueImagesApi
        let accountId = accountId
        return { image in
            let uploaded = try await api.upload(
                accountId: accountId,
                issueId: issueId,
                data: image.data,
                filename: image.filename,
                contentType: image.contentType
            )
            return uploaded.url
        }
    }

    private func scheduleAutosave() {
        autosaveTask?.cancel()
        autosaveTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 1_200_000_000)
            guard !Task.isCancelled else { return }
            // Run the save in an INDEPENDENT task: the debounce timer above is
            // cancelled by the next keystroke, and we must not let that cancel
            // an in-flight save's network request mid-write.
            self?.saveNow()
        }
    }

    private func saveNow() {
        Task { [weak self] in await self?.commitDescription() }
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

    private func update(_ input: UpdateIssueInput) async {
        do {
            try await issuesApi.update(accountId: accountId, input)
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func refreshPermissions(for issue: IssueEntity) {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
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
        AppDateFormatters.yyyyMMdd.string(from: date)
    }
}
