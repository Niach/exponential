import ExpCore
import Foundation
import GRDB

/// Backs the Actions surface (EXP-253, mobile = view + run only): the active
/// team's action prompts over tRPC (`actions.list` — deliberately NOT an
/// Electric shape) plus the remote-run flow. After the server accepts a start,
/// the model watches the synced `coding_sessions` table for the row the
/// desktop inserts (this action's id + the caller's own userId + a recent
/// startedAt) and surfaces it exactly once as `startedSession` so the view can
/// jump into the existing live steer screen.
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

    // Run feedback (the AgentsView split): success caption (informational,
    // tertiary) vs failure (red) — a start error must read as an error and a
    // fresh attempt supersedes both.
    var sentCaption: String?
    var startError: String?
    var startedSession: StartedSession?

    private let accountId: String
    private let db: DatabaseManager
    private let actionsApi: ActionsApi
    private let steerApi: SteerApi

    private var loadedTeamId: String?
    private var watchTask: Task<Void, Never>?
    private var watchDeadlineTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager, actionsApi: ActionsApi, steerApi: SteerApi) {
        self.accountId = accountId
        self.db = db
        self.actionsApi = actionsApi
        self.steerApi = steerApi
    }

    func load(teamId: String) async {
        if loadedTeamId != teamId {
            // New team context — drop the previous team's rows.
            actions = []
            loadError = nil
        }
        loadedTeamId = teamId
        if actions.isEmpty { isLoading = true }
        defer { isLoading = false }
        do {
            let rows = try await actionsApi.list(accountId: accountId, teamId: teamId)
            guard loadedTeamId == teamId else { return }
            actions = rows
            loadError = nil
        } catch is CancellationError {
            // Expected when the hosting .task is torn down mid-flight.
        } catch {
            guard loadedTeamId == teamId else { return }
            loadError = error.localizedDescription
        }
    }

    /// Remote-run `action` on `device` (Claude-only v1 — model/effort are the
    /// only options; nil = desktop settings default). `userId` is the caller's
    /// server user id, used to recognize the desktop-inserted session row.
    func run(
        action: ActionDto,
        device: SteerDevice,
        model: String?,
        effort: String?,
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
                    model: model,
                    effort: effort
                )
                sentCaption = "Start sent to \(label) — waiting for the desktop…"
                watchForStartedRun(actionId: action.id, userId: userId)
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
    /// action, the caller's own userId, and a startedAt after the send (with
    /// clock-skew slack) — an old run of the same action must never re-trigger
    /// navigation. A wall-clock deadline task gives up INDEPENDENTLY of DB
    /// emissions (with no coding_sessions writes the observation never fires,
    /// so an emission-gated check alone would show the caption forever) and
    /// clears the caption so the send doesn't read as still-pending.
    private func watchForStartedRun(actionId: String, userId: String?) {
        stopWatching()
        guard let userId, let pool = try? db.pool(forAccountId: accountId) else { return }
        let cutoff = Date().addingTimeInterval(-120)
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("action_id") == actionId)
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
