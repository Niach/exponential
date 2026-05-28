import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "MultiAccountWorkspaceLoader")

struct ServerWorkspaceGroup: Identifiable, Sendable {
    let accountId: String
    let hostname: String
    let userEmail: String?
    let workspaces: [WorkspaceEntity]

    var id: String { accountId }
}

@Observable
final class MultiAccountWorkspaceLoader: @unchecked Sendable {
    private let auth: AuthRepository
    private let db: DatabaseManager

    private var observationTasks: [String: Task<Void, Never>] = [:]
    private var workspacesByAccount: [String: [WorkspaceEntity]] = [:]

    var groups: [ServerWorkspaceGroup] {
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
                hostname: account.displayName,
                userEmail: account.userEmail,
                workspaces: wsList.sorted { $0.name.lowercased() < $1.name.lowercased() }
            )
        }
    }

    init(auth: AuthRepository, db: DatabaseManager) {
        self.auth = auth
        self.db = db
        refresh()
    }

    func refresh() {
        let desired = Set(auth.accounts.filter { $0.token != nil }.map { $0.id })

        for accountId in Array(observationTasks.keys) where !desired.contains(accountId) {
            observationTasks[accountId]?.cancel()
            observationTasks[accountId] = nil
            workspacesByAccount[accountId] = nil
        }

        let allKnown = Set(auth.accounts.map { $0.id })
        for cachedId in Array(workspacesByAccount.keys) where !allKnown.contains(cachedId) {
            workspacesByAccount[cachedId] = nil
        }

        for accountId in desired where observationTasks[accountId] == nil {
            startObserving(accountId: accountId)
        }
    }

    private func startObserving(accountId: String) {
        let pool: DatabasePool
        do {
            pool = try db.pool(forAccountId: accountId)
        } catch {
            logger.error("Failed to open pool for \(accountId, privacy: .public): \(error.localizedDescription)")
            return
        }

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
                logger.error("Workspace stream for \(accountId, privacy: .public) ended: \(error.localizedDescription)")
            }
        }
        observationTasks[accountId] = task
    }

    deinit {
        for (_, task) in observationTasks { task.cancel() }
    }
}
