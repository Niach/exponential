import ExpCore
import Foundation
import GRDB

@MainActor @Observable
final class InboxViewModel {
    struct Group: Identifiable {
        let issue: IssueEntity
        /// Newest first — `latest` drives the single-row stream anatomy.
        let notifications: [NotificationEntity]
        var id: String { issue.id }
        var unread: Int { notifications.filter { $0.readAt == nil }.count }
        var latest: NotificationEntity? { notifications.first }
    }

    /// Synthetic per-team group for issue-less `support_reply` rows (EXP-180:
    /// helpdesk tickets are standalone — no issue to anchor on). One group per
    /// resolved team; rows with a NULL/unknown team_id collapse into one
    /// generic group (`teamId == nil`) — web inbox parity.
    struct SupportGroup: Identifiable {
        /// The ticket team's id when it resolves to a synced team; nil for the
        /// generic (NULL/unknown team) group.
        let teamId: String?
        /// Resolved team name (nil when unresolved — the row renders as plain
        /// "Support").
        let teamName: String?
        /// Newest first.
        let notifications: [NotificationEntity]
        var id: String { "support:\(teamId ?? "unknown")" }
        var unread: Int { notifications.filter { $0.readAt == nil }.count }
        var latest: NotificationEntity? { notifications.first }
    }

    /// One merged stream (web parity): issue groups and Support groups
    /// interleaved by latest activity, newest first.
    enum Entry: Identifiable {
        case issue(Group)
        case support(SupportGroup)

        var id: String {
            switch self {
            case .issue(let group): return "issue:\(group.id)"
            case .support(let group): return group.id
            }
        }

        var unread: Int {
            switch self {
            case .issue(let group): return group.unread
            case .support(let group): return group.unread
            }
        }
    }

    var entries: [Entry] = []
    var totalUnread = 0
    /// Web parity: the Support row shows its team name only when the user is
    /// in more than one team.
    var hasMultipleTeams = false

    private let accountId: String
    private let db: DatabaseManager
    private let auth: AuthRepository
    private let notificationsApi: NotificationsApi
    private var observationTask: Task<Void, Never>?

    // Backing arrays for the observations; any firing rebuilds the entries.
    private var notifications: [NotificationEntity] = []
    private var issues: [IssueEntity] = []
    private var teams: [TeamEntity] = []

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

            // Teams resolve the Support groups' names (and their >1-team label
            // gate).
            let teamObs = ValueObservation.tracking { db in try TeamEntity.fetchAll(db) }
            Task {
                for try await teams in teamObs.values(in: pool) {
                    self.teams = teams
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
        let teamsById = Dictionary(teams.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // Newest-first; first-seen insertion order = groups sorted by latest
        // activity, issue and Support groups interleaved in one stream.
        let sorted = notifications.sorted { $0.createdAt > $1.createdAt }
        var order: [String] = []
        var byIssue: [String: [NotificationEntity]] = [:]
        // Keyed by resolved team id; "" is the generic (NULL/unknown) bucket.
        var supportByTeam: [String: [NotificationEntity]] = [:]
        for n in sorted {
            guard let iid = n.issueId else {
                // Issue-less rows: only support_reply is expected (helpdesk
                // tickets have no issue). A team_id that doesn't resolve to a
                // synced team collapses into the generic group — web parity.
                guard n.type == DomainContract.notificationTypeSupportReply else { continue }
                let teamKey = n.teamId.flatMap { teamsById[$0]?.id } ?? ""
                if supportByTeam[teamKey] == nil {
                    order.append("support:\(teamKey)")
                    supportByTeam[teamKey] = []
                }
                supportByTeam[teamKey]?.append(n)
                continue
            }
            guard issuesById[iid] != nil else { continue }
            if byIssue[iid] == nil {
                order.append("issue:\(iid)")
                byIssue[iid] = []
            }
            byIssue[iid]?.append(n)
        }
        hasMultipleTeams = teams.count > 1
        entries = order.compactMap { key in
            if key.hasPrefix("support:") {
                let teamKey = String(key.dropFirst("support:".count))
                guard let ns = supportByTeam[teamKey] else { return nil }
                let team = teamsById[teamKey]
                return .support(SupportGroup(teamId: team?.id, teamName: team?.name, notifications: ns))
            }
            let iid = String(key.dropFirst("issue:".count))
            guard let issue = issuesById[iid], let ns = byIssue[iid] else { return nil }
            return .issue(Group(issue: issue, notifications: ns))
        }
        // Support groups count too — an unread support_reply must never light
        // the tab-bar dot without a row here to see and clear it.
        totalUnread = entries.reduce(0) { $0 + $1.unread }
    }

    func markGroupRead(_ group: Group) {
        markRead(group.notifications)
    }

    func markSupportGroupRead(_ group: SupportGroup) {
        markRead(group.notifications)
    }

    private func markRead(_ notifications: [NotificationEntity]) {
        Task {
            for n in notifications where n.readAt == nil {
                try? await notificationsApi.markRead(accountId: accountId, id: n.id)
            }
        }
    }

    func markAllRead() {
        Task { try? await notificationsApi.markAllRead(accountId: accountId) }
    }
}
