import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "SyncManager")

// One TanStack Start instance, one shape protocol for every client.
// Web uses @electric-sql/client; iOS and Android implement the same wire
// format by hand. See packages/electric-protocol/README.md for the contract.
//
// Every shape runs an independent long-polling loop in its own Task. Each
// poll either returns immediately with new rows or holds the connection
// open ~60s before returning an `up-to-date` control message. There is
// no polling timer anywhere.
final class SyncManager: @unchecked Sendable {
    private let auth: AuthRepository
    let db: DatabaseManager
    private var shapeTasks: [Task<Void, Never>] = []
    private var observationTask: Task<Void, Never>?

    init(auth: AuthRepository, db: DatabaseManager) {
        self.auth = auth
        self.db = db
    }

    func start() {
        observationTask = Task { [weak self] in
            guard let self else { return }
            var previousUrl: String? = self.auth.instanceUrl
            var previousToken: String? = self.auth.token

            if previousUrl != nil && previousToken != nil {
                self.launchShapes()
            }

            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(500))
                let currentUrl = self.auth.instanceUrl
                let currentToken = self.auth.token

                if currentUrl != previousUrl || currentToken != previousToken {
                    logger.info("Auth state changed — restarting shape sync")
                    previousUrl = currentUrl
                    previousToken = currentToken
                    self.cancelShapes()
                    if currentUrl != nil && currentToken != nil {
                        self.launchShapes()
                    }
                }
            }
        }
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
        cancelShapes()
    }

    func signOut() async {
        cancelShapes()
        do {
            try db.clearAllData()
        } catch {
            logger.error("Failed to clear data: \(error.localizedDescription)")
        }
    }

    /// Wait up to ~5s for the workspaces shape to land its initial snapshot.
    /// Live sync runs automatically — this exists so UI loading indicators
    /// have a meaningful signal to wait on instead of returning instantly.
    func initialSync() async {
        let start = Date()
        while Date().timeIntervalSince(start) < 5 {
            let hasData = (try? await db.dbPool.read { db in
                try WorkspaceEntity.fetchCount(db) > 0
            }) ?? false
            if hasData { return }
            try? await Task.sleep(for: .milliseconds(100))
        }
    }

    // MARK: - Live shape sync

    private func cancelShapes() {
        shapeTasks.forEach { $0.cancel() }
        shapeTasks.removeAll()
    }

    private func launchShapes() {
        logger.info("Launching live shape sync (8 shapes)")
        let db = self.db
        let auth = self.auth
        let baseUrl: @Sendable () -> String? = { auth.instanceUrl }
        let token: @Sendable () -> String? = { auth.token }

        shapeTasks.append(makeShapeTask(
            name: "workspaces", path: "/api/shapes/workspaces", table: "workspace",
            type: WorkspaceEntity.self, db: db, baseUrl: baseUrl, token: token
        ))
        shapeTasks.append(makeShapeTask(
            name: "projects", path: "/api/shapes/projects", table: "project",
            type: ProjectEntity.self, db: db, baseUrl: baseUrl, token: token
        ))
        shapeTasks.append(makeShapeTask(
            name: "issues", path: "/api/shapes/issues", table: "issue",
            type: IssueEntity.self, db: db, baseUrl: baseUrl, token: token
        ))
        shapeTasks.append(makeShapeTask(
            name: "labels", path: "/api/shapes/labels", table: "label",
            type: LabelEntity.self, db: db, baseUrl: baseUrl, token: token
        ))
        shapeTasks.append(makeShapeTask(
            name: "issue-labels", path: "/api/shapes/issue-labels", table: "issue_label",
            type: IssueLabelEntity.self, db: db, baseUrl: baseUrl, token: token
        ))
        shapeTasks.append(makeShapeTask(
            name: "users", path: "/api/shapes/users", table: "user",
            type: UserEntity.self, db: db, baseUrl: baseUrl, token: token
        ))
        shapeTasks.append(makeShapeTask(
            name: "workspace-members", path: "/api/shapes/workspace-members", table: "workspace_member",
            type: WorkspaceMemberEntity.self, db: db, baseUrl: baseUrl, token: token
        ))
        shapeTasks.append(makeShapeTask(
            name: "workspace-invites", path: "/api/shapes/workspace-invites", table: "workspace_invite",
            type: WorkspaceInviteEntity.self, db: db, baseUrl: baseUrl, token: token
        ))
    }

    private func makeShapeTask<T: Codable & FetchableRecord & PersistableRecord & Sendable>(
        name: String, path: String, table: String, type: T.Type,
        db: DatabaseManager,
        baseUrl: @escaping @Sendable () -> String?,
        token: @escaping @Sendable () -> String?
    ) -> Task<Void, Never> {
        let client = ShapeClient<T>(
            shapeName: name,
            urlPath: path,
            baseUrlProvider: baseUrl,
            tokenProvider: token,
            db: db,
            onMessages: { messages in
                try await applyBatch(messages: messages, table: table, db: db)
            }
        )
        return Task {
            do {
                try await client.run()
            } catch is CancellationError {
                // Expected on auth change / stop()
            } catch {
                logger.error("[\(name)] shape task ended: \(error.localizedDescription)")
            }
        }
    }
}

// One transaction per long-poll batch — never one transaction per row.
// Per-row writes from 8 concurrent shape loops were what starved the GRDB
// writer and forced live sync off in the first place. Keep batched.
private func applyBatch<T: PersistableRecord & Sendable>(
    messages: [ShapeMessage<T>], table: String, db: DatabaseManager
) async throws {
    guard !messages.isEmpty else { return }
    try await db.dbPool.write { gdb in
        for message in messages {
            switch message {
            case let .insert(_, value):
                try value.save(gdb)
            case let .update(_, value):
                try value.save(gdb)
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

// Electric shape keys arrive as `"table"/"id"` (quoted). Strip the table
// segment and the surrounding quotes to recover the bare primary key.
private func parseIdFromKey(_ key: String) -> String? {
    let parts = key.split(separator: "/")
    guard let last = parts.last else { return nil }
    return last.trimmingCharacters(in: CharacterSet(charactersIn: "\""))
}
