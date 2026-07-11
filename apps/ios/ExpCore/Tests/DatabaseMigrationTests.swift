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
        XCTAssertEqual(try appliedMigrations(pool).count, 7)
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
        XCTAssertEqual(try appliedMigrations(pool).count, 7)
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
        XCTAssertEqual(try appliedMigrations(pool).count, 7)
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
        XCTAssertEqual(try appliedMigrations(pool).count, 7)
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
        // v4 project-type columns must be present after a full migration.
        let projectCols = try columnNames(pool, "projects")
        XCTAssertTrue(projectCols.contains("type"))
        XCTAssertTrue(projectCols.contains("public_show_comments"))
        XCTAssertTrue(projectCols.contains("public_show_activity"))
        XCTAssertTrue(projectCols.contains("public_show_coding"))
        // v5 protection flag.
        XCTAssertTrue(projectCols.contains("is_protected"))
        // v6: the invite bearer token is no longer synced (server allowlist),
        // so the local column must be nullable on every path.
        let inviteToken = try pool.read { db in
            try db.columns(in: "workspace_invites").first { $0.name == "token" }
        }
        XCTAssertNotNil(inviteToken)
        XCTAssertFalse(inviteToken?.isNotNull ?? true)
    }
}
