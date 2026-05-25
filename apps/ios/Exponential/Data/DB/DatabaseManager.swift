import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "DatabaseManager")

/// Multi-account database manager. Holds one open `DatabasePool` per signed-in
/// account (keyed by accountId). There is no global "active" pool — callers
/// must always pass an accountId so writes land in the right per-server file.
final class DatabaseManager: @unchecked Sendable {
    private let lock = NSLock()
    private var pools: [String: DatabasePool] = [:]
    // Transitional: the most-recently-used account's id, set every time
    // pool(forAccountId:) is called. Lets the legacy `dbPool` getter and
    // `open(accountId:)`/`close()` APIs keep working for callers that haven't
    // yet been migrated to the multi-account API. Removed once Phase B/C are
    // complete and every caller passes an accountId.
    private var lastUsedAccountId: String?

    /// **Transitional** getter — returns the pool for the most-recently-used
    /// account. Crashes if no pool has been opened yet. All new code should
    /// use `pool(forAccountId:)` instead so writes can never land in the
    /// wrong per-server file.
    var dbPool: DatabasePool {
        lock.withLock {
            guard let id = lastUsedAccountId, let pool = pools[id] else {
                fatalError("DatabaseManager.dbPool accessed before any account pool was opened")
            }
            return pool
        }
    }

    /// **Transitional** wrapper around `pool(forAccountId:)`. Marks the given
    /// account as the most-recently-used so the legacy `dbPool` getter
    /// resolves to its pool.
    func open(accountId: String) throws {
        _ = try pool(forAccountId: accountId)
    }

    /// **Transitional**: close every pool (used by the old single-active sign-out
    /// path). New code should call `closePool(forAccountId:)` for the specific
    /// account that signed out.
    func close() {
        closeAll()
    }

    /// Get (or open) the pool for the given account. Subsequent calls for the
    /// same accountId return the cached pool.
    @discardableResult
    func pool(forAccountId accountId: String) throws -> DatabasePool {
        lock.lock()
        defer { lock.unlock() }
        if let existing = pools[accountId] { return existing }

        // Any device that ran a pre-consolidation build has an
        // `exponential-<account>.sqlite` carrying the legacy singular-name
        // schema and v1..v8 migration history. The new schema lives in the
        // `-v2.sqlite` file, so the legacy file is unreachable forever —
        // purge it on first launch so it doesn't sit on disk eating space.
        DatabaseManager.removeLegacyFile(for: accountId)

        let path = try DatabaseManager.fileURL(for: accountId).path
        var config = Configuration()
        config.foreignKeysEnabled = true
        config.journalMode = .wal
        let pool = try DatabasePool(path: path, configuration: config)
        try DatabaseManager.runMigrations(on: pool)
        pools[accountId] = pool
        lastUsedAccountId = accountId
        logger.info("Opened DB pool for account \(accountId, privacy: .public)")
        return pool
    }

    /// Pool lookup without opening. Returns nil if not yet opened for this
    /// account.
    func poolIfOpen(forAccountId accountId: String) -> DatabasePool? {
        lock.withLock { pools[accountId] }
    }

    /// Close the pool for an account (e.g., after sign-out or removal). The
    /// underlying SQLite file stays on disk; use `deleteFiles(forAccountId:)`
    /// to also wipe it.
    func closePool(forAccountId accountId: String) {
        lock.lock()
        defer { lock.unlock() }
        pools[accountId] = nil
        if lastUsedAccountId == accountId {
            lastUsedAccountId = pools.keys.first
        }
        logger.info("Closed DB pool for account \(accountId, privacy: .public)")
    }

    /// Close every open pool. Intended for app teardown / sign-out-all.
    func closeAll() {
        lock.lock()
        defer { lock.unlock() }
        pools.removeAll()
        lastUsedAccountId = nil
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
        // Also remove any legacy pre-v2 file if it survived an upgrade.
        removeLegacyFile(for: accountId)
    }

