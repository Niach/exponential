import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "at.exponential", category: "SyncManager")

// One TanStack Start instance, one shape protocol for every client.
// Web uses @electric-sql/client; iOS and Android implement the same wire
// format by hand. See packages/electric-protocol/README.md for the contract.
//
// Multi-account: each signed-in account runs its own set of 14 shape Tasks in
// parallel, each writing to that account's per-account SQLite pool. There is
// no global active account here — sign-out on one account just cancels its
// pipeline without affecting any others.
public final class SyncManager: @unchecked Sendable {
    private let auth: AuthRepository
    public let db: DatabaseManager

    private let lock = NSLock()
    private var pipelines: [String: [Task<Void, Never>]] = [:]
    private var observationTask: Task<Void, Never>?
    // Accounts with a resync in flight — a concurrent second resync would
    // relaunch the pipeline and overwrite `pipelines[accountId]`, orphaning
    // 14 uncancellable shape Tasks (duplicate long-polls racing the wipe).
    private var resyncing: Set<String> = []

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

    /// Full local resync ("Resync now"): cancel the account's pipeline, purge
    /// any URL-cached shape responses (poisoned-cache guard), wipe every synced
    /// row + saved offset, then relaunch so all 14 shapes refetch from scratch.
    public func resync(accountId: String) async {
        // Serialize per account: bail if a resync is already running so a
        // double-trigger can never launch a second pipeline over the first.
        let alreadyResyncing = lock.withLock { !resyncing.insert(accountId).inserted }
        if alreadyResyncing {
            SyncDebug.shared.log("[resync] already in flight for account, ignoring")
            return
        }
        defer { lock.withLock { _ = resyncing.remove(accountId) } }

        let tasks = lock.withLock { pipelines.removeValue(forKey: accountId) ?? [] }
        for task in tasks { task.cancel() }
        // Give any in-flight batch write a beat to drain before wiping.
        try? await Task.sleep(for: .milliseconds(250))
        URLCache.shared.removeAllCachedResponses()
        do {
            try db.clearAllData(forAccountId: accountId)
        } catch {
            logger.error("Resync: clearAllData failed for \(accountId, privacy: .public): \(error.localizedDescription)")
            SyncDebug.shared.log("[resync] clearAllData failed: \(error.localizedDescription)")
        }
        // Report instead of no-op: a missing token or a pool-open/migration
        // failure here is exactly the silent blackout §9.1 chased — surface it
        // so "Resync now" tells the user (and us) why nothing happened.
        guard auth.accounts.first(where: { $0.id == accountId })?.token != nil else {
            SyncDebug.shared.reportFatal("Resync couldn't relaunch: no auth token for this account")
            return
        }
        let pool: DatabasePool
        do {
            pool = try db.pool(forAccountId: accountId)
        } catch {
            logger.error("Resync: pool open failed for \(accountId, privacy: .public): \(error.localizedDescription)")
            SyncDebug.shared.reportFatal("Resync couldn't open the local database: \(error.localizedDescription)")
            return
        }
        SyncDebug.shared.log("[resync] wiped local data, relaunching pipeline")
        launchPipeline(accountId: accountId, pool: pool)
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
                // Pool open == running GRDB migrations. A throw here (the §9.1
                // duplicate-column blackout) previously vanished into os.Logger
                // and left the diagnostics screen showing "no shape activity".
                // Surface it as a fatal so it's never silent again.
                logger.error(
                    "Failed to open DB pool for account \(accountId, privacy: .public): \(error.localizedDescription)"
                )
                SyncDebug.shared.reportFatal("Local database open/migration failed: \(error.localizedDescription)")
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
        logger.info("Launching live shape sync (14 shapes) for account \(accountId, privacy: .public)")
        // A visible "we got past pool open + migrations and started polling"
        // marker in the diagnostics log — the positive counterpart to the
        // fatal path above (§9.1: pipeline launched must never be ambiguous).
        SyncDebug.shared.log("[pipeline] launched 14 shapes")
        SyncDebug.shared.clearFatal()

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
        tasks.append(makeShapeTask(
            name: "notifications", path: "/api/shapes/notifications", table: "notifications",
            type: NotificationEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "issue-events", path: "/api/shapes/issue-events", table: "issue_events",
            type: IssueEventEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "issue-subscribers", path: "/api/shapes/issue-subscribers", table: "issue_subscribers",
            type: IssueSubscriberEntity.self, pool: pool, baseUrl: baseUrl, token: token
        ))
        tasks.append(makeShapeTask(
            name: "coding-sessions", path: "/api/shapes/coding-sessions", table: "coding_sessions",
            type: CodingSessionEntity.self, pool: pool, baseUrl: baseUrl, token: token
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
                } else {
                    try deleteByKey(key: key, table: table, db: gdb)
                }
            case .upToDate:
                break
            case .mustRefetch:
                try gdb.execute(sql: "DELETE FROM \(table)")
            }
        }
    }
}

// Delete a row by its Electric key when the message carries no value. Handles
// composite primary keys (e.g. issue_labels has no surrogate `id`): the key
// encodes each PK value as a `/`-separated, quoted segment after the table
// segment, in PK-column order — `"public"."issue_labels"/"<issue_id>"/"<label_id>"`.
private func deleteByKey(key: String, table: String, db: Database) throws {
    let pkColumns = (try? db.primaryKey(table).columns) ?? []
    let values = parseKeyComponents(key)
    guard !pkColumns.isEmpty, values.count == pkColumns.count else { return }
    let whereClause = pkColumns.map { "\"\($0)\" = ?" }.joined(separator: " AND ")
    try db.execute(sql: "DELETE FROM \"\(table)\" WHERE \(whereClause)",
                   arguments: StatementArguments(values))
}

// Value segments of an Electric key (everything after the table segment), unquoted.
private func parseKeyComponents(_ key: String) -> [String] {
    let parts = key.split(separator: "/").map(String.init)
    guard parts.count > 1 else { return [] }
    return parts.dropFirst().map { $0.trimmingCharacters(in: CharacterSet(charactersIn: "\"")) }
}

private func applyPartialUpdate(key: String, columnData: Data, table: String, db: Database) throws {
    // Only single-`id` tables get partial updates here; composite-PK tables
    // (e.g. issue_labels) are insert/delete-only — skip rather than run a
    // `WHERE id` the table doesn't have.
    guard ((try? db.primaryKey(table).columns) ?? ["id"]) == ["id"] else { return }
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
