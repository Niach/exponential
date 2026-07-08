import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "at.exponential", category: "DatabaseManager")

/// Multi-account database manager. Holds one open `DatabasePool` per signed-in
/// account (keyed by accountId). There is no global "active" pool — callers
/// must always pass an accountId so writes land in the right per-server file.
public final class DatabaseManager: @unchecked Sendable {
    private let lock = NSLock()
    private var pools: [String: DatabasePool] = [:]

    public init() {}

    /// Get (or open) the pool for the given account. Subsequent calls for the
    /// same accountId return the cached pool.
    @discardableResult
    public func pool(forAccountId accountId: String) throws -> DatabasePool {
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
        logger.info("Opened DB pool for account \(accountId, privacy: .public)")
        return pool
    }

    /// Pool lookup without opening. Returns nil if not yet opened for this
    /// account.
    public func poolIfOpen(forAccountId accountId: String) -> DatabasePool? {
        lock.withLock { pools[accountId] }
    }

    /// Close the pool for an account (e.g., after sign-out or removal). The
    /// underlying SQLite file stays on disk; use `deleteFiles(forAccountId:)`
    /// to also wipe it.
    public func closePool(forAccountId accountId: String) {
        lock.lock()
        defer { lock.unlock() }
        pools[accountId] = nil
        logger.info("Closed DB pool for account \(accountId, privacy: .public)")
    }

    /// Close every open pool. Intended for app teardown / sign-out-all.
    public func closeAll() {
        lock.lock()
        defer { lock.unlock() }
        pools.removeAll()
    }

    /// Delete the underlying SQLite files for the given account.
    public static func deleteFiles(forAccountId accountId: String) {
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
        // Purge every superseded file-name generation: the pre-v2 singular-name
        // file, the v2 file, and the v3 file (replaced by -v4 in the hard-cut
        // greenfield reshape — dropped agent/calendar columns, added coding_sessions).
        let legacyBases = [
            "exponential-\(accountId).sqlite",
            "exponential-\(accountId)-v2.sqlite",
            "exponential-\(accountId)-v3.sqlite",
        ]
        for legacyBase in legacyBases {
            for suffix in ["", "-wal", "-shm"] {
                let target = parent.appendingPathComponent(legacyBase + suffix)
                try? fm.removeItem(at: target)
            }
        }
    }

    public static func fileURL(for accountId: String) throws -> URL {
        let fm = FileManager.default
        let appSupportDir = try fm.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let dbDir = appSupportDir.appendingPathComponent("Exponential", isDirectory: true)
        try fm.createDirectory(at: dbDir, withIntermediateDirectories: true)
        // The `-vN` suffix marks the canonical file naming. Bumping the suffix is
        // how we force a wipe-and-resync on every existing device when the local
        // schema is fundamentally reshaped (table renames, dropped columns).
        // `-v4` (greenfield reshape): dropped the agent_runs table + the stale
        // agent_plan_state / google_calendar_* columns from `issues`, added the
        // `coding_sessions` shape, `issues.duplicate_of_id`, and
        // `issue_subscribers.email` (with a nullable user_id).
        return dbDir.appendingPathComponent("exponential-\(accountId)-v4.sqlite")
    }

    static func runMigrations(on dbPool: DatabasePool) throws {
        try makeMigrator().migrate(dbPool)
    }

