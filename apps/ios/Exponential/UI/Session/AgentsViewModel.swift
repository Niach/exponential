import ExpCore
import Foundation
import GRDB

/// Backs the Devices tab (machines) AND the Agent page (EXP-825: the
/// sessions and the composer's pools): the signed-in user's own live coding
/// sessions in the ACTIVE TEAM of the active account (the synced
/// `coding_sessions` shape) — running AND in_review (EXP-194), joined to
/// their issues for display. Desktop is the only session runner — this list
/// is the mobile window into what YOU are coding right now; teammates' runs
/// are owner-only (EXP-312) and never listed here, and the team scoping
/// mirrors web's `use-agents-data.ts`.
@MainActor @Observable
final class AgentsViewModel {
    struct Row: Identifiable {
        let session: CodingSessionEntity
        let issue: IssueEntity?
        /// EXP-876: a BATCH row's covered issues, in naming order — empty on
        /// every other subject, and on a batch whose issues are unknown.
        let batchIssues: [IssueEntity]
        /// EXP-549/550: the host machine as it presents right now — the LIVE
        /// devices row's label (a rename never rewrites the session's
        /// snapshot) plus whether that machine stopped heartbeating, which
        /// makes a still-coding run read "Paused" instead of live.
        let device: SessionDevicePresentation
        var id: String { session.id }
    }

    /// EXP-746: one finished run under "Recent" — an ended, PERSON-started run
    /// of the caller's in the active team. Automation runs are not here: they
    /// live under Automations' "Recent automated runs" (EXP-676) and the two
    /// sets are disjoint by `started_reason`, so the two Resume paths can
    /// never double-fire on the same row.
    struct PastRow: Identifiable {
        let session: CodingSessionEntity
        let issue: IssueEntity?
        /// EXP-876: a BATCH row's covered issues, in naming order.
        let batchIssues: [IssueEntity]
        /// The host machine as it presents right now (the live devices row's
        /// label, not the session's start-time snapshot).
        let device: SessionDevicePresentation
        /// Non-nil when a Resume would be accepted: the run's OWN machine,
        /// online and advertising `resume-run`.
        let resume: SteerDevice?
        var id: String { session.id }
    }

    var rows: [Row] = []

    /// EXP-996: what the sessions list needs BEYOND the rows to draw the
    /// GROUPS — the active team's synced workflows (whose names the workflow
    /// group rows wear), their nodes, and the stack edges
    /// (`issues.pr_base_branch`) of the issues the listed rows name. Rebuilt in
    /// the same pass as `rows`, so a group can never disagree with its runs.
    ///
    /// The stack edges come from the LISTED rows' own issues (web
    /// `useSessionTreeContext`): a stack only becomes a group when two of its
    /// runs are listed, so an unlisted lower member would change nothing but
    /// the group's root — and a root nobody can see is worse than the lowest
    /// one they can.
    private(set) var sessionTreeContext = SessionTree.Context()

    /// EXP-746: the caller's most recent finished runs, newest first, capped
    /// at `PastRuns.cap`. Empty = the section is absent entirely.
    private(set) var pastRows: [PastRow] = []

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
    /// The last refusal, keyed by DEVICE id (Android `commandStates`): the
    /// caption belongs under the chips of the machine that refused, which is
    /// where the tap was — a single page-level slot at the bottom of the page
    /// is invisible from the machine rows at the top.
    private(set) var accountActionErrors: [String: String] = [:]
    /// EXP-829: the command queue the refreshes ride — set by the Devices
    /// page only. Nil (the Agent page, the onboarding step) = the accounts
    /// are still derived but nothing is ever queued from there.
    var devicesApi: DevicesApi?
    /// EXP-481: the synced worktree inventory (shape 18) — the composer's
    /// resume probe. EXP-1042: nothing else reads it on a phone; the
    /// device-settings list left for the IDE.
    var worktrees: [DeviceWorktreeEntity] = []
    /// EXP-694: the synced actions/automations, account-wide (a session names
    /// its own team). The session rows' trailing control resolves its glyph and
    /// its editor target through these — no network read.
    var actions: [ActionDto] = []
    var automations: [AutomationDto] = []

    /// The team the surrounding view currently shows — kept current by
    /// `AgentsView` (the LIVE sessions observation is account-wide and the
    /// list filters it; the ended-runs one is team-scoped in SQL, EXP-758).
    /// nil until the team state resolves: no rows.
    var activeTeamId: String? {
        didSet {
            guard oldValue != activeTeamId else { return }
            // EXP-758: the "Recent" query is USER- and TEAM-scoped in SQL, so a
            // team switch has to RE-ARM that observation — re-filtering what
            // the previous one emitted would show the old team's rows.
            startEndedObservation()
            rebuild()
            rebuildDevices()
        }
    }

