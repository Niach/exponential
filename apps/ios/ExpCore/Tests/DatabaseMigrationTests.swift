import GRDB
import XCTest
@testable import ExpCore

// Migration gate for the local sync cache (born as the regression gate for the
// iOS sync blackout — masterplan §9.1, EXP-12: a throwing migrator means
// `db.pool()` throws and sync never starts, with the failure only visible in
// os.Logger).
//
// EXP-180 (the great rename): the `-v5` file suffix wiped every previous local
// snapshot, so the migration list is collapsed back to a single `v1_initial`
// that creates the renamed tables (teams/boards/team_members/team_invites,
// team_id/board_id columns) directly. There is deliberately NO upgrade path —
// these tests pin the fresh-install schema and the migration count so a
// reintroduced incremental migration is a conscious decision, not an accident.
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

    // A brand-new `-v5` DB must migrate green, and the collapsed migrator is
    // exactly ONE migration — the v2…v11 incrementals died with the `-v4` file.
    func testFreshInstallMigratesGreen() throws {
        let pool = try makePool("fresh")
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
        XCTAssertEqual(try appliedMigrations(pool), ["v1_initial"])
    }

    // Idempotency: running the full migrator twice on the same file is a no-op,
    // never a duplicate-column throw.
    func testReMigrateIsIdempotent() throws {
        let pool = try makePool("twice")
        try DatabaseManager.runMigrations(on: pool)
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
        XCTAssertEqual(try appliedMigrations(pool), ["v1_initial"])
    }

    // The end-state schema must expose the tables + key columns sync writes to,
    // so a green migration can't silently produce the wrong shape. This pins
    // the EXP-180 rename: teams/boards/team_members/team_invites exist, the
    // workspace/project-era names do NOT.
    func testMigratedSchemaHasSyncTables() throws {
        let pool = try makePool("schema")
        try DatabaseManager.runMigrations(on: pool)
        for table in ["teams", "boards", "issues", "labels", "issue_labels",
                      "users", "team_members", "team_invites", "comments",
                      "attachments", "notifications", "issue_subscribers",
                      "issue_events", "coding_sessions", "electric_offsets"] {
            let exists = try pool.read { db in try db.tableExists(table) }
            XCTAssertTrue(exists, "missing table \(table)")
        }
        // The renamed-away tables must be gone on a fresh install.
        for table in ["workspaces", "projects", "workspace_members", "workspace_invites", "releases"] {
            let exists = try pool.read { db in try db.tableExists(table) }
            XCTAssertFalse(exists, "legacy table \(table) must not exist")
        }

        // electric_offsets carries the 409-refetch persistence + live gating
        // flags directly in the collapsed create.
        let offsetCols = try columnNames(pool, "electric_offsets")
        XCTAssertTrue(offsetCols.contains("needs_refetch"))
        XCTAssertTrue(offsetCols.contains("is_live"))

        // Renamed FK columns: board_id/team_id everywhere the wire has them.
        XCTAssertTrue(try columnNames(pool, "issues").contains("board_id"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("project_id"))
        for table in ["boards", "labels", "issue_labels", "team_members", "team_invites",
                      "comments", "attachments", "issue_subscribers", "issue_events",
                      "coding_sessions"] {
            let cols = try columnNames(pool, table)
            XCTAssertTrue(cols.contains("team_id"), "\(table) missing team_id")
            XCTAssertFalse(cols.contains("workspace_id"), "\(table) still has workspace_id")
        }
        XCTAssertTrue(try columnNames(pool, "coding_sessions").contains("board_id"))
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("project_id"))

        XCTAssertTrue(try columnNames(pool, "issues").contains("duplicate_of_id"))
        XCTAssertTrue(try columnNames(pool, "issue_subscribers").contains("email"))
        // The deleted releases/recurrence features never existed in this schema.
        XCTAssertFalse(try columnNames(pool, "issues").contains("release_id"))
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("release_id"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("recurrence_interval"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("recurrence_unit"))
        // coding_sessions.issue_id stays nullable (issueless batch sessions).
        let sessionIssueId = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "issue_id" }
        }
        XCTAssertNotNil(sessionIssueId)
        XCTAssertFalse(sessionIssueId?.isNotNull ?? true)

        // The public-board columns (and the legacy `type` relic) are gone;
        // helpdesk_enabled is deliberately NOT stored yet (a later stage adds
        // it — the tolerant apply path drops the unknown wire column).
        let boardCols = try columnNames(pool, "boards")
        XCTAssertFalse(boardCols.contains("type"))
        XCTAssertFalse(boardCols.contains("public_show_comments"))
        XCTAssertFalse(boardCols.contains("public_show_activity"))
        XCTAssertFalse(boardCols.contains("is_public"))
        XCTAssertFalse(boardCols.contains("public_show_coding"))
        XCTAssertTrue(boardCols.contains("is_protected"))
        XCTAssertTrue(boardCols.contains("icon"))
        XCTAssertFalse(try columnNames(pool, "teams").contains("helpdesk_enabled"))

        // The invite bearer token is not synced (server allowlist), so the
        // local column must be nullable.
        let inviteToken = try pool.read { db in
            try db.columns(in: "team_invites").first { $0.name == "token" }
        }
        XCTAssertNotNil(inviteToken)
        XCTAssertFalse(inviteToken?.isNotNull ?? true)
    }

    // The `-v5` canonical file name + the legacy-file purge list are the wipe
    // mechanism for the rename — pin the suffix so a stray edit can't silently
    // strand every device on the old snapshot.
    func testFileURLUsesV5Suffix() throws {
        let url = try DatabaseManager.fileURL(for: "acct")
        XCTAssertEqual(url.lastPathComponent, "exponential-acct-v5.sqlite")
    }
}
