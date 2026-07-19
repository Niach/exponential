import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "at.exponential", category: "MultiAccountBoardLoader")

/// One team's block inside the cross-server Home tree.
public struct TeamBlock: Identifiable, Sendable {
    public let team: TeamEntity
    public let boards: [BoardEntity]

    public init(team: TeamEntity, boards: [BoardEntity]) {
        self.team = team
        self.boards = boards
    }

    public var id: String { team.id }
}

/// One server's block inside the cross-server Home tree: every team the
/// signed-in user is a member of, with its non-archived boards.
public struct ServerBoardGroup: Identifiable, Sendable {
    public let accountId: String
    public let hostname: String
    public let userEmail: String?
    public let teamBlocks: [TeamBlock]

    public init(accountId: String, hostname: String, userEmail: String?, teamBlocks: [TeamBlock]) {
        self.accountId = accountId
        self.hostname = hostname
        self.userEmail = userEmail
        self.teamBlocks = teamBlocks
    }

    public var id: String { accountId }
}

/// Observes every signed-in account's `teams` + `boards` tables and
/// merges them into a single `[ServerBoardGroup]` for the Home screen tree.
/// Each account's pool is opened once and shared with SyncManager via
/// `DatabaseManager.pool(forAccountId:)`, so writes from the parallel sync
/// pipelines fire ValueObservation callbacks here immediately.
@Observable
public final class MultiAccountBoardLoader: @unchecked Sendable {
    private let auth: AuthRepository
    private let db: DatabaseManager

    private var observationTasks: [String: [Task<Void, Never>]] = [:]
    private var teamsByAccount: [String: [TeamEntity]] = [:]
    private var boardsByAccount: [String: [BoardEntity]] = [:]

    public var groups: [ServerBoardGroup] {
        let activeId = auth.activeAccountId
        let signedIn = auth.accounts.filter { $0.token != nil }
        let sorted = signedIn.sorted { a, b in
            if a.id == activeId { return true }
            if b.id == activeId { return false }
            return a.lastUsedAt > b.lastUsedAt
        }
        return sorted.compactMap { account in
            let teams = (teamsByAccount[account.id] ?? [])
                .sorted { $0.name.lowercased() < $1.name.lowercased() }
            let boards = boardsByAccount[account.id] ?? []
            let boardsByTeam = Dictionary(grouping: boards) { $0.teamId }

            let blocks: [TeamBlock] = teams.compactMap { ws in
                let wsBoards = (boardsByTeam[ws.id] ?? [])
                    .filter { $0.archivedAt == nil }
                    .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
                guard !wsBoards.isEmpty else { return nil }
                return TeamBlock(team: ws, boards: wsBoards)
            }

            guard !blocks.isEmpty else { return nil }
            return ServerBoardGroup(
                accountId: account.id,
                hostname: account.displayName,
                userEmail: account.userEmail,
                teamBlocks: blocks
            )
        }
    }

    public init(auth: AuthRepository, db: DatabaseManager) {
        self.auth = auth
        self.db = db
        refresh()
    }

    /// Reconcile observation tasks with the current accounts list. Call when
    /// `auth.accounts` may have changed (account added / signed-out / removed).
    public func refresh() {
        let desired = Set(auth.accounts.filter { $0.token != nil }.map { $0.id })

        // Cancel observations for accounts no longer signed in.
        for accountId in Array(observationTasks.keys) where !desired.contains(accountId) {
            observationTasks[accountId]?.forEach { $0.cancel() }
            observationTasks[accountId] = nil
            teamsByAccount[accountId] = nil
            boardsByAccount[accountId] = nil
        }

        // Drop cached entries for accounts that disappeared entirely.
        let allKnown = Set(auth.accounts.map { $0.id })
        for cachedId in Array(teamsByAccount.keys) where !allKnown.contains(cachedId) {
            teamsByAccount[cachedId] = nil
            boardsByAccount[cachedId] = nil
        }

        // Start observations for newly-signed-in accounts.
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

        let wsObs = ValueObservation.tracking { db in
            try TeamEntity.fetchAll(db)
        }
        let projObs = ValueObservation.tracking { db in
            try BoardEntity.fetchAll(db)
        }

        let wsTask = Task { @MainActor [weak self] in
            do {
                for try await ws in wsObs.values(in: pool) {
                    guard let self else { return }
                    self.teamsByAccount[accountId] = ws
                    self.writeMirror()
                }
            } catch {
                logger.error("Team stream for \(accountId, privacy: .public) ended: \(error.localizedDescription)")
            }
        }
        let projTask = Task { @MainActor [weak self] in
            do {
                for try await boards in projObs.values(in: pool) {
                    guard let self else { return }
                    self.boardsByAccount[accountId] = boards
                    self.writeMirror()
                }
            } catch {
                logger.error("Board stream for \(accountId, privacy: .public) ended: \(error.localizedDescription)")
            }
        }
        observationTasks[accountId] = [wsTask, projTask]
    }

    /// Mirror every signed-in account's non-archived boards into the shared
    /// app-group container so the Share Extension can populate its picker
    /// without opening the (per-account, non-shared) GRDB database.
    @MainActor
    private func writeMirror() {
        let signedIn = auth.accounts.filter { $0.token != nil }
        var out: [MirroredBoard] = []
        for account in signedIn {
            let teamsById = Dictionary(
                uniqueKeysWithValues: (teamsByAccount[account.id] ?? []).map { ($0.id, $0) }
            )
            for board in (boardsByAccount[account.id] ?? []) where board.archivedAt == nil {
                guard let team = teamsById[board.teamId] else { continue }
                out.append(MirroredBoard(
                    accountId: account.id,
                    accountName: account.displayName,
                    teamId: team.id,
                    teamName: team.name,
                    boardId: board.id,
                    boardName: board.name,
                    prefix: board.prefix
                ))
            }
        }
        SharedBoardMirror.write(boards: out)
    }

    deinit {
        for (_, tasks) in observationTasks { tasks.forEach { $0.cancel() } }
    }
}
