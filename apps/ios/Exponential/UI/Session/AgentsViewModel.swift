import ExpCore
import ExpUI
import Foundation
import GRDB

/// Backs the Devices tab (machines) AND the Agent page composer's pools
/// (EXP-825: issues, boards, actions, open PRs, devices, worktrees and the
/// live runs' issues). EXP-1288: the session ROWS it once built (Running /
/// Recent) are gone — the Agent page lists threads (`ThreadListModel`). The
/// composer's pools stay bound to the ACTIVE team.
@MainActor @Observable
final class AgentsViewModel {
    /// EXP-481: the machines list, composed from the synced `devices` shape
    /// (own rows + the active team's shared servers; online-ness derives from
    /// last_seen_at freshness) — the sync-fed replacement for the old 15s
    /// `devices.list` poll. nil until the first observation emission, so the
    /// view can tell "loading" from "no machines".
    var devices: [SteerDevice]?
    /// EXP-909: the machine × agent × profile rows behind the machines list —
    /// every login every listed machine reported, derived in the same pass as
    /// `devices` so the two can never disagree. Each device row draws its own
    /// (`deviceLoginRows`); there is no cross-device Accounts section any more
    /// (EXP-909 folded it away — a login belongs to the machine holding it).
    private(set) var accountRows: [AgentProfileUsageRow] = []
    var accountsLoaded: Bool { deviceEntities != nil }
    /// EXP-829: the LOGINS a usage refresh is in flight for (keyed by
    /// `AgentProfileUsageRow.key`). Cleared when the machine's re-report moves
    /// the stamp, or after `Self.refreshPendingWindow` with no answer.
    private(set) var refreshingAccounts: Set<String> = []
    /// EXP-849: the account ACTIONS a machine chip offers (re-login, "use this
    /// account here") — the rows a command is in flight for, keyed by
    /// `AgentProfileUsageRow.key`, and the last refusal. The outcome itself
    /// lands via sync: the machine re-probes and its `agent_accounts` report
    /// moves.
    private(set) var accountActions: Set<String> = []
    /// EXP-829: the command queue the refreshes ride — set by the Devices
    /// page only. Nil (the Agent page, the onboarding step) = the accounts
    /// are still derived but nothing is ever queued from there.
    var devicesApi: DevicesApi?
    /// EXP-481: the synced worktree inventory (shape 18) — the composer's
    /// resume probe. EXP-1042: nothing else reads it on a phone; the
    /// device-settings list left for the IDE.
    var worktrees: [DeviceWorktreeEntity] = []
    /// EXP-694: the synced actions, account-wide (a session names its own
    /// team). The session rows' trailing control resolves its glyph and its
    /// editor target through these — no network read.
    var actions: [ActionDto] = []

    /// The active team — kept current by the surrounding view. EXP-1186: it
    /// no longer scopes the session lists (cross-team); it scopes `devices`
    /// (the composer's start targets: a server shared with ANOTHER team
    /// cannot run this team's work) unless `deviceTeamIds` widens them.
    var activeTeamId: String? {
        didSet {
            guard oldValue != activeTeamId else { return }
            rebuildDevices()
        }
    }

    /// EXP-1186: the Devices tab is cross-team — set to the caller's member
    /// team ids there, so teammates' servers shared with ANY of them list
    /// (once each). nil = the active team only (the Agent page composer).
    var deviceTeamIds: Set<String>? {
        didSet {
            guard oldValue != deviceTeamIds else { return }
            rebuildDevices()
        }
    }

    /// The team ids teammates' shared servers are matched against.
    private var sharedDeviceTeamIds: Set<String> {
        deviceTeamIds ?? activeTeamId.map { [$0] } ?? []
    }

    private let accountId: String
    private let userId: String?
    private let db: DatabaseManager
    // Stored and cancelled individually — a single wrapper task would not
    // propagate cancellation into unstructured inner loops, and the view
    // re-arms on every appear.
    private var sessionTask: Task<Void, Never>?
    private var issueTask: Task<Void, Never>?
    private var boardTask: Task<Void, Never>?
    private var livenessTask: Task<Void, Never>?
    private var deviceTask: Task<Void, Never>?
    private var worktreeTask: Task<Void, Never>?
    private var userTask: Task<Void, Never>?
    // EXP-694: the action store behind the session rows' editor buttons.
    private var actionTask: Task<Void, Never>?
    /// EXP-656: wakes when our own `devices` shape completes a poll — the
    /// missing foreground re-derivation hook. Presence is only as current as
    /// that cursor, so a machine's badge must repaint the moment it advances
    /// (and not before: an unrefreshed cursor renders presence as unknown).
    private var freshnessTask: Task<Void, Never>?

