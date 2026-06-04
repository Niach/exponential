import ExpCore
import Foundation
import GRDB

@MainActor @Observable
final class InboxViewModel {
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
    private let auth: AuthRepository
    private let notificationsApi: NotificationsApi
    private var observationTask: Task<Void, Never>?

    // Backing arrays for the two observations; either firing rebuilds the groups.
    private var notifications: [NotificationEntity] = []
    private var issues: [IssueEntity] = []

    init(accountId: String, db: DatabaseManager, auth: AuthRepository, notificationsApi: NotificationsApi) {
        self.accountId = accountId
        self.db = db
        self.auth = auth
        self.notificationsApi = notificationsApi
    }

    func startObserving() {
        observationTask = Task { [weak self] in
            guard let self else { return }
            guard let pool = try? self.db.pool(forAccountId: self.accountId) else { return }

            // The notifications shape is already scoped to the signed-in user.
            let notifObs = ValueObservation.tracking { db in try NotificationEntity.fetchAll(db) }
            Task {
                for try await notifications in notifObs.values(in: pool) {
                    self.notifications = notifications
                    self.rebuild()
                }
            }

            let issueObs = ValueObservation.tracking { db in try IssueEntity.fetchAll(db) }
            Task {
                for try await issues in issueObs.values(in: pool) {
                    self.issues = issues
                    self.rebuild()
                }
            }
        }
    }

    func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
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
