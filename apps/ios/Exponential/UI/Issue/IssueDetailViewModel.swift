import ExpUI
import ExpCore
import Foundation
import GRDB

@MainActor @Observable
final class IssueDetailViewModel {
    var issue: IssueEntity?
    var labels: [LabelEntity] = []
    var issueLabels: [IssueLabelEntity] = []
    var users: [UserEntity] = []
    /// Live coding sessions for this issue (synced coding_sessions shape) —
    /// drives the "Coding now" badge + Watch/Steer entry (masterplan §5c).
    var runningSessions: [CodingSessionEntity] = []
    /// The canonical issue when this one is marked a duplicate — resolves the
    /// "Duplicate of {IDENTIFIER}" banner (masterplan §5e).
    var duplicateOf: IssueEntity?
    /// The issue's project — carries `workspaceId` + `repositoryId` so the repo
    /// name chip can resolve the backing repo (masterplan §6, R4).
    var project: ProjectEntity?
    /// The issue's workspace — needed (with the project + issue identifier) to
    /// build the shareable web URL.
    var workspace: WorkspaceEntity?
    /// Every synced project (all workspaces) — filtered to the issue's
    /// workspace by `moveTargetProjects` for the "Move to project" picker
    /// (EXP-57). Trashed projects never reach the local store (the projects
    /// shape filters `deleted_at IS NULL` server-side).
    var projects: [ProjectEntity] = []

