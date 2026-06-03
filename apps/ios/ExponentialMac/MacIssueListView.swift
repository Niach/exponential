import ExpCore
import ExpUI
import GRDB
import SwiftUI

@MainActor
@Observable
final class MacIssueListModel {
    var issues: [IssueEntity] = []
    var users: [UserEntity] = []
    var labels: [LabelEntity] = []
    var issueLabels: [IssueLabelEntity] = []
    var project: ProjectEntity?
    var permissions: WorkspacePermissions?
    var filters = IssueFilters()

    let accountId: String
    let projectId: String
    private let deps: MacAppDependencies
    private var tasks: [Task<Void, Never>] = []

    init(deps: MacAppDependencies, accountId: String, projectId: String) {
        self.deps = deps
        self.accountId = accountId
        self.projectId = projectId
    }

    var canCreate: Bool { permissions?.canCreate ?? false }
    var availableLabels: [LabelEntity] { labels.sorted { $0.name < $1.name } }

    func start() {
        guard tasks.isEmpty, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let projectId = projectId
        let issueObs = ValueObservation.tracking { db in
            try IssueEntity.filter(Column("project_id") == projectId).fetchAll(db)
        }
        let projectObs = ValueObservation.tracking { db in try ProjectEntity.fetchOne(db, key: projectId) }
        let labelObs = ValueObservation.tracking { db in try LabelEntity.fetchAll(db) }
        let issueLabelObs = ValueObservation.tracking { db in try IssueLabelEntity.fetchAll(db) }
        let userObs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }

        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in issueObs.values(in: pool) { self?.issues = rows } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await row in projectObs.values(in: pool) { self?.applyProject(row) } } catch {}
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
    }

    func stop() {
        tasks.forEach { $0.cancel() }
        tasks = []
    }

    private func applyProject(_ project: ProjectEntity?) {
        self.project = project
        guard let project, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let workspace = try? pool.read { db in try WorkspaceEntity.fetchOne(db, key: project.workspaceId) }
        permissions = WorkspacePermissions.resolve(
            workspace: workspace ?? nil,
            currentUserId: deps.auth.userId,
            isAdmin: deps.auth.isAdmin,
            dbPool: pool
        )
    }

    private func labelIds(for issue: IssueEntity) -> Set<String> {
        Set(issueLabels.filter { $0.issueId == issue.id }.map(\.labelId))
    }

    func issues(in status: IssueStatus, search: String) -> [IssueEntity] {
        issues
            .filter { IssueStatus.from($0.status) == status }
            .filter {
                matchesFilters(
                    status: status,
                    priority: IssuePriority.from($0.priority),
                    issueLabelIds: labelIds(for: $0),
                    filters: filters
                )
            }
            .filter { search.isEmpty || $0.title.localizedCaseInsensitiveContains(search) }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }

    func labelChips(for issue: IssueEntity) -> [LabelEntity] {
        let ids = labelIds(for: issue)
        return labels.filter { ids.contains($0.id) }.sorted { $0.name < $1.name }
    }

    func assignee(for issue: IssueEntity) -> UserEntity? {
        guard let aid = issue.assigneeId else { return nil }
        return users.first { $0.id == aid }
    }
}

struct MacIssueListView: View {
    @Environment(MacAppDependencies.self) private var deps
    let accountId: String
    let projectId: String

    @State private var model: MacIssueListModel?
    @State private var search = ""
    @State private var showCreate = false

