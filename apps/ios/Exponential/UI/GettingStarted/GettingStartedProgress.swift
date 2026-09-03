import ExpCore
import Foundation
import GRDB

/// Live completion state of the getting-started checklist (EXP-698 r5).
///
/// Every signal but one is a synced table this client already holds, so the
/// checklist ticks itself off the moment the desktop registers, a board gains a
/// repository or the first coding session starts — no polling, no refresh
/// button. The exception is the GitHub App install, which is server-only
/// (`repositories`/`github_installation_links` are never synced): that is one
/// tRPC read on mount.
///
/// The derivation is a 1:1 port of web's `deriveEntryStates`
/// (`getting-started-model.ts`) minus the three web-only entries — same order,
/// same gating, same lock chain. Natives have no `canManageMembers` capability
/// of their own, so the two entries web gates on it (invite) and on ownership
/// (action) both gate on `isOwner` here.
@Observable
@MainActor
final class GettingStartedProgress {
    enum EntryState {
        case done
        case available
        case locked
    }

    struct Entry: Identifiable {
        let key: GettingStartedEntryKey
        let state: EntryState
        /// For locked entries: the step whose completion unlocks this one.
        let lockedBy: GettingStartedEntryKey?

        var id: String { key.rawValue }
    }

    /// The raw signals, exactly the subset of web's `GettingStartedSignals`
    /// mobile can observe.
    private struct Signals: Equatable {
        var hasDesktopDevice = false
        var hasServerDevice = false
        var githubInstalled = false
        var hasInvitedTeam = false
        var hasBoard = false
        var hasRepoBoard = false
        var hasCodingSession = false
        var hasAction = false
        /// Natives have no `canManageMembers` capability of their own, so the
        /// two entries web gates on it (invite) and on ownership (action) both
        /// gate on this — read off the viewer's own `team_members` row.
        var isOwner = false
    }

    private(set) var entries: [Entry] = []
    private(set) var done = 0
    private(set) var total = 0
    /// True until the first observation emission lands. Loading counts as
    /// incomplete-unknown: the cards render neutral rather than flashing a
    /// full checklist of "available" steps.
    private(set) var loading = true

    /// EXP-548: the checklist has no dismissal — it simply disappears once
    /// every visible entry is done.
    var complete: Bool { total > 0 && done == total }

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

        // ONE observation over the six tables: GRDB tracks every region the
        // fetch reads, so a device registering, a board gaining a repo and the
        // first action all re-emit through the same stream.
        let observation = ValueObservation.tracking { db -> Signals in
            var next = Signals()
            // OWN machines only. The devices shape also carries teammates'
            // SHARED servers, and an unknown viewer id must read as "no
            // machine of mine" rather than counting somebody else's — the
            // checklist would tick off Get-the-desktop-app for a user who
            // never installed it, and unlock the coding step behind it.
            if let userId {
                for device in try DeviceEntity.fetchAll(db) where device.userId == userId {
                    if device.kind == "desktop" { next.hasDesktopDevice = true }
                    if device.kind == "server" { next.hasServerDevice = true }
                }
            }
            let boards = try BoardEntity.fetchAll(db).filter { $0.teamId == teamId }
            next.hasBoard = !boards.isEmpty
            next.hasRepoBoard = boards.contains { $0.repositoryId != nil }
            next.hasCodingSession = try CodingSessionEntity.fetchAll(db).contains { $0.teamId == teamId }
            next.hasAction = try ActionEntity.fetchAll(db).contains { $0.teamId == teamId }
            let members = try TeamMemberEntity.fetchAll(db).filter { $0.teamId == teamId }
            let invites = try TeamInviteEntity.fetchAll(db).filter { $0.teamId == teamId }
            next.hasInvitedTeam = members.count > 1 || !invites.isEmpty
            next.isOwner = members.contains { $0.userId == userId && $0.role == "owner" }
            return next
        }

        tasks.add(Task { [weak self] in
            do {
                for try await next in observation.values(in: pool) {
                    guard let self else { return }
                    // The one-shot GitHub read below owns that flag; keep it.
                    var merged = next
                    merged.githubInstalled = self.signals.githubInstalled
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

    /// The GitHub install state is server-only — `repositories` and
    /// `github_installation_links` are never synced — so it can only ever
    /// arrive through a tRPC read. A transient failure at bind time would
    /// otherwise leave the step "available" for the whole session, so the host
    /// re-runs this when the app comes back to the foreground (and after a
    /// trip to team settings, where the install actually happens).
    func refreshGithubStatus() {
        guard !signals.githubInstalled,
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
            self.signals.githubInstalled = true
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

    // MARK: - Derivation (port of web's deriveEntryStates)

    private func recompute() {
        var next: [Entry] = []

        next.append(Entry(
            key: .desktop,
            state: signals.hasDesktopDevice ? .done : .available,
            lockedBy: nil
        ))

        next.append(Entry(
            key: .github,
            state: signals.githubInstalled ? .done : .available,
            lockedBy: nil
        ))

        if signals.isOwner {
            next.append(Entry(
                key: .invite,
                state: signals.hasInvitedTeam ? .done : .available,
                lockedBy: nil
            ))
        }

        next.append(Entry(
            key: .board,
            state: signals.hasBoard ? .done : .available,
            lockedBy: nil
        ))

        // Coding needs a repo-backed board and a machine to run on; when
        // locked, point at whichever feeder step is still missing, in display
        // order (desktop first — without any machine nothing can run; then
        // GitHub — without it the board step can't attach a repo either).
        let hasDevice = signals.hasDesktopDevice || signals.hasServerDevice
        if signals.hasCodingSession {
            next.append(Entry(key: .coding, state: .done, lockedBy: nil))
        } else if signals.hasRepoBoard && hasDevice {
            next.append(Entry(key: .coding, state: .available, lockedBy: nil))
        } else {
            next.append(Entry(
                key: .coding,
                state: .locked,
                lockedBy: !hasDevice ? .desktop : (signals.githubInstalled ? .board : .github)
            ))
        }

        // EXP-548: actions are authored by the builtin creator run, which —
        // like any coding session — needs a machine; the desktop step is the
        // feeder.
        if signals.isOwner {
            if signals.hasAction {
                next.append(Entry(key: .action, state: .done, lockedBy: nil))
            } else if hasDevice {
                next.append(Entry(key: .action, state: .available, lockedBy: nil))
            } else {
                next.append(Entry(key: .action, state: .locked, lockedBy: .desktop))
            }
        }

        next.append(Entry(
            key: .server,
            state: signals.hasServerDevice ? .done : .available,
            lockedBy: nil
        ))

        entries = next
        done = next.filter { $0.state == .done }.count
        total = next.count
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
