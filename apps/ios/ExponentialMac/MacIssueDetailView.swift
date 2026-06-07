import ExpCore
import ExpUI
import Foundation
import GRDB
import os
import SwiftUI

private let detailLog = Logger(subsystem: "at.exponential.mac", category: "MacIssueDetail")

@MainActor
@Observable
final class MacIssueDetailModel {
    var issue: IssueEntity?
    var labels: [LabelEntity] = []
    var issueLabels: [IssueLabelEntity] = []
    var users: [UserEntity] = []
    var comments: [CommentEntity] = []
    var issueEvents: [IssueEventEntity] = []
    var attachments: [AttachmentEntity] = []
    var agentRun: AgentRunEntity?
    var permissions: WorkspacePermissions?
    var workspaceId: String?
    var projectName: String?
    var isSubscribed = false
    var error: String?

    // Title is a local buffer (seeded once so live sync doesn't clobber typing);
    // the description is owned by the shared block editor.
    var editingTitle = ""
    let editor = IssueEditorModel()
    private var seeded = false
    private var saveTask: Task<Void, Never>?

    let accountId: String
    let issueId: String
    private let deps: MacAppDependencies
    private var tasks: [Task<Void, Never>] = []

    var baseURL: URL? { deps.auth.instanceBaseURL(forAccountId: accountId) }
    var httpClient: HTTPClient { deps.httpClient }

    init(deps: MacAppDependencies, accountId: String, issueId: String) {
        self.deps = deps
        self.accountId = accountId
        self.issueId = issueId
    }

    // MARK: - Derived

