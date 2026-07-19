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

        // Devices that ran older builds have superseded `exponential-<account>`
        // files (the pre-v2 singular-name schema through the `-v4`
        // workspace/project-era schema). The current schema lives in the
        // `-v5.sqlite` file, so every older file is unreachable forever —
        // purge them on first launch so they don't sit on disk eating space.
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
        // Wipe the main file plus -wal / -shm side files. The exists-check
        // matters under XCTest: removeItem on a missing file materializes an
        // ENOENT NSError (even though try? swallows it), and when this runs in
        // a test's deferred cleanup that error can shadow the test's real
        // failure in the failure report.
        for suffix in ["", "-wal", "-shm"] {
            let target = parent.appendingPathComponent(base + suffix)
            if fm.fileExists(atPath: target.path) {
                try? fm.removeItem(at: target)
            }
        }
        // Also remove any legacy pre-v5 file if it survived an upgrade.
        removeLegacyFile(for: accountId)
    }

    private static func removeLegacyFile(for accountId: String) {
        let fm = FileManager.default
        guard let parent = try? fileURL(for: accountId).deletingLastPathComponent() else { return }
        // Purge every superseded file-name generation: the pre-v2 singular-name
        // file, the v2 file, the v3 file (replaced by -v4 in the hard-cut
        // greenfield reshape), and the v4 file (replaced by -v5 in the EXP-180
        // workspace→team / project→board rename — renamed tables + columns, so
        // the old snapshot is a resyncable cache we simply drop).
        let legacyBases = [
            "exponential-\(accountId).sqlite",
            "exponential-\(accountId)-v2.sqlite",
            "exponential-\(accountId)-v3.sqlite",
            "exponential-\(accountId)-v4.sqlite",
        ]
        for legacyBase in legacyBases {
            for suffix in ["", "-wal", "-shm"] {
                let target = parent.appendingPathComponent(legacyBase + suffix)
                if fm.fileExists(atPath: target.path) {
                    try? fm.removeItem(at: target)
                }
            }
        }
    }

    /// Delete every canonical (`-v5`) DB file whose account id isn't in
    /// `accountIds` — orphans left behind by the id re-key migrations (widening
    /// 4-byte ids to 8-byte). One-shot cleanup; a full resync of the surviving
    /// accounts follows naturally.
    public static func deleteOrphanDatabaseFiles(keeping accountIds: Set<String>) {
        let fm = FileManager.default
        // Any accountId resolves the shared directory; the id itself is unused.
        guard let dir = try? fileURL(for: "x").deletingLastPathComponent(),
              let entries = try? fm.contentsOfDirectory(atPath: dir.path) else { return }
        let prefix = "exponential-"
        let suffix = "-v5.sqlite"
        for name in entries where name.hasPrefix(prefix) && name.hasSuffix(suffix) {
            let id = String(name.dropFirst(prefix.count).dropLast(suffix.count))
            guard !id.isEmpty, !accountIds.contains(id) else { continue }
            for sideSuffix in ["", "-wal", "-shm"] {
                let target = dir.appendingPathComponent(name + sideSuffix)
                if fm.fileExists(atPath: target.path) {
                    try? fm.removeItem(at: target)
                }
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
        // `-v5` (EXP-180 great rename): workspaces→teams, projects→boards,
        // workspace_members→team_members, workspace_invites→team_invites, and
        // the workspace_id/project_id columns→team_id/board_id everywhere. The
        // migration list was collapsed back to a single v1_initial that creates
        // the renamed schema directly (the store is a resyncable cache — the
        // documented precedent for breaking local-schema changes).
        return dbDir.appendingPathComponent("exponential-\(accountId)-v5.sqlite")
    }

    static func runMigrations(on dbPool: DatabasePool) throws {
        try makeMigrator().migrate(dbPool)
    }

    /// The canonical migrator. Extracted (and `internal`, not `private`) so the
    /// migration test suite can build fixture DBs and prove a full `migrate`
    /// runs green.
    static func makeMigrator() -> DatabaseMigrator {
        var migrator = DatabaseMigrator()

        // Single canonical schema (collapsed into one migration — the `-v5` file
        // suffix forces a clean wipe-and-resync, so there's no upgrade path to
        // preserve; the old v2…v11 incremental migrations died with the `-v4`
        // file). Mirrors the Postgres tables Electric syncs to mobile, with
        // column names and nullability matching packages/db-schema. SQLite type
        // affinities are looser than Postgres — uuid/timestamp/date columns are
        // stored as text (ISO-8601 for timestamps), enums as text, jsonb
        // (issues.description, comments.body) as text.
        //
        // NOTE: the teams shape additionally serves `helpdesk_enabled`; it is
        // deliberately NOT stored yet (a later stage adds it) — the tolerant
        // partial-update path drops unknown columns and full-row Codable
        // decoding ignores unknown keys, so the extra wire column is harmless.
        migrator.registerMigration("v1_initial") { db in
            try db.create(table: "electric_offsets", ifNotExists: true) { t in
                t.primaryKey("shape", .text)
                t.column("handle", .text).notNull()
                t.column("offset", .text).notNull()
                // A 409 / must-refetch happened and the next poll must refetch
                // from scratch (offset -1, atomic DELETE+reinsert). Persisted so
                // a quit between the 409 and the refetch can't strand stale rows.
                t.column("needs_refetch", .boolean).notNull().defaults(to: false)
                // True once up-to-date was seen for the current handle — only
                // then do polls switch to live long-polling.
                t.column("is_live", .boolean).notNull().defaults(to: false)
            }

            try db.create(table: "teams", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("name", .text).notNull()
                t.column("slug", .text).notNull()
                t.column("icon_url", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "boards", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("name", .text).notNull()
                t.column("slug", .text).notNull()
                t.column("prefix", .text).notNull()
                t.column("color", .text).notNull().defaults(to: "#6366f1")
                t.column("sort_order", .double).notNull().defaults(to: 0)
                t.column("archived_at", .text)
                // Repos live in a server-only registry; the column stays so the
                // (now-inert) repo-picker UI still compiles. Electric no longer
                // populates it.
                t.column("github_repo", .text)
                // The repo backing this board (Electric ride-along on the
                // boards shape). Nullable — repos are optional on every board;
                // coding affordances gate on presence.
                t.column("repository_id", .text)
                // Curated glyph name (nullable — nil falls back to a derived icon).
                t.column("icon", .text)
                // Server-managed protection flag: a protected board (the
                // bootstrap dogfood board) can't be deleted/archived/repointed.
                t.column("is_protected", .boolean).notNull().defaults(to: false)
                // Display-only mirror of the preview run targets + feedback target.
                t.column("preview_config", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "issues", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("board_id", .text).notNull().indexed()
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
                t.column("team_id", .text).notNull().indexed()
                t.column("name", .text).notNull()
                t.column("color", .text).notNull().defaults(to: "#6366f1")
                t.column("sort_order", .double).notNull().defaults(to: 0)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            // Composite PK matches Postgres exactly. The shape proxy sends
            // (issue_id, label_id, team_id) — no synthetic surrogate `id`.
            try db.create(table: "issue_labels", ifNotExists: true) { t in
                t.column("issue_id", .text).notNull()
                t.column("label_id", .text).notNull().indexed()
                t.column("team_id", .text).notNull().indexed()
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

            try db.create(table: "team_members", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("user_id", .text).notNull().indexed()
                t.column("role", .text).notNull()
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "team_invites", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("role", .text).notNull()
                // Nullable: the shape's server-side columns allowlist excludes
                // the bearer token (REV-4/14) — synced rows never carry it.
                t.column("token", .text).indexed()
                t.column("expires_at", .text).notNull()
                t.column("accepted_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "comments", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull().indexed()
                t.column("team_id", .text).notNull().indexed()
                t.column("author_id", .text).notNull()
                t.column("body", .text)
                t.column("kind", .text).notNull().defaults(to: "regular")
                t.column("edited_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "attachments", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("team_id", .text).notNull().indexed()
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
                // Nullable: widget_reporter rows carry `email` instead.
                t.column("user_id", .text).indexed()
                t.column("email", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("source", .text).notNull()
                t.column("unsubscribed", .boolean).notNull().defaults(to: false)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "issue_events", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull().indexed()
                t.column("team_id", .text).notNull().indexed()
                t.column("actor_user_id", .text)
                t.column("type", .text).notNull()
                t.column("payload", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            // The live "coding now" record — one row per interactive desktop
            // coding session (14th shape). issue_id/board_id are nullable: a
            // desktop batch (multi-issue) run spawns an issueless session.
            try db.create(table: "coding_sessions", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).indexed()
                t.column("board_id", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("user_id", .text).notNull().indexed()
                t.column("device_label", .text)
                t.column("status", .text).notNull().defaults(to: "running")
                t.column("started_at", .text).notNull()
                t.column("ended_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
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
            try db.execute(sql: "DELETE FROM boards")
            try db.execute(sql: "DELETE FROM team_members")
            try db.execute(sql: "DELETE FROM team_invites")
            try db.execute(sql: "DELETE FROM teams")
            try db.execute(sql: "DELETE FROM users")
        }
    }
}
