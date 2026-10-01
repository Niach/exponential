import ExpCore
import Foundation
import GRDB

/// Backs the action page (SLOP-2): ONE action live off the synced store — its
/// metadata and its `triggers` ride the actions shape — plus every run it
/// ever started (`coding_sessions.action_id`), the devices its triggers are
/// bound to and the caller's permissions on its team.
///
/// The only trigger write is `actions.update({ id, triggers })`, a WHOLE-ARRAY
/// replace (owner-only server-side): add, edit, toggle and delete all send the
/// action's readable triggers with one element changed. Success needs no
/// local write — Electric echoes the row; until it does, the mutation's own
/// answer is the base of the next write (`TriggerWriteBase`).
@MainActor @Observable
final class ActionDetailViewModel {

    /// nil until the row resolves — and again once it is gone.
    var action: ActionDto?
    /// The first read came back: `action == nil` now means "not here", not
    /// "still loading".
    var resolved = false
    /// Every run of this action, newest first — person-started and triggered
    /// alike (a triggered one carries `started_reason` schedule | event).
    var runs: [CodingSessionEntity] = []
    /// EVERY synced device, offline included: a trigger's bound deviceId
    /// resolves to a label + online dot here, and an offline machine must
    /// still be nameable AND pickable (its missed schedule fires once it
    /// comes back).
    var allDevices: [SteerDevice] = []
    /// Trigger writes are owner-gated server-side — mirror it instead of
    /// bouncing on submit.
    var permissions: TeamPermissions = .denied
    /// The trigger with an in-flight write (`newTriggerKey` for an add) —
    /// one write at a time, since each replaces the whole array.
    var busyTriggerId: String?
    /// The server's refusal of the last trigger write, in its own words.
    var triggerError: String?

    /// What the last trigger write returned — the busy flag clears when tRPC
    /// answers, BEFORE the row echoes, so a write in that gap must build on
    /// this, not on the synced row's pre-write array.
    private var writtenAction: ActionDto?

    /// The action's triggers as of the last write this page made — what the
    /// tab lists and what every write is built from.
    var triggers: [ActionTrigger] {
        guard let action else { return [] }
        return TriggerWriteBase.triggers(synced: action, written: writtenAction)
    }

    static let newTriggerKey = "new"

    private let accountId: String
    private let db: DatabaseManager
    private let actionsApi: ActionsApi
    private let auth: AuthRepository

    private var loadedActionId: String?
    private var loadedTeamId: String?
    private var actionObservationTask: Task<Void, Never>?
    private var runsObservationTask: Task<Void, Never>?

    init(
        accountId: String,
        db: DatabaseManager,
        actionsApi: ActionsApi,
        auth: AuthRepository
    ) {
        self.accountId = accountId
        self.db = db
        self.actionsApi = actionsApi
        self.auth = auth
    }

    func load(actionId: String) {
        guard loadedActionId != actionId else { return }
        loadedActionId = actionId
        writtenAction = nil
        actionObservationTask?.cancel()
        runsObservationTask?.cancel()
        guard let pool = try? db.pool(forAccountId: accountId) else {
            resolved = true
            return
        }
        observeAction(actionId: actionId, pool: pool)
        observeRuns(actionId: actionId, pool: pool)
    }

    private func observeAction(actionId: String, pool: DatabasePool) {
        let observation = ValueObservation.tracking { db in
            try ActionEntity.fetchOne(db, key: actionId)
        }
        actionObservationTask = Task { [weak self] in
            do {
                for try await row in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    guard self.loadedActionId == actionId else { return }
                    self.action = row.map { ActionDto(entity: $0) }
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

    /// The owner gate and the device labels both hang off the action's team.
    private func loadTeam(teamId: String, pool: DatabasePool) async {
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
    }

    private func observeRuns(actionId: String, pool: DatabasePool) {
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity.filter(Column("action_id") == actionId).fetchAll(db)
        }
        runsObservationTask = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    guard self.loadedActionId == actionId else { return }
                    self.runs = rows.sorted { $0.startedAt > $1.startedAt }
                }
            } catch {
                // Non-fatal — the list simply stays as it was.
            }
        }
    }

    // MARK: - Trigger writes

    /// Flip one trigger's paused flag.
    func setEnabled(_ trigger: ActionTrigger, enabled: Bool) {
        write(triggers.settingEnabled(id: trigger.id, enabled), busy: trigger.id)
    }

    /// Owner-only delete. The element leaves via Electric.
    func delete(_ trigger: ActionTrigger) {
        write(triggers.removing(id: trigger.id), busy: trigger.id)
    }

    /// Add (`editing` nil) or change a trigger from the form sheet. The sheet
    /// dismisses on submit; a refusal surfaces as `triggerError` on the tab.
    func save(_ input: ActionTriggerInput, editing: ActionTrigger?) {
        if let editing {
            write(triggers.replacing(id: editing.id, with: input), busy: editing.id)
        } else {
            write(triggers.adding(input), busy: Self.newTriggerKey)
        }
    }

    private func write(_ triggers: [ActionTriggerInput], busy key: String) {
        guard let action, busyTriggerId == nil else { return }
        busyTriggerId = key
        triggerError = nil
        Task {
            do {
                writtenAction = try await actionsApi.update(
                    accountId: accountId,
                    id: action.id,
                    patch: ActionPatch(triggers: triggers)
                )
            } catch {
                triggerError = error.userFacingMessage
            }
            busyTriggerId = nil
        }
    }
}
