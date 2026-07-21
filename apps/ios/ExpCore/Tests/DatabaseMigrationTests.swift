import GRDB
import XCTest
@testable import ExpCore

// Migration gate for the local sync cache (born as the regression gate for the
// iOS sync blackout — masterplan §9.1, EXP-12: a throwing migrator means
// `db.pool()` throws and sync never starts, with the failure only visible in
// os.Logger).
//
// EXP-180 (the great rename): the `-v5` file suffix wiped every previous local
// snapshot, so the migration list was collapsed back to a single `v1_initial`
// that creates the renamed tables (teams/boards/team_members/team_invites,
// team_id/board_id columns) directly. Additive columns added AFTER `-v5`
// stores shipped ride incremental guarded-ALTER steps again (the old v3…v6
// precedent) — v2_notification_team_id was the first, v3_team_invite_email
// (EXP-188) the second, v4_coding_session_needs_input (EXP-214) the third,
// v5_drop_user_is_agent + v6_issue_source_nullable_creator (issues.source /
// nullable creator_id, is_agent removal) the fourth/fifth.
// These tests pin the fresh-install schema and the
// exact migration identifiers so a new incremental migration is a conscious
// decision, not an accident.
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

    // A brand-new `-v5` DB must migrate green. The collapsed v1_initial plus
    // the additive post-`-v5` steps — the old v2…v11 incrementals died with
    // the `-v4` file.
    func testFreshInstallMigratesGreen() throws {
        let pool = try makePool("fresh")
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
        XCTAssertEqual(
            try appliedMigrations(pool),
            ["v1_initial", "v2_notification_team_id", "v3_team_invite_email",
             "v4_coding_session_needs_input", "v5_drop_user_is_agent",
             "v6_issue_source_nullable_creator"]
        )
    }

    // Idempotency: running the full migrator twice on the same file is a no-op,
    // never a duplicate-column throw.
    func testReMigrateIsIdempotent() throws {
        let pool = try makePool("twice")
        try DatabaseManager.runMigrations(on: pool)
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
        XCTAssertEqual(
            try appliedMigrations(pool),
            ["v1_initial", "v2_notification_team_id", "v3_team_invite_email",
             "v4_coding_session_needs_input", "v5_drop_user_is_agent",
             "v6_issue_source_nullable_creator"]
        )
    }

    // v2 (EXP-180 helpdesk follow-up): a `-v5` store created before
    // notifications.team_id existed must gain the column via the guarded ALTER
    // and get its notifications shape offset reset so already-synced rows
    // re-snapshot with the new column (the old invite-token test's playbook:
    // hand-build the pre-migration state, then run the full migrator).
    func testNotificationTeamIdAddedToExistingV5Store() throws {
        let pool = try makePool("notif-team-id")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v1_initial")
        try pool.write { db in
            // Hand-build the pre-v2 state: notifications without team_id
            // (today's v1 create already declares it — that overlap is exactly
            // what the guarded ALTER has to tolerate) + a live offset row.
            try db.drop(table: "notifications")
            try db.create(table: "notifications") { t in
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
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('notifications', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertEqual(
            try appliedMigrations(pool),
            ["v1_initial", "v2_notification_team_id", "v3_team_invite_email",
             "v4_coding_session_needs_input", "v5_drop_user_is_agent",
             "v6_issue_source_nullable_creator"]
        )
        let teamIdColumn = try pool.read { db in
            try db.columns(in: "notifications").first { $0.name == "team_id" }
        }
        XCTAssertNotNil(teamIdColumn)
        XCTAssertFalse(teamIdColumn?.isNotNull ?? true)
        // The ALTER must force a refetch of the notifications shape.
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'notifications'
                    """
            )
        }
        let handle: String? = offset?["handle"]
        let offsetValue: String? = offset?["offset"]
        let needsRefetch: Bool? = offset?["needs_refetch"]
        let isLive: Bool? = offset?["is_live"]
        XCTAssertEqual(handle, "")
        XCTAssertEqual(offsetValue, "-1")
        XCTAssertEqual(needsRefetch, true)
        XCTAssertEqual(isLive, false)
    }

    // v3 (EXP-188 invite-by-email): a `-v5` store created before
    // team_invites.email existed must gain the column via the guarded ALTER
    // and get its team-invites shape offset reset (the shape key is
    // 'team-invites' WITH A DASH — the proxy route name, not the table name).
    func testTeamInviteEmailAddedToExistingV5Store() throws {
        let pool = try makePool("invite-email")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v2_notification_team_id")
        try pool.write { db in
            // Hand-build the pre-v3 state: team_invites without email
            // (today's v1 create already declares it — that overlap is exactly
            // what the guarded ALTER has to tolerate) + a live offset row.
            try db.drop(table: "team_invites")
            try db.create(table: "team_invites") { t in
                t.primaryKey("id", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("role", .text).notNull()
                t.column("token", .text).indexed()
                t.column("expires_at", .text).notNull()
                t.column("accepted_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('team-invites', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertEqual(
            try appliedMigrations(pool),
            ["v1_initial", "v2_notification_team_id", "v3_team_invite_email",
             "v4_coding_session_needs_input", "v5_drop_user_is_agent",
             "v6_issue_source_nullable_creator"]
        )
        let emailColumn = try pool.read { db in
            try db.columns(in: "team_invites").first { $0.name == "email" }
        }
        XCTAssertNotNil(emailColumn)
        XCTAssertFalse(emailColumn?.isNotNull ?? true)
        // The ALTER must force a refetch of the team-invites shape.
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'team-invites'
                    """
            )
        }
        let handle: String? = offset?["handle"]
        let offsetValue: String? = offset?["offset"]
        let needsRefetch: Bool? = offset?["needs_refetch"]
        let isLive: Bool? = offset?["is_live"]
        XCTAssertEqual(handle, "")
        XCTAssertEqual(offsetValue, "-1")
        XCTAssertEqual(needsRefetch, true)
        XCTAssertEqual(isLive, false)
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
        // issues.source ('user'|'widget') + a nullable creator_id (a
        // widget-sourced issue has no human creator).
        XCTAssertTrue(try columnNames(pool, "issues").contains("source"))
        let issueCreatorId = try pool.read { db in
            try db.columns(in: "issues").first { $0.name == "creator_id" }
        }
        XCTAssertNotNil(issueCreatorId)
        XCTAssertFalse(issueCreatorId?.isNotNull ?? true)
        // users.is_agent was removed with the synced 6-column users shape.
        XCTAssertFalse(try columnNames(pool, "users").contains("is_agent"))
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

        // The public-board columns (and the legacy `type` relic) are gone.
        let boardCols = try columnNames(pool, "boards")
        XCTAssertFalse(boardCols.contains("type"))
        XCTAssertFalse(boardCols.contains("public_show_comments"))
        XCTAssertFalse(boardCols.contains("public_show_activity"))
        XCTAssertFalse(boardCols.contains("is_public"))
        XCTAssertFalse(boardCols.contains("public_show_coding"))
        XCTAssertTrue(boardCols.contains("is_protected"))
        XCTAssertTrue(boardCols.contains("icon"))
        // The team-level helpdesk switch (EXP-180 Support inbox) IS stored —
        // the teams shape serves it and the Support segment gates on it.
        XCTAssertTrue(try columnNames(pool, "teams").contains("helpdesk_enabled"))
        // notifications.team_id (nullable): set on issue-less support_reply
        // rows so the inbox can group them per team.
        let notifTeamId = try pool.read { db in
            try db.columns(in: "notifications").first { $0.name == "team_id" }
        }
        XCTAssertNotNil(notifTeamId)
        XCTAssertFalse(notifTeamId?.isNotNull ?? true)

        // The invite bearer token is not synced (server allowlist), so the
        // local column must be nullable.
        let inviteToken = try pool.read { db in
            try db.columns(in: "team_invites").first { $0.name == "token" }
        }
        XCTAssertNotNil(inviteToken)
        XCTAssertFalse(inviteToken?.isNotNull ?? true)

        // team_invites.email (nullable, EXP-188): set when the invite was
        // sent by email; rides the team-invites shape for the pending list.
        let inviteEmail = try pool.read { db in
            try db.columns(in: "team_invites").first { $0.name == "email" }
        }
        XCTAssertNotNil(inviteEmail)
        XCTAssertFalse(inviteEmail?.isNotNull ?? true)
    }

    // The `-v5` canonical file name + the legacy-file purge list are the wipe
    // mechanism for the rename — pin the suffix so a stray edit can't silently
    // strand every device on the old snapshot.
    func testFileURLUsesV5Suffix() throws {
        let url = try DatabaseManager.fileURL(for: "acct")
        XCTAssertEqual(url.lastPathComponent, "exponential-acct-v5.sqlite")
    }
}