    private let accountId: String
    private let userId: String?
    private let db: DatabaseManager
    // Stored and cancelled individually — a single wrapper task would not
    // propagate cancellation into unstructured inner loops, and the view
    // re-arms on every appear.
    private var sessionTask: Task<Void, Never>?
    /// EXP-746: ended rows are observed separately from the live ones — the
    /// live query filters on status and would otherwise have to carry them
    /// through every liveness rule that only makes sense for a running run.
    private var endedTask: Task<Void, Never>?
    private var issueTask: Task<Void, Never>?
    private var boardTask: Task<Void, Never>?
    private var livenessTask: Task<Void, Never>?
    private var deviceTask: Task<Void, Never>?
    private var worktreeTask: Task<Void, Never>?
    private var userTask: Task<Void, Never>?
    // EXP-694: the action/automation stores behind the session rows' editor
    // buttons.
    private var actionTask: Task<Void, Never>?
    private var automationTask: Task<Void, Never>?
    // EXP-996: the workflow shapes behind the tree's group rows.
    private var workflowTask: Task<Void, Never>?
    private var workflowNodeTask: Task<Void, Never>?
    /// EXP-656: wakes when our own `devices` shape completes a poll — the
    /// missing foreground re-derivation hook. Presence is only as current as
    /// that cursor, so a machine's badge must repaint the moment it advances
    /// (and not before: an unrefreshed cursor renders presence as unknown).
    private var freshnessTask: Task<Void, Never>?
    /// EXP-758: are the observations armed? The ended-runs one re-arms on a
    /// team switch, which must be a no-op while the view is off screen.
    private var observing = false

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
    private var endedSessions: [CodingSessionEntity] = []
    private var issues: [IssueEntity] = []
    // EXP-996: the two workflow shapes behind the tree's workflow GROUP rows —
    // a run groups only under a workflow that is HERE, because this is where
    // the group row's name comes from.
    private var workflows: [WorkflowEntity] = []
    private var workflowNodes: [WorkflowNodeEntity] = []
    // Observed so the composer's issue pool can resolve repo-backed boards
    // (EXP-156) and so the batch-PR resolution can scope issues to the
    // active team (EXP-535 — issues don't sync team_id).
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
        observing = true

