import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "MultiAccountProjectLoader")

/// One workspace's block inside the cross-server Home tree.
public struct WorkspaceBlock: Identifiable, Sendable {
    public let workspace: WorkspaceEntity
    public let projects: [ProjectEntity]

    public init(workspace: WorkspaceEntity, projects: [ProjectEntity]) {
        self.workspace = workspace
        self.projects = projects
    }

    public var id: String { workspace.id }
}

/// One server's block inside the cross-server Home tree: every workspace the
/// signed-in user is a member of, with its non-archived projects.
public struct ServerProjectGroup: Identifiable, Sendable {
    public let accountId: String
    public let hostname: String
    public let userEmail: String?
    public let workspaceBlocks: [WorkspaceBlock]

    public init(accountId: String, hostname: String, userEmail: String?, workspaceBlocks: [WorkspaceBlock]) {
        self.accountId = accountId
        self.hostname = hostname
        self.userEmail = userEmail
        self.workspaceBlocks = workspaceBlocks
    }

    public var id: String { accountId }
}

/// Observes every signed-in account's `workspaces` + `projects` tables and
/// merges them into a single `[ServerProjectGroup]` for the Home screen tree.
/// Each account's pool is opened once and shared with SyncManager via
/// `DatabaseManager.pool(forAccountId:)`, so writes from the parallel sync
/// pipelines fire ValueObservation callbacks here immediately.
@Observable
public final class MultiAccountProjectLoader: @unchecked Sendable {
    private let auth: AuthRepository
    private let db: DatabaseManager

    private var observationTasks: [String: [Task<Void, Never>]] = [:]
    private var workspacesByAccount: [String: [WorkspaceEntity]] = [:]
    private var projectsByAccount: [String: [ProjectEntity]] = [:]

    public var groups: [ServerProjectGroup] {
        let activeId = auth.activeAccountId
        let signedIn = auth.accounts.filter { $0.token != nil }
        let sorted = signedIn.sorted { a, b in
            if a.id == activeId { return true }
            if b.id == activeId { return false }
            return a.lastUsedAt > b.lastUsedAt
        }
        return sorted.compactMap { account in
            let workspaces = (workspacesByAccount[account.id] ?? [])
                .sorted { $0.name.lowercased() < $1.name.lowercased() }
            let projects = projectsByAccount[account.id] ?? []
            let projectsByWorkspace = Dictionary(grouping: projects) { $0.workspaceId }

            let blocks: [WorkspaceBlock] = workspaces.compactMap { ws in
                let wsProjects = (projectsByWorkspace[ws.id] ?? [])
                    .filter { $0.archivedAt == nil }
                    .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
                guard !wsProjects.isEmpty else { return nil }
                return WorkspaceBlock(workspace: ws, projects: wsProjects)
            }

            guard !blocks.isEmpty else { return nil }
            return ServerProjectGroup(
                accountId: account.id,
                hostname: account.displayName,
                userEmail: account.userEmail,
                workspaceBlocks: blocks
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
            workspacesByAccount[accountId] = nil
            projectsByAccount[accountId] = nil
        }

        // Drop cached entries for accounts that disappeared entirely.
        let allKnown = Set(auth.accounts.map { $0.id })
        for cachedId in Array(workspacesByAccount.keys) where !allKnown.contains(cachedId) {
            workspacesByAccount[cachedId] = nil
            projectsByAccount[cachedId] = nil
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
            try WorkspaceEntity.fetchAll(db)
        }
        let projObs = ValueObservation.tracking { db in
            try ProjectEntity.fetchAll(db)
        }

        let wsTask = Task { @MainActor [weak self] in
            do {
                for try await ws in wsObs.values(in: pool) {
                    guard let self else { return }
                    self.workspacesByAccount[accountId] = ws
                    self.writeMirror()
                }
            } catch {
                logger.error("Workspace stream for \(accountId, privacy: .public) ended: \(error.localizedDescription)")
            }
        }
        let projTask = Task { @MainActor [weak self] in
            do {
                for try await projects in projObs.values(in: pool) {
                    guard let self else { return }
                    self.projectsByAccount[accountId] = projects
                    self.writeMirror()
                }
            } catch {
                logger.error("Project stream for \(accountId, privacy: .public) ended: \(error.localizedDescription)")
            }
        }
        observationTasks[accountId] = [wsTask, projTask]
    }

    /// Mirror every signed-in account's non-archived projects into the shared
    /// app-group container so the Share Extension can populate its picker
    /// without opening the (per-account, non-shared) GRDB database.
    @MainActor
    private func writeMirror() {
        let signedIn = auth.accounts.filter { $0.token != nil }
        var out: [MirroredProject] = []
        for account in signedIn {
            let workspacesById = Dictionary(
                uniqueKeysWithValues: (workspacesByAccount[account.id] ?? []).map { ($0.id, $0) }
            )
            for project in (projectsByAccount[account.id] ?? []) where project.archivedAt == nil {
                guard let workspace = workspacesById[project.workspaceId] else { continue }
                out.append(MirroredProject(
                    accountId: account.id,
                    accountName: account.displayName,
                    workspaceId: workspace.id,
                    workspaceName: workspace.name,
                    projectId: project.id,
                    projectName: project.name,
                    prefix: project.prefix
                ))
            }
        }
        SharedProjectMirror.write(projects: out)
    }

    deinit {
        for (_, tasks) in observationTasks { tasks.forEach { $0.cancel() } }
    }
}