    private static func removeLegacyFile(for accountId: String) {
        let fm = FileManager.default
        guard let parent = try? fileURL(for: accountId).deletingLastPathComponent() else { return }
        let legacyBase = "exponential-\(accountId).sqlite"
        for suffix in ["", "-wal", "-shm"] {
            let target = parent.appendingPathComponent(legacyBase + suffix)
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
        // `-v2` suffix marks the post-consolidation file naming. Bumping the
        // suffix is how we force a wipe-and-resync on every existing device when
        // the local schema is fundamentally reshaped (table renames, dropped
        // columns).
        return dbDir.appendingPathComponent("exponential-\(accountId)-v2.sqlite")
    }

    private static func runMigrations(on dbPool: DatabasePool) throws {
        var migrator = DatabaseMigrator()

        // Single canonical schema. Mirrors the Postgres tables Electric syncs to
        // mobile, with column names and nullability matching packages/db-schema.
        // SQLite type affinities are looser than Postgres — uuid/timestamp/date
        // columns are stored as text (ISO-8601 for timestamps), enums as text,
        // jsonb (issues.description, comments.body) as text.
        migrator.registerMigration("v1_initial") { db in
            try db.create(table: "electric_offsets", ifNotExists: true) { t in
                t.primaryKey("shape", .text)
                t.column("handle", .text).notNull()
                t.column("offset", .text).notNull()
            }

            try db.create(table: "workspaces", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("name", .text).notNull()
                t.column("slug", .text).notNull()
                t.column("icon_url", .text)
                t.column("is_public", .boolean).notNull().defaults(to: false)
                t.column("public_write_policy", .text).notNull().defaults(to: "members")
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "projects", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull().indexed()
                t.column("name", .text).notNull()
                t.column("slug", .text).notNull()
                t.column("prefix", .text).notNull()
                t.column("color", .text).notNull().defaults(to: "#6366f1")
                t.column("sort_order", .double).notNull().defaults(to: 0)
                t.column("archived_at", .text)
                t.column("github_repo", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "issues", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("project_id", .text).notNull().indexed()
                t.column("number", .integer).notNull().defaults(to: 0)
                t.column("identifier", .text).notNull().defaults(to: "")
                t.column("title", .text).notNull()
                t.column("description", .text)
                t.column("status", .text).notNull().defaults(to: "backlog")
                t.column("priority", .text).notNull().defaults(to: "none")
                t.column("assignee_id", .text)
                t.column("creator_id", .text).notNull()
                t.column("due_date", .text)
                t.column("due_time", .text)
                t.column("end_time", .text)
                t.column("sort_order", .double).notNull().defaults(to: 0)
                t.column("completed_at", .text)
                t.column("archived_at", .text)
                t.column("recurrence_interval", .integer)
                t.column("recurrence_unit", .text)
                t.column("google_calendar_event_id", .text)
                t.column("google_calendar_last_synced_at", .text)
                t.column("google_calendar_last_sync_error", .text)
                t.column("agent_plan_state", .text)
                t.column("agent_plan_revision", .integer).notNull().defaults(to: 0)
                t.column("agent_plan_approved_at", .text)
                t.column("agent_plan_approved_by", .text)
                t.column("agent_last_comment_seen_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "labels", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull().indexed()
                t.column("name", .text).notNull()
                t.column("color", .text).notNull().defaults(to: "#6366f1")
                t.column("sort_order", .double).notNull().defaults(to: 0)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            // Composite PK matches Postgres exactly. The shape proxy sends
            // (issue_id, label_id, workspace_id) — no synthetic surrogate `id`.
            try db.create(table: "issue_labels", ifNotExists: true) { t in
                t.column("issue_id", .text).notNull()
                t.column("label_id", .text).notNull().indexed()
                t.column("workspace_id", .text).notNull().indexed()
                t.primaryKey(["issue_id", "label_id"])
            }

            try db.create(table: "users", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("name", .text)
                t.column("email", .text).notNull()
                t.column("image", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "workspace_members", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull().indexed()
                t.column("user_id", .text).notNull().indexed()
                t.column("role", .text).notNull()
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "workspace_invites", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull().indexed()
                t.column("role", .text).notNull()
                t.column("token", .text).notNull().indexed()
                t.column("expires_at", .text).notNull()
                t.column("accepted_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "comments", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull().indexed()
                t.column("workspace_id", .text).notNull().indexed()
                t.column("author_id", .text).notNull()
                t.column("body", .text)
                t.column("kind", .text).notNull().defaults(to: "regular")
                t.column("edited_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "attachments", ifNotExists: true) { t in
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

        try migrator.migrate(dbPool)
    }

    func clearAllData(forAccountId accountId: String) throws {
        guard let pool = lock.withLock({ pools[accountId] }) else { return }
        try pool.write { db in
            try db.execute(sql: "DELETE FROM electric_offsets")
            try db.execute(sql: "DELETE FROM attachments")
            try db.execute(sql: "DELETE FROM comments")
            try db.execute(sql: "DELETE FROM issue_labels")
            try db.execute(sql: "DELETE FROM issues")
            try db.execute(sql: "DELETE FROM labels")
            try db.execute(sql: "DELETE FROM projects")
            try db.execute(sql: "DELETE FROM workspace_members")
            try db.execute(sql: "DELETE FROM workspace_invites")
            try db.execute(sql: "DELETE FROM workspaces")
            try db.execute(sql: "DELETE FROM users")
        }
    }
}
