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

    /// One agent message (EXP-801): an issue-less `agent_message` row is its
    /// own entry — never bundled, each is a distinct thing someone's agent
    /// said. Tapping marks it read; there is nowhere to navigate.
    struct MessageEntry: Identifiable {
        let notification: NotificationEntity
        /// Resolved team name (nil when unresolved); shown only with >1 team.
        let teamName: String?
        var id: String { "message:\(notification.id)" }
        var unread: Int { notification.readAt == nil ? 1 : 0 }
    }

    /// One blocked run (EXP-980): an issue-less `session_blocked` row is its
    /// own entry, like an agent message. Tapping marks it read AND opens the
    /// run; a row whose `sessionId` is NULL (the run has been pruned) still
    /// renders and only marks read.
    struct BlockedRunEntry: Identifiable {
        let notification: NotificationEntity
        /// Resolved team name (nil when unresolved); shown only with >1 team.
        let teamName: String?
        var id: String { "blocked-run:\(notification.id)" }
        var unread: Int { notification.readAt == nil ? 1 : 0 }
        var sessionId: String? { notification.sessionId }
    }

    /// One merged stream (web parity): issue groups, agent messages and
    /// blocked runs interleaved by latest activity, newest first. SLOP-4: a
    /// widget reporter's reply is an ISSUE row (`reporter_reply` is
    /// issue-scoped like `issue_comment`), so no synthetic Support group.
    enum Entry: Identifiable {
        case issue(Group)
        case message(MessageEntry)
        case blockedRun(BlockedRunEntry)

        var id: String {
            switch self {
            case .issue(let group): return "issue:\(group.id)"
            case .message(let entry): return entry.id
            case .blockedRun(let entry): return entry.id
            }
        }

        var unread: Int {
            switch self {
            case .issue(let group): return group.unread
            case .message(let entry): return entry.unread
            case .blockedRun(let entry): return entry.unread
            }
        }
    }

    var entries: [Entry] = []
    var totalUnread = 0
    /// Web parity: an issue-less row shows its team name only when the user
    /// is in more than one team.
    var hasMultipleTeams = false

    private let accountId: String
    private let db: DatabaseManager
    private let auth: AuthRepository
    private let notificationsApi: NotificationsApi
    // Each observation loop is stored and cancelled individually — a single
    // wrapper task would NOT propagate cancellation into unstructured inner
    // `Task {}` loops, and MyWorkView re-arms on every appear, so leaked
    // loops would accumulate per push/pop.
    private var observationTasks: [Task<Void, Never>] = []

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
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }

        // The notifications shape is already scoped to the signed-in user.
        let notifObs = ValueObservation.tracking { db in try NotificationEntity.fetchAll(db) }
        observationTasks.append(Task { [weak self] in
            do {
                for try await notifications in notifObs.values(in: pool) {
                    guard let self else { return }
                    self.notifications = notifications
                    self.rebuild()
                }
            } catch {}
        })

        let issueObs = ValueObservation.tracking { db in try IssueEntity.fetchAll(db) }
        observationTasks.append(Task { [weak self] in
            do {
                for try await issues in issueObs.values(in: pool) {
                    guard let self else { return }
                    self.issues = issues
                    self.rebuild()
                }
            } catch {}
        })

        // Teams resolve the issue-less rows' team names (and their >1-team
        // label gate).
        let teamObs = ValueObservation.tracking { db in try TeamEntity.fetchAll(db) }
        observationTasks.append(Task { [weak self] in
            do {
                for try await teams in teamObs.values(in: pool) {
                    guard let self else { return }
                    self.teams = teams
                    self.rebuild()
                }
            } catch {}
        })
    }

    func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
    }

    /// The ONE visibility rule for notification rows: a row may count toward
    /// an unread signal only when the inbox can render AND clear it —
    /// issue-keyed rows need their issue in the local store (the notifications
    /// shape is static per user, so delivered rows outlive membership and a
    /// left team's rows keep syncing without their issues), and issue-less
    /// rows are agent messages (EXP-801) or blocked
    /// runs (EXP-980 — renderable with or WITHOUT a `sessionId`: the run may
    /// have been pruned, and the row still has to be readable and clearable).
    /// MainNavigator's tab-bar dot applies the same rule (REV-15) so the dot
    /// can never stay lit over an inbox that shows "You're all caught up"
    /// with no Mark-all-read escape.
    nonisolated static func isRenderable(_ notification: NotificationEntity, issueIds: Set<String>) -> Bool {
        guard let issueId = notification.issueId else {
            return notification.type == DomainContract.notificationTypeAgentMessage
                || notification.type == DomainContract.notificationTypeSessionBlocked
        }
        return issueIds.contains(issueId)
    }

    private func rebuild() {
        let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let issueIds = Set(issuesById.keys)
        let teamsById = Dictionary(teams.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // Newest-first; first-seen insertion order = groups sorted by latest
        // activity, issue groups and single rows interleaved in one stream.
        let sorted = notifications.sorted { $0.createdAt > $1.createdAt }
        var order: [String] = []
        var byIssue: [String: [NotificationEntity]] = [:]
        var messagesById: [String: NotificationEntity] = [:]
        var blockedRunsById: [String: NotificationEntity] = [:]
        for n in sorted {
            guard Self.isRenderable(n, issueIds: issueIds) else { continue }
            guard let iid = n.issueId else {
                // Issue-less rows: an agent message (EXP-801) and a blocked
                // run (EXP-980) are one entry per row (`isRenderable` admits
                // nothing else).
                if n.type == DomainContract.notificationTypeAgentMessage {
                    order.append("message:\(n.id)")
                    messagesById[n.id] = n
                } else {
                    order.append("blocked-run:\(n.id)")
                    blockedRunsById[n.id] = n
                }
                continue
            }
            if byIssue[iid] == nil {
                order.append("issue:\(iid)")
                byIssue[iid] = []
            }
            byIssue[iid]?.append(n)
        }
        hasMultipleTeams = teams.count > 1
        entries = order.compactMap { key in
            if key.hasPrefix("message:") {
                let id = String(key.dropFirst("message:".count))
                guard let n = messagesById[id] else { return nil }
                let teamName = n.teamId.flatMap { teamsById[$0]?.name }
                return .message(MessageEntry(notification: n, teamName: teamName))
            }
            if key.hasPrefix("blocked-run:") {
                let id = String(key.dropFirst("blocked-run:".count))
                guard let n = blockedRunsById[id] else { return nil }
                let teamName = n.teamId.flatMap { teamsById[$0]?.name }
                return .blockedRun(BlockedRunEntry(notification: n, teamName: teamName))
            }
            let iid = String(key.dropFirst("issue:".count))
            guard let issue = issuesById[iid], let ns = byIssue[iid] else { return nil }
            return .issue(Group(issue: issue, notifications: ns))
        }
        // Every entry counts — an unread row must never light the tab-bar dot
        // without a row here to see and clear it.
        totalUnread = entries.reduce(0) { $0 + $1.unread }
    }

    func markGroupRead(_ group: Group) {
        markRead(group.notifications)
    }

    func markMessageRead(_ entry: MessageEntry) {
        markRead([entry.notification])
    }

    func markBlockedRunRead(_ entry: BlockedRunEntry) {
        markRead([entry.notification])
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
