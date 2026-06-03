import ExpCore
import ExpUI
import Foundation
import GRDB
import SwiftUI

@MainActor
@Observable
final class MacIssueDetailModel {
    var issue: IssueEntity?
    var labels: [LabelEntity] = []
    var issueLabels: [IssueLabelEntity] = []
    var users: [UserEntity] = []
    var comments: [CommentEntity] = []
    var attachments: [AttachmentEntity] = []
    var permissions: WorkspacePermissions?
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
        let workspace = try? pool.read { db -> WorkspaceEntity? in
            guard let project = try ProjectEntity.fetchOne(db, key: issue.projectId) else { return nil }
            return try WorkspaceEntity.fetchOne(db, key: project.workspaceId)
        }
        permissions = WorkspacePermissions.resolve(
            workspace: workspace ?? nil,
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
            input.description = IssueDescription(text: markdown)
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

    func toggleLabel(_ labelId: String) async {
        guard let issue else { return }
        do {
            if assignedLabelIds.contains(labelId) {
                try await deps.labelsApi.removeFromIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
            } else {
                try await deps.labelsApi.addToIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
            }
        } catch { self.error = error.localizedDescription }
    }

    func addComment(_ text: String) async {
        guard let issue else { return }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        do { try await deps.commentsApi.create(accountId: accountId, issueId: issue.id, text: trimmed) }
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
}

struct MacIssueDetailView: View {
    @Environment(MacAppDependencies.self) private var deps
    let accountId: String
    let issueId: String
    var onDelete: () -> Void = {}

    @State private var model: MacIssueDetailModel?
    @State private var showDeleteConfirm = false
    @State private var draftComment = ""
    @State private var showDuePicker = false
    @FocusState private var titleFocused: Bool

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
            // swapped out wholesale (`.id(selectedIssue)`), so a pending title or
            // description edit would otherwise be dropped.
            if let m = model { Task { await m.saveTitle(); await m.commitDescription(); m.stop() } }
        }
    }

