import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// Ported from the iOS `InboxViewModel` (which lives in the iOS-only target, so
/// it isn't visible here). Groups the user's notifications by issue and surfaces
/// issues that need review. Observation Tasks are stored and cancelled on stop —
/// no orphan inner Tasks.
@MainActor
@Observable
final class MacInboxViewModel {
    struct Group: Identifiable {
        let issue: IssueEntity
        let notifications: [NotificationEntity]
        var id: String { issue.id }
        var unread: Int { notifications.filter { $0.readAt == nil }.count }
    }

    var groups: [Group] = []
    var reviewIssues: [IssueEntity] = []
    var totalUnread = 0

    private let accountId: String
    private let db: DatabaseManager
    private let notificationsApi: NotificationsApi
    private var tasks: [Task<Void, Never>] = []

    private var notifications: [NotificationEntity] = []
    private var issues: [IssueEntity] = []

    init(accountId: String, db: DatabaseManager, notificationsApi: NotificationsApi) {
        self.accountId = accountId
        self.db = db
        self.notificationsApi = notificationsApi
    }

    func startObserving() {
        guard tasks.isEmpty, let pool = try? db.pool(forAccountId: accountId) else { return }
        // The notifications shape is already scoped to the signed-in user.
        let notifObs = ValueObservation.tracking { db in try NotificationEntity.fetchAll(db) }
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in notifObs.values(in: pool) { self?.notifications = rows; self?.rebuild() } } catch {}
        })
        let issueObs = ValueObservation.tracking { db in try IssueEntity.fetchAll(db) }
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in issueObs.values(in: pool) { self?.issues = rows; self?.rebuild() } } catch {}
        })
    }

    func stopObserving() {
        tasks.forEach { $0.cancel() }
        tasks = []
    }

    private func rebuild() {
        let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // Newest-first, grouped by issue (insertion order = newest activity first).
        let sorted = notifications.sorted { $0.createdAt > $1.createdAt }
        var order: [String] = []
        var byIssue: [String: [NotificationEntity]] = [:]
        for n in sorted {
            guard let iid = n.issueId, issuesById[iid] != nil else { continue }
            if byIssue[iid] == nil { order.append(iid); byIssue[iid] = [] }
            byIssue[iid]?.append(n)
        }
        groups = order.compactMap { iid in
            guard let issue = issuesById[iid], let ns = byIssue[iid] else { return nil }
            return Group(issue: issue, notifications: ns)
        }
        reviewIssues = issues.filter { $0.agentPlanState == "awaiting_approval" || $0.prState == "open" }
        totalUnread = groups.reduce(0) { $0 + $1.unread }
    }

    func markGroupRead(_ group: Group) {
        Task {
            for n in group.notifications where n.readAt == nil {
                try? await notificationsApi.markRead(accountId: accountId, id: n.id)
            }
        }
    }

    func markAllRead() {
        Task { try? await notificationsApi.markAllRead(accountId: accountId) }
    }
}

/// The macOS inbox: a segmented "For me" (notifications grouped by issue) /
/// "Needs your review" (plan-ready + open-PR issues). Tapping a row opens the
/// issue via `onOpenIssue` (the shell pushes it onto the detail stack).
struct MacInboxView: View {
    @Environment(MacAppDependencies.self) private var deps
    let accountId: String
    var onOpenIssue: (String) -> Void = { _ in }

    @State private var vm: MacInboxViewModel?
    @State private var tab = 0

    var body: some View {
        VStack(spacing: 0) {
            Picker("", selection: $tab) {
                Text("For me").tag(0)
                Text("Needs your review").tag(1)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(12)
            Divider()
            if let vm {
                content(vm)
            } else {
                Spacer()
            }
        }
        .navigationTitle("Inbox")
        .toolbar {
            ToolbarItem {
                Button("Mark all read") { vm?.markAllRead() }
                    .disabled((vm?.totalUnread ?? 0) == 0)
            }
        }
        .onAppear {
            if vm == nil {
                let m = MacInboxViewModel(accountId: accountId, db: deps.db, notificationsApi: deps.notificationsApi)
                vm = m
                m.startObserving()
            }
        }
        .onDisappear { vm?.stopObserving() }
    }

    @ViewBuilder
    private func content(_ vm: MacInboxViewModel) -> some View {
        if tab == 0 {
            if vm.groups.isEmpty {
                emptyState("You're all caught up.")
            } else {
                List {
                    ForEach(vm.groups) { group in
                        Button {
                            vm.markGroupRead(group)
                            onOpenIssue(group.issue.id)
                        } label: {
                            groupCard(group)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .listStyle(.inset)
            }
        } else {
            if vm.reviewIssues.isEmpty {
                emptyState("Nothing needs your review.")
            } else {
                List {
                    ForEach(vm.reviewIssues) { issue in
                        Button { onOpenIssue(issue.id) } label: { reviewRow(issue) }
                            .buttonStyle(.plain)
                    }
                }
                .listStyle(.inset)
            }
        }
    }

    private func groupCard(_ group: MacInboxViewModel.Group) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                if group.unread > 0 {
                    Circle().fill(Accent.indigo).frame(width: 7, height: 7)
                }
                Text(group.issue.identifier ?? "").font(.caption.monospaced()).foregroundStyle(.tertiary)
                Text(group.issue.title).font(.subheadline.weight(.medium)).lineLimit(1)
                Spacer()
                if group.unread > 0 {
                    Text("\(group.unread)")
                        .font(.caption2)
                        .padding(.horizontal, 6).padding(.vertical, 1)
                        .background(Accent.indigo.opacity(0.2)).clipShape(Capsule())
                }
            }
            ForEach(group.notifications.prefix(3)) { n in
                Text(n.title).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
    }

    private func reviewRow(_ issue: IssueEntity) -> some View {
        HStack(spacing: 8) {
            Text(issue.identifier ?? "").font(.caption.monospaced()).foregroundStyle(.tertiary)
            Text(issue.title).font(.subheadline).lineLimit(1)
            Spacer()
            if issue.agentPlanState == "awaiting_approval" {
                badge("Plan ready", Accent.indigo)
            } else if issue.prState == "open" {
                badge("In review", .green)
            }
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
    }

    private func badge(_ text: String, _ color: Color) -> some View {
        Text(text).font(.caption2.weight(.medium))
            .padding(.horizontal, 6).padding(.vertical, 1)
            .background(color.opacity(0.15)).foregroundStyle(color).clipShape(Capsule())
    }

    private func emptyState(_ text: String) -> some View {
        VStack {
            Spacer()
            ContentUnavailableView("Inbox", systemImage: "tray", description: Text(text))
            Spacer()
        }
    }
}