    var body: some View {
        Group {
            if let model {
                List {
                    ForEach(IssueStatus.displayOrder, id: \.self) { status in
                        let items = model.issues(in: status, search: search)
                        if !items.isEmpty {
                            Section {
                                ForEach(items) { issue in
                                    NavigationLink(value: IssueRef(accountId: accountId, issueId: issue.id)) {
                                        row(issue, model: model)
                                    }
                                }
                            } header: {
                                Label(status.label, systemImage: status.sfSymbol).foregroundStyle(status.color)
                            }
                        }
                    }
                }
                .listStyle(.inset)
                .searchable(text: $search, prompt: "Search issues")
                .navigationTitle(model.project?.name ?? "Issues")
                .toolbar { toolbar(model) }
                .sheet(isPresented: $showCreate) {
                    MacCreateIssueView(accountId: accountId, projectId: projectId, users: model.users) {
                        showCreate = false
                    }
                }
                // Publish ⌘N for the app menu while this project is in scene focus.
                .focusedSceneValue(\.createIssueAction, model.canCreate ? { showCreate = true } : nil)
            } else {
                ProgressView()
            }
        }
        .onAppear {
            if model == nil {
                let m = MacIssueListModel(deps: deps, accountId: accountId, projectId: projectId)
                model = m
                m.start()
            }
        }
        .onDisappear { model?.stop() }
    }

    @ToolbarContentBuilder
    private func toolbar(_ model: MacIssueListModel) -> some ToolbarContent {
        ToolbarItem {
            Menu {
                Section("Status") {
                    ForEach(IssueStatus.displayOrder, id: \.self) { s in
                        Toggle(s.label, isOn: statusBinding(model, s))
                    }
                }
                Section("Priority") {
                    ForEach(IssuePriority.displayOrder, id: \.self) { p in
                        Toggle(p.label, isOn: priorityBinding(model, p))
                    }
                }
                if !model.availableLabels.isEmpty {
                    Section("Labels") {
                        ForEach(model.availableLabels) { l in
                            Toggle(l.name, isOn: labelBinding(model, l.id))
                        }
                    }
                }
                if !model.filters.isEmpty {
                    Button("Clear Filters") { model.filters = IssueFilters() }
                }
            } label: {
                Image(systemName: model.filters.isEmpty ? "line.3.horizontal.decrease.circle" : "line.3.horizontal.decrease.circle.fill")
            }
            .help("Filter")
        }
        ToolbarItem {
            Button { showCreate = true } label: { Image(systemName: "plus") }
                .disabled(!model.canCreate)
                .help("New issue")
        }
    }

    private func statusBinding(_ model: MacIssueListModel, _ s: IssueStatus) -> Binding<Bool> {
        Binding(
            get: { model.filters.statuses.contains(s) },
            set: { on in if on { model.filters.statuses.insert(s) } else { model.filters.statuses.remove(s) } }
        )
    }

    private func priorityBinding(_ model: MacIssueListModel, _ p: IssuePriority) -> Binding<Bool> {
        Binding(
            get: { model.filters.priorities.contains(p) },
            set: { on in if on { model.filters.priorities.insert(p) } else { model.filters.priorities.remove(p) } }
        )
    }

    private func labelBinding(_ model: MacIssueListModel, _ id: String) -> Binding<Bool> {
        Binding(
            get: { model.filters.labelIds.contains(id) },
            set: { on in if on { model.filters.labelIds.insert(id) } else { model.filters.labelIds.remove(id) } }
        )
    }

    private func row(_ issue: IssueEntity, model: MacIssueListModel) -> some View {
        HStack(spacing: 8) {
            Image(systemName: IssuePriority.from(issue.priority).sfSymbol)
                .foregroundStyle(IssuePriority.from(issue.priority).color)
                .frame(width: 16)
            if let identifier = issue.identifier {
                Text(identifier).font(.caption.monospaced()).foregroundStyle(.tertiary)
            }
            Text(issue.title).lineLimit(1)
            Spacer()
            ForEach(model.labelChips(for: issue)) { label in
                Circle().fill(Color(hex: label.color) ?? .gray).frame(width: 8, height: 8)
            }
            if let assignee = model.assignee(for: issue) {
                Text((assignee.name ?? assignee.email).prefix(1).uppercased())
                    .font(.caption2.weight(.bold))
                    .frame(width: 18, height: 18)
                    .background(Circle().fill(Accent.indigo.opacity(0.6)))
            }
        }
        .padding(.vertical, 2)
    }
}
