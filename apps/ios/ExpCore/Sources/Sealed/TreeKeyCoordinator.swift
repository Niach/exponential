import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "at.exponential", category: "TreeKeys")

/// Drives `TreeKeyHolder` from the app's state (EXP-1459): every signed-in
/// account syncs its wraps and answers `sessionKeys.pending` on start and
/// every minute (every few seconds until the install is registered); a row
/// that can't be opened asks for its root's wraps; and once secrets land the
/// rows they open are touched in the local database, so every observation
/// reading them re-renders (the locked placeholder turns into content).
public final class TreeKeyCoordinator: @unchecked Sendable {
    private static let retryTick: Duration = .seconds(5)

    private let holder: TreeKeyHolder
    private let auth: AuthRepository
    private let db: DatabaseManager
    private let lock = NSLock()
    private var task: Task<Void, Never>?
    private var observer: NSObjectProtocol?

    public init(holder: TreeKeyHolder, opener: SealedOpener, auth: AuthRepository, db: DatabaseManager) {
        self.holder = holder
        self.auth = auth
        self.db = db
        opener.onMissingKey { [weak self] rootId in
            guard let self else { return }
            Task.detached(priority: .utility) { await self.request(rootId: rootId) }
        }
        observer = NotificationCenter.default.addObserver(forName: .sealedKeysChanged, object: opener.ring, queue: nil) { [weak self] note in
            let roots = note.userInfo?[SealedKeyRing.rootIdsKey] as? [String]
            Task.detached(priority: .utility) { self?.touch(rootIds: roots) }
        }
    }

    deinit {
        if let observer { NotificationCenter.default.removeObserver(observer) }
    }

    public func start() {
        lock.withLock {
            guard task == nil else { return }
            task = Task.detached(priority: .utility) { [weak self] in
                while !Task.isCancelled {
                    guard let self else { return }
                    var ready = true
                    for accountId in self.accountIds() {
                        let ran = await self.holder.sync(accountId: accountId)
                        if !ran { ready = false }
                    }
                    try? await Task.sleep(for: ready ? TreeKeyHolder.pendingInterval : Self.retryTick)
                }
            }
        }
    }

    public func stop() {
        lock.withLock {
            task?.cancel()
            task = nil
        }
    }

    private func accountIds() -> [String] {
        auth.accounts.filter { $0.token != nil }.map(\.id).sorted()
    }

    /// The account whose database holds `rootId` asks for its wraps.
    func request(rootId: String) async {
        for accountId in db.openAccountIds {
            guard let pool = db.poolIfOpen(forAccountId: accountId) else { continue }
            let known = try? await pool.read { db in
                try Bool.fetchOne(db, sql: #"SELECT EXISTS(SELECT 1 FROM "sessions" WHERE "id" = ?)"#, arguments: [rootId])
            }
            if known == true {
                await holder.request(accountId: accountId, rootId: rootId)
                return
            }
        }
    }

    /// Touches the rows a new secret opens (nil = every root).
    func touch(rootIds: [String]?) {
        for accountId in db.openAccountIds {
            guard let pool = db.poolIfOpen(forAccountId: accountId) else { continue }
            do {
                try pool.write { db in try SealedRowsTouch.touch(db, rootIds: rootIds) }
            } catch {
                logger.error("Re-opening sealed rows failed: \(error.localizedDescription)")
            }
        }
    }
}

/// No-op UPDATEs over the sealed rows of some roots: GRDB observations
/// tracking those tables fire again and re-open them with the new keys.
public enum SealedRowsTouch {
    public static func touch(_ db: Database, rootIds: [String]?) throws {
        if let rootIds {
            guard !rootIds.isEmpty else { return }
            let marks = Array(repeating: "?", count: rootIds.count).joined(separator: ", ")
            let args = StatementArguments(rootIds)
            try db.execute(sql: #"UPDATE "sessions" SET "root_id" = "root_id" WHERE "root_id" IN (\#(marks))"#, arguments: args)
            try db.execute(sql: #"UPDATE "messages" SET "root_id" = "root_id" WHERE "root_id" IN (\#(marks))"#, arguments: args)
            try db.execute(sql: #"""
                UPDATE "conversations" SET "session_id" = "session_id"
                WHERE "session_id" IN (SELECT "id" FROM "sessions" WHERE "root_id" IN (\#(marks)))
                """#, arguments: args)
        } else {
            try db.execute(sql: #"UPDATE "sessions" SET "root_id" = "root_id""#)
            try db.execute(sql: #"UPDATE "conversations" SET "session_id" = "session_id""#)
        }
    }
}