    @ViewBuilder
    private func content(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                header(model, issue: issue)
                Divider()
                propertyRow(model, issue: issue)
                labelsSection(model)
                Divider()
                descriptionSection(model)
                Divider()
                attachmentsSection(model)
                Divider()
                commentsSection(model)
                if let error = model.error {
                    Text(error).font(.callout).foregroundStyle(.red)
                }
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .navigationTitle(issue.identifier ?? "Issue")
        .toolbar {
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

    // MARK: - Properties (status / priority / assignee / due date)

    private func propertyRow(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        HStack(spacing: 14) {
            Menu {
                ForEach(IssueStatus.displayOrder, id: \.self) { s in
                    Button { Task { await model.setStatus(s) } } label: { Label(s.label, systemImage: s.sfSymbol) }
                }
            } label: {
                let s = IssueStatus.from(issue.status)
                Label(s.label, systemImage: s.sfSymbol).foregroundStyle(s.color)
            }
            .menuStyle(.borderlessButton).fixedSize().disabled(!model.canModerate)

            Menu {
                ForEach(IssuePriority.displayOrder, id: \.self) { p in
                    Button { Task { await model.setPriority(p) } } label: { Label(p.label, systemImage: p.sfSymbol) }
                }
            } label: {
                let p = IssuePriority.from(issue.priority)
                Label(p.label, systemImage: p.sfSymbol).foregroundStyle(p.color)
            }
            .menuStyle(.borderlessButton).fixedSize().disabled(!model.canModerate)

            Menu {
                Button("Unassigned") { Task { await model.setAssignee(nil) } }
                Divider()
                ForEach(model.users) { u in
                    Button(u.name ?? u.email) { Task { await model.setAssignee(u.id) } }
                }
            } label: {
                Label(model.assignee?.name ?? model.assignee?.email ?? "Unassigned", systemImage: "person.crop.circle")
                    .foregroundStyle(.secondary)
            }
            .menuStyle(.borderlessButton).fixedSize().disabled(!model.canModerate)

            dueDateControl(model, issue: issue)
            Spacer()
        }
    }

    // Due date as a button that opens a graphical calendar popover (mirrors the
    // web's react-day-picker popover + iOS). One tap = one mutation; avoids the
    // inline stepper-field DatePicker whose computed binding fought the async
    // Electric round-trip (each sub-component edit mutated, value snapped back).
    @ViewBuilder
    private func dueDateControl(_ model: MacIssueDetailModel, issue: IssueEntity) -> some View {
        let due = Self.parseDate(issue.dueDate)
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

    // MARK: - Labels

    @ViewBuilder
    private func labelsSection(_ model: MacIssueDetailModel) -> some View {
        if !model.availableLabels.isEmpty {
            HStack(spacing: 6) {
                Menu {
                    ForEach(model.availableLabels) { label in
                        Button {
                            Task { await model.toggleLabel(label.id) }
                        } label: {
                            Label(label.name, systemImage: model.assignedLabelIds.contains(label.id) ? "checkmark" : "")
                        }
                    }
                } label: {
                    Image(systemName: "tag")
                }
                .menuStyle(.borderlessButton).fixedSize().disabled(!model.canEditContent)

                ForEach(model.assignedLabels) { label in
                    HStack(spacing: 4) {
                        Circle().fill(Color(hex: label.color) ?? .gray).frame(width: 8, height: 8)
                        Text(label.name).font(.caption)
                    }
                    .padding(.horizontal, 8).padding(.vertical, 3)
                    .glassButton()
                }
            }
        }
    }

    // MARK: - Description (rich block-markdown editor)

    private func descriptionSection(_ model: MacIssueDetailModel) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Description").font(.subheadline.weight(.medium)).foregroundStyle(.secondary)
            MacMarkdownEditor(
                model: model.editor,
                baseURL: model.baseURL,
                accountId: model.accountId,
                httpClient: model.httpClient
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

    // MARK: - Comments

    private func commentsSection(_ model: MacIssueDetailModel) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Comments").font(.subheadline.weight(.medium)).foregroundStyle(.secondary)
            ForEach(model.comments) { comment in
                commentRow(comment, model: model)
            }
            HStack(alignment: .bottom, spacing: 8) {
                TextField("Add a comment…", text: $draftComment, axis: .vertical)
                    .textFieldStyle(.roundedBorder)
                    .lineLimit(1...5)
                Button {
                    let text = draftComment
                    draftComment = ""
                    Task { await model.addComment(text) }
                } label: { Image(systemName: "paperplane.fill") }
                .disabled(draftComment.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    @ViewBuilder
    private func commentRow(_ comment: CommentEntity, model: MacIssueDetailModel) -> some View {
        let author = model.user(comment.authorId)
        let body = getCommentBodyText(comment.body)
        let kind = comment.commentKind
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(author?.name ?? author?.email ?? "Unknown").font(.caption.weight(.semibold))
                if kind == .question { Text("asks").font(.caption).foregroundStyle(.purple) }
                if kind == .plan { Text("plan").font(.caption).foregroundStyle(.blue) }
                Spacer()
                if comment.authorId == deps.auth.userId || deps.auth.isAdmin {
                    Button { Task { await model.deleteComment(comment.id) } } label: {
                        Image(systemName: "trash").font(.caption2)
                    }
                    .buttonStyle(.borderless).foregroundStyle(.tertiary)
                }
            }
            Text(body).font(.callout).textSelection(.enabled)
            if kind == .plan, model.issue?.agentPlanState == "awaiting_approval",
               model.permissions?.canApprovePlan(creatorId: model.issue?.creatorId) == true {
                HStack(spacing: 8) {
                    Button("Approve") { Task { await model.approvePlan() } }
                    Button("Request changes") { Task { await model.requestChanges() } }
                }
                .controlSize(.small)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(kindBackground(kind))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func kindBackground(_ kind: CommentKind) -> Color {
        switch kind {
        case .regular: Color.white.opacity(0.04)
        case .question: Color.purple.opacity(0.10)
        case .plan: Color.blue.opacity(0.10)
        }
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
