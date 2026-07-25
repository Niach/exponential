import ExpCore
import Foundation
import GRDB

/// Backs the Actions surface (EXP-253, mobile = view + run only): the active
/// team's action prompts LIVE from the synced local store (EXP-268 — actions
/// became the 15th Electric shape, minus `body`, which nothing here needs)
/// plus the remote-run flow. After the server accepts a start,
/// the model watches the synced `coding_sessions` table for the row the
/// desktop inserts (this action's NAME snapshot + the caller's own userId + a
/// recent startedAt — never the action id: the builtin "Create action" run's
/// row carries `action_id` NULL, EXP-257) and surfaces it exactly once as
/// `startedSession` so the view can jump into the existing live steer screen.
@MainActor @Observable
final class ActionsViewModel {
    /// The freshly-started run's session — consumed once by the view's
    /// navigation push.
    struct StartedSession: Hashable {
        let sessionId: String
    }

    var actions: [ActionDto] = []
    var isLoading = false
    var loadError: String?
    // Issues the unified sheet's Issues tab can queue (Android parity —
    // the AgentsViewModel.startCandidates rules): the loaded team's
    // repo-backed non-archived boards; open issues, recency-ordered.
    // Rebuilt on every Run tap; the sheet's candidate pool self-heals if
    // the read lands after presentation.
    var startCandidates: [StartCodingSheet.IssueOption] = []

    // Run feedback (the AgentsView split): success caption (informational,
    // tertiary) vs failure (red) — a start error must read as an error and a
    // fresh attempt supersedes both.
    var sentCaption: String?
    var startError: String?
    var startedSession: StartedSession?

    private let accountId: String
    private let db: DatabaseManager
    private let steerApi: SteerApi

