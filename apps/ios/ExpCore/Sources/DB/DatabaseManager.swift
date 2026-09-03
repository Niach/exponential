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
        // (issue_events.payload) as text.
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
                // Team-level helpdesk switch (EXP-180): gates the Support
                // inbox on every client. Synced on the teams shape.
                t.column("helpdesk_enabled", .boolean).notNull().defaults(to: false)
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
                // The repo backing this board (Electric ride-along on the
                // boards shape). Nullable — repos are optional on every board;
                // coding affordances gate on presence.
                t.column("repository_id", .text)
                // Curated glyph name (nullable — nil falls back to a derived icon).
                t.column("icon", .text)
                // EXP-712: the board's own branch (worktree base + PR target).
                // Nullable — nil follows the repo's default branch.
                t.column("default_branch", .text)
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
                // Nullable: a widget-sourced issue has no human creator.
                t.column("creator_id", .text)
                // Issue origin ('user' | 'widget').
                t.column("source", .text)
                t.column("due_date", .text)
                t.column("sort_order", .double).notNull().defaults(to: 0)
                t.column("completed_at", .text)
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
                // Optional recipient address (EXP-188 invite-by-email) —
                // synced for the pending-invite list.
                t.column("email", .text)
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
                // Nullable (REV-7): widget screenshot attachments have no human
                // uploader, and the server FK is ON DELETE SET NULL.
                t.column("uploader_id", .text)
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
                // Set on issue-less support_reply rows (the ticket's team);
                // NULL on issue-anchored rows. Rides the notifications shape.
                t.column("team_id", .text)
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
                // EXP-484: the coding agent the run was launched with
                // (contract `codingAgent`); NULL on rows that never named one.
                t.column("agent", .text)
                // EXP-545: the pr_open batch flip's stamped head branch — the
                // batch row's own-PR linkage for the Merge shortcut.
                t.column("branch", .text)
                // EXP-549/550: the host machine's steer deviceId — the join
                // key onto the live `devices` row (renamed label + offline
                // detection).
                t.column("device_id", .text)
                t.column("needs_input", .boolean).notNull().defaults(to: false)
                // Action run linkage (EXP-253): both NULL on ordinary
                // issue/batch sessions; action_name outlives a deleted action
                // (server FK SET NULL keeps the snapshot label).
                t.column("action_id", .text)
                t.column("action_name", .text)
                // EXP-637: the agent's own close-out (`exponential_sessions_end`
                // writes the summary; EXP-686 dropped the outcome beside it),
                // who ended the run, and the ended run this one resumed.
                t.column("summary", .text)
                t.column("ended_by", .text)
                t.column("resumed_from_id", .text)
                t.column("started_at", .text).notNull()
                t.column("ended_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
        }

        // v2 (EXP-180 helpdesk follow-up): `notifications.team_id` rides along
        // on the notifications shape — set on issue-less support_reply rows,
        // NULL otherwise. Additive ALTER for `-v5` stores created before the
        // column existed; guarded on column presence so fresh installs (which
        // get it from the v1 create above) converge on the same schema. Never
        // bump the `-v5` file suffix for an additive column (that would wipe
        // every local snapshot; ALTER TABLE preserves rows + cursors).
        migrator.registerMigration("v2_notification_team_id") { db in
            // Table-existence guard (old v3-v6 precedent): migration-fixture
            // DBs that carry only the minimal schema don't have the table.
            guard try db.tableExists("notifications") else { return }
            let existing = Set(try db.columns(in: "notifications").map(\.name))
            if !existing.contains("team_id") {
                try db.alter(table: "notifications") { t in
                    t.add(column: "team_id", .text)
                }
            }
            // The server-side columns allowlist change rotates the shape handle
            // anyway (409 → refetch), but mark the notifications offset
            // needs_refetch here too — mirroring ShapeClient's must-refetch
            // write (handle="", offset="-1", needs_refetch set, is_live
            // cleared) — so already-synced rows re-snapshot with team_id even
            // without waiting on the server round-trip. WHERE-guarded: a fresh
            // install has no offset row yet and snapshots from scratch anyway.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'notifications'
                    """)
            }
        }

        // v3 (EXP-188 invite-by-email): `team_invites.email` rides along on
        // the team-invites shape — set when the owner sent the invite by
        // email, NULL otherwise. Additive ALTER for `-v5` stores created
        // before the column existed; guarded on column presence so fresh
        // installs (which get it from the v1 create above) converge on the
        // same schema. Never bump the `-v5` file suffix for an additive
        // column (that would wipe every local snapshot; ALTER TABLE preserves
        // rows + cursors).
        migrator.registerMigration("v3_team_invite_email") { db in
            // Table-existence guard (old v3-v6 precedent): migration-fixture
            // DBs that carry only the minimal schema don't have the table.
            guard try db.tableExists("team_invites") else { return }
            let existing = Set(try db.columns(in: "team_invites").map(\.name))
            if !existing.contains("email") {
                try db.alter(table: "team_invites") { t in
                    t.add(column: "email", .text)
                }
            }
            // The server-side columns allowlist change rotates the shape handle
            // anyway (409 → refetch), but mark the team-invites offset
            // needs_refetch here too — mirroring ShapeClient's must-refetch
            // write (handle="", offset="-1", needs_refetch set, is_live
            // cleared) — so already-synced rows re-snapshot with email even
            // without waiting on the server round-trip. WHERE-guarded: a fresh
            // install has no offset row yet and snapshots from scratch anyway.
            // NOTE: the shape key is 'team-invites' WITH A DASH (the proxy
            // route name), not the SQLite table name.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'team-invites'
                    """)
            }
        }

        // v4 (EXP-214 needs-input): `coding_sessions.needs_input` rides along
        // on the coding-sessions shape — the desktop-written attention flag
        // while the agent waits on a plan-approval / question picker.
        // Additive ALTER for stores created before the column existed;
        // guarded on column presence so fresh installs (which get it from the
        // v1 create above) converge on the same schema.
        migrator.registerMigration("v4_coding_session_needs_input") { db in
            // Table-existence guard (old v3-v6 precedent): migration-fixture
            // DBs that carry only the minimal schema don't have the table.
            guard try db.tableExists("coding_sessions") else { return }
            let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
            if !existing.contains("needs_input") {
                try db.alter(table: "coding_sessions") { t in
                    t.add(column: "needs_input", .boolean).notNull().defaults(to: false)
                }
            }
            // Force a re-snapshot so already-synced rows pick up the new
            // column (mirrors the v2/v3 refetch write). NOTE: the shape key
            // is 'coding-sessions' WITH A DASH (the proxy route name), not
            // the SQLite table name.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'coding-sessions'
                    """)
            }
        }

        // v5 (users.is_agent removal): the users shape dropped `is_agent` (the
        // synced users shape is now 6 columns). Drop the now-dead column from
        // the local cache. Guarded on presence so fresh installs (which never
        // create it above) and re-runs are both no-ops. SQLite 3.35+ (shipped
        // on every supported iOS) supports ALTER TABLE ... DROP COLUMN, which
        // GRDB's `t.drop(column:)` emits.
        migrator.registerMigration("v5_drop_user_is_agent") { db in
            guard try db.tableExists("users") else { return }
            let existing = Set(try db.columns(in: "users").map(\.name))
            if existing.contains("is_agent") {
                try db.alter(table: "users") { t in
                    t.drop(column: "is_agent")
                }
            }
        }

        // v6 (issues.creator_id nullable + issues.source): a widget-sourced
        // issue has a NULL creator, which the old NOT NULL constraint would
        // reject, and issues now carry a `source` origin column. SQLite can't
        // relax a NOT NULL via ALTER, so rebuild the table (create nullable +
        // source, copy rows, drop, rename), then force a full re-snapshot so
        // already-synced rows re-arrive with `source`.
        migrator.registerMigration("v6_issue_source_nullable_creator") { db in
            guard try db.tableExists("issues") else { return }
            let cols = try db.columns(in: "issues")
            let creatorNotNull = cols.first { $0.name == "creator_id" }?.isNotNull ?? false
            let hasSource = cols.contains { $0.name == "source" }
            // Already converged (fresh installs get the v1 create shape).
            if !creatorNotNull && hasSource { return }

            try db.create(table: "issues_new") { t in
                t.primaryKey("id", .text)
                t.column("board_id", .text).notNull().indexed()
                t.column("number", .integer).notNull().defaults(to: 0)
                t.column("identifier", .text).notNull().defaults(to: "")
                t.column("title", .text).notNull()
                t.column("description", .text)
                t.column("status", .text).notNull().defaults(to: "backlog")
                t.column("priority", .text).notNull().defaults(to: "none")
                t.column("assignee_id", .text)
                // Nullable: a widget-sourced issue has no human creator.
                t.column("creator_id", .text)
                // Issue origin ('user' | 'widget').
                t.column("source", .text)
                t.column("due_date", .text)
                t.column("due_time", .text)
                t.column("end_time", .text)
                t.column("sort_order", .double).notNull().defaults(to: 0)
                t.column("completed_at", .text)
                t.column("archived_at", .text)
                t.column("duplicate_of_id", .text)
                t.column("pr_url", .text)
                t.column("pr_number", .integer)
                t.column("pr_state", .text)
                t.column("branch", .text)
                t.column("pr_merged_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            // Copy the shared columns (source is new → left NULL).
            try db.execute(sql: """
                INSERT INTO "issues_new" (
                    "id", "board_id", "number", "identifier", "title", "description",
                    "status", "priority", "assignee_id", "creator_id", "due_date",
                    "due_time", "end_time", "sort_order", "completed_at", "archived_at",
                    "duplicate_of_id", "pr_url", "pr_number", "pr_state", "branch",
                    "pr_merged_at", "created_at", "updated_at"
                )
                SELECT
                    "id", "board_id", "number", "identifier", "title", "description",
                    "status", "priority", "assignee_id", "creator_id", "due_date",
                    "due_time", "end_time", "sort_order", "completed_at", "archived_at",
                    "duplicate_of_id", "pr_url", "pr_number", "pr_state", "branch",
                    "pr_merged_at", "created_at", "updated_at"
                FROM "issues"
                """)

            try db.drop(table: "issues")
            try db.rename(table: "issues_new", to: "issues")

            // Force a re-snapshot so already-synced rows re-arrive with the
            // `source` column (mirrors the v2/v3/v4 refetch write).
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'issues'
                    """)
            }
        }

        // v7 (REV2-91 JSONB-era cleanup): `boards.github_repo` (repos live in
        // the server-only registry) and `boards.preview_config` (the deleted
        // releases-era preview feature) no longer exist server-side and the
        // boards shape's columns allowlist never carried them — nothing ever
        // populated the local columns. Drop them from the cache (the
        // v5_drop_user_is_agent precedent); guarded on presence so fresh
        // installs (which never create them above) and re-runs are no-ops.
        migrator.registerMigration("v7_drop_board_dead_columns") { db in
            guard try db.tableExists("boards") else { return }
            let existing = Set(try db.columns(in: "boards").map(\.name))
            for column in ["github_repo", "preview_config"] where existing.contains(column) {
                try db.alter(table: "boards") { t in
                    t.drop(column: column)
                }
            }
        }

        // v8 (EXP-253 actions): `coding_sessions.action_id` + `action_name`
        // ride along on the coding-sessions shape — an action run's row is
        // batch-shaped (issue_id NULL) with action_name labeling it. Additive
        // ALTERs for stores created before the columns existed; guarded on
        // column presence so fresh installs (which get them from the v1
        // create above) converge on the same schema.
        migrator.registerMigration("v8_coding_session_action_fields") { db in
            // Table-existence guard (old v3-v6 precedent): migration-fixture
            // DBs that carry only the minimal schema don't have the table.
            guard try db.tableExists("coding_sessions") else { return }
            let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
            if !existing.contains("action_id") {
                try db.alter(table: "coding_sessions") { t in
                    t.add(column: "action_id", .text)
                }
            }
            if !existing.contains("action_name") {
                try db.alter(table: "coding_sessions") { t in
                    t.add(column: "action_name", .text)
                }
            }
            // Force a re-snapshot so already-synced rows pick up the new
            // columns (mirrors the v2/v3/v4 refetch write). NOTE: the shape
            // key is 'coding-sessions' WITH A DASH (the proxy route name),
            // not the SQLite table name.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'coding-sessions'
                    """)
            }
        }

        // v9 (EXP-268 actions shape): team action prompts became the 15th
        // Electric shape, so they get a local table. The server-side columns
        // allowlist deliberately EXCLUDES `body` (the ≤64KB markdown prompt
        // never rides sync — tRPC `actions.get` stays the only body path).
        // Additive new table for stores created before it existed; ifNotExists
        // keeps re-runs converging. Never bump the `-v5` file suffix for an
        // additive table (that would wipe every local snapshot). No offset
        // reset needed — a brand-new shape has no offset row and snapshots
        // from scratch.
        migrator.registerMigration("v9_actions") { db in
            try db.create(table: "actions", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("team_id", .text).notNull().indexed()
                // Nil for repo-less actions (scratch-dir runs).
                t.column("repository_id", .text)
                t.column("name", .text).notNull()
                t.column("description", .text)
                // Typed inputs schema (jsonb) stored as stringified JSON.
                t.column("inputs", .text)
                t.column("sort_order", .double).notNull().defaults(to: 0)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
        }

        // EXP-273: the action's curated registry glyph — the same set
        // boards.icon draws from. Additive column on the existing actions
        // shape; a fresh install gets it from v9 above, an upgrade here.
        migrator.registerMigration("v10_action_icon") { db in
            try db.alter(table: "actions") { t in
                t.add(column: "icon", .text)
            }
        }

        // v11 (REV2-103): the SYNCED archiving columns are gone —
        // `boards.archived_at` / `issues.archived_at` no longer reach any
        // client. Drop the dead columns from the cache (the
        // v5_drop_user_is_agent / v7_drop_board_dead_columns precedent); guarded
        // on presence so fresh installs (which never create them above) and
        // re-runs are no-ops. Stale stores that only now run the v6 issues
        // rebuild still get the column from that historical create — this drops
        // it right after. Board TRASH (`deleted_at`) is a different feature and
        // is filtered server-side by the boards shape, so nothing here touches it.
        //
        // EXP-500 later brought BOARD archiving back, but deliberately without
        // a synced column: the boards shape excludes archived boards and the
        // archive fan-out pulls their issues out of the issue-child shapes, so
        // an archived board just stops arriving and iOS needs no filter and no
        // new migration. This drop stays correct — do not resurrect the column
        // (syncing it and filtering per-client is what leaked in the first
        // place). Never rename this migration identifier: it is applied
        // history.
        migrator.registerMigration("v11_drop_archived_at") { db in
            for table in ["boards", "issues"] {
                guard try db.tableExists(table) else { continue }
                let existing = Set(try db.columns(in: table).map(\.name))
                guard existing.contains("archived_at") else { continue }
                try db.alter(table: table) { t in
                    t.drop(column: "archived_at")
                }
            }
        }

        // v12 (REV2-103 / REV2-49): time-of-day on issues is gone from the
        // product — `issues.due_time` / `issues.end_time` no longer exist
        // server-side and the issues shape no longer carries them. Drop the
        // dead columns from the cache (the v11_drop_archived_at precedent);
        // guarded on presence so fresh installs (which never create them
        // above) and re-runs are no-ops. A stale store that only now runs the
        // v6 issues rebuild still gets them from that historical create —
        // this drops them right after. Due DATE (`due_date`) is untouched.
        migrator.registerMigration("v12_drop_issue_times") { db in
            guard try db.tableExists("issues") else { return }
            let existing = Set(try db.columns(in: "issues").map(\.name))
            for column in ["due_time", "end_time"] where existing.contains(column) {
                try db.alter(table: "issues") { t in
                    t.drop(column: column)
                }
            }
        }

        // v13 (EXP-314 custom issue statuses): the 16th Electric shape
        // `issue_statuses` (team-scoped, 7 locked builtin rows + customs) gets
        // a local table, and `issues` gains the nullable `status_id` FK the
        // shape now carries. Both additive: `ifNotExists` for the table, a
        // presence guard for the column (the v11 pattern) so fresh installs —
        // which create `issues` without it above — and re-runs converge. Never
        // bump the `-v5` file suffix for an additive change (that would wipe
        // every local snapshot). The brand-new shape has no offset row and
        // snapshots from scratch; `issues` rotates its shape identity
        // server-side, so the atomic refetch backfills status_id.
        migrator.registerMigration("v13_issue_statuses") { db in
            try db.create(table: "issue_statuses", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("category", .text).notNull()
                t.column("name", .text).notNull()
                // Only CUSTOM rows render this hex; builtins keep the
                // platform's design-token colors.
                t.column("color", .text)
                t.column("sort_order", .double).notNull().defaults(to: 0)
                // Non-null on the 7 locked builtin rows, null on customs.
                t.column("builtin_key", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            guard try db.tableExists("issues") else { return }
            let existing = Set(try db.columns(in: "issues").map(\.name))
            guard !existing.contains("status_id") else { return }
            try db.alter(table: "issues") { t in
                t.add(column: "status_id", .text)
            }
        }

        // v14 (EXP-364): protected boards are gone from the product —
        // `boards.is_protected` no longer exists server-side and the boards
        // shape no longer carries it. Drop the dead column from the cache (the
        // v7_drop_board_dead_columns / v11_drop_archived_at precedent); guarded
        // on presence so fresh installs (which never create it above) and
        // re-runs are no-ops. No offset reset — a column going AWAY needs no
        // re-snapshot, and the decoder ignores the key if a stale live stream
        // still emits it.
        migrator.registerMigration("v14_drop_board_is_protected") { db in
            guard try db.tableExists("boards") else { return }
            let existing = Set(try db.columns(in: "boards").map(\.name))
            guard existing.contains("is_protected") else { return }
            try db.alter(table: "boards") { t in
                t.drop(column: "is_protected")
            }
        }

        // v15 (REV-7): `attachments.uploader_id` is NULLABLE server-side — a
        // widget screenshot attachment has no human uploader, and the FK is ON
        // DELETE SET NULL, so deleting an account nulls the uploader on the
        // attachments it left behind in a surviving team. The local NOT NULL
        // turned both into sync failures: a full-row insert with a null uploader
        // was decode-dropped forever, and a SET NULL update bound NULL into the
        // constrained column, throwing inside applyBatch BEFORE the offset save
        // — the same batch refetched and refailed on every poll, permanently
        // stalling the attachments shape. SQLite can't relax a NOT NULL via
        // ALTER, so rebuild the table (the v6_issue_source_nullable_creator
        // precedent), then force a full re-snapshot so the rows that were
        // dropped on the way in finally land.
        migrator.registerMigration("v15_attachment_nullable_uploader") { db in
            guard try db.tableExists("attachments") else { return }
            let cols = try db.columns(in: "attachments")
            let uploaderNotNull = cols.first { $0.name == "uploader_id" }?.isNotNull ?? false
            // Already converged (fresh installs get the v1 create shape).
            guard uploaderNotNull else { return }

            try db.create(table: "attachments_new") { t in
                t.primaryKey("id", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("issue_id", .text).notNull().indexed()
                t.column("comment_id", .text)
                // Nullable: widget screenshots + SET NULL on account deletion.
                t.column("uploader_id", .text)
                t.column("filename", .text).notNull()
                t.column("content_type", .text).notNull()
                t.column("size_bytes", .integer).notNull()
                t.column("storage_key", .text).notNull()
                t.column("url", .text).notNull()
                t.column("width", .integer)
                t.column("height", .integer)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.execute(sql: """
                INSERT INTO "attachments_new" (
                    "id", "team_id", "issue_id", "comment_id", "uploader_id",
                    "filename", "content_type", "size_bytes", "storage_key",
                    "url", "width", "height", "created_at", "updated_at"
                )
                SELECT
                    "id", "team_id", "issue_id", "comment_id", "uploader_id",
                    "filename", "content_type", "size_bytes", "storage_key",
                    "url", "width", "height", "created_at", "updated_at"
                FROM "attachments"
                """)

            try db.drop(table: "attachments")
            try db.rename(table: "attachments_new", to: "attachments")

            // Force a re-snapshot so the inserts this store dropped (widget
            // screenshots) re-arrive — nothing else ever refetches them.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'attachments'
                    """)
            }
        }

        // v16 (EXP-481 device management): `devices` + `device_worktrees`
        // became the 17th/18th Electric shapes (server-authoritative device
        // state — launch defaults, worktree inventory; online-ness derives
        // from last_seen_at freshness client-side). Additive new tables for
        // stores created before they existed; ifNotExists keeps re-runs
        // converging. Never bump the `-v5` file suffix for an additive table
        // (that would wipe every local snapshot). Brand-new shapes have no
        // offset rows and snapshot from scratch. `busy` is declared .boolean
        // so the partial-update wire-bool mapping engages (the
        // coding_sessions `needs_input` precedent); jsonb columns store
        // stringified JSON.
        migrator.registerMigration("v16_devices_worktrees") { db in
            try db.create(table: "devices", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("user_id", .text).notNull().indexed()
                // The steer deviceId (start target) — not the row id.
                t.column("device_id", .text).notNull()
                t.column("label", .text).notNull().defaults(to: "")
                t.column("kind", .text)
                t.column("platform", .text)
                t.column("version", .text)
                t.column("agents", .text)
                t.column("caps", .text)
                t.column("unauthed_agents", .text)
                t.column("launch_defaults", .text)
                t.column("launch_defaults_updated_at", .text)
                // EXP-484: the machine's per-agent auth status + rate-limit
                // usage (jsonb, stored as stringified JSON) and when the
                // server last stored the usage.
                t.column("agent_accounts", .text)
                t.column("agent_usage", .text)
                t.column("agent_usage_at", .text)
                t.column("active_sessions", .integer).notNull().defaults(to: 0)
                t.column("last_seen_at", .text)
                t.column("shared_team_id", .text)
                t.column("update_requested_at", .text)
                t.column("created_at", .text)
                t.column("updated_at", .text)
            }
            try db.create(table: "device_worktrees", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                // The devices ROW id (uuid), never the steer device-id string.
                t.column("device_row_id", .text).notNull().indexed()
                t.column("repo_full_name", .text).notNull()
                t.column("branch", .text).notNull()
                t.column("issue_identifier", .text)
                t.column("agents", .text)
                t.column("dirty", .text)
                t.column("busy", .boolean).notNull().defaults(to: false)
                t.column("reported_at", .text)
                t.column("created_at", .text)
                t.column("updated_at", .text)
            }
        }

        // v17 (EXP-545 batch↔PR linkage): `coding_sessions.branch` rides
        // along on the coding-sessions shape — the head branch the server's
        // pr_open batch flip stamps so a batch row's Merge shortcut targets
        // its OWN PR (not "the team's sole open batch PR", which could be a
        // teammate's once the session's own PR closed unmerged). Additive
        // ALTER for stores created before the column existed; guarded on
        // column presence so fresh installs (which get it from the v1 create
        // above) converge on the same schema.
        migrator.registerMigration("v17_coding_session_branch") { db in
            // Table-existence guard (old v3-v6 precedent): migration-fixture
            // DBs that carry only the minimal schema don't have the table.
            guard try db.tableExists("coding_sessions") else { return }
            let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
            if !existing.contains("branch") {
                try db.alter(table: "coding_sessions") { t in
                    t.add(column: "branch", .text)
                }
            }
            // Force a re-snapshot so already-synced rows pick up the new
            // column (the v4 needs_input precedent). NOTE: the shape key is
            // 'coding-sessions' WITH A DASH (the proxy route name), not the
            // SQLite table name.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'coding-sessions'
                    """)
            }
        }

        // v18 (EXP-530 action automations): `actions.trigger` (jsonb stored
        // as stringified JSON — the schedule/event automation config) and
        // `coding_sessions.started_reason` (non-null on automation-started
        // runs) ride along on their shapes. Additive ALTERs — fresh installs
        // get the columns HERE too (the v9/v1 creates predate them); guarded
        // on table + column presence so migration-fixture DBs converge.
        migrator.registerMigration("v18_action_automations") { db in
            if try db.tableExists("actions") {
                let existing = Set(try db.columns(in: "actions").map(\.name))
                if !existing.contains("trigger") {
                    try db.alter(table: "actions") { t in
                        t.add(column: "trigger", .text)
                    }
                }
            }
            if try db.tableExists("coding_sessions") {
                let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
                if !existing.contains("started_reason") {
                    try db.alter(table: "coding_sessions") { t in
                        t.add(column: "started_reason", .text)
                    }
                }
            }
            // Force re-snapshots so already-synced rows pick up the new
            // columns (the v17 precedent). NOTE: the coding-sessions shape
            // key has A DASH (the proxy route name), not the table name.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'actions'
                    """)
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'coding-sessions'
                    """)
            }
        }

        // v19 (EXP-549/EXP-550 session host device): `coding_sessions.device_id`
        // rides along on the coding-sessions shape — the host machine's steer
        // deviceId, which joins a session to its LIVE `devices` row so the
        // Agents list shows the RENAMED label (not the start-time hostname
        // snapshot) and a session whose machine stopped heartbeating reads
        // "paused · offline" instead of a live dot. Additive ALTER for stores
        // created before the column existed; guarded on table + column
        // presence so fresh installs (which get it from the v1 create above)
        // and migration-fixture DBs converge on the same schema.
        migrator.registerMigration("v19_coding_session_device_id") { db in
            guard try db.tableExists("coding_sessions") else { return }
            let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
            if !existing.contains("device_id") {
                try db.alter(table: "coding_sessions") { t in
                    t.add(column: "device_id", .text)
                }
            }
            // Force a re-snapshot so already-synced rows pick up the new
            // column (the v17/v18 precedent). NOTE: the shape key is
            // 'coding-sessions' WITH A DASH (the proxy route name), not the
            // SQLite table name.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'coding-sessions'
                    """)
            }
        }

        // v20 (EXP-583 automations): automations became their own entity — a
        // new team-scoped table for the 19th Electric shape, plus
        // `coding_sessions.automation_id` (which automation fired the run)
        // riding along on the coding-sessions shape. The server dropped
        // `actions.trigger`; the local column from v18 stays (a table rebuild
        // for a column nobody reads is not worth it) and simply decodes nil
        // forever. A brand-new shape has no offset row and snapshots from
        // scratch, so only the coding-sessions offset needs the reset.
        migrator.registerMigration("v20_automations") { db in
            try db.create(table: "automations", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("action_id", .text).notNull().indexed()
                // The steer device id (text), not the devices ROW id.
                t.column("device_id", .text).notNull()
                t.column("enabled", .boolean).notNull().defaults(to: true)
                // The when-part jsonb, stored as stringified JSON.
                t.column("trigger", .text)
                // Null = the device's launch defaults.
                t.column("agent", .text)
                t.column("model", .text)
                t.column("effort", .text)
                t.column("sort_order", .double).notNull().defaults(to: 0)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
            if try db.tableExists("coding_sessions") {
                let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
                if !existing.contains("automation_id") {
                    try db.alter(table: "coding_sessions") { t in
                        t.add(column: "automation_id", .text)
                    }
                }
            }
            // Force a re-snapshot so already-synced rows pick up the new
            // column (the v17/v18/v19 precedent). NOTE: the shape key is
            // 'coding-sessions' WITH A DASH (the proxy route name), not the
            // SQLite table name.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'coding-sessions'
                    """)
            }
        }

        // v21 (EXP-622 default device): `devices.is_default` — the owner's
        // default machine, which every device picker prefills. Rides the
        // existing devices shape, so the offset reset re-snapshots the rows
        // already stored without it (the v19/v20 precedent).
        migrator.registerMigration("v21_device_is_default") { db in
            guard try db.tableExists("devices") else { return }
            let existing = Set(try db.columns(in: "devices").map(\.name))
            if !existing.contains("is_default") {
                try db.alter(table: "devices") { t in
                    t.add(column: "is_default", .boolean).notNull().defaults(to: false)
                }
            }
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'devices'
                    """)
            }
        }

        // v22 (EXP-637 agent run close-outs): three columns ride along on the
        // coding-sessions shape — `summary`, written by the agent's own
        // `exponential_sessions_end` call, `ended_by` (which path ended the
        // run) and `resumed_from_id` (the ended run a resume continues).
        // EXP-686 removed the fourth (`outcome`) from the list rather than
        // adding a column v24 immediately drops again — stores that already
        // ran v22 keep it until v24.
        // Additive ALTERs guarded on column presence so fresh installs (which
        // get them from the v1 create above) and migration-fixture DBs converge
        // on the same schema, then the offset reset so already-synced rows
        // re-arrive carrying them (the v17/v18/v19/v20 precedent).
        migrator.registerMigration("v22_coding_session_outcome") { db in
            guard try db.tableExists("coding_sessions") else { return }
            let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
            for column in ["summary", "ended_by", "resumed_from_id"]
            where !existing.contains(column) {
                try db.alter(table: "coding_sessions") { t in
                    t.add(column: column, .text)
                }
            }
            // NOTE: the shape key is 'coding-sessions' WITH A DASH (the proxy
            // route name), not the SQLite table name.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'coding-sessions'
                    """)
            }
        }

        // v23 (EXP-484 agent auth status + usage): the machine's per-agent
        // account/usage report rides the devices shape
        // (`agent_accounts`/`agent_usage` jsonb + `agent_usage_at`), and the
        // agent a run was launched with rides the coding-sessions shape
        // (`coding_sessions.agent`). Guarded additive ALTERs so fresh installs
        // (which get the columns from the v1 create above) and older stores
        // converge, then BOTH shape offsets reset so the rows already synced
        // re-arrive carrying them (the v17/v18/v19/v20/v21/v22 precedent).
        migrator.registerMigration("v23_agent_status") { db in
            if try db.tableExists("devices") {
                let existing = Set(try db.columns(in: "devices").map(\.name))
                for column in ["agent_accounts", "agent_usage", "agent_usage_at"]
                where !existing.contains(column) {
                    try db.alter(table: "devices") { t in
                        t.add(column: column, .text)
                    }
                }
            }
            if try db.tableExists("coding_sessions") {
                let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
                if !existing.contains("agent") {
                    try db.alter(table: "coding_sessions") { t in
                        t.add(column: "agent", .text)
                    }
                }
            }
            // Two shapes rotate here. NOTE: the coding-sessions shape key has
            // A DASH (the proxy route name), not the SQLite table name.
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" IN ('devices', 'coding-sessions')
                    """)
            }
        }

        // v24 (EXP-686): the self-reported run `outcome` is gone from the
        // server, the shape allowlist and every client. Drop the local column
        // so an old store converges on the same schema a fresh install gets
        // from the v1 create above. Guarded on presence (a v1 install made
        // after EXP-686 never had it). No offset reset: nothing new arrives,
        // the wire simply stops carrying a key the decoder no longer reads.
        migrator.registerMigration("v24_drop_coding_session_outcome") { db in
            guard try db.tableExists("coding_sessions") else { return }
            let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
            guard existing.contains("outcome") else { return }
            try db.alter(table: "coding_sessions") { t in
                t.drop(column: "outcome")
            }
        }

        // v25 (EXP-712): a board carries its OWN branch — the branch its
        // coding sessions start from and its PRs target (NULL = follow the
        // repo). Guarded additive ALTER so fresh installs (which get the
        // column from the v1 create above) and older stores converge, then the
        // boards shape offset resets so the rows already synced re-arrive
        // carrying it (the v17…v23 precedent).
        migrator.registerMigration("v25_board_default_branch") { db in
            guard try db.tableExists("boards") else { return }
            let existing = Set(try db.columns(in: "boards").map(\.name))
            if !existing.contains("default_branch") {
                try db.alter(table: "boards") { t in
                    t.add(column: "default_branch", .text)
                }
            }
            if try db.tableExists("electric_offsets") {
                try db.execute(sql: """
                    UPDATE "electric_offsets"
                    SET "handle" = '', "offset" = '-1', "needs_refetch" = 1, "is_live" = 0
                    WHERE "shape" = 'boards'
                    """)
            }
        }

        return migrator
    }

    public func clearAllData(forAccountId accountId: String) throws {
        guard let pool = lock.withLock({ pools[accountId] }) else { return }
        try pool.write { db in
            try db.execute(sql: "DELETE FROM electric_offsets")
            // EXP-481: child before parent, like the issue tables below.
            try db.execute(sql: "DELETE FROM device_worktrees")
            try db.execute(sql: "DELETE FROM devices")
            // EXP-583: automations reference actions — child first.
            try db.execute(sql: "DELETE FROM automations")
            try db.execute(sql: "DELETE FROM actions")
            try db.execute(sql: "DELETE FROM coding_sessions")
            try db.execute(sql: "DELETE FROM notifications")
            try db.execute(sql: "DELETE FROM issue_events")
            try db.execute(sql: "DELETE FROM issue_subscribers")
            try db.execute(sql: "DELETE FROM attachments")
            try db.execute(sql: "DELETE FROM comments")
            try db.execute(sql: "DELETE FROM issue_labels")
            // EXP-314: must be wiped with everything else — a "Resync now" that
            // left stale status rows behind would keep resolving issues into
            // statuses the server has since renamed or deleted.
            try db.execute(sql: "DELETE FROM issue_statuses")
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
