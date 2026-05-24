import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "DatabaseManager")

final class DatabaseManager: @unchecked Sendable {
    // Mutated via open(accountId:) when the active server account changes.
    // Marked `var` (with a lock) so we can hot-swap the underlying file
    // without rebuilding the manager — but consumers that hold ValueObservations
    // bound to a specific pool must re-bind after a swap (see AppNavigator's
    // .id(activeAccountId) trick).
    private let lock = NSLock()
    private var currentPool: DatabasePool?
    private var currentAccountId: String?

    var dbPool: DatabasePool {
        lock.withLock {
            guard let pool = currentPool else {
                fatalError("DatabaseManager.dbPool accessed before open(accountId:) was called")
            }
            return pool
        }
    }

    var isOpen: Bool {
        lock.withLock { currentPool != nil }
    }

    /// Open (or switch to) the DB file for the given account. Closes the previous pool first.
    func open(accountId: String) throws {
        lock.lock()
        defer { lock.unlock() }
        if currentAccountId == accountId, currentPool != nil { return }

        currentPool = nil // releases the previous pool's connections
        let path = try DatabaseManager.fileURL(for: accountId).path
        var config = Configuration()
        config.foreignKeysEnabled = true
        config.journalMode = .wal
        let pool = try DatabasePool(path: path, configuration: config)
        try DatabaseManager.runMigrations(on: pool)
        currentPool = pool
        currentAccountId = accountId
        logger.info("Opened DB for account \(accountId, privacy: .public)")
    }

    /// Close the current pool without opening a new one (e.g., when no account is active).
    func close() {
        lock.lock()
        defer { lock.unlock() }
        currentPool = nil
        currentAccountId = nil
    }

    /// Delete the underlying SQLite files for the given account.
    static func deleteFiles(forAccountId accountId: String) {
        let fm = FileManager.default
        guard let url = try? fileURL(for: accountId) else { return }
        let parent = url.deletingLastPathComponent()
        let base = url.lastPathComponent
        // Wipe the main file plus -wal / -shm side files.
        for suffix in ["", "-wal", "-shm"] {
            let target = parent.appendingPathComponent(base + suffix)
            try? fm.removeItem(at: target)
        }
    }

    static func fileURL(for accountId: String) throws -> URL {
        let fm = FileManager.default
        let appSupportDir = try fm.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let dbDir = appSupportDir.appendingPathComponent("Exponential", isDirectory: true)
        try fm.createDirectory(at: dbDir, withIntermediateDirectories: true)
        return dbDir.appendingPathComponent("exponential-\(accountId).sqlite")
    }

    private static func runMigrations(on dbPool: DatabasePool) throws {
        var migrator = DatabaseMigrator()

        migrator.registerMigration("v1_initial") { db in
            try db.create(table: "electric_offset", ifNotExists: true) { t in
                t.primaryKey("shape", .text)
                t.column("handle", .text).notNull()
                t.column("offset", .text).notNull()
            }

            try db.create(table: "workspace", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("name", .text).notNull()
                t.column("slug", .text).notNull()
                t.column("icon_url", .text)
                t.column("is_public", .boolean).notNull().defaults(to: false)
                t.column("public_write_policy", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "project", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull()
                t.column("name", .text).notNull()
                t.column("slug", .text).notNull()
                t.column("prefix", .text).notNull()
                t.column("color", .text)
                t.column("sort_order", .double)
                t.column("archived_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "issue", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("project_id", .text).notNull()
                t.column("number", .integer)
                t.column("identifier", .text)
                t.column("title", .text).notNull()
                t.column("description", .text)
                t.column("status", .text).notNull()
                t.column("priority", .text).notNull()
                t.column("assignee_id", .text)
                t.column("creator_id", .text)
                t.column("due_date", .text)
                t.column("due_time", .text)
                t.column("end_time", .text)
                t.column("sort_order", .double)
                t.column("completed_at", .text)
                t.column("archived_at", .text)
                t.column("recurrence_interval", .integer)
                t.column("recurrence_unit", .text)
                t.column("google_calendar_event_id", .text)
                t.column("google_calendar_last_synced_at", .text)
                t.column("google_calendar_last_sync_error", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "label", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull()
                t.column("name", .text).notNull()
                t.column("color", .text).notNull()
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "issue_label", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull()
                t.column("label_id", .text).notNull()
                t.column("created_at", .text).notNull()
            }

            try db.create(table: "user", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("name", .text)
                t.column("email", .text).notNull()
                t.column("image", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "workspace_member", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull()
                t.column("user_id", .text).notNull()
                t.column("role", .text).notNull()
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "workspace_invite", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull()
                t.column("role", .text).notNull()
                t.column("token", .text).notNull()
                t.column("expires_at", .text).notNull()
                t.column("accepted_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
        }

        migrator.registerMigration("v2_public_workspace") { db in
            try db.alter(table: "workspace") { t in
                t.add(column: "is_public", .boolean).notNull().defaults(to: false)
                t.add(column: "public_write_policy", .text)
            }
        }

        migrator.registerMigration("v3_comments") { db in
            try db.create(table: "comment", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull().indexed()
                t.column("workspace_id", .text).notNull().indexed()
                t.column("author_id", .text).notNull()
                t.column("body", .text)
                t.column("edited_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
        }

        migrator.registerMigration("v4_comment_kind") { db in
            try db.alter(table: "comment") { t in
                t.add(column: "kind", .text).notNull().defaults(to: "regular")
            }
        }

        migrator.registerMigration("v5_issue_agent_plan") { db in
            try db.alter(table: "issue") { t in
                t.add(column: "agent_plan_state", .text)
                t.add(column: "agent_plan_revision", .integer).notNull().defaults(to: 0)
                t.add(column: "agent_plan_approved_at", .text)
                t.add(column: "agent_plan_approved_by", .text)
                t.add(column: "agent_last_comment_seen_at", .text)
            }
        }

        migrator.registerMigration("v6_attachments") { db in
            try db.create(table: "attachment", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull().indexed()
                t.column("issue_id", .text).notNull().indexed()
                t.column("comment_id", .text)
                t.column("uploader_id", .text).notNull()
                t.column("filename", .text).notNull()
                t.column("content_type", .text).notNull()
                t.column("size_bytes", .integer).notNull()
                t.column("storage_key", .text).notNull()
                t.column("url", .text).notNull()
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
        }

        migrator.registerMigration("v7_project_github_repo") { db in
            try db.alter(table: "project") { t in
                t.add(column: "github_repo", .text)
            }
        }

        try migrator.migrate(dbPool)
    }

    func clearAllData() throws {
        guard let pool = lock.withLock({ currentPool }) else { return }
        try pool.write { db in
            try db.execute(sql: "DELETE FROM electric_offset")
            try db.execute(sql: "DELETE FROM attachment")
            try db.execute(sql: "DELETE FROM comment")
            try db.execute(sql: "DELETE FROM issue_label")
            try db.execute(sql: "DELETE FROM issue")
            try db.execute(sql: "DELETE FROM label")
            try db.execute(sql: "DELETE FROM project")
            try db.execute(sql: "DELETE FROM workspace_member")
            try db.execute(sql: "DELETE FROM workspace_invite")
            try db.execute(sql: "DELETE FROM workspace")
            try db.execute(sql: "DELETE FROM user")
        }
    }
}
