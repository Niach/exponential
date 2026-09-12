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
        /// EXP-535: a batch session's resolved open PR, as a representative
        /// linked issue (merging through it merges the ONE batch PR — Reviews
        /// pattern). Set only on issueless batch rows in review with an
        /// UNAMBIGUOUS match.
        let batchPrIssue: IssueEntity?
        /// EXP-734: what this row's Merge button merges through — the issue's
        /// PR (issue and batch runs) or the run's OWN issue-less PR (an action
        /// or chat run that opened one). Nil when there is nothing to merge.
        let mergeTarget: MergeTarget?
        /// EXP-549/550: the host machine as it presents right now — the LIVE
        /// devices row's label (a rename never rewrites the session's
        /// snapshot) plus whether that machine stopped heartbeating, which
        /// makes a still-coding run read "Paused" instead of live.
        let device: SessionDevicePresentation
        var id: String { session.id }
    }

    /// EXP-746: one finished run under "Past" — an ended, PERSON-started run
    /// of the caller's in the active team. Automation runs are not here: they
    /// live under Automations' "Recent automated runs" (EXP-676) and the two
    /// sets are disjoint by `started_reason`, so the two Resume paths can
    /// never double-fire on the same row.
    struct PastRow: Identifiable {
        let session: CodingSessionEntity
        let issue: IssueEntity?
        /// The host machine as it presents right now (the live devices row's
        /// label, not the session's start-time snapshot).
        let device: SessionDevicePresentation
        /// Non-nil when a Resume would be accepted: the run's OWN machine,
        /// online and advertising `resume-run`.
        let resume: SteerDevice?
        var id: String { session.id }
    }

    var rows: [Row] = []

    /// EXP-746: the caller's most recent finished runs, newest first, capped
    /// at `PastRuns.cap`. Empty = the section is absent entirely.
    private(set) var pastRows: [PastRow] = []

    /// EXP-481: the machines list, composed from the synced `devices` shape
    /// (own rows + the active team's shared servers; online-ness derives from
    /// last_seen_at freshness) — the sync-fed replacement for the old 15s
    /// `devices.list` poll. nil until the first observation emission, so the
    /// view can tell "loading" from "no machines".
    var devices: [SteerDevice]?
    /// EXP-829: the Devices page's Accounts section (web/desktop EXP-818) —
    /// one row per agent ACCOUNT off the same devices rows, in agent bands,
    /// attention first. Empty until the first devices emission; see
    /// `accountsLoaded` for "loading" vs "no machine reported an account".
    private(set) var accountSections: [AgentAccountSection] = []
    private(set) var accountGroups: [AgentAccountUsageGroup] = []
    /// EXP-849: the FLAT machine × agent × profile rows behind both surfaces —
    /// the Accounts rows fold them by login, the machines list draws each
    /// machine's own as chips (health badge, re-login, "use this account
    /// here"). Derived in the same pass, so the two can never disagree.
    private(set) var accountRows: [AgentProfileUsageRow] = []
    var accountsLoaded: Bool { deviceEntities != nil }
    /// EXP-829: the accounts a usage refresh is in flight for (keyed like
    /// `accountGroups`) — the row shows a spinner in place of its refresh
    /// glyph. Cleared when the machine's re-report moves the stamp, or after
    /// `Self.refreshPendingWindow` with no answer.
    private(set) var refreshingAccounts: Set<String> = []
    /// EXP-829: the last refresh that could not be queued, for the row.
    var accountError: String?
    /// EXP-849: the account ACTIONS a machine chip offers (re-login, "use this
    /// account here") — the rows a command is in flight for, keyed by
    /// `AgentProfileUsageRow.key`, and the last refusal. The outcome itself
    /// lands via sync: the machine re-probes and its `agent_accounts` report
    /// moves.
    private(set) var accountActions: Set<String> = []
    var accountActionError: String?
    /// EXP-829: the command queue the refreshes ride — set by the Devices
    /// page only. Nil (the Agent page, the onboarding step) = the accounts
    /// are still derived but nothing is ever queued from there.
    var devicesApi: DevicesApi?
    /// EXP-481: the synced worktree inventory (shape 18) — the composer's
    /// resume probe and the device-settings worktree list.
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
            // EXP-758: the "Past" query is USER- and TEAM-scoped in SQL, so a
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

    private var sessions: [CodingSessionEntity] = []
    private var endedSessions: [CodingSessionEntity] = []
    private var issues: [IssueEntity] = []
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
                .filter([
                    DomainContract.codingSessionStatusRunning,
                    DomainContract.codingSessionStatusInReview,
                ].contains(Column("status")))
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

    /// EXP-746/758: the finished runs behind "Past" — the caller's OWN rows in
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
        // online and `resume-run`-capable, so a heartbeat repaints Past too.
        rebuildPast()
    }

    // MARK: - Accounts (EXP-829)

    /// EXP-829: the Accounts section off the same rows the machines list
    /// reads — own machines plus the servers teammates shared with the
    /// ACTIVE team (web `AgentAccountsSection`'s filter), folded by login
    /// (`AgentAccountsRows`, the ×4 rule). A refresh may only be queued on
    /// one of MY online machines that advertises the cap.
    private func rebuildAccounts(_ entities: [DeviceEntity], now: Date) {
        let scoped = entities.filter { row in
            (userId != nil && row.userId == userId)
                || (activeTeamId.map { row.sharedTeamIds.contains($0) } == true && row.kind == "server")
        }
        let capsByDevice: [String: [String]] = Dictionary(
            (devices ?? []).map { ($0.deviceId, $0.caps ?? []) },
            uniquingKeysWith: { a, _ in a }
        )
        let rows = AgentAccountsRows.profileRows(
            devices: scoped,
            currentUserId: userId,
            isOnline: { DeviceLiveness.isOnline(lastSeenAt: $0, now: now) }
        )
        let groups = AgentAccountsRows.sortGroupsAttentionFirst(
            AgentAccountsRows.accountGroups(rows) { row in
                row.mine && row.online
                    && (capsByDevice[row.deviceId] ?? []).contains(AgentAccountsRows.refreshCap)
            }
        )
        accountRows = rows
        accountGroups = groups
        accountSections = AgentAccountsRows.sections(groups)
        pruneRefreshing(groups, now: now)
        autoRefreshAccounts(groups, now: now)
    }

    /// An in-flight mark clears when the account's stamp moved (the machine
    /// answered) or when nobody answered inside the pending window.
    private func pruneRefreshing(_ groups: [AgentAccountUsageGroup], now: Date) {
        guard !refreshMarks.isEmpty else { return }
        for (key, mark) in refreshMarks {
            let stamp = groups.first { $0.key == key }?.usage?.fetchedAt
            if stamp != mark.fetchedAt || now.timeIntervalSince(mark.at) > Self.refreshPendingWindow {
                refreshMarks[key] = nil
            }
        }
        refreshingAccounts = Set(refreshMarks.keys)
    }

    /// EXP-817: keep the section current while it is open. Every pass, each
    /// account with an eligible machine and a freshest report past the floor
    /// gets ONE refresh queued — never while one is in flight, never twice
    /// inside `autoRefreshRetry`. The floor is the device's own 429 budget,
    /// so this can never out-poll what the machine allows itself. Passes run
    /// on every devices emission and on the 30s liveness tick, exactly the
    /// web effect's `[groups, now]`.
    private func autoRefreshAccounts(_ groups: [AgentAccountUsageGroup], now: Date) {
        guard devicesApi != nil else { return }
        for group in groups {
            guard group.refreshTarget != nil else { continue }
            if refreshMarks[group.key] != nil { continue }
            if AgentAccountsRows.refreshAllowedAt(group.usage, now: now) != nil { continue }
            if let last = autoRefreshAttempts[group.key],
               now.timeIntervalSince(last) < Self.autoRefreshRetry {
                continue
            }
            autoRefreshAttempts[group.key] = now
            refreshAccount(group, silent: true)
        }
    }

    /// Whether the section refreshes by itself — any account has a machine
    /// that may run the command.
    var accountsAutoRefresh: Bool {
        accountGroups.contains { $0.refreshTarget != nil }
    }

    /// Queue `agent_usage_refresh` for the account on its refresh target.
    /// The page's own refresh (`silent`) fails quietly: a command still
    /// queued from the last round is a CONFLICT, and the next pass simply
    /// looks again.
    func refreshAccount(_ group: AgentAccountUsageGroup, silent: Bool = false) {
        guard let target = group.refreshTarget, let devicesApi else { return }
        if !silent { accountError = nil }
        refreshMarks[group.key] = (fetchedAt: group.usage?.fetchedAt, at: Date())
        refreshingAccounts = Set(refreshMarks.keys)
        let accountId = accountId
        Task { [weak self] in
            do {
                _ = try await devicesApi.createCommand(
                    accountId: accountId,
                    deviceId: target.deviceId,
                    kind: "agent_usage_refresh",
                    agent: target.agent,
                    profileId: target.profileId
                )
            } catch {
                guard let self else { return }
                self.refreshMarks[group.key] = nil
                self.refreshingAccounts = Set(self.refreshMarks.keys)
                if !silent { self.accountError = error.userFacingMessage }
            }
        }
    }

    // MARK: - Accounts: the two surfaces (EXP-849)

    /// EXP-849: the agents the Accounts section offers a TAB for, in contract
    /// order. One agent = no tabs (a single band of rows reads fine); two or
    /// more and codex stops crowding claude.
    var accountAgents: [String] {
        accountSections.map(\.agent)
    }

    /// The account rows under one agent tab — already attention-first
    /// (`sortGroupsAttentionFirst` ran over the whole set). Deliberately NOT
    /// named `accountGroups(agent:)`: a method may not share its base name
    /// with the stored property above.
    func groupsForAgent(_ agent: String) -> [AgentAccountUsageGroup] {
        accountSections.first { $0.agent == agent }?.groups ?? []
    }

    /// EXP-849: one machine's logins, as its row draws them (attention first).
    func deviceAccountRows(_ deviceId: String) -> [AgentProfileUsageRow] {
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

    /// EXP-849: make this login the machine's ACTIVE one for its agent — the
    /// Devices surface's "Use this account here".
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

    private func queueAccountCommand(
        _ row: AgentProfileUsageRow,
        kind: String
    ) {
        guard let devicesApi, row.mine, row.online else { return }
        guard !accountActions.contains(row.key) else { return }
        accountActionError = nil
        accountActions.insert(row.key)
        let accountId = accountId
        let key = row.key
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
                        self.accountActionError =
                            command.result ?? "The machine refused the command."
                    }
                    break
                }
            } catch {
                self?.accountActionError = error.userFacingMessage
            }
            self?.accountActions.remove(key)
        }
    }

    /// EXP-746: the "Past" rows — own, active-team, ended, person-started,
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
        // EXP-535: the active team's open batch PRs, collapsed once per
        // rebuild — each in-review batch row then resolves ITS OWN PR by the
        // branch the pr_open flip stamped on it (EXP-545, see
        // BatchPrResolution).
        let teamBoardIds = Set(boards.filter { $0.teamId == activeTeamId }.map(\.id))
        let openBatchPrs = BatchPrResolution.openBatchPrs(
            issues: issues, teamBoardIds: teamBoardIds
        )
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
            // those rows render without an issue link, but an issueless,
            // actionless batch run whose PR is open (status in_review —
            // flipped in the pr_open transaction) gets the resolved batch PR
            // for its Merge button (EXP-535).
            .map { session in
                let isBatch = session.issueId == nil && session.actionName == nil
                let issue = session.issueId.flatMap { issuesById[$0] }
                return Row(
                    session: session,
                    issue: issue,
                    batchPrIssue: isBatch
                        && session.status == DomainContract.codingSessionStatusInReview
                        ? BatchPrResolution.resolve(
                            sessionBranch: session.branch,
                            openBatchPrs: openBatchPrs
                        ) : nil,
                    // EXP-734: one rule for every merge surface — an action or
                    // chat run's PR links no issue, so it merges through the
                    // session row the server stamped it on.
                    mergeTarget: MergeTargetResolution.resolve(
                        session: session, issue: issue, openBatchPrs: openBatchPrs
                    ),
                    device: SessionDevicePresentation.resolve(
                        session: session, devices: deviceRows, now: now, devicesFresh: fresh
                    )
                )
            }
        // The Past rows join the same issues and device rows this pass read.
        rebuildPast()
    }
}
