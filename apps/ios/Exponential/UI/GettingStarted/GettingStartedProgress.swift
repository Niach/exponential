import ExpCore
import Foundation
import GRDB

/// Live completion state of Getting started (EXP-698 r5, EXP-1270).
///
/// Every signal but one is a synced table this client already holds, so the
/// block ticks itself off the moment the desktop registers, a board appears or
/// the first run starts: no polling, no refresh button. The exception is the
/// GitHub connection, which is server-only (`repositories` is never synced and
/// installations list live off the member's GitHub account): that is one tRPC
/// read on mount.
///
/// EXP-1270 (EXP-1259 D6): the derivation is ExpCore's
/// `GettingStarted.derive` (the fixture-locked port of web's
/// `deriveGettingStarted`) on the `phone` platform: the eight steps minus the
/// two web-managed ones, the next one highlighted, no locked state.
/// `canManageMembers` = the viewer OWNS the team (web `use-team-permissions`),
/// read off their own `team_members` row.
@Observable
@MainActor
final class GettingStartedProgress {
    /// The raw signals the phones can observe (MCP + widget never show here).
    private struct Signals: Equatable {
        var model = GettingStarted.Signals()
        var isOwner = false
    }

    /// The block as the model derives it (nil until bound).
    private(set) var state: GettingStarted.State?
    /// True until the first observation emission lands: the block stays
    /// hidden rather than flashing every step as open.
    private(set) var loading = true

    var entries: [GettingStarted.Entry] { state?.entries ?? [] }
    /// The block (and its entry) disappear once every visible step is done.
    var complete: Bool { state?.complete ?? false }

    private var signals = Signals()
    private var boundKey: String?
    /// What the GitHub retry needs to re-run itself after a transient failure.
    private var boundAccountId: String?
    private var boundTeamId: String?
    private var integrationsApi: IntegrationsApi?

    /// The observation tasks. They live in a nonisolated Sendable bag rather
    /// than in main-actor state because `deinit` has to cancel them and cannot
    /// touch main-actor properties: `AppNavigator` is `.id(activeAccountId)`,
    /// so switching servers DROPS this model without anyone calling `stop()`,
    /// and an orphaned `ValueObservation` would keep streaming the old
    /// account's pool.
    private let tasks = TaskBag()

    init() {}

    deinit {
        tasks.cancelAll()
    }

    /// (Re)bind to one account+team. Idempotent for the same pair, so the view
    /// can call it from `task(id:)` on every render pass.
    func bind(accountId: String, teamId: String, deps: AppDependencies) {
        let key = "\(accountId)/\(teamId)"
        guard boundKey != key else { return }
        stop()
        boundKey = key
        loading = true
        signals = Signals()
        recompute()

        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let userId = deps.auth.userId
        boundAccountId = accountId
        boundTeamId = teamId
        integrationsApi = deps.integrationsApi

        // ONE observation over the five tables: GRDB tracks every region the
        // fetch reads, so a device registering, the first board and the first
        // run all re-emit through the same stream.
        let observation = ValueObservation.tracking { db -> Signals in
            var next = Signals()
            // OWN machines only. The devices shape also carries teammates'
            // SHARED servers, and an unknown viewer id must read as "no
            // machine of mine" rather than counting somebody else's.
            if let userId {
                for device in try DeviceEntity.fetchAll(db) where device.userId == userId {
                    if device.kind == "desktop" { next.model.hasDesktopDevice = true }
                    if device.kind == "server" { next.model.hasServerDevice = true }
                }
            }
            next.model.hasBoard = try BoardEntity.fetchAll(db).contains { $0.teamId == teamId }
            next.model.hasCodingSession = try CodingSessionEntity.fetchAll(db).contains { $0.teamId == teamId }
            let members = try TeamMemberEntity.fetchAll(db).filter { $0.teamId == teamId }
            let invites = try TeamInviteEntity.fetchAll(db).filter { $0.teamId == teamId }
            next.model.hasInvitedTeam = members.count > 1 || !invites.isEmpty
            next.isOwner = members.contains { $0.userId == userId && $0.role == "owner" }
            return next
        }

        tasks.add(Task { [weak self] in
            do {
                for try await next in observation.values(in: pool) {
                    guard let self else { return }
                    // The one-shot GitHub read below owns that flag; keep it.
                    var merged = next
                    merged.model.githubInstalled = self.signals.model.githubInstalled
                    self.signals = merged
                    self.loading = false
                    self.recompute()
                }
            } catch {
                // Observation ended (pool closed on sign-out) — keep the last
                // snapshot; `bind` restarts it on the next account/team.
            }
        })

        readGithubStatus(accountId: accountId, teamId: teamId, api: deps.integrationsApi)
    }

    /// The GitHub install state is server-only — `repositories` is never
    /// synced and installations list live off GitHub — so it can only ever
    /// arrive through a tRPC read. A transient failure at bind time would
    /// otherwise leave the step "available" for the whole session, so the host
    /// re-runs this when the app comes back to the foreground (and after a
    /// trip to team settings, where the install actually happens).
    func refreshGithubStatus() {
        guard !signals.model.githubInstalled,
              let accountId = boundAccountId,
              let teamId = boundTeamId,
              let api = integrationsApi
        else { return }
        readGithubStatus(accountId: accountId, teamId: teamId, api: api)
    }

    private func readGithubStatus(accountId: String, teamId: String, api: IntegrationsApi) {
        tasks.add(Task { [weak self] in
            let status = try? await api.githubStatus(accountId: accountId, teamId: teamId)
            guard let self, let status, status.installed else { return }
            self.signals.model.githubInstalled = true
            self.recompute()
        })
    }

    func stop() {
        tasks.cancelAll()
        boundKey = nil
        boundAccountId = nil
        boundTeamId = nil
        integrationsApi = nil
    }

    // MARK: - Derivation

    private func recompute() {
        state = GettingStarted.derive(
            signals.model,
            permissions: GettingStarted.Permissions(
                canManageMembers: signals.isOwner,
                // The widget step never shows on a phone.
                canManageWidgets: signals.isOwner
            ),
            platform: .phone
        )
    }
}

/// A nonisolated, lock-guarded bag of cancellable tasks.
///
/// `GettingStartedProgress` is `@MainActor`, so its own stored properties are
/// unreachable from the nonisolated `deinit` that has to cancel them. The bag
/// is the smallest thing that closes that gap: `Sendable`, no actor hop, and
/// cancelling twice (an explicit `stop()` followed by deallocation) is free.
private final class TaskBag: @unchecked Sendable {
    private let lock = NSLock()
    private var tasks: [Task<Void, Never>] = []

    func add(_ task: Task<Void, Never>) {
        lock.lock()
        defer { lock.unlock() }
        tasks.append(task)
    }

    func cancelAll() {
        lock.lock()
        let drained = tasks
        tasks = []
        lock.unlock()
        for task in drained { task.cancel() }
    }
}