    /// EXP-829: how long a queued refresh shows as in flight before giving
    /// up on the device answering (it answers by re-reporting on its next
    /// beat). Web `REFRESH_PENDING_MS`.
    private static let refreshPendingWindow: TimeInterval = 45
    /// EXP-829: the page's own refresh never re-tries one account faster than
    /// this — a queued command the machine has not answered yet is a CONFLICT
    /// on the server, and hammering it buys nothing. Web `AUTO_REFRESH_RETRY_MS`.
    private static let autoRefreshRetry: TimeInterval = 60
    /// In-flight refreshes: the usage stamp the account carried when it was
    /// queued (the device's re-report moves it, which clears the mark) and
    /// when it was queued.
    private var refreshMarks: [String: (fetchedAt: String?, at: Date)] = [:]
    private var autoRefreshAttempts: [String: Date] = [:]
    /// EXP-909: the caps of the machines behind `accountRows`, captured in the
    /// same pass — `canRefresh` is asked per LOGIN now, and a row carries its
    /// device id but not its caps.
    private var refreshCaps: [String: [String]] = [:]

    private var sessions: [CodingSessionEntity] = []
    private var issues: [IssueEntity] = []
    // Observed so the composer's issue pool can resolve repo-backed boards
    // (EXP-156) and so the pull-request pool can scope issues to the active
    // team (EXP-535 — issues don't sync team_id).
    private var boards: [BoardEntity] = []
    // EXP-481: raw synced rows behind `devices` (users resolve shared-row
    // owner names — a sharing owner is always inside the users shape).
    private var deviceEntities: [DeviceEntity]?
    private var users: [UserEntity] = []

    init(accountId: String, userId: String?, db: DatabaseManager) {
        self.accountId = accountId
        self.userId = userId
        self.db = db
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }

