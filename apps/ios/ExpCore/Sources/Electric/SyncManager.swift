import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "SyncManager")

// One TanStack Start instance, one shape protocol for every client.
// Web uses @electric-sql/client; iOS and Android implement the same wire
// format by hand. See packages/electric-protocol/README.md for the contract.
//
// Multi-account: each signed-in account runs its own set of 10 shape Tasks in
// parallel, each writing to that account's per-account SQLite pool. There is
// no global active account here — sign-out on one account just cancels its
// pipeline without affecting any others.
public final class SyncManager: @unchecked Sendable {
    private let auth: AuthRepository
    public let db: DatabaseManager

    private let lock = NSLock()
    private var pipelines: [String: [Task<Void, Never>]] = [:]
    private var observationTask: Task<Void, Never>?

    public init(auth: AuthRepository, db: DatabaseManager) {
        self.auth = auth
        self.db = db
    }

    public func start() {
        observationTask = Task { [weak self] in
            guard let self else { return }
            // Snapshot of the signed-in accountIds we've launched pipelines for.
            var running: Set<String> = []

            // Spin once immediately so the active account's shapes start before
            // the first poll tick (matches the previous launch-on-start behavior).
            self.reconcile(running: &running)

            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(500))
                self.reconcile(running: &running)
            }
        }
    }

    public func stop() {
        observationTask?.cancel()
        observationTask = nil
        cancelAll()
    }

    public func signOut(accountId: String) async {
        // With per-account DBs and per-account pipelines, signing out just
        // cancels that one account's shape tasks. The local cache stays so the
        // user can resume offline browsing if they sign back in. Full deletion
        // happens via DatabaseManager.deleteFiles(forAccountId:) from Settings.
        let tasks = lock.withLock { pipelines.removeValue(forKey: accountId) ?? [] }
        for task in tasks { task.cancel() }
    }

    /// Wait up to ~5s for the active account's workspaces shape to land its
    /// initial snapshot. Live sync runs automatically — this exists so UI
    /// loading indicators have a meaningful signal to wait on. Resolves the
    /// active account's pool directly via `db.pool(forAccountId:)`.
    public func initialSync() async {
        guard let activeId = auth.activeAccountId,
              let pool = try? db.pool(forAccountId: activeId) else { return }
        let start = Date()
        while Date().timeIntervalSince(start) < 5 {
            let hasData = (try? await pool.read { db in
                try WorkspaceEntity.fetchCount(db) > 0
            }) ?? false
            if hasData { return }
            try? await Task.sleep(for: .milliseconds(100))
        }
    }

    // MARK: - Reconciliation

    private func reconcile(running: inout Set<String>) {
        let signedIn = Set(auth.accounts.filter { $0.token != nil }.map { $0.id })

        // Cancel pipelines for accounts no longer signed in.
        for accountId in running.subtracting(signedIn) {
            cancelPipeline(accountId: accountId)
            running.remove(accountId)
        }

        // Launch pipelines for newly signed-in accounts.
        for accountId in signedIn.subtracting(running) {
            do {
                let pool = try db.pool(forAccountId: accountId)
                launchPipeline(accountId: accountId, pool: pool)
                running.insert(accountId)
            } catch {
                logger.error(
                    "Failed to open DB pool for account \(accountId, privacy: .public): \(error.localizedDescription)"
                )
            }
        }
    }

    private func cancelAll() {
        let snapshot = lock.withLock { () -> [String: [Task<Void, Never>]] in
            let copy = pipelines
            pipelines.removeAll()
            return copy
        }
        for (_, tasks) in snapshot { tasks.forEach { $0.cancel() } }
    }

    private func cancelPipeline(accountId: String) {
        let tasks = lock.withLock { pipelines.removeValue(forKey: accountId) ?? [] }
        for task in tasks { task.cancel() }
        logger.info("Cancelled shape pipeline for account \(accountId, privacy: .public)")
    }

    // MARK: - Per-account shape launch

    private func launchPipeline(accountId: String, pool: DatabasePool) {
        logger.info("Launching live shape sync (10 shapes) for account \(accountId, privacy: .public)")

        let auth = self.auth
        // Per-account credential providers: read the specific account's URL +
        // token from AuthRepository at call time, so a token refresh or a
        // sign-out picked up by the next poll naturally flows through.
        let baseUrl: @Sendable () -> String? = {
            auth.accounts.first { $0.id == accountId }?.instanceUrl
        }
        let token: @Sendable () -> String? = {
            auth.accounts.first { $0.id == accountId }?.token
        }

        var tasks: [Task<Void, Never>] = []
        tasks.append(makeShapeTask(
            name: "workspaces", path: "/api/shapes/workspaces", table: "workspaces",
            type: WorkspaceEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "projects", path: "/api/shapes/projects", table: "projects",
            type: ProjectEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "issues", path: "/api/shapes/issues", table: "issues",
            type: IssueEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "labels", path: "/api/shapes/labels", table: "labels",
            type: LabelEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "issue-labels", path: "/api/shapes/issue-labels", table: "issue_labels",
            type: IssueLabelEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "users", path: "/api/shapes/users", table: "users",
            type: UserEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "workspace-members", path: "/api/shapes/workspace-members", table: "workspace_members",
            type: WorkspaceMemberEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "workspace-invites", path: "/api/shapes/workspace-invites", table: "workspace_invites",
            type: WorkspaceInviteEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "comments", path: "/api/shapes/comments", table: "comments",
            type: CommentEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "attachments", path: "/api/shapes/attachments", table: "attachments",
            type: AttachmentEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))

        lock.withLock { pipelines[accountId] = tasks }
    }

    private func makeShapeTask<T: Codable & FetchableRecord & PersistableRecord & Sendable>(
        name: String, path: String, table: String, type: T.Type,
        pool: DatabasePool,
        baseUrl: @escaping @Sendable () -> String?,
        token: @escaping @Sendable () -> String?
    ) -> Task<Void, Never> {
        let client = ShapeClient<T>(
            shapeName: name,
            urlPath: path,
            baseUrlProvider: baseUrl,
            tokenProvider: token,
            pool: pool,
            onMessages: { messages in
                try await applyBatch(messages: messages, table: table, pool: pool)
            }
        )
        return Task {
            do {
                try await client.run()
            } catch is CancellationError {
                // Expected on sign-out / stop()
            } catch {
                logger.error("[\(name)] shape task ended: \(error.localizedDescription)")
            }
        }
    }
}