        let sessionObservation = ValueObservation.tracking { db in
            try CodingSessionEntity
                // Every live status — an in_review session is still watchable
                // (EXP-194); `rebuild()`'s liveness filter drops stale rows.
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
                    self?.rebuild()
                }
            } catch {}
        }

        startEndedObservation()

        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity.fetchAll(db)
        }
        issueTask = Task { [weak self] in
            do {
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues
                    self?.rebuild()
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
                    self?.rebuild()
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
                    // EXP-549/550: the session rows carry a device label +
                    // liveness derived from these — a rename or a machine
                    // going quiet has to repaint the running list too.
                    self?.rebuild()
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
        // came from (its glyph, and the editor the button opens), so the two
        // action-side shapes ride along here.
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
        let automationObservation = ValueObservation.tracking { db in
            try AutomationEntity.fetchAll(db)
        }
        automationTask = Task { [weak self] in
            do {
                for try await rows in automationObservation.values(in: pool) {
                    self?.automations = rows.map { AutomationDto(entity: $0) }
                }
            } catch {}
        }

        // EXP-996: a run of a workflow node nests under ITS workflow, named by
        // the `workflows` shape — so both shapes feed the same rebuild the
        // sessions do. Fetched whole and scoped to the active team in
        // `rebuild()` (the issues/boards pattern), which keeps a team switch a
        // pure re-derivation instead of a re-armed observation.
        let workflowObservation = ValueObservation.tracking { db in
            try WorkflowEntity.fetchAll(db)
        }
        workflowTask = Task { [weak self] in
            do {
                for try await rows in workflowObservation.values(in: pool) {
                    self?.workflows = rows
                    self?.rebuild()
                }
            } catch {}
        }
        let workflowNodeObservation = ValueObservation.tracking { db in
            try WorkflowNodeEntity.fetchAll(db)
        }
        workflowNodeTask = Task { [weak self] in
            do {
                for try await rows in workflowNodeObservation.values(in: pool) {
                    self?.workflowNodes = rows
                    self?.rebuild()
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
                self.rebuild()
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
                self.rebuild()
                self.rebuildDevices()
            }
        }
    }

    /// EXP-746/758: the finished runs behind "Recent" — the caller's OWN rows in
    /// the ACTIVE team, PERSON-started (`started_reason IS NULL`), newest end
    /// first and hard-capped, all of it in the QUERY. Android's
    /// `CodingSessionDao.observePastByTeamAndUser` is the reference predicate:
    /// an unscoped, unbounded fetch is every ended run of every team on the
    /// device, rebuilt on every `coding_sessions` write. Queried WIDER than
    /// `PastRuns.cap` so a row the pure filter drops can't pull a real one off
    /// the end, and the pure `PastRuns.select` applies the same rules again on
    /// the way out.
    private func startEndedObservation() {
        endedTask?.cancel()
        endedTask = nil
        guard observing else { return }
        // No resolved account or team owns nothing (`CodingSessionOwnership`):
        // the section is empty rather than everyone's history.
        guard let userId, !userId.isEmpty,
              let teamId = activeTeamId, !teamId.isEmpty,
              let pool = try? db.pool(forAccountId: accountId)
        else {
            endedSessions = []
            rebuildPast()
            return
        }
        let endedObservation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("user_id") == userId)
                .filter(Column("team_id") == teamId)
                .filter(Column("status") == DomainContract.codingSessionStatusEnded)
                .filter(Column("started_reason") == nil)
                // The ×4 ordering key (`PastRuns.endedAt`): a row swept before
                // it ever stamped `ended_at` still orders off its heartbeat.
                .order(sql: "COALESCE(ended_at, updated_at) DESC")
                .limit(PastRuns.queryLimit)
                .fetchAll(db)
        }
        endedTask = Task { [weak self] in
            do {
                for try await sessions in endedObservation.values(in: pool) {
                    self?.endedSessions = sessions
                    self?.rebuildPast()
                }
            } catch {}
        }
    }

    func stopObserving() {
        observing = false
        sessionTask?.cancel()
        sessionTask = nil
        endedTask?.cancel()
        endedTask = nil
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
        automationTask?.cancel()
        automationTask = nil
        workflowTask?.cancel()
        workflowTask = nil
        workflowNodeTask?.cancel()
        workflowNodeTask = nil
        freshnessTask?.cancel()
        freshnessTask = nil
    }

    /// EXP-656: may a stale `last_seen_at` be read as "that machine is gone"?
    /// Only when our own `devices` shape polled within the contract window —
    /// otherwise the rows are pre-sleep knowledge and presence is unknown.
    private func devicesFresh(now: Date = Date()) -> Bool {
        DeviceFreshness.isTrustworthy(
            devicesPolledAt: SyncFreshness.shared.devicesPolledAt(accountId: accountId),
            now: now
        )
    }

    /// EXP-481: recompose the machines list from the observed rows — own
    /// machines first (most recently seen), then the active team's shared
    /// servers, exactly the `devices.list` ordering the view already renders.
    private func rebuildDevices() {
        guard let deviceEntities else { return }
        let now = Date()
        devices = DeviceQueries.compose(
            rows: deviceEntities,
            users: users,
            teamId: activeTeamId,
            userId: userId,
            now: now
        )
        rebuildAccounts(deviceEntities, now: now)
        // EXP-746: the Resume affordance is gated on the run's machine being
        // online and `resume-run`-capable, so a heartbeat repaints Recent too.
        rebuildPast()
    }

    // MARK: - Agent logins (EXP-829/EXP-909)

    /// The logins off the same rows the machines list reads — own machines
    /// plus the servers teammates shared with the ACTIVE team — one row per
    /// machine × agent × profile (`AgentAccountsRows`, the ×4 rule). Every
    /// device row draws its own; EXP-909 folded the cross-device Accounts
    /// section away. A refresh may only be queued on one of MY online
    /// machines that advertises the cap.
    private func rebuildAccounts(_ entities: [DeviceEntity], now: Date) {
        let scoped = entities.filter { row in
            (userId != nil && row.userId == userId)
                || (activeTeamId.map { row.sharedTeamIds.contains($0) } == true && row.kind == "server")
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
    /// (contract agent order, its active login first). The ONE per-device
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

    /// The last account-command refusal for this machine, for the caption
    /// under ITS chips.
    func accountActionError(deviceId: String) -> String? {
        accountActionErrors[deviceId]
    }

    /// EXP-849: make this login the machine's ACTIVE one for its agent — the
    /// chip menu's "Set as default" (EXP-862 renamed the entry; the command is
    /// unchanged).
    ///
    /// Its own command kind (`agent_profile_use`, payload `{agent,
    /// profileId}`): the machine points its active-profile pointer at a login
    /// it ALREADY holds and re-reports `agent_accounts`. Deliberately not
    /// `agent_login` — that one drives a sign-in flow (and a switch's logout
    /// revokes a codex token server-side), neither of which this needs. No
    /// credential is read, copied or moved.
    ///
    /// Offered only on one of MY machines that is online; the server re-checks
    /// ownership and an unknown profile is refused there too. The outcome
    /// arrives via sync, when the machine's next report moves `active`.
    func useAccountHere(_ row: AgentProfileUsageRow) {
        queueAccountCommand(row, kind: Self.useAccountCommandKind)
    }

    /// EXP-849: the command kind that activates an EXISTING login on a
    /// machine. Accepted by `devices.createCommand` and handled by the desktop
    /// and the headless daemon (`agent_profiles::set_active_profile`).
    private static let useAccountCommandKind = "agent_profile_use"

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

    private func queueAccountCommand(
        _ row: AgentProfileUsageRow,
        kind: String
    ) {
        guard let devicesApi, row.mine, row.online else { return }
        guard !accountActions.contains(row.key) else { return }
        accountActionErrors[row.deviceId] = nil
        accountActions.insert(row.key)
        let accountId = accountId
        let key = row.key
        let deviceId = row.deviceId
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
                    guard let self, !Task.isCancelled else { return }
                    guard let command = try? await devicesApi.getCommand(
                        accountId: accountId, commandId: created.id
                    ) else { continue }
                    guard !command.isPending else { continue }
                    if command.isFailed {
                        self.accountActionErrors[deviceId] =
                            command.result ?? "The device refused the command."
                    }
                    break
                }
            } catch {
                self?.accountActionErrors[deviceId] = error.userFacingMessage
            }
            self?.accountActions.remove(key)
        }
    }

    /// EXP-746: the "Recent" rows — own, active-team, ended, person-started,
    /// newest by `ended_at ?? updated_at`, capped at 20. The predicate, the
    /// ordering key and the cap are the ×4-locked `PastRuns` rules; only the
    /// joins (issue, device presentation, resume target) are local.
    private func rebuildPast() {
        let now = Date()
        let fresh = devicesFresh(now: now)
        let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let deviceRows = deviceEntities ?? []
        let startTargets = devices ?? []
        pastRows = PastRuns.select(
            endedSessions, userId: userId, teamId: activeTeamId
        ).map { session in
            PastRow(
                session: session,
                issue: session.issueId.flatMap { issuesById[$0] },
                batchIssues: BatchRun.issues(session, issues: issues),
                device: SessionDevicePresentation.resolve(
                    session: session, devices: deviceRows, now: now, devicesFresh: fresh
                ),
                resume: RunResume.target(
                    for: session, devices: startTargets, currentUserId: userId
                )
            )
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

    private func rebuild() {
        // One clock for the whole pass (EXP-550): every row's offline-ness is
        // read against the same instant.
        let now = Date()
        let fresh = devicesFresh(now: now)
        let deviceRows = deviceEntities ?? []
        let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        rows = sessions
            // Own runs in the active team only: a teammate's session can't be
            // opened or steered (EXP-312), so listing it only read as "computer
            // not online" — and an own run in another team belongs under that
            // team (web parity, `use-agents-data.ts`).
            .filter {
                CodingSessionOwnership.isOwn($0, userId: userId, teamId: activeTeamId)
            }
            // Heartbeat-stale rows render as absent (EXP-153).
            .filter { CodingSessionLiveness.isLive($0) }
            .sorted { $0.startedAt > $1.startedAt }
            // issueId is nil for a desktop batch (multi-issue) run's session —
            // those rows render without an issue link. EXP-893: a row only
            // OPENS the run, so the batch-PR / merge-target resolution the
            // row's Merge circle needed lives in `AgentSessionModel` alone.
            .map { session in
                let issue = session.issueId.flatMap { issuesById[$0] }
                return Row(
                    session: session,
                    issue: issue,
                    // EXP-876: what names a batch row.
                    batchIssues: BatchRun.issues(session, issues: issues),
                    device: SessionDevicePresentation.resolve(
                        session: session, devices: deviceRows, now: now, devicesFresh: fresh
                    )
                )
            }
        // EXP-996: the grouping context off the SAME pass — the rows and the
        // groups they sit under are derived together or not at all.
        sessionTreeContext = buildSessionTreeContext()
        // The Recent rows join the same issues and device rows this pass read.
        rebuildPast()
    }

    /// EXP-996: the tree's grouping context — the ACTIVE team's workflows and
    /// nodes (no team, no groups), plus the stack edges of the issues the
    /// listed rows name (an issue run's own, a batch run's covered set).
    private func buildSessionTreeContext() -> SessionTree.Context {
        guard let teamId = activeTeamId, !teamId.isEmpty else { return SessionTree.Context() }
        var edges: [String: IssueEntity] = [:]
        for row in rows {
            if let issue = row.issue { edges[issue.id] = issue }
            for issue in row.batchIssues { edges[issue.id] = issue }
        }
        return SessionTree.Context(
            workflows: workflows.filter { $0.teamId == teamId },
            workflowNodes: workflowNodes.filter { $0.teamId == teamId },
            // Id-ordered: a duplicated branch is resolved first-writer-wins
            // inside `PrStack`, and the store's dictionary order is no order.
            issues: edges.values.sorted { $0.id < $1.id }
        )
    }
}