    // Non-agent members offered by the editor's @-mention autocomplete.
    var mentionMembers: [MentionMember] {
        users.filter { !$0.isAgent }.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) }
    }
    var editingTitle: String = ""
    /// Single source of truth for the description editor (blocks + pending images).
    let editor = IssueEditorModel()
    var saving = false
    var error: String?
    var permissions: WorkspacePermissions = .denied
    // True while a signed-in viewer looks like a non-member ONLY because the
    // workspace_members shape hasn't synced yet (drives a "Syncing workspace…"
    // banner rather than silently rendering the issue read-only).
    var permissionsPending = false
    var isSubscribed = false
    /// True when the issue's workspace has exactly one human member: the
    /// assignee picker row is hidden (nothing to reassign to) — EXP-50.
    var singleMemberWorkspace = false

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let issueImagesApi: IssueImagesApi
    private let labelsApi: LabelsApi
    private let subscriptionsApi: SubscriptionsApi
    private let auth: AuthRepository
    private let baseURL: URL?
    /// Raw instance base string for building shareable web links.
    private let instanceUrl: String?
    private var observationTask: Task<Void, Never>?
    private var autosaveTask: Task<Void, Never>?

    init(
        accountId: String,
        issueId: String,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        issueImagesApi: IssueImagesApi,
        labelsApi: LabelsApi,
        subscriptionsApi: SubscriptionsApi,
        auth: AuthRepository
    ) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.issueImagesApi = issueImagesApi
        self.labelsApi = labelsApi
        self.subscriptionsApi = subscriptionsApi
        self.auth = auth
        let instanceUrl = auth.accounts.first(where: { $0.id == accountId })?.instanceUrl ?? auth.instanceUrl
        self.instanceUrl = instanceUrl
        self.baseURL = instanceUrl.flatMap { URL(string: $0) }
        editor.onEdit = { [weak self] in self?.scheduleAutosave() }
        // Inline `#IDENTIFIER` refs render as tappable pills when they resolve
        // against the local issues store (render-only; see IssueRefs).
        editor.issueRefResolver = { [weak self] identifier in
            self?.resolveIssueRef(identifier)
        }
        // Typing `#` offers same-workspace issues; selecting one inserts the
        // plain `#IDENTIFIER` interchange token.
        editor.issueRefSearch = { [weak self] query in
            self?.searchIssueRefs(query) ?? []
        }
    }

    /// identifier (e.g. `VER-12`) → local issue id, from the synced GRDB store
    /// (same workspace only). Synchronous lookup; nil when unknown (token
    /// stays plain).
    func resolveIssueRef(_ identifier: String) -> String? {
        IssueRefLookup.resolve(identifier, scope: .issue(id: issueId), db: db, accountId: accountId)
    }

    /// Issues offered by the description editor's #-autocomplete
    /// (workspace-scoped; identifier + title substring match).
    func searchIssueRefs(_ query: String) -> [IssueRefCandidate] {
        IssueRefLookup.search(query, scope: .issue(id: issueId), db: db, accountId: accountId)
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
                    self.refreshProject(issue: issue, pool: pool)
                    self.refreshDuplicateOf(issue: issue, pool: pool)
                }
            }

            // Live "coding now" sessions for this issue (14th synced shape).
            let sessionObs = ValueObservation.tracking { db in
                try CodingSessionEntity
                    .filter(Column("issue_id") == self.issueId)
                    .filter(Column("status") == "running")
                    .fetchAll(db)
            }
            Task {
                for try await sessions in sessionObs.values(in: pool) {
                    self.runningSessions = sessions
                }
            }

            let labelObs = ValueObservation.tracking { db in try LabelEntity.fetchAll(db) }
            Task {
                for try await labels in labelObs.values(in: pool) {
                    self.labels = labels
                }
            }

            // Workspace projects for the "Move to project" picker (EXP-57).
            let projectObs = ValueObservation.tracking { db in try ProjectEntity.fetchAll(db) }
            Task {
                for try await projects in projectObs.values(in: pool) {
                    self.projects = projects
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
                    // is_agent flips / member user rows arriving can change the
                    // human-member count, which gates the assignee row (EXP-50).
                    if let issue = self.issue {
                        self.refreshPermissions(for: issue)
                    }
                }
            }

            // Subscription state for the Bell toggle in the detail toolbar.
            let subObs = ValueObservation.tracking { db in
                try IssueSubscriberEntity.filter(Column("issue_id") == self.issueId).fetchAll(db)
            }
            Task {
                for try await subs in subObs.values(in: pool) {
                    let me = self.auth.userId
                    self.isSubscribed = me != nil && subs.contains { $0.userId == me && !$0.unsubscribed }
                }
            }

            // Recompute permissions when membership or the members-shape sync
            // state changes. The issue row observed above may not change again
            // after the members shape snapshots in, so without this the "Syncing
            // workspace…" banner would stick and the issue would stay read-only
            // until the view is remounted. Tracks the two regions the
            // computation reads: the workspace_members table and the
            // "workspace-members" offset row (isLive).
            let membersObs = ValueObservation.tracking { db -> (Int, Bool) in
                let count = try WorkspaceMemberEntity.fetchCount(db)
                let live = try ElectricOffset.fetchOne(db, key: "workspace-members")?.isLive ?? false
                return (count, live)
            }
            Task {
                for try await _ in membersObs.values(in: pool) {
                    if let issue = self.issue {
                        self.refreshPermissions(for: issue)
                    }
                }
            }
        }
    }

    func toggleSubscribe() async {
        let wasSubscribed = isSubscribed
        do {
            if wasSubscribed {
                try await subscriptionsApi.unsubscribe(accountId: accountId, issueId: issueId)
            } else {
                try await subscriptionsApi.subscribe(accountId: accountId, issueId: issueId)
            }
        } catch {
            self.error = error.localizedDescription
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

    /// Same-workspace projects the issue can move to (EXP-57): the current
    /// project and archived boards are excluded; name-sorted. Empty on a
    /// single-project workspace — the "Move to project" action hides then.
    var moveTargetProjects: [ProjectEntity] {
        guard let issue, let workspaceId = project?.workspaceId else { return [] }
        return projects
            .filter { $0.workspaceId == workspaceId && $0.id != issue.projectId && $0.archivedAt == nil }
            .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    /// Move the issue to another project in the same workspace (EXP-57). The
    /// issue keeps its id — the detail view's by-id observation stays valid —
    /// but the server renumbers it in the target project; the new
    /// projectId/identifier arrive via Electric sync (standard mutation
    /// pattern — no local optimistic write).
    func moveToProject(_ projectId: String) async {
        guard let issue, issue.projectId != projectId else { return }
        do {
            try await issuesApi.move(accountId: accountId, id: issue.id, projectId: projectId)
        } catch {
            self.error = error.localizedDescription
        }
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
            input.description = markdown
        }
        // Only baseline the editor when the server accepted the write: a failed
        // save must leave isDirty true so the next autosave / onDisappear commit
        // retries (the lastSavedMarkdown guard above stays open) and applyRemote
        // stashes remote content behind the reload banner instead of clobbering
        // the unsaved edit.
        guard await update(input) else { return }
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

    /// Create a workspace label and assign it to this issue in one step
    /// (parity with Android's createAndAssignLabel).
    func createAndAssignLabel(name: String, color: String) async {
        guard let issue, let workspaceId = project?.workspaceId else { return }
        do {
            let labelId = try await labelsApi.create(
                accountId: accountId,
                CreateLabelInput(name: name, color: color, workspaceId: workspaceId)
            )
            try await labelsApi.addToIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
        } catch {
            self.error = error.localizedDescription
        }
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

    // MARK: - Duplicate-of (masterplan §5e)

    /// Mark this issue as a duplicate of `canonical`: sets `duplicateOfId` AND
    /// flips status to the terminal `duplicate` in ONE atomic update.
    func markDuplicate(of canonical: IssueEntity) async {
        guard let issue, canonical.id != issue.id else { return }
        await update(UpdateIssueInput(
            id: issue.id,
            status: IssueStatus.duplicate.rawValue,
            duplicateOfId: canonical.id
        ))
    }

    /// Clear the duplicate marking: null the FK and restore a working status.
    func unmarkDuplicate() async {
        guard let issue else { return }
        var input = UpdateIssueInput(id: issue.id, status: IssueStatus.backlog.rawValue)
        input.explicitNulls.insert("duplicateOfId")
        await update(input)
    }

    /// Candidate canonical issues for the duplicate picker: every other issue
    /// in the same workspace (across projects), newest first. One-shot read —
    /// the picker is transient.
    func duplicateCandidates() async -> [IssueEntity] {
        guard let issue, let pool = try? db.pool(forAccountId: accountId) else { return [] }
        let issueId = issue.id
        let projectId = issue.projectId
        let result: [IssueEntity]? = try? await pool.read { db in
            guard let project = try ProjectEntity.fetchOne(db, key: projectId) else { return [] }
            let workspaceProjectIds = try ProjectEntity
                .filter(Column("workspace_id") == project.workspaceId)
                .fetchAll(db)
                .map(\.id)
            return try IssueEntity
                .filter(workspaceProjectIds.contains(Column("project_id")))
                .fetchAll(db)
                .filter { $0.id != issueId && $0.archivedAt == nil }
                .sorted { $0.updatedAt > $1.updatedAt }
        }
        return result ?? []
    }

    private func refreshProject(issue: IssueEntity, pool: DatabasePool) {
        guard project?.id != issue.projectId else { return }
        project = (try? pool.read { db in
            try ProjectEntity.fetchOne(db, key: issue.projectId)
        }) ?? nil
    }

    private func refreshDuplicateOf(issue: IssueEntity, pool: DatabasePool) {
        guard let canonicalId = issue.duplicateOfId else {
            duplicateOf = nil
            return
        }
        guard duplicateOf?.id != canonicalId else { return }
        duplicateOf = (try? pool.read { db in
            try IssueEntity.fetchOne(db, key: canonicalId)
        }) ?? nil
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

    /// Runs the tRPC issue update. Returns false (and surfaces the error via
    /// `self.error`) on failure so callers can keep local state dirty and retry
    /// instead of treating the write as landed.
    @discardableResult
    private func update(_ input: UpdateIssueInput) async -> Bool {
        do {
            try await issuesApi.update(accountId: accountId, input)
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    private func refreshPermissions(for issue: IssueEntity) {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let (workspace, membersLive): (WorkspaceEntity?, Bool) = (try? pool.read { db -> (WorkspaceEntity?, Bool) in
            let project = try ProjectEntity.fetchOne(db, key: issue.projectId)
            let ws = try project.flatMap { try WorkspaceEntity.fetchOne(db, key: $0.workspaceId) }
            let live = try ElectricOffset.fetchOne(db, key: "workspace-members")?.isLive ?? false
            return (ws, live)
        }) ?? (nil, false)
        self.workspace = workspace
        permissions = WorkspacePermissions.resolve(
            workspace: workspace,
            currentUserId: auth.userId,
            isAdmin: auth.isAdmin,
            dbPool: pool
        )
        permissionsPending = permissions.isAuthed && !permissions.isMember && !membersLive
        if let workspace {
            let humanIds = (try? pool.read { db in
                try humanWorkspaceMemberIds(workspaceId: workspace.id, db: db)
            }) ?? []
            singleMemberWorkspace = humanIds.count == 1
        } else {
            singleMemberWorkspace = false
        }
    }

    // MARK: - Share

    /// The shareable web URL for this issue, once the workspace, project and
    /// issue identifier are all resolved locally. Nil until then (or if the
    /// instance URL is unknown).
    var shareURL: URL? {
        guard let workspace, let project, let identifier = issue?.identifier, !identifier.isEmpty
        else { return nil }
        return WebLinks.issue(
            instanceUrl: instanceUrl,
            workspaceSlug: workspace.slug,
            projectSlug: project.slug,
            identifier: identifier
        )
    }

    /// Human-readable share subject: `{identifier}: {title}`.
    var shareText: String {
        guard let issue else { return "" }
        let id = issue.identifier ?? ""
        return id.isEmpty ? issue.title : "\(id): \(issue.title)"
    }

    private func formatDate(_ date: Date) -> String {
        AppDateFormatters.yyyyMMdd.string(from: date)
    }
}
