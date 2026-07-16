import GRDB
import XCTest
@testable import ExpCore

// Regression gate for the iOS sync blackout (masterplan §9.1, EXP-12).
//
// The blackout root cause: the v1 `projects` create already declares
// `repository_id`, while the additive `v3_project_repository_id` migration also
// ran `ALTER TABLE projects ADD COLUMN repository_id`. On a FRESH `-v4` install
// (v1 then v3) that second add throws SQLite "duplicate column name", the whole
// migrator aborts, `db.pool()` throws, and sync never starts — with the failure
// only visible in os.Logger. These tests build fixture DBs at each historical
// schema version and prove `makeMigrator().migrate` runs green from every one.
final class DatabaseMigrationTests: XCTestCase {
    private var tempDir: URL!

    override func setUpWithError() throws {
        tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("db-migration-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: tempDir)
        tempDir = nil
    }

    private func makePool(_ name: String) throws -> DatabasePool {
        try DatabasePool(path: tempDir.appendingPathComponent("\(name).sqlite").path)
    }

    private func columnNames(_ pool: DatabasePool, _ table: String) throws -> Set<String> {
        try pool.read { db in Set(try db.columns(in: table).map(\.name)) }
    }

    private func appliedMigrations(_ pool: DatabasePool) throws -> Set<String> {
        try pool.read { db in try DatabaseManager.makeMigrator().appliedIdentifiers(db) }
    }

    // THE regression: a brand-new `-v4` DB must migrate all the way green.
    // Before the fix this threw "duplicate column name: repository_id".
    func testFreshInstallMigratesGreen() throws {
        let pool = try makePool("fresh")
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
        XCTAssertEqual(try appliedMigrations(pool).count, 10)
        XCTAssertTrue(try columnNames(pool, "projects").contains("repository_id"))
    }

    // A device that stopped at v1 (pre-refetch-state, pre-repository_id) must
    // upgrade cleanly: v2 adds the offset flags, v3 adds repository_id.
    func testUpgradeFromV1OnlyMigratesGreen() throws {
        let pool = try makePool("from-v1")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v1_initial")
        // A v1 fixture must NOT yet carry the later columns...
        XCTAssertFalse(try columnNames(pool, "electric_offsets").contains("needs_refetch"))
        // ...but the v1 create already declares repository_id (that overlap is
        // exactly what the guarded v3 ALTER has to tolerate).
        XCTAssertTrue(try columnNames(pool, "projects").contains("repository_id"))

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertEqual(try appliedMigrations(pool).count, 10)
        let offsetCols = try columnNames(pool, "electric_offsets")
        XCTAssertTrue(offsetCols.contains("needs_refetch"))
        XCTAssertTrue(offsetCols.contains("is_live"))
    }

    // A device that stopped at v1+v2 (existing `-v4` install pre-repository_id)
    // must run only v3 — the historically real upgrade path.
    func testUpgradeFromV1PlusV2MigratesGreen() throws {
        let pool = try makePool("from-v2")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v2_offset_refetch_state")
        XCTAssertTrue(try columnNames(pool, "electric_offsets").contains("is_live"))

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertEqual(try appliedMigrations(pool).count, 10)
        XCTAssertTrue(try columnNames(pool, "projects").contains("repository_id"))
    }

    // v6 (REV-4/14): a device whose workspace_invites table still declares the
    // legacy NOT NULL `token` (the shape no longer syncs the bearer token) must
    // get the table rebuilt nullable and its shape offset reset to refetch.
    func testLegacyNotNullInviteTokenRebuilds() throws {
        let pool = try makePool("invite-token")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v2_offset_refetch_state")
        try pool.write { db in
            // Hand-build the pre-v6 state: NOT NULL token + a live offset row.
            try db.drop(table: "workspace_invites")
            try db.create(table: "workspace_invites") { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull().indexed()
                t.column("role", .text).notNull()
                t.column("token", .text).notNull().indexed()
                t.column("expires_at", .text).notNull()
                t.column("accepted_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('workspace-invites', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertEqual(try appliedMigrations(pool).count, 10)
        let tokenColumn = try pool.read { db in
            try db.columns(in: "workspace_invites").first { $0.name == "token" }
        }
        XCTAssertNotNil(tokenColumn)
        XCTAssertFalse(tokenColumn?.isNotNull ?? true)
        // The rebuild must force a refetch of the (now empty) invites shape.
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "needs_refetch" FROM "electric_offsets"
                    WHERE "shape" = 'workspace-invites'
                    """
            )
        }
        let handle: String? = offset?["handle"]
        let needsRefetch: Bool? = offset?["needs_refetch"]
        XCTAssertEqual(handle, "")
        XCTAssertEqual(needsRefetch, true)
    }

    // EXP-106: a device that ran the original v7 (releases table + release_id
    // on issues / coding_sessions) must have all of it dropped by v8, while the
    // coding_sessions rows survive with their release_id-less columns intact.
    // The current v7 body no longer creates those artifacts, so hand-build the
    // pre-v8 state (the invite-token test's playbook) before running v8.
    func testV8DropsReleaseArtifacts() throws {
        let pool = try makePool("drop-releases")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v7_releases")
        try pool.write { db in
            try db.create(table: "releases") { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull()
                t.column("name", .text).notNull()
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
            try db.alter(table: "issues") { t in t.add(column: "release_id", .text) }
            try db.alter(table: "coding_sessions") { t in t.add(column: "release_id", .text) }
            try db.execute(sql: """
                INSERT INTO "coding_sessions"
                    ("id", "workspace_id", "user_id", "status", "started_at",
                     "created_at", "updated_at")
                VALUES ('s1', 'w1', 'u1', 'running', '2026-01-01', '2026-01-01', '2026-01-01')
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertFalse(try pool.read { db in try db.tableExists("releases") })
        XCTAssertFalse(try columnNames(pool, "issues").contains("release_id"))
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("release_id"))
        // The dropped column is in-place surgery — the row must survive.
        let surviving = try pool.read { db in
            try Int.fetchOne(db, sql: "SELECT COUNT(*) FROM coding_sessions")
        }
        XCTAssertEqual(surviving, 1)
    }

    // EXP-107: a device that ran the original v1 create (with the recurrence
    // columns) must have both dropped by v10, while its issue rows survive. The
    // current v1 body no longer creates those columns, so hand-add them to the
    // issues table (the release-drop test's playbook) before running v10.
    func testV10DropsRecurrenceColumns() throws {
        let pool = try makePool("drop-recurrence")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v9_project_is_public_icon")
        try pool.write { db in
            try db.alter(table: "issues") { t in t.add(column: "recurrence_interval", .integer) }
            try db.alter(table: "issues") { t in t.add(column: "recurrence_unit", .text) }
            try db.execute(sql: """
                INSERT INTO "issues"
                    ("id", "project_id", "creator_id", "title", "created_at", "updated_at")
                VALUES ('i1', 'p1', 'u1', 'keep me', '2026-01-01', '2026-01-01')
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertFalse(try columnNames(pool, "issues").contains("recurrence_interval"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("recurrence_unit"))
        // The dropped columns are in-place surgery — the row must survive.
        let surviving = try pool.read { db in
            try Int.fetchOne(db, sql: "SELECT COUNT(*) FROM issues")
        }
        XCTAssertEqual(surviving, 1)
    }

    // Idempotency: running the full migrator twice on the same file is a no-op,
    // never a duplicate-column throw.
    func testReMigrateIsIdempotent() throws {
        let pool = try makePool("twice")
        try DatabaseManager.runMigrations(on: pool)
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // The end-state schema must expose the tables + key columns sync writes to,
    // so a green migration can't silently produce the wrong shape.
    func testMigratedSchemaHasSyncTables() throws {
        let pool = try makePool("schema")
        try DatabaseManager.runMigrations(on: pool)
        for table in ["workspaces", "projects", "issues", "issue_labels",
                      "coding_sessions", "electric_offsets"] {
            let exists = try pool.read { db in try db.tableExists(table) }
            XCTAssertTrue(exists, "missing table \(table)")
        }
        XCTAssertTrue(try columnNames(pool, "issues").contains("duplicate_of_id"))
        XCTAssertTrue(try columnNames(pool, "issue_subscribers").contains("email"))
        // EXP-106: the releases feature is deleted — a fresh install must have
        // no `releases` table and no `release_id` on issues / coding_sessions.
        XCTAssertFalse(try pool.read { db in try db.tableExists("releases") })
        XCTAssertFalse(try columnNames(pool, "issues").contains("release_id"))
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("release_id"))
        // EXP-107: the recurrence feature is deleted — a fresh install must have
        // neither recurrence column on issues.
        XCTAssertFalse(try columnNames(pool, "issues").contains("recurrence_interval"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("recurrence_unit"))
        // coding_sessions.issue_id stays nullable (issueless batch sessions).
        let sessionIssueId = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "issue_id" }
        }
        XCTAssertNotNil(sessionIssueId)
        XCTAssertFalse(sessionIssueId?.isNotNull ?? true)
        // v4 project columns must be present after a full migration. `type` is
        // now dead server-side (EXP-129) but the local column survives as a
        // NOT-NULL-with-default("dev") relic — SQLite can't cheaply drop it and
        // inserts that omit it fall back to the default, so it stays harmless.
        let projectCols = try columnNames(pool, "projects")
        XCTAssertTrue(projectCols.contains("type"))
        XCTAssertTrue(projectCols.contains("public_show_comments"))
        XCTAssertTrue(projectCols.contains("public_show_activity"))
        // EXP-90: public_show_coding is gone from fresh installs.
        XCTAssertFalse(projectCols.contains("public_show_coding"))
        // v5 protection flag.
        XCTAssertTrue(projectCols.contains("is_protected"))
        // v9 public-board switch + curated icon (project-type collapse).
        XCTAssertTrue(projectCols.contains("is_public"))
        XCTAssertTrue(projectCols.contains("icon"))
        // v6: the invite bearer token is no longer synced (server allowlist),
        // so the local column must be nullable on every path.
        let inviteToken = try pool.read { db in
            try db.columns(in: "workspace_invites").first { $0.name == "token" }
        }
        XCTAssertNotNil(inviteToken)
        XCTAssertFalse(inviteToken?.isNotNull ?? true)
    }
}
