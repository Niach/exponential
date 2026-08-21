import ExpCore
import Foundation
import GRDB

/// Backs the Actions surface (EXP-253, mobile = view + run only): the active
/// team's action prompts LIVE from the synced local store (EXP-268 — actions
/// became the 15th Electric shape, minus `body`, which nothing here needs)
/// plus the remote-run flow. EXP-536: after the server accepts ANY start —
/// an action run or an issue/batch run off the sheet's Issues tab — the shared
/// `StartedRunWatcher` waits for the row the desktop inserts and surfaces it
/// exactly once, so the view jumps into the live steer screen.
@MainActor @Observable
final class ActionsViewModel {

    var actions: [ActionDto] = []
    var isLoading = false
    var loadError: String?
    /// EXP-583: the team's automations, live off the synced `automations`
    /// shape — sortOrder-then-createdAt, the server list's order.
    var automations: [AutomationDto] = []
    // EXP-530 Automations tab: recent automation-started runs (started_reason
    // non-null), newest first, capped at 10.
    var automationRuns: [CodingSessionEntity] = []
    /// Newest run per automation id (EXP-583) — the rows' "Last run" line.
    var lastRunByAutomation: [String: CodingSessionEntity] = [:]
    /// EVERY synced device, offline included (EXP-530) — the Automations tab
    /// resolves an automation's bound deviceId to a label + online dot, and an
    /// offline machine must still be nameable AND pickable (its missed
    /// schedule fires once it comes back).
    var allDevices: [SteerDevice] = []
    /// EXP-583: automation writes are owner-gated (the `automations` router is
    /// owner-only server-side — mirror it instead of bouncing on submit).
    var permissions: TeamPermissions = .denied
    /// Automation id with an in-flight write (disables that row's controls);
    /// Electric echoes the changed row back into `automations`.
    var automationBusyId: String?
    var automationError: String?
    // Issues the unified sheet's Issues tab can queue (Android parity —
    // the AgentsViewModel.startCandidates rules): the loaded team's
    // repo-backed boards; open issues, recency-ordered.
    // Rebuilt on every Run tap; the sheet's candidate pool self-heals if
    // the read lands after presentation.
    var startCandidates: [StartCodingSheet.IssueOption] = []

    // Run feedback (the AgentsView split): an informational "waiting for the
    // desktop" caption vs a red failure — a start error must read as an error
    // and a fresh attempt supersedes both. EXP-536: both live on the watcher
    // now, alongside the one-shot navigation target it resolves.
    let startWatcher = StartedRunWatcher()

    private let accountId: String
    private let db: DatabaseManager
    private let steerApi: SteerApi
    private let automationsApi: AutomationsApi
    private let auth: AuthRepository

    private var loadedTeamId: String?
    private var actionsObservationTask: Task<Void, Never>?
    private var runsObservationTask: Task<Void, Never>?
    private var automationsObservationTask: Task<Void, Never>?

    init(
        accountId: String,
        db: DatabaseManager,
        steerApi: SteerApi,
        automationsApi: AutomationsApi,
        auth: AuthRepository
    ) {
        self.accountId = accountId
        self.db = db
        self.steerApi = steerApi
        self.automationsApi = automationsApi
        self.auth = auth
    }