// One transaction per long-poll batch — never one transaction per row.
// Per-row writes from the concurrent shape loops were what starved the GRDB
// writer and forced live sync off in the first place. Keep batched.
private func applyBatch<T: PersistableRecord & Sendable>(
    messages: [ShapeMessage<T>], table: String, pool: DatabasePool
) async throws {
    guard !messages.isEmpty else { return }
    try await pool.write { gdb in
        for message in messages {
            switch message {
            case let .insert(_, value):
                try value.save(gdb)
            case let .update(_, value):
                try value.save(gdb)
            case let .partialUpdate(key, columnData):
                try applyPartialUpdate(key: key, columnData: columnData, table: table, db: gdb)
            case let .delete(key, value):
                if let value {
                    try value.delete(gdb)
                } else if let id = parseIdFromKey(key) {
                    try gdb.execute(sql: "DELETE FROM \(table) WHERE id = ?", arguments: [id])
                }
            case .upToDate:
                break
            case .mustRefetch:
                try gdb.execute(sql: "DELETE FROM \(table)")
            }
        }
    }
}

private func applyPartialUpdate(key: String, columnData: Data, table: String, db: Database) throws {
    guard let id = parseIdFromKey(key) else { return }
    guard let columns = try? JSONSerialization.jsonObject(with: columnData) as? [String: Any] else { return }
    let filtered = columns.filter { $0.key != "id" }
    guard !filtered.isEmpty else { return }

    let setClauses = filtered.keys.sorted().map { "\"\($0)\" = :\($0)" }
    let sql = "UPDATE \"\(table)\" SET \(setClauses.joined(separator: ", ")) WHERE \"id\" = :_pk_id"

    var args: [String: (any DatabaseValueConvertible)?] = ["_pk_id": id]
    for (col, val) in filtered {
        args[col] = sqlValue(from: val)
    }
    try db.execute(sql: sql, arguments: StatementArguments(args))
}

private func sqlValue(from value: Any) -> (any DatabaseValueConvertible)? {
    switch value {
    case let s as String: return s
    case let i as Int: return i
    case let d as Double: return d
    case let b as Bool: return b
    case is NSNull: return nil
    default:
        if let data = try? JSONSerialization.data(withJSONObject: value),
           let str = String(data: data, encoding: .utf8) {
            return str
        }
        return nil
    }
}

// Electric shape keys arrive as `"table"/"id"` (quoted). Strip the table
// segment and the surrounding quotes to recover the bare primary key.
private func parseIdFromKey(_ key: String) -> String? {
    let parts = key.split(separator: "/")
    guard let last = parts.last else { return nil }
    return last.trimmingCharacters(in: CharacterSet(charactersIn: "\""))
}
