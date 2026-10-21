import ExpCore
import ExpUI
import Foundation
import GRDB

/// Backs the action page: ONE action live off the synced store plus the
/// caller's permissions on its team (writes are owner-only server-side).
/// EXP-1291: an action is a saved prompt — no triggers, no runs of its own.
@MainActor @Observable
final class ActionDetailViewModel {

    /// nil until the row resolves — and again once it is gone.
    var action: ActionDto?
    /// The first read came back: `action == nil` now means "not here", not
    /// "still loading".
    var resolved = false
    var permissions: TeamPermissions = .denied

    private let accountId: String
    private let db: DatabaseManager
    private let auth: AuthRepository

    private var loadedActionId: String?
    private var loadedTeamId: String?
    private var actionObservationTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager, auth: AuthRepository) {
        self.accountId = accountId
        self.db = db
        self.auth = auth
    }

    func load(actionId: String) {
        guard loadedActionId != actionId else { return }
        loadedActionId = actionId
        actionObservationTask?.cancel()
        guard let pool = try? db.pool(forAccountId: accountId) else {
            resolved = true
            return
        }
        let observation = ValueObservation.tracking { db in
            // NEW-2: a saved prompt (the `actions` shape is served empty).
            try PromptEntity.fetchOne(db, key: actionId)
        }
        actionObservationTask = Task { [weak self] in
            do {
                for try await row in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    guard self.loadedActionId == actionId else { return }
                    self.action = row.map { ActionDto(prompt: $0) }
                    self.resolved = true
                    if let teamId = row?.teamId, teamId != self.loadedTeamId {
                        self.loadedTeamId = teamId
                        await self.loadTeam(teamId: teamId, pool: pool)
                    }
                }
            } catch {
                guard let self, !Task.isCancelled else { return }
                self.resolved = true
            }
        }
    }

    /// The owner gate hangs off the action's team.
    private func loadTeam(teamId: String, pool: DatabasePool) async {
        let team = (try? await pool.read { db in try TeamEntity.fetchOne(db, key: teamId) }) ?? nil
        permissions = TeamPermissions.resolve(
            team: team,
            currentUserId: auth.userId,
            isAdmin: auth.isAdmin,
            dbPool: pool
        )
    }
}