    /// The canonical migrator. Extracted (and `internal`, not `private`) so the
    /// migration test suite can build fixture DBs at each historical schema
    /// version (v1-only, v1+v2, …) and prove a full `migrate` still runs green.
    static func makeMigrator() -> DatabaseMigrator {
        var migrator = DatabaseMigrator()

        // Single canonical schema (collapsed into one migration — the `-v4` file
        // suffix forces a clean wipe-and-resync, so there's no upgrade path to
        // preserve). Mirrors the Postgres tables Electric syncs to mobile, with
        // column names and nullability matching packages/db-schema. SQLite type
        // affinities are looser than Postgres — uuid/timestamp/date columns are
        // stored as text (ISO-8601 for timestamps), enums as text, jsonb
        // (issues.description, comments.body) as text.
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
                // is_public / public_write_policy were dropped when public
                // boards moved to a per-project `type`; the shape no longer
                // carries them.
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
                // Repos move to a server-only registry in a later phase; the
                // column stays so the (now-inert) repo-picker UI still compiles.
                // Electric no longer populates it.
                t.column("github_repo", .text)
                // The repo backing this project (Electric ride-along on the
                // projects shape). Nullable — only `dev` projects require a repo.
                t.column("repository_id", .text)
                // Board type: dev | tasks | feedback. Drives type icons + the
                // repo-required gate. Defaults to `dev` (matches the server).
                t.column("type", .text).notNull().defaults(to: "dev")
                // Anonymous-visitor visibility toggles (feedback boards only).
                t.column("public_show_comments", .boolean).notNull().defaults(to: true)
                t.column("public_show_activity", .boolean).notNull().defaults(to: false)
                t.column("public_show_coding", .text).notNull().defaults(to: "off")
                // Display-only mirror of the preview run targets + feedback target.
                t.column("preview_config", .text)
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
                // Duplicate resolution (pairs with status='duplicate').
                t.column("duplicate_of_id", .text)
                // PR linkage (one issue = one PR); all nullable.
                t.column("pr_url", .text)
                t.column("pr_number", .integer)
                t.column("pr_state", .text)
                t.column("branch", .text)
                t.column("pr_merged_at", .text)
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
                // Widget helpdesk bot marker — excluded from mention/assignee lists.
                t.column("is_agent", .boolean).notNull().defaults(to: false)
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
                // Intrinsic image dimensions (nullable for non-image rows).
                t.column("width", .integer)
                t.column("height", .integer)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "notifications", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("user_id", .text).notNull()
                t.column("issue_id", .text)
                t.column("type", .text).notNull()
                t.column("title", .text).notNull()
                t.column("body", .text)
                t.column("read_at", .text)
                t.column("pushed_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
            try db.create(
                index: "idx_notifications_user_unread",
                on: "notifications",
                columns: ["user_id", "read_at"],
                options: .ifNotExists
            )

            try db.create(table: "issue_subscribers", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull()
                // Nullable now: widget_reporter rows carry `email` instead.
                t.column("user_id", .text).indexed()
                t.column("email", .text)
                t.column("workspace_id", .text).notNull().indexed()
                t.column("source", .text).notNull()
                t.column("unsubscribed", .boolean).notNull().defaults(to: false)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "issue_events", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull().indexed()
                t.column("workspace_id", .text).notNull().indexed()
                t.column("actor_user_id", .text)
                t.column("type", .text).notNull()
                t.column("payload", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            // The live "coding now" record — one row per interactive desktop
            // coding session. Replaces the old agent_runs shape (14th shape).
            try db.create(table: "coding_sessions", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull().indexed()
                t.column("workspace_id", .text).notNull().indexed()
                t.column("user_id", .text).notNull().indexed()
                t.column("device_label", .text)
                t.column("status", .text).notNull().defaults(to: "running")
                t.column("started_at", .text).notNull()
                t.column("ended_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
        }

        // v2: 409-refetch persistence + live gating on electric_offsets —
        // `needs_refetch` survives a quit between a 409 and its refetch (the
        // next launch still applies the atomic DELETE+reinsert), `is_live`
        // records that up-to-date was seen so only then do polls switch to
        // live long-polling. Strictly additive: never re-order or edit v1,
        // and never bump the `-v4` file suffix for this (a suffix bump wipes
        // every local snapshot; ALTER TABLE preserves rows + cursors).
        migrator.registerMigration("v2_offset_refetch_state") { db in
            // Idempotent add: a fresh install could gain these columns from a
            // future v1 edit, so guard each ADD against an already-present
            // column. Re-adding a column throws SQLite "duplicate column name",
            // which would abort the whole migrator and blacklist sync (the
            // v3/repository_id blackout — never let it recur on any ALTER).
            let existing = Set(try db.columns(in: "electric_offsets").map(\.name))
            if !existing.contains("needs_refetch") || !existing.contains("is_live") {
                try db.alter(table: "electric_offsets") { t in
                    if !existing.contains("needs_refetch") {
                        t.add(column: "needs_refetch", .boolean).notNull().defaults(to: false)
                    }
                    if !existing.contains("is_live") {
                        t.add(column: "is_live", .boolean).notNull().defaults(to: false)
                    }
                }
            }
        }

        // v3 (masterplan v4 R4): `projects.repository_id` rides along on the
        // projects shape. Additive ALTER for DBs already past v1; fresh installs
        // get it from the v1 create above. Strictly additive — never bump the
        // `-v4` file suffix for this (that would wipe local snapshots + cursors).
        migrator.registerMigration("v3_project_repository_id") { db in
            // THE iOS sync blackout (masterplan §9.1): the v1 `projects` create
            // above now already includes `repository_id`, so a FRESH `-v4`
            // install runs v1 (column created) then this ALTER (column re-added)
            // → SQLite "duplicate column name: repository_id" → the migrator
            // throws → db.pool() throws → launchPipeline never starts and
            // resync() early-returns → total sync blackout. Existing `-v4`
            // devices sat at v2 (no column yet) so the bare ALTER worked for
            // them but crashed every new device / reinstall. Guard the ALTER on
            // column presence so both paths converge on the same schema.
            let hasColumn = try db.columns(in: "projects").contains { $0.name == "repository_id" }
            if !hasColumn {
                try db.alter(table: "projects") { t in
                    t.add(column: "repository_id", .text)
                }
            }
        }

        // v4 (project types): `projects` gains `type` + the three public
        // visibility toggles. Additive for DBs already past v1; fresh installs
        // get them from the v1 create above. Same column-presence guard as v3 —
        // a fresh `-v4` install runs v1 (columns created) then this ALTER, so
        // re-adding without the guard would throw "duplicate column name" and
        // blacklist sync. Strictly additive — never bump the `-v4` file suffix
        // (that would wipe local snapshots + cursors). The dropped
        // workspaces.is_public / public_write_policy columns are left in place
        // on existing devices (harmless — no record references them).
        migrator.registerMigration("v4_project_types") { db in
            let existing = Set(try db.columns(in: "projects").map(\.name))
            let needed = ["type", "public_show_comments", "public_show_activity", "public_show_coding"]
            guard needed.contains(where: { !existing.contains($0) }) else { return }
            try db.alter(table: "projects") { t in
                if !existing.contains("type") {
                    t.add(column: "type", .text).notNull().defaults(to: "dev")
                }
                if !existing.contains("public_show_comments") {
                    t.add(column: "public_show_comments", .boolean).notNull().defaults(to: true)
                }
                if !existing.contains("public_show_activity") {
                    t.add(column: "public_show_activity", .boolean).notNull().defaults(to: false)
                }
                if !existing.contains("public_show_coding") {
                    t.add(column: "public_show_coding", .text).notNull().defaults(to: "off")
                }
            }
        }

        return migrator
    }

    public func clearAllData(forAccountId accountId: String) throws {
        guard let pool = lock.withLock({ pools[accountId] }) else { return }
        try pool.write { db in
            try db.execute(sql: "DELETE FROM electric_offsets")
            try db.execute(sql: "DELETE FROM coding_sessions")
            try db.execute(sql: "DELETE FROM notifications")
            try db.execute(sql: "DELETE FROM issue_events")
            try db.execute(sql: "DELETE FROM issue_subscribers")
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
