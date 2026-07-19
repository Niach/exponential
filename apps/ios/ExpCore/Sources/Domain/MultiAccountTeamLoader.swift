import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "at.exponential", category: "MultiAccountTeamLoader")

public struct ServerTeamGroup: Identifiable, Sendable {
    public let accountId: String
    public let hostname: String
    public let userEmail: String?
    public let teams: [TeamEntity]

    public init(accountId: String, hostname: String, userEmail: String?, teams: [TeamEntity]) {
        self.accountId = accountId
        self.hostname = hostname
        self.userEmail = userEmail
        self.teams = teams
    }

    public var id: String { accountId }
}

@Observable
public final class MultiAccountTeamLoader: @unchecked Sendable {
    private let auth: AuthRepository
    private let db: DatabaseManager

    private var observationTasks: [String: Task<Void, Never>] = [:]
    private var teamsByAccount: [String: [TeamEntity]] = [:]

    public var groups: [ServerTeamGroup] {
        let activeId = auth.activeAccountId
        let signedIn = auth.accounts.filter { $0.token != nil }
        let sorted = signedIn.sorted { a, b in
            if a.id == activeId { return true }
            if b.id == activeId { return false }
            return a.lastUsedAt > b.lastUsedAt
        }
        return sorted.compactMap { account in
            let wsList = teamsByAccount[account.id] ?? []
            guard !wsList.isEmpty else { return nil }
            return ServerTeamGroup(
                accountId: account.id,
                hostname: account.displayName,
                userEmail: account.userEmail,
                teams: wsList.sorted { $0.name.lowercased() < $1.name.lowercased() }
            )
        }
    }

    public init(auth: AuthRepository, db: DatabaseManager) {
        self.auth = auth
        self.db = db
        refresh()
    }

    public func refresh() {
        let desired = Set(auth.accounts.filter { $0.token != nil }.map { $0.id })

        for accountId in Array(observationTasks.keys) where !desired.contains(accountId) {
            observationTasks[accountId]?.cancel()
            observationTasks[accountId] = nil
            teamsByAccount[accountId] = nil
        }

        let allKnown = Set(auth.accounts.map { $0.id })
        for cachedId in Array(teamsByAccount.keys) where !allKnown.contains(cachedId) {
            teamsByAccount[cachedId] = nil
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
            try TeamEntity.fetchAll(db)
        }
        let task = Task { @MainActor [weak self] in
            do {
                for try await ws in observation.values(in: pool) {
                    guard let self else { return }
                    self.teamsByAccount[accountId] = ws
                }
            } catch {
                logger.error("Team stream for \(accountId, privacy: .public) ended: \(error.localizedDescription)")
            }
        }
        observationTasks[accountId] = task
    }

    deinit {
        for (_, task) in observationTasks { task.cancel() }
    }
}