    private var loadedTeamId: String?
    private var actionsObservationTask: Task<Void, Never>?
    private var watchTask: Task<Void, Never>?
    private var watchDeadlineTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager, steerApi: SteerApi) {
        self.accountId = accountId
        self.db = db
        self.steerApi = steerApi
    }

    /// Observe the team's synced actions (EXP-268: the local GRDB store, not
    /// tRPC — the list stays live as sync lands rows). The builtin "Create
    /// action" is PREPENDED locally — pinned FIRST by the `builtin` flag (the
    /// EXP-257 contract — never by sort order); real rows sort
    /// sortOrder-then-name like the server list did.
    func load(teamId: String) async {
        if loadedTeamId != teamId {
            // New team context — drop the previous team's rows.
            actions = []
            loadError = nil
        }
        loadedTeamId = teamId
        if actions.isEmpty { isLoading = true }
        actionsObservationTask?.cancel()
        guard let pool = try? db.pool(forAccountId: accountId) else {
            isLoading = false
            loadError = "The local database is unavailable."
            return
        }
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
                    self.actions = [ActionDto.builtinCreateAction(teamId: teamId)] + dtos
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
        // A fresh attempt supersedes the previous outcome (success or error).
        sentCaption = nil
        startError = nil
        let label = device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
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
                sentCaption = "Start sent to \(label) — waiting for the desktop…"
                watchForStartedRun(actionName: action.name, userId: userId)
            } catch {
                startError = error.localizedDescription
            }
        }
    }

    /// One-shot rebuild of `startCandidates` from the synced store — the
    /// same eligibility as the Agents-tab picker (repo-backed non-archived
    /// boards, open issues, no merged PR), scoped to the loaded team.
    func refreshStartCandidates() async {
        guard let teamId = loadedTeamId, let pool = try? db.pool(forAccountId: accountId) else {
            startCandidates = []
            return
        }
        let boards = (try? await pool.read { db in try BoardEntity.fetchAll(db) }) ?? []
        let issues = (try? await pool.read { db in try IssueEntity.fetchAll(db) }) ?? []
        // Repo-backed, non-archived boards only — boardId → repositoryId.
        var repoByBoard: [String: String] = [:]
        for board in boards where board.archivedAt == nil && board.teamId == teamId {
            if let repoId = board.repositoryId {
                repoByBoard[board.id] = repoId
            }
        }
        let terminal: Set<String> = [
            IssueStatus.done.rawValue,
            IssueStatus.cancelled.rawValue,
            IssueStatus.duplicate.rawValue,
        ]
        startCandidates = issues
            .filter { row in
                guard repoByBoard[row.boardId] != nil else { return false }
                if row.archivedAt != nil { return false }
                if terminal.contains(row.status) { return false }
                if row.prState == DomainContract.prStateMerged { return false }
                return true
            }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map { row in
                StartCodingSheet.IssueOption(
                    id: row.id,
                    identifier: row.identifier,
                    title: row.title,
                    repositoryId: repoByBoard[row.boardId],
                    status: row.status,
                    priority: row.priority
                )
            }
    }

    /// Remote-start issues from the unified sheet's Issues tab (the
    /// AgentsView.start twin, surfaced through the same captions): 1 id
    /// launches a plain single-issue session, 2+ a batch. No session watch —
    /// the run surfaces on the Agents tab via sync like any desktop start.
    func startCoding(device: SteerDevice, issueIds: [String], options: SteerStartOptions) {
        guard !issueIds.isEmpty else { return }
        // A fresh attempt supersedes the previous outcome (success or error).
        sentCaption = nil
        startError = nil
        let isBatch = issueIds.count > 1
        let label = device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
        Task {
            do {
                if isBatch {
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
                let sent = isBatch
                    ? "Batch start sent to \(label) — it'll appear on the Agents tab when it spins up."
                    : "Start sent to \(label) — it'll appear on the Agents tab when it spins up."
                sentCaption = sent
                // The informational caption clears after a grace window; a
                // newer attempt's caption must not be clobbered (errors
                // persist until the next attempt so they can't be missed).
                try? await Task.sleep(for: .seconds(30))
                if sentCaption == sent {
                    sentCaption = nil
                }
            } catch {
                startError = error.localizedDescription
            }
        }
    }

    func stopWatching() {
        watchTask?.cancel()
        watchTask = nil
        watchDeadlineTask?.cancel()
        watchDeadlineTask = nil
    }

    /// Observe the synced coding_sessions table (the AgentsViewModel
    /// mechanism) until the desktop's row for THIS start appears: matching
    /// action NAME (the `action_name` display snapshot — the builtin "Create
    /// action" row carries `action_id` NULL, so the id can never match,
    /// EXP-257), the caller's own userId, and a startedAt after the send (with
    /// clock-skew slack) — an old run of the same action must never re-trigger
    /// navigation. A wall-clock deadline task gives up INDEPENDENTLY of DB
    /// emissions (with no coding_sessions writes the observation never fires,
    /// so an emission-gated check alone would show the caption forever) and
    /// clears the caption so the send doesn't read as still-pending.
    private func watchForStartedRun(actionName: String, userId: String?) {
        stopWatching()
        guard let userId, let pool = try? db.pool(forAccountId: accountId) else { return }
        let cutoff = Date().addingTimeInterval(-120)
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("action_name") == actionName)
                .fetchAll(db)
        }
        watchTask = Task { [weak self] in
            do {
                for try await sessions in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    let match = sessions.first { session in
                        session.userId == userId
                            && CodingSessionLiveness.parseIso(session.startedAt)
                                .map { $0 >= cutoff } ?? false
                    }
                    if let match {
                        self.stopWatching()
                        self.sentCaption = nil
                        self.startedSession = StartedSession(sessionId: match.id)
                        return
                    }
                }
            } catch {}
        }
        watchDeadlineTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 180 * 1_000_000_000)
            guard let self, !Task.isCancelled else { return }
            self.stopWatching()
            self.sentCaption = nil
        }
    }
}
