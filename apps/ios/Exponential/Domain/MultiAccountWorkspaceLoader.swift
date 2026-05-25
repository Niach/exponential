import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "MultiAccountWorkspaceLoader")

/// A grouping of workspaces by signed-in server account, used by the workspace
/// picker to render a unified cross-server list.
struct ServerWorkspaceGroup: Identifiable, Sendable {
    let accountId: String
    let hostname: String
    let userEmail: String?
    let workspaces: [WorkspaceEntity]

    var id: String { accountId }
}

/// Observes every signed-in account's local `workspaces` table and publishes a
/// combined `[ServerWorkspaceGroup]` for the cross-server picker. The active
/// account's workspaces are fed in by MainNavigator's existing observation
/// (avoiding a double-open of the read-write DB pool); inactive accounts are
/// observed via read-only shadow pools opened against their per-account
/// `-v2.sqlite` files.
@Observable
final class MultiAccountWorkspaceLoader: @unchecked Sendable {
    private let auth: AuthRepository

    private var shadowPools: [String: DatabasePool] = [:]
    private var observationTasks: [String: Task<Void, Never>] = [:]
    private var workspacesByAccount: [String: [WorkspaceEntity]] = [:]

    var groups: [ServerWorkspaceGroup] {
        // Order: active account first (so the picker opens with focus on the
        // current server), then remaining signed-in accounts by lastUsedAt
        // descending. Skip accounts that are signed out or have no workspaces
        // synced yet (empty groups would be visual noise).
        let activeId = auth.activeAccountId
        let signedIn = auth.accounts.filter { $0.token != nil }
        let sorted = signedIn.sorted { a, b in
            if a.id == activeId { return true }
            if b.id == activeId { return false }
            return a.lastUsedAt > b.lastUsedAt
        }
        return sorted.compactMap { account in
            let wsList = workspacesByAccount[account.id] ?? []
            guard !wsList.isEmpty else { return nil }
            return ServerWorkspaceGroup(
                accountId: account.id,
                hostname: account.displayHost,
                userEmail: account.userEmail,
                workspaces: wsList.sorted { $0.name.lowercased() < $1.name.lowercased() }
            )
        }
    }

    init(auth: AuthRepository) {
        self.auth = auth
        refresh()
    }

    /// Reconcile shadow pools with the current accounts list. Call whenever
    /// auth.accounts or auth.activeAccountId may have changed.
    func refresh() {
        let activeId = auth.activeAccountId
        let desiredShadowIds = Set(
            auth.accounts
                .filter { $0.token != nil && $0.id != activeId }
                .map { $0.id }
        )

        // Close shadow pools that are no longer wanted (account removed,
        // signed out, or became the active one).
        for accountId in Array(shadowPools.keys) where !desiredShadowIds.contains(accountId) {
            observationTasks[accountId]?.cancel()
            observationTasks[accountId] = nil
            shadowPools[accountId] = nil
            workspacesByAccount[accountId] = nil
        }

        // The active account is now sourced via setActiveAccountWorkspaces; if
        // it used to be a shadow account we already cleared its cache above.
        // Drop entries for any account no longer in the auth list at all.
        let allKnown = Set(auth.accounts.map { $0.id })
        for cachedId in Array(workspacesByAccount.keys) where !allKnown.contains(cachedId) {
            workspacesByAccount[cachedId] = nil
        }

        // Open pools for newly-eligible accounts.
        for accountId in desiredShadowIds where shadowPools[accountId] == nil {
            tryOpenShadow(for: accountId)
        }
    }

    /// MainNavigator forwards the active account's workspaces here so the
    /// combined `groups` list includes them without us opening a second
    /// `DatabasePool` against the read-write file.
    func setActiveAccountWorkspaces(_ workspaces: [WorkspaceEntity]) {
        guard let activeId = auth.activeAccountId else { return }
        workspacesByAccount[activeId] = workspaces
    }

    private func tryOpenShadow(for accountId: String) {
        do {
            let url = try DatabaseManager.fileURL(for: accountId)
            // Account just added but has never synced — skip until the file shows up.
            guard FileManager.default.fileExists(atPath: url.path) else {
                logger.info("Skipping shadow pool for \(accountId, privacy: .public): DB file not present yet")
                return
            }
            var config = Configuration()
            config.foreignKeysEnabled = false
            config.readonly = true
            let pool = try DatabasePool(path: url.path, configuration: config)
            shadowPools[accountId] = pool
            startObserving(accountId: accountId, pool: pool)
            logger.info("Opened shadow workspace pool for \(accountId, privacy: .public)")
        } catch {
            logger.error("Failed to open shadow pool for \(accountId, privacy: .public): \(error.localizedDescription)")
        }
    }

    private func startObserving(accountId: String, pool: DatabasePool) {
        let observation = ValueObservation.tracking { db in
            try WorkspaceEntity.fetchAll(db)
        }
        let task = Task { @MainActor [weak self] in
            do {
                for try await ws in observation.values(in: pool) {
                    guard let self else { return }
                    self.workspacesByAccount[accountId] = ws
                }
            } catch {
                logger.error(
                    "Shadow workspace stream for \(accountId, privacy: .public) ended: \(error.localizedDescription)"
                )
            }
        }
        observationTasks[accountId] = task
    }

    deinit {
        for (_, task) in observationTasks { task.cancel() }
    }
}
