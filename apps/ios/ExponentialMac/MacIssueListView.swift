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

    private let accountId: String
    private let projectId: String
    private let db: DatabaseManager
    private var tasks: [Task<Void, Never>] = []

    init(accountId: String, projectId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.projectId = projectId
        self.db = db
    }

    func start() {
        guard tasks.isEmpty, let pool = try? db.pool(forAccountId: accountId) else { return }
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
            do { for try await row in projectObs.values(in: pool) { self?.project = row } } catch {}
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

    func issues(in status: IssueStatus, search: String) -> [IssueEntity] {
        issues
            .filter { IssueStatus.from($0.status) == status }
            .filter { search.isEmpty || $0.title.localizedCaseInsensitiveContains(search) }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }

    func labelChips(for issue: IssueEntity) -> [LabelEntity] {
        let ids = Set(issueLabels.filter { $0.issueId == issue.id }.map(\.labelId))
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
    @Binding var selectedIssue: IssueRef?

    @State private var model: MacIssueListModel?
    @State private var search = ""

    var body: some View {
        Group {
            if let model {
                List(selection: $selectedIssue) {
                    ForEach(IssueStatus.displayOrder, id: \.self) { status in
                        let items = model.issues(in: status, search: search)
                        if !items.isEmpty {
                            Section {
                                ForEach(items) { issue in
                                    row(issue, model: model)
                                        .tag(IssueRef(accountId: accountId, issueId: issue.id))
                                }
                            } header: {
                                Label(status.label, systemImage: status.sfSymbol)
                                    .foregroundStyle(status.color)
                            }
                        }
                    }
                }
                .listStyle(.inset)
                .searchable(text: $search, prompt: "Search issues")
                .navigationTitle(model.project?.name ?? "Issues")
            } else {
                ProgressView()
            }
        }
        .onAppear {
            if model == nil {
                let m = MacIssueListModel(accountId: accountId, projectId: projectId, db: deps.db)
                model = m
                m.start()
            }
        }
        .onDisappear { model?.stop() }
    }

    private func row(_ issue: IssueEntity, model: MacIssueListModel) -> some View {
        HStack(spacing: 8) {
            Image(systemName: IssuePriority.from(issue.priority).sfSymbol)
                .foregroundStyle(IssuePriority.from(issue.priority).color)
                .frame(width: 16)
            if let identifier = issue.identifier {
                Text(identifier)
                    .font(.caption.monospaced())
                    .foregroundStyle(.tertiary)
            }
            Text(issue.title).lineLimit(1)
            Spacer()
            ForEach(model.labelChips(for: issue)) { label in
                Circle()
                    .fill(Color(hex: label.color) ?? .gray)
                    .frame(width: 8, height: 8)
            }
            if let assignee = model.assignee(for: issue) {
                Text((assignee.name ?? assignee.email).prefix(1).uppercased())
                    .font(.caption2.weight(.bold))
                    .frame(width: 18, height: 18)
                    .background(Circle().fill(Color.blue.opacity(0.6)))
            }
        }
        .padding(.vertical, 2)
    }
}