    /// Observe the team's synced actions (EXP-268: the local GRDB store, not
    /// tRPC — the list stays live as sync lands rows). The "Fix merge
    /// conflicts" builtin is PREPENDED locally — pinned FIRST by the
    /// `builtin` flag (the EXP-257 contract — never by sort order); "Create
    /// action" is deliberately NOT listed (EXP-431 — creation lives behind
    /// the toolbar's "New action" button instead of posing as a runnable
    /// action). Real rows sort sortOrder-then-name like the server list did.
    func load(teamId: String) async {
        if loadedTeamId != teamId {
            // New team context — drop the previous team's rows.
            actions = []
            loadError = nil
        }
        loadedTeamId = teamId
        if actions.isEmpty { isLoading = true }
        actionsObservationTask?.cancel()
        runsObservationTask?.cancel()
        automationsObservationTask?.cancel()
        guard let pool = try? db.pool(forAccountId: accountId) else {
            isLoading = false
            loadError = "The local database is unavailable."
            return
        }
        // EXP-530: owner-gate + device labels for the Automations tab.
        let team = (try? await pool.read { db in try TeamEntity.fetchOne(db, key: teamId) }) ?? nil
        permissions = TeamPermissions.resolve(
            team: team,
            currentUserId: auth.userId,
            isAdmin: auth.isAdmin,
            dbPool: pool
        )
        allDevices = await DeviceQueries.devices(
            db: db, accountId: accountId, teamId: teamId, userId: auth.userId
        )
        observeAutomationRuns(teamId: teamId, pool: pool)
        observeAutomations(teamId: teamId, pool: pool)
        let observation = ValueObservation.tracking { db in
            try ActionEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }
        actionsObservationTask = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    guard self.loadedTeamId == teamId else { return }
                    let dtos = rows
                        .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
                        .map { ActionDto(entity: $0) }
                    self.actions = ActionDto.builtinActions(teamId: teamId)
                        .filter { $0.id != DomainContract.builtinCreateActionId } + dtos
                    self.isLoading = false
                    self.loadError = nil
                }
            } catch {
                guard let self, !Task.isCancelled, self.loadedTeamId == teamId else { return }
                self.isLoading = false
                self.loadError = error.localizedDescription
            }
        }
    }

    /// Observe the team's automation-started runs (EXP-530: coding_sessions
    /// rows with a non-null started_reason), newest first, capped at 10 for
    /// the "Recent automated runs" list. EXP-583: the same rows carry
    /// `automation_id`, which gives every automation row its "Last run".
    private func observeAutomationRuns(teamId: String, pool: DatabasePool) {
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("team_id") == teamId)
                .filter(Column("started_reason") != nil)
                .fetchAll(db)
        }
        runsObservationTask = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    guard self.loadedTeamId == teamId else { return }
                    let ordered = rows.sorted { $0.startedAt > $1.startedAt }
                    self.automationRuns = Array(ordered.prefix(10))
                    // Newest first, so the FIRST row per automation wins.
                    var newest: [String: CodingSessionEntity] = [:]
                    for row in ordered {
                        guard let automationId = row.automationId,
                              newest[automationId] == nil else { continue }
                        newest[automationId] = row
                    }
                    self.lastRunByAutomation = newest
                }
            } catch {
                // Non-fatal — the list simply stays as it was.
            }
        }
    }

    /// Observe the team's automations (EXP-583: the synced `automations`
    /// shape, not tRPC — the list stays live as sync lands rows), in the
    /// server list's sortOrder-then-createdAt order.
    private func observeAutomations(teamId: String, pool: DatabasePool) {
        let observation = ValueObservation.tracking { db in
            try AutomationEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }
        automationsObservationTask = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    guard self.loadedTeamId == teamId else { return }
                    self.automations = rows
                        .sorted { ($0.sortOrder ?? 0, $0.createdAt) < ($1.sortOrder ?? 0, $1.createdAt) }
                        .map { AutomationDto(entity: $0) }
                }
            } catch {
                // Non-fatal — the list simply stays as it was.
            }
        }
    }

    /// Flip an automation's paused flag (EXP-583): `automations.update` takes
    /// just { id, enabled }. Owner-only server-side — the UI disables the
    /// control for everyone else. Success needs no local write: Electric
    /// echoes the row.
    func setAutomationEnabled(_ automation: AutomationDto, enabled: Bool) {
        guard automationBusyId == nil else { return }
        automationBusyId = automation.id
        automationError = nil
        Task {
            do {
                try await automationsApi.update(
                    accountId: accountId,
                    id: automation.id,
                    enabled: enabled
                )
            } catch {
                automationError = error.localizedDescription
            }
            automationBusyId = nil
        }
    }

    /// Owner-only delete (EXP-583). The row leaves via Electric.
    func deleteAutomation(_ automation: AutomationDto) {
        guard automationBusyId == nil else { return }
        automationBusyId = automation.id
        automationError = nil
        Task {
            do {
                try await automationsApi.delete(accountId: accountId, id: automation.id)
            } catch {
                automationError = error.localizedDescription
            }
            automationBusyId = nil
        }
    }

    /// Create or update an automation from the form sheet (EXP-583). `editing`
    /// nil = create. The sheet dismisses on submit; failures surface as
    /// `automationError` on the list, like a failed start does.
    func saveAutomation(
        editing: AutomationDto?,
        teamId: String,
        actionId: String,
        deviceId: String,
        trigger: AutomationTrigger,
        launch: AutomationLaunchPatch
    ) {
        automationError = nil
        Task {
            do {
                if let editing {
                    try await automationsApi.update(
                        accountId: accountId,
                        id: editing.id,
                        actionId: actionId,
                        deviceId: deviceId,
                        trigger: trigger,
                        launch: launch
                    )
                } else {
                    try await automationsApi.create(
                        accountId: accountId,
                        teamId: teamId,
                        actionId: actionId,
                        deviceId: deviceId,
                        trigger: trigger,
                        agent: launch.agent,
                        model: launch.model,
                        effort: launch.effort
                    )
                }
            } catch {
                automationError = error.localizedDescription
            }
        }
    }

    /// Remote-run `action` on `device` (EXP-257: the full option set with the
    /// same per-agent vocabulary as issue runs, plus typed `inputs` — key →
    /// text or picked repo/board uuid). `teamId` rides only for the builtin
    /// "Create action" (the server requires it there, forbids it otherwise).
    /// `userId` is the caller's server user id, used to recognize the
    /// desktop-inserted session row.
    func run(
        action: ActionDto,
        device: SteerDevice,
        options: SteerStartOptions,
        inputs: [String: String],
        userId: String?
    ) {
        startWatcher.sending()
        Task {
            do {
                try await steerApi.startSession(
                    accountId: accountId,
                    actionId: action.id,
                    deviceId: device.deviceId,
                    teamId: action.isBuiltin ? action.teamId : nil,
                    options: options,
                    inputs: inputs.isEmpty ? nil : inputs
                )
                startWatcher.begin(
                    key: .action(name: action.name),
                    userId: userId,
                    device: device,
                    db: db,
                    accountId: accountId
                )
            } catch {
                startWatcher.failed(error.localizedDescription)
            }
        }
    }

    /// One-shot rebuild of `startCandidates` from the synced store — the
    /// same eligibility as the Agents-tab picker (repo-backed boards, open
    /// issues, no merged PR), scoped to the loaded team.
    func refreshStartCandidates() async {
        startCandidates = await StartCodingSheet.IssueOption.loadCandidates(
            db: db,
            accountId: accountId,
            teamId: loadedTeamId
        )
    }

    /// Remote-start issues from the unified sheet's Issues tab (the
    /// AgentsView.start twin, surfaced through the same captions): 1 id
    /// launches a plain single-issue session, 2+ a batch. EXP-536: both wait
    /// for the desktop's row and push the live session, exactly like an
    /// action run.
    func startCoding(
        device: SteerDevice,
        issueIds: [String],
        options: SteerStartOptions,
        userId: String?
    ) {
        guard let key = StartedRunKey.forIssues(issueIds) else { return }
        startWatcher.sending()
        Task {
            do {
                if issueIds.count > 1 {
                    try await steerApi.startSession(
                        accountId: accountId,
                        issueIds: issueIds,
                        deviceId: device.deviceId,
                        options: options
                    )
                } else {
                    try await steerApi.startSession(
                        accountId: accountId,
                        issueId: issueIds[0],
                        deviceId: device.deviceId,
                        options: options
                    )
                }
                startWatcher.begin(
                    key: key,
                    userId: userId,
                    device: device,
                    db: db,
                    accountId: accountId
                )
            } catch {
                startWatcher.failed(error.localizedDescription)
            }
        }
    }

    func stopWatching() {
        startWatcher.stop()
    }
}