    var assignedLabelIds: Set<String> { Set(issueLabels.map(\.labelId)) }
    var assignedLabels: [LabelEntity] {
        labels.filter { assignedLabelIds.contains($0.id) }.sorted { $0.name < $1.name }
    }
    var availableLabels: [LabelEntity] { labels.sorted { $0.name < $1.name } }
    var assignee: UserEntity? {
        guard let aid = issue?.assigneeId else { return nil }
        return users.first { $0.id == aid }
    }
    var canModerate: Bool { permissions?.isModerator ?? false }
    var canEditContent: Bool { permissions?.canMutateIssue(creatorId: issue?.creatorId) ?? false }
    func user(_ id: String?) -> UserEntity? { id.flatMap { uid in users.first { $0.id == uid } } }
    // Assignee picker segmentation: human members vs agent sub-users (is_agent).
    var peopleUsers: [UserEntity] { users.filter { !$0.isAgent } }
    var agentUsers: [UserEntity] { users.filter { $0.isAgent } }
    // Non-agent members offered by the editors' @-mention autocomplete.
    var mentionMembers: [MentionMember] {
        peopleUsers.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) }
    }

    // MARK: - Observation

    func start() {
        guard tasks.isEmpty, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let issueId = issueId
        let issueObs = ValueObservation.tracking { db in try IssueEntity.fetchOne(db, key: issueId) }
        let labelObs = ValueObservation.tracking { db in try LabelEntity.fetchAll(db) }
        let issueLabelObs = ValueObservation.tracking { db in
            try IssueLabelEntity.filter(Column("issue_id") == issueId).fetchAll(db)
        }
        let userObs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
        let commentObs = ValueObservation.tracking { db in
            try CommentEntity.filter(Column("issue_id") == issueId).order(Column("created_at").asc).fetchAll(db)
        }
        let attachmentObs = ValueObservation.tracking { db in
            try AttachmentEntity.filter(Column("issue_id") == issueId).order(Column("created_at").asc).fetchAll(db)
        }
        let eventObs = ValueObservation.tracking { db in
            try IssueEventEntity.filter(Column("issue_id") == issueId).order(Column("created_at").asc).fetchAll(db)
        }
        let subObs = ValueObservation.tracking { db in
            try IssueSubscriberEntity.filter(Column("issue_id") == issueId).fetchAll(db)
        }
        let agentRunObs = ValueObservation.tracking { db in
            try AgentRunEntity.fetchOne(db, key: issueId)
        }

        tasks.append(Task { @MainActor [weak self] in
            do { for try await row in issueObs.values(in: pool) { self?.applyIssue(row) } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in labelObs.values(in: pool) { self?.labels = rows } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in issueLabelObs.values(in: pool) { self?.issueLabels = rows } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in userObs.values(in: pool) { self?.users = rows } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in commentObs.values(in: pool) { self?.comments = rows } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in attachmentObs.values(in: pool) { self?.attachments = rows } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in eventObs.values(in: pool) { self?.issueEvents = rows } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await row in agentRunObs.values(in: pool) { self?.agentRun = row } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do {
                for try await subs in subObs.values(in: pool) {
                    let me = self?.deps.auth.userId
                    self?.isSubscribed = me != nil && subs.contains { $0.userId == me && !$0.unsubscribed }
                }
            } catch {}
        })
    }

    func stop() {
        tasks.forEach { $0.cancel() }
        tasks = []
    }

    private func applyIssue(_ issue: IssueEntity?) {
        self.issue = issue
        guard let issue else { return }
        if !seeded {
            seeded = true
            editingTitle = issue.title
            editor.load(markdown: getIssueDescriptionText(issue.description), baseURL: baseURL)
            editor.onEdit = { [weak self] in self?.scheduleDescriptionSave() }
        } else {
            editor.applyRemote(markdown: getIssueDescriptionText(issue.description), baseURL: baseURL)
        }
        resolvePermissions(for: issue)
    }

    private func resolvePermissions(for issue: IssueEntity) {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        // Fetch project + workspace in a single read so they stay consistent if a
        // concurrent sync deletes the project mid-resolution.
        let resolved = try? pool.read { db -> (ProjectEntity?, WorkspaceEntity?) in
            let project = try ProjectEntity.fetchOne(db, key: issue.projectId)
            let workspace = try project.flatMap { try WorkspaceEntity.fetchOne(db, key: $0.workspaceId) }
            return (project, workspace)
        }
        projectName = resolved?.0?.name
        let workspace = resolved?.1
        workspaceId = workspace?.id
        permissions = WorkspacePermissions.resolve(
            workspace: workspace,
            currentUserId: deps.auth.userId,
            isAdmin: deps.auth.isAdmin,
            dbPool: pool
        )
    }

    // MARK: - Mutations

    private func update(_ input: UpdateIssueInput) async {
        do { try await deps.issuesApi.update(accountId: accountId, input) }
        catch { self.error = error.localizedDescription }
    }

    func saveTitle() async {
        guard let issue, !editingTitle.isEmpty, editingTitle != issue.title else { return }
        await update(UpdateIssueInput(id: issue.id, title: editingTitle))
    }

    func scheduleDescriptionSave() {
        saveTask?.cancel()
        saveTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(1.2))
            guard !Task.isCancelled else { return }
            await self?.commitDescription()
        }
    }

    /// Upload any pending images, then persist the derived markdown (mirrors the
    /// iOS IssueDetailViewModel.commitDescription flow).
    func commitDescription() async {
        guard let issue else { return }
        let uploader = makeImageUploader(issueId: issue.id)
        let allUploaded = await editor.commitPendingImages(uploader: uploader)
        guard allUploaded, !editor.hasUncommittedDrafts else {
            error = "Some images couldn't be uploaded."
            return
        }
        let markdown = editor.currentMarkdown()
        guard markdown != editor.lastSavedMarkdown else { return }
        var input = UpdateIssueInput(id: issue.id)
        if markdown.isEmpty {
            input.explicitNulls.insert("description")
        } else {
            input.description = markdown
        }
        await update(input)
        editor.markSaved(markdown)
    }

    private func makeImageUploader(issueId: String) -> @Sendable (PendingImage) async throws -> String {
        let api = deps.issueImagesApi
        let accountId = accountId
        return { image in
            let uploaded = try await api.upload(
                accountId: accountId, issueId: issueId,
                data: image.data, filename: image.filename, contentType: image.contentType
            )
            return uploaded.url
        }
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
        var input = UpdateIssueInput(id: issue.id)
        if let userId {
            input.assigneeId = userId
        } else {
            input.explicitNulls.insert("assigneeId")
        }
        await update(input)
    }

    func setDueDate(_ date: Date?) async {
        guard let issue else { return }
        var input = UpdateIssueInput(id: issue.id)
        if let date {
            let f = DateFormatter()
            f.dateFormat = "yyyy-MM-dd"
            f.locale = Locale(identifier: "en_US_POSIX")
            input.dueDate = f.string(from: date)
        } else {
            input.explicitNulls.insert("dueDate")
        }
        await update(input)
    }

    func setDueTime(_ time: String?) async {
        guard let issue else { return }
        var input = UpdateIssueInput(id: issue.id)
        if let time { input.dueTime = time } else { input.explicitNulls.insert("dueTime") }
        await update(input)
    }

    func setEndTime(_ time: String?) async {
        guard let issue else { return }
        var input = UpdateIssueInput(id: issue.id)
        if let time { input.endTime = time } else { input.explicitNulls.insert("endTime") }
        await update(input)
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
                try await deps.labelsApi.removeFromIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
            } else {
                try await deps.labelsApi.addToIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
            }
        } catch {
            detailLog.error("toggleLabel failed: \(error.localizedDescription, privacy: .public)")
            self.error = error.localizedDescription
        }
    }

    func createLabelAndAssign(name: String) async {
        guard let issue, let workspaceId else {
            self.error = "Can't create label — workspace not resolved yet."
            return
        }
        let color = LABEL_COLORS[labels.count % LABEL_COLORS.count]
        do {
            let newId = try await deps.labelsApi.create(
                accountId: accountId,
                CreateLabelInput(name: name, color: color, workspaceId: workspaceId)
            )
            try await deps.labelsApi.addToIssue(accountId: accountId, issueId: issue.id, labelId: newId)
        } catch {
            detailLog.error("createLabel failed: \(error.localizedDescription, privacy: .public)")
            self.error = error.localizedDescription
        }
    }

    func addComment(_ text: String) async {
        guard let issue else { return }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        do { try await deps.commentsApi.create(accountId: accountId, issueId: issue.id, text: trimmed) }
        catch { self.error = error.localizedDescription }
    }

    func updateComment(_ id: String, text: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        do { try await deps.commentsApi.update(accountId: accountId, id: id, text: trimmed) }
        catch { self.error = error.localizedDescription }
    }

    func deleteComment(_ id: String) async {
        do { try await deps.commentsApi.delete(accountId: accountId, id: id) }
        catch { self.error = error.localizedDescription }
    }

    func attachmentURL(_ attachment: AttachmentEntity) -> URL? {
        if attachment.url.hasPrefix("http") { return URL(string: attachment.url) }
        guard let base = deps.auth.instanceBaseURL(forAccountId: accountId) else { return nil }
        return URL(string: base.absoluteString + attachment.url)
    }

    func deleteIssue() async -> Bool {
        guard let issue else { return false }
        do {
            try await deps.issuesApi.delete(accountId: accountId, id: issue.id)
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    func approvePlan() async {
        guard let issue else { return }
        try? await deps.agentPlanApi.approvePlan(accountId: accountId, issueId: issue.id)
    }

    func requestChanges() async {
        guard let issue else { return }
        try? await deps.agentPlanApi.requestChanges(accountId: accountId, issueId: issue.id)
    }

    func answerQuestion(_ answer: String) async {
        guard let issue else { return }
        let trimmed = answer.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        do { try await deps.agentPlanApi.answerQuestion(accountId: accountId, issueId: issue.id, answer: trimmed) }
        catch { self.error = error.localizedDescription }
    }

    func retry() async {
        guard let issue else { return }
        try? await deps.agentPlanApi.retry(accountId: accountId, issueId: issue.id)
    }

    /// Approve the plan with the human session, THEN resume the interactive session
    /// to implement it. Order is load-bearing — the agent credential can't approve;
    /// the host approves (human session), then only resumes the session.
    func approveAndContinue(workspaceId: String) async {
        guard let issue else { return }
        await approvePlan()
        deps.agentService.approveInteractive(workspaceId: workspaceId, issueId: issue.id)
    }

    func toggleSubscribe() async {
        guard let issue else { return }
        do {
            if isSubscribed {
                try await deps.subscriptionsApi.unsubscribe(accountId: accountId, issueId: issue.id)
            } else {
                try await deps.subscriptionsApi.subscribe(accountId: accountId, issueId: issue.id)
            }
        } catch {
            self.error = error.localizedDescription
        }
    }
}

struct MacIssueDetailView: View {
    @Environment(MacAppDependencies.self) private var deps
    let accountId: String
    let issueId: String
    var onDelete: () -> Void = {}

    @State private var model: MacIssueDetailModel?
    @State private var showDeleteConfirm = false
    @State private var composerEditor = IssueEditorModel()
    @State private var composerHasText = false
    @State private var submittingComment = false
    @State private var showDuePicker = false
    @State private var showLabelPicker = false
    @State private var newLabelName = ""
    @State private var editingCommentId: String?
    @State private var editDraft = ""
    @FocusState private var titleFocused: Bool

    // Below this content width the right rail collapses inline under the title.
    private let railBreakpoint: CGFloat = 900

    var body: some View {
        Group {
            if let model, let issue = model.issue {
                content(model, issue: issue)
            } else {
                ProgressView()
            }
        }
        .onAppear {
            if model == nil {
                let m = MacIssueDetailModel(deps: deps, accountId: accountId, issueId: issueId)
                model = m
                m.start()
            }
        }
        .onDisappear {
            // Flush both buffers — focus-loss may not fire when the detail view is
            // swapped out wholesale, so a pending title/description edit would
            // otherwise be dropped.
            if let m = model { Task { await m.saveTitle(); await m.commitDescription(); m.stop() } }
        }
    }

    // Two-pane on wide windows (web parity: main content + right property rail);
    // single column with the rail inline under the title when narrow.
    @ViewBuilder
    private func content(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        GeometryReader { geo in
            let wide = geo.size.width >= railBreakpoint
            if wide {
                HStack(alignment: .top, spacing: 0) {
                    ScrollView {
                        mainColumn(model, issue: issue)
                            .padding(24)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    Divider()
                    ScrollView {
                        propertyRail(model, issue: issue).padding(20)
                    }
                    .frame(width: 320)
                }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 18) {
                        header(model, issue: issue)
                        propertyRail(model, issue: issue)
                        Divider()
                        descriptionSection(model)
                        Divider()
                        attachmentsSection(model)
                        MacAgentPanel(model: model, issue: issue)
                        MacAgentActivityFeed(events: model.issueEvents, user: model.user)
                        Divider()
                        commentsSection(model)
                        errorText(model)
                    }
                    .padding(24)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .navigationTitle(issue.identifier ?? "Issue")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { Task { await model.toggleSubscribe() } } label: {
                    Image(systemName: model.isSubscribed ? "bell.fill" : "bell.slash")
                }
                .help(model.isSubscribed ? "Unsubscribe from this issue" : "Subscribe to this issue")
            }
            if deps.agentService.canRunInteractive(workspaceId: model.workspaceId ?? "") {
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        if let wid = model.workspaceId {
                            deps.agentService.requestInteractive(workspaceId: wid, issueId: issue.id)
                        }
                    } label: {
                        Label("AI", systemImage: "sparkles")
                    }
                    .help("Start an interactive agent session for this issue")
                    .disabled(model.workspaceId == nil)
                }
            }
            ToolbarItem(placement: .destructiveAction) {
                Button(role: .destructive) { showDeleteConfirm = true } label: {
                    Image(systemName: "trash")
                }
                .disabled(!model.canModerate)
                .help("Delete issue")
            }
        }
        .confirmationDialog("Delete this issue?", isPresented: $showDeleteConfirm, titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                Task { if await model.deleteIssue() { onDelete() } }
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    private func mainColumn(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        VStack(alignment: .leading, spacing: 18) {
            header(model, issue: issue)
            Divider()
            descriptionSection(model)
            Divider()
            attachmentsSection(model)
            MacAgentPanel(model: model, issue: issue)
            MacAgentActivityFeed(events: model.issueEvents, user: model.user)
            Divider()
            commentsSection(model)
            errorText(model)
        }
    }

    @ViewBuilder
    private func errorText(_ model: MacIssueDetailModel) -> some View {
        if let error = model.error {
            Text(error).font(.callout).foregroundStyle(.red)
        }
    }

    // MARK: - Header (identifier + title)

    private func header(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let identifier = issue.identifier {
                Text(identifier).font(.caption.monospaced()).foregroundStyle(.tertiary)
            }
            TextField("Title", text: Binding(get: { model.editingTitle }, set: { model.editingTitle = $0 }), axis: .vertical)
                .font(.title2.weight(.semibold))
                .textFieldStyle(.plain)
                .focused($titleFocused)
                .disabled(!model.canEditContent)
                .onChange(of: titleFocused) { _, focused in
                    if !focused { Task { await model.saveTitle() } }
                }
        }
    }

    // MARK: - Property rail (web order: Status / Priority / Assignee / Labels / Due / Project)

    @ViewBuilder
    private func propertyRail(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            railRow("Status") { statusMenu(model, issue: issue) }
            railRow("Priority") { priorityMenu(model, issue: issue) }
            railRow("Assignee") { assigneeMenu(model) }
            labelsRailRow(model)
            railRow("Due date") { dueDateControl(model, issue: issue) }
            if issue.dueDate != nil {
                railRow("Start time") {
                    MacTimeFieldButton(value: issue.dueTime) { v in Task { await model.setDueTime(v) } }
                        .disabled(!model.canModerate)
                }
                railRow("End time") {
                    MacTimeFieldButton(value: issue.endTime) { v in Task { await model.setEndTime(v) } }
                        .disabled(!model.canModerate)
                }
            }
            railRow("Repeat") {
                MacRecurrenceMenu(
                    interval: issue.recurrenceInterval,
                    unit: issue.recurrenceUnit,
                    onSelect: { i, u in Task { await model.setRecurrence(interval: i, unit: u) } },
                    enabled: model.canModerate
                )
            }
            railRow("Project") {
                Text(model.projectName ?? "—").font(.subheadline).foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .glassSection()
        .opacity(model.canModerate ? 1 : 0.7)
    }

    @ViewBuilder
    private func railRow<Content: View>(_ label: String, @ViewBuilder content: () -> Content) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(label).font(.subheadline).foregroundStyle(.secondary).frame(width: 84, alignment: .leading)
            Spacer(minLength: 8)
            content()
        }
        .padding(.vertical, 6)
    }

    private func statusMenu(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        Menu {
            ForEach(IssueStatus.displayOrder, id: \.self) { s in
                Button { Task { await model.setStatus(s) } } label: { Label(s.label, systemImage: s.sfSymbol) }
            }
        } label: {
            let s = IssueStatus.from(issue.status)
            Label(s.label, systemImage: s.sfSymbol).foregroundStyle(s.color)
        }
        .menuStyle(.borderlessButton).fixedSize().disabled(!model.canModerate)
    }

    private func priorityMenu(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        Menu {
            ForEach(IssuePriority.displayOrder, id: \.self) { p in
                Button { Task { await model.setPriority(p) } } label: { Label(p.label, systemImage: p.sfSymbol) }
            }
        } label: {
            let p = IssuePriority.from(issue.priority)
            Label(p.label, systemImage: p.sfSymbol).foregroundStyle(p.color)
        }
        .menuStyle(.borderlessButton).fixedSize().disabled(!model.canModerate)
    }

    private func assigneeMenu(_ model: MacIssueDetailModel) -> some View {
        let assignee = model.assignee
        return Menu {
            Button("Unassigned") { Task { await model.setAssignee(nil) } }
            if !model.peopleUsers.isEmpty {
                Section("People") {
                    ForEach(model.peopleUsers) { u in
                        Button(u.name ?? u.email) { Task { await model.setAssignee(u.id) } }
                    }
                }
            }
            if !model.agentUsers.isEmpty {
                Section("Agents") {
                    ForEach(model.agentUsers) { u in
                        Button { Task { await model.setAssignee(u.id) } } label: {
                            Label("\(u.name ?? u.email) · agent", systemImage: "cpu")
                        }
                    }
                }
            }
        } label: {
            Label(
                assignee?.name ?? assignee?.email ?? "Unassigned",
                systemImage: assignee?.isAgent == true ? "cpu" : "person.crop.circle"
            )
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
        .menuStyle(.borderlessButton).fixedSize().disabled(!model.canModerate)
    }

    // Due date as a button that opens a graphical calendar popover (mirrors the
    // web's react-day-picker popover + iOS).
    @ViewBuilder
    private func dueDateControl(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        let due = Self.parseDate(issue.dueDate)
        HStack(spacing: 4) {
            Button { showDuePicker = true } label: {
                Label(
                    due.map { $0.formatted(date: .abbreviated, time: .omitted) } ?? "Add due date",
                    systemImage: due == nil ? "calendar.badge.plus" : "calendar"
                )
                .foregroundStyle(due == nil ? Color.secondary : Color.primary)
            }
            .buttonStyle(.borderless)
            .fixedSize()
            .disabled(!model.canModerate)
            .popover(isPresented: $showDuePicker, arrowEdge: .bottom) {
                DatePicker(
                    "Due date",
                    selection: Binding(
                        get: { due ?? Date() },
                        set: { newDate in
                            showDuePicker = false
                            Task { await model.setDueDate(newDate) }
                        }
                    ),
                    displayedComponents: [.date]
                )
                .datePickerStyle(.graphical)
                .labelsHidden()
                .padding(12)
            }
            if due != nil {
                Button { Task { await model.setDueDate(nil) } } label: {
                    Image(systemName: "xmark.circle.fill")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.tertiary)
                .disabled(!model.canModerate)
            }
        }
    }

    // MARK: - Labels (rail row)

    @ViewBuilder
    private func labelsRailRow(_ model: MacIssueDetailModel) -> some View {
        HStack(alignment: .top) {
            Text("Labels").font(.subheadline).foregroundStyle(.secondary).frame(width: 84, alignment: .leading)
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 6) {
                Button { showLabelPicker = true } label: { Label("Add", systemImage: "tag").font(.caption) }
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
                    .disabled(!model.canEditContent)
                    .popover(isPresented: $showLabelPicker, arrowEdge: .bottom) { labelPicker(model) }
                if !model.assignedLabels.isEmpty {
                    MacFlowLayout(spacing: 6) {
                        ForEach(model.assignedLabels) { label in
                            Button { Task { await model.toggleLabel(label.id) } } label: {
                                HStack(spacing: 4) {
                                    Circle().fill(Color(hex: label.color) ?? .gray).frame(width: 8, height: 8)
                                    Text(label.name).font(.caption)
                                    Image(systemName: "xmark").font(.system(size: 8, weight: .bold)).foregroundStyle(.tertiary)
                                }
                                .padding(.horizontal, 8).padding(.vertical, 3)
                            }
                            .buttonStyle(.plain)
                            .glassButton()
                            .disabled(!model.canEditContent)
                        }
                    }
                }
            }
        }
        .padding(.vertical, 6)
    }

    @ViewBuilder
    private func labelPicker(_ model: MacIssueDetailModel) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            if model.availableLabels.isEmpty {
                Text("No labels yet — create one below.").font(.caption).foregroundStyle(.secondary)
            } else {
                ForEach(model.availableLabels) { label in
                    Button { Task { await model.toggleLabel(label.id) } } label: {
                        HStack(spacing: 8) {
                            Circle().fill(Color(hex: label.color) ?? .gray).frame(width: 9, height: 9)
                            Text(label.name)
                            Spacer()
                            if model.assignedLabelIds.contains(label.id) {
                                Image(systemName: "checkmark").foregroundStyle(.tint)
                            }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            Divider().padding(.vertical, 2)
            HStack(spacing: 6) {
                TextField("New label", text: $newLabelName)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit { submitNewLabel(model) }
                Button("Create") { submitNewLabel(model) }
                    .disabled(newLabelName.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(12)
        .frame(width: 240)
    }

    private func submitNewLabel(_ model: MacIssueDetailModel) {
        let name = newLabelName.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        newLabelName = ""
        Task { await model.createLabelAndAssign(name: name) }
    }

    // MARK: - Description (rich block-markdown editor)

    private func descriptionSection(_ model: MacIssueDetailModel) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Description").font(.subheadline.weight(.medium)).foregroundStyle(.secondary)
            MacMarkdownEditor(
                model: model.editor,
                baseURL: model.baseURL,
                accountId: model.accountId,
                httpClient: model.httpClient,
                mentionMembers: model.mentionMembers
            )
            // Grow with content (no fixed/max height) — the page ScrollView owns
            // scrolling, so the editor never gets its own nested scroll bar.
            .frame(minHeight: 120, alignment: .top)
            .disabled(!model.canEditContent)
        }
    }

    // MARK: - Attachments

    private func attachmentsSection(_ model: MacIssueDetailModel) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Attachments").font(.subheadline.weight(.medium)).foregroundStyle(.secondary)
            if model.attachments.isEmpty {
                Text("No attachments").font(.caption).foregroundStyle(.tertiary)
            } else {
                ForEach(model.attachments) { att in
                    Button {
                        if let url = model.attachmentURL(att) { Platform.open(url) }
                    } label: {
                        HStack(spacing: 6) {
                            Image(systemName: att.contentType.hasPrefix("image") ? "photo" : "paperclip")
                            Text(att.filename).lineLimit(1)
                            Spacer()
                            Text(ByteCountFormatter.string(fromByteCount: Int64(att.sizeBytes), countStyle: .file))
                                .font(.caption).foregroundStyle(.tertiary)
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    // MARK: - Comments (timeline with relative time, edit/delete)

    private func commentsSection(_ model: MacIssueDetailModel) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Activity")
                .font(.subheadline.weight(.medium)).foregroundStyle(.secondary)
            ForEach(timeline(model)) { item in
                switch item {
                case .comment(let comment): commentRow(comment, model: model)
                case .event(let event): eventRow(event, model: model)
                }
            }
            VStack(alignment: .trailing, spacing: 6) {
                MacMarkdownEditor(
                    model: composerEditor,
                    placeholder: "Write a comment…",
                    baseURL: model.baseURL,
                    accountId: model.accountId,
                    httpClient: model.httpClient,
                    mentionMembers: model.mentionMembers
                )
                .frame(minHeight: 60, alignment: .top)
                .overlay(
                    RoundedRectangle(cornerRadius: 6).stroke(Color.white.opacity(0.1), lineWidth: 0.5)
                )
                HStack {
                    Spacer()
                    Button {
                        Task { await submitComment(model) }
                    } label: { Image(systemName: "paperplane.fill") }
                        .buttonStyle(.borderedProminent).tint(Accent.indigo)
                        .disabled(submittingComment || !composerHasText)
                }
            }
            .onAppear {
                composerEditor.onEdit = { composerHasText = !composerEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
            }
        }
    }

    private func submitComment(_ model: MacIssueDetailModel) async {
        submittingComment = true
        defer { submittingComment = false }
        let ok = await composerEditor.commitPendingImages(uploader: makeCommentUploader(model))
        guard ok, !composerEditor.hasUncommittedDrafts else { return }
        let md = composerEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines)
        guard !md.isEmpty else { return }
        await model.addComment(md)
        composerEditor = IssueEditorModel()
        composerHasText = false
        composerEditor.onEdit = { composerHasText = !composerEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }

    private func makeCommentUploader(_ model: MacIssueDetailModel) -> @Sendable (PendingImage) async throws -> String {
        let api = deps.issueImagesApi
        let acc = model.accountId
        let issueId = model.issueId
        return { image in
            let uploaded = try await api.upload(
                accountId: acc, issueId: issueId,
                data: image.data, filename: image.filename, contentType: image.contentType
            )
            return uploaded.url
        }
    }

    // Only regular (human) comments reach this row — plan/question comments are
    // rendered by the Plan Panel now (and filtered out of `timeline`).
    @ViewBuilder
    private func commentRow(_ comment: CommentEntity, model: MacIssueDetailModel) -> some View {
        let author = model.user(comment.authorId)
        let body = getCommentBodyText(comment.body)
        let canModify = comment.authorId == deps.auth.userId || deps.auth.isAdmin
        let isEditing = editingCommentId == comment.id
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(author?.name ?? author?.email ?? "Unknown").font(.caption.weight(.semibold))
                Text(macRelativeDate(comment.createdAt)).font(.caption2).foregroundStyle(.tertiary)
                if comment.editedAt != nil {
                    Text("· edited").font(.caption2).foregroundStyle(.tertiary)
                }
                Spacer()
                if canModify && !isEditing {
                    Menu {
                        Button("Edit") { editDraft = body; editingCommentId = comment.id }
                        Button("Delete", role: .destructive) { Task { await model.deleteComment(comment.id) } }
                    } label: {
                        Image(systemName: "ellipsis").font(.caption2)
                    }
                    .menuStyle(.borderlessButton).fixedSize()
                }
            }
            if isEditing {
                TextField("Edit comment", text: $editDraft, axis: .vertical)
                    .textFieldStyle(.roundedBorder)
                    .lineLimit(1...6)
                HStack {
                    Button("Save") {
                        let t = editDraft
                        editingCommentId = nil
                        Task { await model.updateComment(comment.id, text: t) }
                    }
                    .buttonStyle(.borderedProminent).tint(Accent.indigo).controlSize(.small)
                    Button("Cancel") { editingCommentId = nil }.controlSize(.small)
                }
            } else {
                Text(body).font(.callout).textSelection(.enabled)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white.opacity(0.04))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    // MARK: - Activity timeline (comments + issue_events, merged by created_at)

    private enum MacTimelineItem: Identifiable {
        case comment(CommentEntity)
        case event(IssueEventEntity)
        var id: String {
            switch self {
            case .comment(let c): return "c-\(c.id)"
            case .event(let e): return "e-\(e.id)"
            }
        }
        var createdAt: String {
            switch self {
            case .comment(let c): return c.createdAt
            case .event(let e): return e.createdAt
            }
        }
    }

    // The human conversation timeline: regular comments + non-agent events
    // (status/assignee/label changes). Plan/question comments and agent
    // lifecycle events are surfaced by the Plan Panel + activity feed instead.
    private func timeline(_ model: MacIssueDetailModel) -> [MacTimelineItem] {
        let comments = model.comments.filter { $0.commentKind == .regular }
        let events = model.issueEvents.filter { !macAgentEventTypes.contains($0.type) }
        return (comments.map { MacTimelineItem.comment($0) }
            + events.map { MacTimelineItem.event($0) })
            .sorted { $0.createdAt < $1.createdAt }
    }

    @ViewBuilder
    private func eventRow(_ event: IssueEventEntity, model: MacIssueDetailModel) -> some View {
        let who = model.user(event.actorUserId).map { $0.name ?? $0.email } ?? "Someone"
        HStack(spacing: 8) {
            Circle().fill(Color.secondary.opacity(0.5)).frame(width: 6, height: 6)
            Text("\(who) \(macEventPhrase(event, user: model.user, labelName: { id in model.labels.first { $0.id == id }?.name }))")
                .font(.caption).foregroundStyle(.secondary)
            Text(macRelativeDate(event.createdAt)).font(.caption2).foregroundStyle(.tertiary)
            Spacer()
        }
        .padding(.vertical, 2)
    }

    // MARK: - Helpers

    private static func parseDate(_ s: String?) -> Date? {
        guard let s else { return nil }
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.locale = Locale(identifier: "en_US_POSIX")
        return f.date(from: s)
    }
}
