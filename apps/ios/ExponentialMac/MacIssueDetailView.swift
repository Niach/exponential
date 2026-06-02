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

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private var tasks: [Task<Void, Never>] = []

    init(accountId: String, issueId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
    }

    func start() {
        guard tasks.isEmpty, let pool = try? db.pool(forAccountId: accountId) else { return }
        let issueId = issueId
        let issueObs = ValueObservation.tracking { db in try IssueEntity.fetchOne(db, key: issueId) }
        let labelObs = ValueObservation.tracking { db in try LabelEntity.fetchAll(db) }
        let issueLabelObs = ValueObservation.tracking { db in
            try IssueLabelEntity.filter(Column("issue_id") == issueId).fetchAll(db)
        }
        let userObs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }

        tasks.append(Task { @MainActor [weak self] in
            do { for try await row in issueObs.values(in: pool) { self?.issue = row } } catch {}
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

    var assignedLabels: [LabelEntity] {
        let ids = Set(issueLabels.map(\.labelId))
        return labels.filter { ids.contains($0.id) }.sorted { $0.name < $1.name }
    }

    var assignee: UserEntity? {
        guard let aid = issue?.assigneeId else { return nil }
        return users.first { $0.id == aid }
    }
}

struct MacIssueDetailView: View {
    @Environment(MacAppDependencies.self) private var deps
    let accountId: String
    let issueId: String

    @State private var model: MacIssueDetailModel?

    var body: some View {
        Group {
            if let issue = model?.issue, let model {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        if let identifier = issue.identifier {
                            Text(identifier)
                                .font(.caption.monospaced())
                                .foregroundStyle(.tertiary)
                        }
                        Text(issue.title)
                            .font(.title2.weight(.semibold))
                            .textSelection(.enabled)

                        HStack(spacing: 16) {
                            metaChip(
                                IssueStatus.from(issue.status).label,
                                systemImage: IssueStatus.from(issue.status).sfSymbol,
                                color: IssueStatus.from(issue.status).color
                            )
                            metaChip(
                                IssuePriority.from(issue.priority).label,
                                systemImage: IssuePriority.from(issue.priority).sfSymbol,
                                color: IssuePriority.from(issue.priority).color
                            )
                            if let assignee = model.assignee {
                                metaChip(assignee.name ?? assignee.email, systemImage: "person.crop.circle", color: .secondary)
                            }
                            if let due = issue.dueDate {
                                metaChip(due, systemImage: "calendar", color: .secondary)
                            }
                        }

                        if !model.assignedLabels.isEmpty {
                            HStack(spacing: 6) {
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

                        Divider()

                        let description = getIssueDescriptionText(issue.description)
                        if description.isEmpty {
                            Text("No description")
                                .foregroundStyle(.tertiary)
                                .italic()
                        } else {
                            Text(description)
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        Spacer(minLength: 0)
                    }
                    .padding(24)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .navigationTitle(issue.identifier ?? "Issue")
            } else {
                ProgressView()
            }
        }
        .onAppear {
            if model == nil {
                let m = MacIssueDetailModel(accountId: accountId, issueId: issueId, db: deps.db)
                model = m
                m.start()
            }
        }
        .onDisappear { model?.stop() }
    }

    private func metaChip(_ text: String, systemImage: String, color: Color) -> some View {
        Label(text, systemImage: systemImage)
            .font(.callout)
            .foregroundStyle(color)
    }
}
