import ExpCore
import Foundation
import GRDB

/// Backs the Actions list (EXP-253): the action prompts of EVERY member team
/// (EXP-1186, cross-team like the Inbox; the view groups them per team) LIVE
/// from the synced local store (EXP-268 — actions became the 15th Electric
/// shape, minus `body`, which nothing here needs). SLOP-2: each row carries
/// its own `triggers`, so the list reads the trigger glyphs straight off it;
/// the triggers themselves and the runs live on the action page
/// (`ActionDetailViewModel`). EXP-825: running an action is NAVIGATION — the
/// Agent page composer owns the send and the post-start watch — so no
/// remote-start plumbing lives here.
@MainActor @Observable
final class ActionsViewModel {

    var actions: [ActionDto] = []
    var isLoading = false
    var loadError: String?
    /// EVERY synced device, offline included (EXP-530) — a suggestion's
    /// trigger block names the runner, and an offline machine must still be
    /// nameable (its missed schedule fires once it comes back).
    var allDevices: [SteerDevice] = []

    private let accountId: String
    private let db: DatabaseManager
    private let auth: AuthRepository

    private var loadedTeamId: String?
    private var actionsObservationTask: Task<Void, Never>?

    init(
        accountId: String,
        db: DatabaseManager,
        auth: AuthRepository
    ) {
        self.accountId = accountId
        self.db = db
        self.auth = auth
    }

    /// Observe every member team's synced actions (EXP-268: the local GRDB
    /// store, not tRPC — the list stays live as sync lands rows; EXP-1186:
    /// the synced set IS the member teams'). Real rows sort
    /// sortOrder-then-name like the server list did; the view splits them
    /// per team. `teamId` = the ACTIVE team, which only scopes the
    /// suggestions' trigger machines (a suggestion opens that team's
    /// composer).
    func load(teamId: String) async {
        loadedTeamId = teamId
        if actions.isEmpty { isLoading = true }
        actionsObservationTask?.cancel()
        guard let pool = try? db.pool(forAccountId: accountId) else {
            isLoading = false
            loadError = "The local database is unavailable."
            return
        }
        allDevices = await DeviceQueries.devices(
            db: db, accountId: accountId, teamId: teamId, userId: auth.userId
        )
        let observation = ValueObservation.tracking { db in
            try ActionEntity.fetchAll(db)
        }
        actionsObservationTask = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    guard self.loadedTeamId == teamId else { return }
                    let dtos = rows
                        .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
                        .map { ActionDto(entity: $0) }
                    // EXP-686: no builtins in the LIST — "Fix merge conflicts"
                    // stays launchable from Reviews/Changes, and the Agent page
                    // composer builds its own pool that still carries it.
                    self.actions = dtos
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
}