        let sessionObservation = ValueObservation.tracking { db in
            try CodingSessionEntity
                // Every live status — an in_review session is still watchable
                // (EXP-194); `liveRunIssueIds` drops heartbeat-stale rows.
                // EXP-888: a sweep end (`ended_by = 'stale'`) is not an end —
                // the host ignores the flip and heartbeats the row back to
                // `running`, so it stays in the live list (`PastRuns.hasEnded`).
                .filter(
                    [
                        DomainContract.codingSessionStatusRunning,
                        DomainContract.codingSessionStatusInReview,
                    ].contains(Column("status"))
                        || (Column("status") == DomainContract.codingSessionStatusEnded
                            && Column("ended_by") == DomainContract.codingSessionEndedByStale)
                )
                .fetchAll(db)
        }
        sessionTask = Task { [weak self] in
            do {
                for try await sessions in sessionObservation.values(in: pool) {
                    self?.sessions = sessions
                }
            } catch {}
        }

        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity.fetchAll(db)
        }
        issueTask = Task { [weak self] in
            do {
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues
                }
            } catch {}
        }

        // Boards back the composer's issue-pool eligibility filter AND scope
        // the batch-PR resolution (EXP-535: issues don't sync team_id), so
        // the running-session list rebuilds on these too.
        let boardObservation = ValueObservation.tracking { db in
            try BoardEntity.fetchAll(db)
        }
        boardTask = Task { [weak self] in
            do {
                for try await boards in boardObservation.values(in: pool) {
                    self?.boards = boards
                }
            } catch {}
        }

        // EXP-481: the machines list — devices + worktrees + users off sync.
        let deviceObservation = ValueObservation.tracking { db in
            try DeviceEntity.fetchAll(db)
        }
        deviceTask = Task { [weak self] in
            do {
                for try await rows in deviceObservation.values(in: pool) {
                    self?.deviceEntities = rows
                    self?.rebuildDevices()
                }
            } catch {}
        }
        let worktreeObservation = ValueObservation.tracking { db in
            try DeviceWorktreeEntity.fetchAll(db)
        }
        worktreeTask = Task { [weak self] in
            do {
                for try await rows in worktreeObservation.values(in: pool) {
                    self?.worktrees = rows
                }
            } catch {}
        }
        // EXP-694: the session rows' trailing control names the ACTION a run
        // came from (its glyph, and the editor the button opens), so the
        // actions shape rides along here.
        let actionObservation = ValueObservation.tracking { db in
            try ActionEntity.fetchAll(db)
        }
        actionTask = Task { [weak self] in
            do {
                for try await rows in actionObservation.values(in: pool) {
                    self?.actions = rows.map { ActionDto(entity: $0) }
                }
            } catch {}
        }

        let userObservation = ValueObservation.tracking { db in
            try UserEntity.fetchAll(db)
        }
        userTask = Task { [weak self] in
            do {
                for try await rows in userObservation.values(in: pool) {
                    self?.users = rows
                    self?.rebuildDevices()
                }
            } catch {}
        }

        // EXP-656: the devices cursor advancing is neither a row write nor a
        // clock tick, so nothing else in here notices it — and it is exactly
        // the moment a "Paused" row we couldn't vouch for becomes knowledge.
        freshnessTask = Task { [weak self] in
            for await polledAccountId in SyncFreshness.shared.updates() {
                guard let self, !Task.isCancelled else { return }
                guard polledAccountId == self.accountId else { continue }
                self.rebuildDevices()
            }
        }

        // GRDB only re-fires on writes — this clock re-applies the staleness
        // filters so a phantom session clears once its liveness window
        // elapses (EXP-153) and a silent machine drops to "last seen" once
        // its heartbeat window does (EXP-481: 30s against the 90s contract
        // window, so the badge flips within one tick of the boundary).
        livenessTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(30))
                guard let self, !Task.isCancelled else { return }
                self.rebuildDevices()
            }
        }
    }

    func stopObserving() {
        sessionTask?.cancel()
        sessionTask = nil
        issueTask?.cancel()
        issueTask = nil
        boardTask?.cancel()
        boardTask = nil
        livenessTask?.cancel()
        livenessTask = nil
        deviceTask?.cancel()
        deviceTask = nil
        worktreeTask?.cancel()
        worktreeTask = nil
        userTask?.cancel()
        userTask = nil
        actionTask?.cancel()
        actionTask = nil
        freshnessTask?.cancel()
        freshnessTask = nil
    }

    /// EXP-481: recompose the machines list from the observed rows — own
    /// machines first (most recently seen), then the shared servers of the
    /// active team (or, on the Devices tab, of every member team), exactly the `devices.list` ordering the view already renders.
    private func rebuildDevices() {
        guard let deviceEntities else { return }
        let now = Date()
        devices = DeviceQueries.compose(
            rows: deviceEntities,
            users: users,
            teamIds: sharedDeviceTeamIds,
            userId: userId,
            now: now
        )
        rebuildAccounts(deviceEntities, now: now)
    }

    // MARK: - Agent logins (EXP-829/EXP-909)

    /// The logins off the same rows the machines list reads — own machines
    /// plus the servers teammates shared with the ACTIVE team — one row per
    /// machine × agent × profile (`AgentAccountsRows`, the ×4 rule). Every
    /// device row draws its own; EXP-909 folded the cross-device Accounts
    /// section away. A refresh may only be queued on one of MY online
    /// machines that advertises the cap.
    private func rebuildAccounts(_ entities: [DeviceEntity], now: Date) {
        let teamIds = sharedDeviceTeamIds
        let scoped = entities.filter { row in
            (userId != nil && row.userId == userId)
                || (row.kind == "server" && row.sharedTeamIds.contains(where: teamIds.contains))
        }
        let capsByDevice: [String: [String]] = Dictionary(
            (devices ?? []).map { ($0.deviceId, $0.caps ?? []) },
            uniquingKeysWith: { a, _ in a }
        )
        accountRows = AgentAccountsRows.profileRows(
            devices: scoped,
            currentUserId: userId,
            isOnline: { DeviceLiveness.isOnline(lastSeenAt: $0, now: now) }
        )
        refreshCaps = capsByDevice
        pruneRefreshing(accountRows, now: now)
        autoRefreshLogins(accountRows, now: now)
    }

    /// EXP-909: whether a refresh may be QUEUED on this login's machine —
    /// mine, online, and advertising the cap (the server refuses the command
    /// below it). Web `deviceCanRefreshUsage`, the same three gates.
    func canRefresh(_ row: AgentProfileUsageRow) -> Bool {
        row.mine && row.online
            && (refreshCaps[row.deviceId] ?? []).contains(AgentAccountsRows.refreshCap)
    }

    /// An in-flight mark clears when the account's stamp moved (the machine
    /// answered) or when nobody answered inside the pending window.
    private func pruneRefreshing(_ rows: [AgentProfileUsageRow], now: Date) {
        guard !refreshMarks.isEmpty else { return }
        for (key, mark) in refreshMarks {
            let stamp = rows.first { $0.key == key }?.usage?.fetchedAt
            if stamp != mark.fetchedAt || now.timeIntervalSince(mark.at) > Self.refreshPendingWindow {
                refreshMarks[key] = nil
            }
        }
        refreshingAccounts = Set(refreshMarks.keys)
    }

    /// EXP-817/EXP-909: keep the machines list current while it is open.
    /// Every pass, each LOGIN on an eligible machine whose report is past the
    /// floor gets ONE refresh queued — never while one is in flight, never
    /// twice inside `autoRefreshRetry`. The floor is the device's own 429
    /// budget, so this can never out-poll what the machine allows itself.
    /// Passes run on every devices emission and on the 30s liveness tick.
    /// Keyed by `row.key` since EXP-909 folded the account groups away.
    private func autoRefreshLogins(_ rows: [AgentProfileUsageRow], now: Date) {
        guard devicesApi != nil else { return }
        for row in rows {
            guard canRefresh(row) else { continue }
            if refreshMarks[row.key] != nil { continue }
            if AgentAccountsRows.refreshAllowedAt(row.usage, now: now) != nil { continue }
            if let last = autoRefreshAttempts[row.key],
               now.timeIntervalSince(last) < Self.autoRefreshRetry {
                continue
            }
            autoRefreshAttempts[row.key] = now
            refreshLogin(row)
        }
    }

    /// Queue `agent_usage_refresh` for ONE login on its own machine. EXP-862
    /// retired the per-row refresh BUTTON on every client, so this is only
    /// ever the page's own pass — and it fails quietly: a command still queued
    /// from the last round is a CONFLICT, and the next pass simply looks
    /// again.
    private func refreshLogin(_ row: AgentProfileUsageRow) {
        guard let devicesApi else { return }
        refreshMarks[row.key] = (fetchedAt: row.usage?.fetchedAt, at: Date())
        refreshingAccounts = Set(refreshMarks.keys)
        let accountId = accountId
        Task { [weak self] in
            do {
                _ = try await devicesApi.createCommand(
                    accountId: accountId,
                    deviceId: row.deviceId,
                    kind: "agent_usage_refresh",
                    agent: row.agent,
                    profileId: row.profileId
                )
            } catch {
                guard let self else { return }
                self.refreshMarks[row.key] = nil
                self.refreshingAccounts = Set(self.refreshMarks.keys)
            }
        }
    }

    // MARK: - One machine's logins (EXP-909)

    /// EXP-909: one machine's logins, in the order its row draws them
    /// (contract agent order, its last used login first). The ONE per-device
    /// entry point since the cross-device Accounts section was folded away.
    func deviceLoginRows(_ deviceId: String) -> [AgentProfileUsageRow] {
        AgentAccountsRows.deviceRows(accountRows, deviceId: deviceId)
    }

    /// EXP-849: the health badge a machine row wears — the worst of its
    /// logins.
    func deviceHealth(_ deviceId: String) -> AgentAccountHealth {
        AgentAccountsRows.deviceHealth(accountRows, deviceId: deviceId)
    }

    /// Whether a command is in flight for this login.
    func isAccountActionPending(_ row: AgentProfileUsageRow) -> Bool {
        accountActions.contains(row.key)
    }

    /// EXP-862: delete the MACHINE's copy of a login — the chip menu's
    /// destructive entry, confirmed before it gets here.
    ///
    /// What goes is the agent CLI's config dir for that profile (credentials
    /// included) and its index row. The ACCOUNT is untouched: the machine never
    /// runs `codex logout`, which would revoke it server-wide, and nothing
    /// about it leaves the machine. Gated on the machine's `account-remove` cap
    /// (`AgentAccountsRows.canRemoveAccount`) — without it the menu hides the
    /// entry, because the server refuses the command and an older build would
    /// leave the row pending forever.
    func removeAccount(_ row: AgentProfileUsageRow) {
        queueAccountCommand(row, kind: Self.removeAccountCommandKind)
    }

    /// EXP-862's command kind, handled by the desktop and the headless daemon
    /// (`coding::agent_usage::remove_profile`).
    private static let removeAccountCommandKind = "agent_profile_remove"

    /// EXP-1137: sign ONE login out on the machine and keep its row — the
    /// chip menu's other destructive entry, confirmed before it gets here.
    /// claude's own `auth logout` inside that profile's config dir, codex's
    /// credential file deleted (never `codex logout`); the account itself is
    /// untouched. Gated on the machine's `account-sign-out` cap
    /// (`AgentAccountsRows.chipSignsOut`).
    func signOutAccount(_ row: AgentProfileUsageRow) {
        queueAccountCommand(row, kind: Self.signOutAccountCommandKind)
    }

    /// EXP-1137's command kind (`coding::agent_usage::sign_out_profile`).
    private static let signOutAccountCommandKind = "agent_profile_sign_out"

    private func queueAccountCommand(
        _ row: AgentProfileUsageRow,
        kind: String
    ) {
        guard let devicesApi, row.mine, row.online else { return }
        guard !accountActions.contains(row.key) else { return }
        accountActions.insert(row.key)
        let accountId = accountId
        let key = row.key
        // FEED-79: a refusal is the error toast (`Couldn't <verb> the login`
        // + the machine's own words), never a red caption under the chips.
        let title = FeedbackStates.Errors.title(
            verb: kind == Self.removeAccountCommandKind ? "remove" : "sign out",
            thing: "the login"
        )
        Task { [weak self] in
            do {
                let created = try await devicesApi.createCommand(
                    accountId: accountId,
                    deviceId: row.deviceId,
                    kind: kind,
                    agent: row.agent,
                    profileId: row.profileId
                )
                // The MATERIAL outcome lands via sync (the machine re-reports
                // `agent_accounts`), but a refusal would otherwise be silent —
                // including the honest one a machine too old to know the kind
                // answers with. Bounded 2s polls, like the device-settings
                // sheet's: an offline machine keeps the command queued
                // server-side and the poll simply stops watching.
                for _ in 0..<30 {
                    try? await Task.sleep(for: .seconds(2))
                    // The page went away: stop watching.
                    guard self != nil, !Task.isCancelled else { return }
                    guard let command = try? await devicesApi.getCommand(
                        accountId: accountId, commandId: created.id
                    ) else { continue }
                    guard !command.isPending else { continue }
                    if command.isFailed {
                        Toaster.shared.error(
                            title, description: command.result ?? "The device refused the command."
                        )
                    }
                    break
                }
            } catch {
                Toaster.shared.error(title, description: error.userFacingMessage)
            }
            self?.accountActions.remove(key)
        }
    }

    /// Candidate issues for the Agent page composer (EXP-156/EXP-825): every
    /// eligible issue in `teamId` (nil = across all synced teams),
    /// recency-ordered; `exempt` ids (the composer's checked set) skip the
    /// issue-level checks so a seeded issue stays on offer. Reads the
    /// already-observed boards/issues (no DB round-trip), so the pool is LIVE.
    func startCandidates(teamId: String?, exempt: Set<String> = []) -> [IssueOption] {
        IssueOption.build(issues: issues, boards: boards, teamId: teamId, exempt: exempt)
    }

    /// SLOP-3: every issue a LIVE run (anyone's, `CodingSessionLiveness`)
    /// works on, its own issue or a batch's covered ones. The blocked start's
    /// stack plan reads it to mark a line member as already running.
    func liveRunIssueIds() -> Set<String> {
        var ids = Set<String>()
        for session in sessions where CodingSessionLiveness.isLive(session) {
            if let issueId = session.issueId { ids.insert(issueId) }
            ids.formUnion(BatchRun.issueIds(session.batchIssueIds))
        }
        return ids
    }

    /// EXP-825: the team's synced boards, sortOrder-then-name — the `board`
    /// inputs pick from them.
    func teamBoards(teamId: String?) -> [BoardEntity] {
        guard let teamId else { return [] }
        return boards
            .filter { $0.teamId == teamId }
            .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
    }

    /// EXP-825: the team's action rows (the composer prepends the builtins
    /// itself), sortOrder-then-name like the server list.
    func teamActions(teamId: String?) -> [ActionDto] {
        guard let teamId else { return [] }
        return actions
            .filter { $0.teamId == teamId }
            .sorted { ($0.sortOrder, $0.name) < ($1.sortOrder, $1.name) }
    }

    /// EXP-825: the team's open issue-linked pull requests, one option per
    /// PR (EXP-259/EXP-270) — the `pr` inputs pick from them. Issues don't
    /// sync team_id, so the scope comes from the synced boards.
    func openPullRequests(teamId: String?) -> [StartPullRequestOption] {
        guard let teamId else { return [] }
        let boardIds = Set(boards.filter { $0.teamId == teamId }.map(\.id))
        return StartPullRequestOption.build(
            from: issues.filter { $0.prState == DomainContract.prStateOpen },
            teamBoardIds: boardIds
        )
    }

    /// EXP-1233: the team's open-PR issue ROWS (the ones `openPullRequests`
    /// collapses into options) — what the Fix merge conflicts card resolves
    /// its picked pull request off (`FixConflictsPr.resolve`).
    func openPullRequestIssues(teamId: String?) -> [IssueEntity] {
        guard let teamId else { return [] }
        let boardIds = Set(boards.filter { $0.teamId == teamId }.map(\.id))
        return issues.filter {
            boardIds.contains($0.boardId) && $0.prState == DomainContract.prStateOpen
        }
    }
}
