import ExpCore
import ExpUI
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
    var permissions: WorkspacePermissions?
    var error: String?

    // Local edit buffers (seeded once on first load so live sync doesn't clobber typing).
    var editingTitle = ""
    var editingDescription = ""
    private var seeded = false

    let accountId: String
    let issueId: String
    private let deps: MacAppDependencies
    private var tasks: [Task<Void, Never>] = []

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
            editingDescription = getIssueDescriptionText(issue.description)
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

    func saveDescription() async {
        guard let issue else { return }
        let text = editingDescription
        guard text != getIssueDescriptionText(issue.description) else { return }
        var input = UpdateIssueInput(id: issue.id)
        if text.isEmpty {
            input.explicitNulls.insert("description")
        } else {
            input.description = IssueDescription(text: text)
        }
        await update(input)
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
    @FocusState private var titleFocused: Bool
    @FocusState private var descFocused: Bool

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
        .onDisappear { model?.stop() }
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

            DatePicker(
                "Due",
                selection: Binding(
                    get: { Self.parseDate(issue.dueDate) ?? Date() },
                    set: { newDate in Task { await model.setDueDate(newDate) } }
                ),
                displayedComponents: [.date]
            )
            .labelsHidden()
            .disabled(!model.canModerate)
            if issue.dueDate != nil {
                Button { Task { await model.setDueDate(nil) } } label: { Image(systemName: "xmark.circle.fill") }
                    .buttonStyle(.borderless).foregroundStyle(.tertiary).disabled(!model.canModerate)
            }
            Spacer()
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

    // MARK: - Description (plain markdown for A3; rich editor is A4)

    private func descriptionSection(_ model: MacIssueDetailModel) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Description").font(.subheadline.weight(.medium)).foregroundStyle(.secondary)
            TextEditor(text: Binding(get: { model.editingDescription }, set: { model.editingDescription = $0 }))
                .font(.body)
                .frame(minHeight: 120)
                .scrollContentBackground(.hidden)
                .padding(8)
                .glassRow()
                .focused($descFocused)
                .disabled(!model.canEditContent)
                .onChange(of: descFocused) { _, focused in
                    if !focused { Task { await model.saveDescription() } }
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
