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
// nullable creator_id, is_agent removal) the fourth/fifth,
// v7_drop_board_dead_columns (REV2-91: boards.github_repo/preview_config)
// the sixth, v8_coding_session_action_fields (EXP-253) the seventh,
// v9_actions (EXP-268: the synced actions table, 15th shape) the eighth,
// v10_action_icon (EXP-273) the ninth, v11_drop_archived_at (REV2-103:
// archiving deleted from the product) the tenth, and v12_drop_issue_times
// (REV2-103/REV2-49: issue time-of-day deleted from the product — due DATE
// stays) the eleventh, v13_issue_statuses (EXP-314: the synced
// issue_statuses table, 16th shape, + issues.status_id) the twelfth, and
// v14_drop_board_is_protected (EXP-364: protected boards deleted from the
// product) the thirteenth, and v15_attachment_nullable_uploader (REV-7:
// attachments.uploader_id is nullable server-side — a table rebuild, the v6
// precedent, since SQLite can't relax a NOT NULL via ALTER) the fourteenth,
// and v16_devices_worktrees (EXP-481: the synced devices + device_worktrees
// tables, shapes 17/18) the fifteenth, v17_coding_session_branch (EXP-545:
// the batch↔PR head branch ride-along) the sixteenth, and
// v18_action_automations (EXP-530: actions.trigger +
// coding_sessions.started_reason) the seventeenth, and
// v19_coding_session_device_id (EXP-549/550: the session's host-machine
// deviceId ride-along) the eighteenth, and v20_automations (EXP-583:
// automations became their own entity — the synced `automations` table, 19th
// shape, + coding_sessions.automation_id) the nineteenth.
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
             "v6_issue_source_nullable_creator", "v7_drop_board_dead_columns",
             "v8_coding_session_action_fields", "v9_actions", "v10_action_icon",
             "v11_drop_archived_at", "v12_drop_issue_times",
             "v13_issue_statuses", "v14_drop_board_is_protected",
             "v15_attachment_nullable_uploader", "v16_devices_worktrees",
             "v17_coding_session_branch", "v18_action_automations",
             "v19_coding_session_device_id", "v20_automations"]
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
             "v6_issue_source_nullable_creator", "v7_drop_board_dead_columns",
             "v8_coding_session_action_fields", "v9_actions", "v10_action_icon",
             "v11_drop_archived_at", "v12_drop_issue_times",
             "v13_issue_statuses", "v14_drop_board_is_protected",
             "v15_attachment_nullable_uploader", "v16_devices_worktrees",
             "v17_coding_session_branch", "v18_action_automations",
             "v19_coding_session_device_id", "v20_automations"]
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
             "v6_issue_source_nullable_creator", "v7_drop_board_dead_columns",
             "v8_coding_session_action_fields", "v9_actions", "v10_action_icon",
             "v11_drop_archived_at", "v12_drop_issue_times",
             "v13_issue_statuses", "v14_drop_board_is_protected",
             "v15_attachment_nullable_uploader", "v16_devices_worktrees",
             "v17_coding_session_branch", "v18_action_automations",
             "v19_coding_session_device_id", "v20_automations"]
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
             "v6_issue_source_nullable_creator", "v7_drop_board_dead_columns",
             "v8_coding_session_action_fields", "v9_actions", "v10_action_icon",
             "v11_drop_archived_at", "v12_drop_issue_times",
             "v13_issue_statuses", "v14_drop_board_is_protected",
             "v15_attachment_nullable_uploader", "v16_devices_worktrees",
             "v17_coding_session_branch", "v18_action_automations",
             "v19_coding_session_device_id", "v20_automations"]
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

    // v7 (REV2-91): a `-v5` store created while the dead JSONB-era board
    // columns still existed must lose them via the guarded drop (today's v1
    // create no longer declares them — hand-add them to model the old state).
    func testDeadBoardColumnsDroppedFromExistingV5Store() throws {
        let pool = try makePool("board-dead-cols")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v6_issue_source_nullable_creator")
        try pool.write { db in
            try db.alter(table: "boards") { t in
                t.add(column: "github_repo", .text)
                t.add(column: "preview_config", .text)
            }
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        let boardCols = try columnNames(pool, "boards")
        XCTAssertFalse(boardCols.contains("github_repo"))
        XCTAssertFalse(boardCols.contains("preview_config"))
    }

    // v12 (REV2-103/REV2-49): a `-v5` store created while the issue
    // time-of-day columns still existed must lose them via the guarded drop
    // (today's v1 create no longer declares them — hand-add them to model the
    // old state). `due_date` must survive: only the times are deleted.
    func testIssueTimeColumnsDroppedFromExistingV5Store() throws {
        let pool = try makePool("issue-times")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v11_drop_archived_at")
        try pool.write { db in
            try db.alter(table: "issues") { t in
                t.add(column: "due_time", .text)
                t.add(column: "end_time", .text)
            }
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        let issueCols = try columnNames(pool, "issues")
        XCTAssertFalse(issueCols.contains("due_time"))
        XCTAssertFalse(issueCols.contains("end_time"))
        XCTAssertTrue(issueCols.contains("due_date"))
    }

    // v14 (EXP-364): a `-v5` store created while `boards.is_protected` still
    // existed must lose it via the guarded drop (today's v1 create no longer
    // declares it — hand-add it to model the old state). The other board
    // columns must survive.
    func testBoardIsProtectedDroppedFromExistingV5Store() throws {
        let pool = try makePool("board-is-protected")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v13_issue_statuses")
        try pool.write { db in
            try db.alter(table: "boards") { t in
                t.add(column: "is_protected", .boolean).notNull().defaults(to: false)
            }
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        let boardCols = try columnNames(pool, "boards")
        XCTAssertFalse(boardCols.contains("is_protected"))
        XCTAssertTrue(boardCols.contains("icon"))
        XCTAssertTrue(boardCols.contains("repository_id"))
    }

    // v15 (REV-7): a `-v5` store created while `attachments.uploader_id` was
    // still NOT NULL must be rebuilt nullable, keep its rows, and get the
    // attachments shape offset reset (the widget-screenshot inserts it dropped
    // only come back on a full re-snapshot). Today's v1 create already makes the
    // column nullable — hand-build the constrained table to model the old state.
    func testAttachmentUploaderRelaxedInExistingV5Store() throws {
        let pool = try makePool("attachment-uploader")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v14_drop_board_is_protected")
        try pool.write { db in
            try db.drop(table: "attachments")
            try db.create(table: "attachments") { t in
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
                t.column("width", .integer)
                t.column("height", .integer)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
            try db.execute(sql: """
                INSERT INTO "attachments" (
                    "id", "team_id", "issue_id", "uploader_id", "filename",
                    "content_type", "size_bytes", "storage_key", "url",
                    "created_at", "updated_at"
                )
                VALUES ('a1', 'w1', 'i1', 'u1', 'shot.png', 'image/png', 12,
                        'k1', '/api/attachments/a1',
                        '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
                """)
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('attachments', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        let uploader = try pool.read { db in
            try db.columns(in: "attachments").first { $0.name == "uploader_id" }
        }
        XCTAssertNotNil(uploader)
        XCTAssertFalse(uploader?.isNotNull ?? true)
        // The rebuild copies rows, it doesn't drop them.
        let existing = try pool.read { db in
            try String.fetchOne(db, sql: "SELECT \"filename\" FROM \"attachments\" WHERE \"id\" = 'a1'")
        }
        XCTAssertEqual(existing, "shot.png")
        // A null uploader now persists instead of throwing.
        XCTAssertNoThrow(try pool.write { db in
            try db.execute(sql: """
                INSERT INTO "attachments" (
                    "id", "team_id", "issue_id", "uploader_id", "filename",
                    "content_type", "size_bytes", "storage_key", "url",
                    "created_at", "updated_at"
                )
                VALUES ('a2', 'w1', 'i1', NULL, 'widget.png', 'image/png', 34,
                        'k2', '/api/attachments/a2',
                        '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
                """)
        })
        // The rebuild must force a re-snapshot of the attachments shape.
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'attachments'
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

    // v8 (EXP-253 actions): a `-v5` store created before the action columns
    // existed must gain both via the guarded ALTERs and get its
    // coding-sessions shape offset reset (the shape key is 'coding-sessions'
    // WITH A DASH — the proxy route name, not the table name).
    func testCodingSessionActionFieldsAddedToExistingV5Store() throws {
        let pool = try makePool("session-action-fields")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v7_drop_board_dead_columns")
        try pool.write { db in
            // Hand-build the pre-v8 state: coding_sessions without the action
            // columns (today's v1 create already declares them — that overlap
            // is exactly what the guarded ALTERs have to tolerate) + a live
            // offset row.
            try db.drop(table: "coding_sessions")
            try db.create(table: "coding_sessions") { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).indexed()
                t.column("board_id", .text)
                t.column("team_id", .text).notNull().indexed()
                t.column("user_id", .text).notNull().indexed()
                t.column("device_label", .text)
                t.column("status", .text).notNull().defaults(to: "running")
                t.column("needs_input", .boolean).notNull().defaults(to: false)
                t.column("started_at", .text).notNull()
                t.column("ended_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        let sessionCols = try pool.read { db in try db.columns(in: "coding_sessions") }
        for column in ["action_id", "action_name"] {
            let added = sessionCols.first { $0.name == column }
            XCTAssertNotNil(added, "missing column \(column)")
            XCTAssertFalse(added?.isNotNull ?? true)
        }
        // The ALTERs must force a refetch of the coding-sessions shape.
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'coding-sessions'
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

    // v18 (EXP-530 action automations): a store created before
    // `actions.trigger` / `coding_sessions.started_reason` existed must gain
    // both via the guarded ALTERs and get BOTH shape offsets reset (the
    // coding-sessions key has A DASH — the proxy route name, not the table
    // name) so already-synced rows re-arrive with the new columns.
    func testActionAutomationColumnsAddedToExistingStore() throws {
        let pool = try makePool("action-automations")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v17_coding_session_branch")
        try pool.write { db in
            // Hand-build the pre-v18 state: drop the columns the earlier
            // creates already declare (that overlap is exactly what the
            // guarded ALTERs have to tolerate) + live offset rows.
            let actionCols = Set(try db.columns(in: "actions").map(\.name))
            if actionCols.contains("trigger") {
                try db.alter(table: "actions") { t in t.drop(column: "trigger") }
            }
            let sessionCols = Set(try db.columns(in: "coding_sessions").map(\.name))
            if sessionCols.contains("started_reason") {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "started_reason") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('actions', 'h', '0_0', 0, 1),
                       ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertTrue(try columnNames(pool, "actions").contains("trigger"))
        XCTAssertTrue(try columnNames(pool, "coding_sessions").contains("started_reason"))
        // The ALTERs must force a refetch of BOTH shapes.
        for shape in ["actions", "coding-sessions"] {
            let offset = try pool.read { db in
                try Row.fetchOne(
                    db,
                    sql: """
                        SELECT "handle", "offset", "needs_refetch", "is_live"
                        FROM "electric_offsets" WHERE "shape" = '\(shape)'
                        """
                )
            }
            let handle: String? = offset?["handle"]
            let offsetValue: String? = offset?["offset"]
            let needsRefetch: Bool? = offset?["needs_refetch"]
            let isLive: Bool? = offset?["is_live"]
            XCTAssertEqual(handle, "", "shape \(shape)")
            XCTAssertEqual(offsetValue, "-1", "shape \(shape)")
            XCTAssertEqual(needsRefetch, true, "shape \(shape)")
            XCTAssertEqual(isLive, false, "shape \(shape)")
        }
    }

    // v20 (EXP-583 automations): a store created before the `automations`
    // table and `coding_sessions.automation_id` existed must gain both, and
    // get the coding-sessions shape offset reset for the new column (a
    // brand-new shape has no offset row, so `automations` needs none).
    func testAutomationsTableAddedToExistingStore() throws {
        let pool = try makePool("automations")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v19_coding_session_device_id")
        try pool.write { db in
            // Hand-build the pre-v20 state: no automations table, no
            // automation_id column (the guarded steps have to tolerate both
            // the missing and the already-present form) + a live offset row.
            if try db.tableExists("automations") {
                try db.drop(table: "automations")
            }
            let sessionCols = Set(try db.columns(in: "coding_sessions").map(\.name))
            if sessionCols.contains("automation_id") {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "automation_id") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertTrue(try pool.read { db in try db.tableExists("automations") })
        XCTAssertEqual(
            try columnNames(pool, "automations"),
            ["id", "team_id", "action_id", "device_id", "enabled", "trigger",
             "agent", "model", "effort", "sort_order", "created_at", "updated_at"]
        )
        XCTAssertTrue(try columnNames(pool, "coding_sessions").contains("automation_id"))
        let automationIdColumn = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "automation_id" }
        }
        XCTAssertFalse(automationIdColumn?.isNotNull ?? true)
        // The ALTER must force a refetch of the coding-sessions shape.
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'coding-sessions'
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

    // v19 (EXP-549/550 session host device): a store created before
    // `coding_sessions.device_id` existed must gain it via the guarded ALTER
    // and get the coding-sessions shape offset reset (the key has A DASH —
    // the proxy route name, not the table name) so already-synced rows
    // re-arrive carrying the host machine's deviceId.
    func testCodingSessionDeviceIdAddedToExistingStore() throws {
        let pool = try makePool("session-device-id")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v18_action_automations")
        try pool.write { db in
            // Hand-build the pre-v19 state: drop the column today's v1 create
            // already declares (that overlap is exactly what the guarded
            // ALTER has to tolerate) + a live offset row.
            let sessionCols = Set(try db.columns(in: "coding_sessions").map(\.name))
            if sessionCols.contains("device_id") {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "device_id") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertTrue(try columnNames(pool, "coding_sessions").contains("device_id"))
        let deviceIdColumn = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "device_id" }
        }
        XCTAssertFalse(deviceIdColumn?.isNotNull ?? true)
        // The ALTER must force a refetch of the coding-sessions shape.
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'coding-sessions'
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
                      "issue_events", "coding_sessions", "actions",
                      "automations", "issue_statuses", "electric_offsets"] {
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
        // Due date is date-only (REV2-49): the time-of-day columns are deleted
        // from the product, `due_date` stays.
        XCTAssertTrue(try columnNames(pool, "issues").contains("due_date"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("due_time"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("end_time"))
        // coding_sessions.issue_id stays nullable (issueless batch sessions).
        let sessionIssueId = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "issue_id" }
        }
        XCTAssertNotNil(sessionIssueId)
        XCTAssertFalse(sessionIssueId?.isNotNull ?? true)
        // Action run linkage (EXP-253): both nullable ride-alongs exist.
        let sessionCols = try columnNames(pool, "coding_sessions")
        XCTAssertTrue(sessionCols.contains("action_id"))
        XCTAssertTrue(sessionCols.contains("action_name"))
        // EXP-549/550: the host machine's steer deviceId ride-along.
        XCTAssertTrue(sessionCols.contains("device_id"))
        // EXP-583: which automation fired the run.
        XCTAssertTrue(sessionCols.contains("automation_id"))

        // The synced actions table (EXP-268, 15th shape) deliberately has NO
        // body column — the shape's allowlist excludes the ≤64KB prompt; tRPC
        // `actions.get` stays the only body path.
        let actionCols = try columnNames(pool, "actions")
        XCTAssertTrue(actionCols.contains("team_id"))
        XCTAssertTrue(actionCols.contains("inputs"))
        XCTAssertFalse(actionCols.contains("body"))

        // EXP-583: automations are their own table (19th shape) — the trigger
        // is the when-part only, device/enabled/agent are columns here.
        let automationCols = try columnNames(pool, "automations")
        XCTAssertTrue(automationCols.contains("team_id"))
        XCTAssertTrue(automationCols.contains("action_id"))
        XCTAssertTrue(automationCols.contains("device_id"))
        XCTAssertTrue(automationCols.contains("enabled"))
        XCTAssertTrue(automationCols.contains("trigger"))

        // Custom issue statuses (EXP-314, 16th shape): the team-scoped table
        // plus the nullable `status_id` on issues. `issues.status` STAYS as the
        // dual-written builtin anchor.
        let statusCols = try columnNames(pool, "issue_statuses")
        XCTAssertTrue(statusCols.contains("team_id"))
        XCTAssertTrue(statusCols.contains("category"))
        XCTAssertTrue(statusCols.contains("builtin_key"))
        XCTAssertTrue(statusCols.contains("sort_order"))
        XCTAssertTrue(try columnNames(pool, "issues").contains("status"))
        let issueStatusId = try pool.read { db in
            try db.columns(in: "issues").first { $0.name == "status_id" }
        }
        XCTAssertNotNil(issueStatusId)
        XCTAssertFalse(issueStatusId?.isNotNull ?? true)

        // The public-board columns (and the legacy `type` relic) are gone.
        let boardCols = try columnNames(pool, "boards")
        XCTAssertFalse(boardCols.contains("type"))
        XCTAssertFalse(boardCols.contains("public_show_comments"))
        XCTAssertFalse(boardCols.contains("public_show_activity"))
        XCTAssertFalse(boardCols.contains("is_public"))
        XCTAssertFalse(boardCols.contains("public_show_coding"))
        // Protected boards are gone from the product (EXP-364) — the boards
        // shape no longer carries the flag, so the cache must not either.
        XCTAssertFalse(boardCols.contains("is_protected"))
        XCTAssertTrue(boardCols.contains("icon"))
        // The JSONB-era board columns are gone (REV2-91): repos live in the
        // server-only registry and the releases-era preview feature is deleted.
        XCTAssertFalse(boardCols.contains("github_repo"))
        XCTAssertFalse(boardCols.contains("preview_config"))
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

        // attachments.uploader_id (nullable, REV-7): the server column is
        // nullable — widget screenshots have no human uploader and the FK is ON
        // DELETE SET NULL — and the shape allowlist carries it, so a NOT NULL
        // here drops inserts and stalls the shape on a SET NULL update.
        let attachmentUploader = try pool.read { db in
            try db.columns(in: "attachments").first { $0.name == "uploader_id" }
        }
        XCTAssertNotNil(attachmentUploader)
        XCTAssertFalse(attachmentUploader?.isNotNull ?? true)
    }

    // v16 (EXP-481): the synced devices + device_worktrees tables exist on a
    // fresh install with the columns the shape allowlists deliver, and `busy`
    // is a BOOLEAN column so the partial-update wire-bool mapping engages.
    func testDeviceTablesCreatedWithBooleanBusy() throws {
        let pool = try makePool("devices")
        try DatabaseManager.runMigrations(on: pool)
        let deviceCols = try columnNames(pool, "devices")
        XCTAssertTrue(deviceCols.isSuperset(of: [
            "id", "user_id", "device_id", "label", "kind", "platform",
            "version", "agents", "caps", "unauthed_agents", "launch_defaults",
            "launch_defaults_updated_at", "active_sessions", "last_seen_at",
            "shared_team_id", "update_requested_at", "created_at", "updated_at",
        ]))
        let worktreeCols = try columnNames(pool, "device_worktrees")
        XCTAssertTrue(worktreeCols.isSuperset(of: [
            "id", "device_row_id", "repo_full_name", "branch",
            "issue_identifier", "agents", "dirty", "busy", "reported_at",
        ]))
        let busy = try pool.read { db in
            try db.columns(in: "device_worktrees").first { $0.name == "busy" }
        }
        XCTAssertTrue(busy?.type.uppercased().contains("BOOL") ?? false)
    }

    // The `-v5` canonical file name + the legacy-file purge list are the wipe
    // mechanism for the rename — pin the suffix so a stray edit can't silently
    // strand every device on the old snapshot.
    func testFileURLUsesV5Suffix() throws {
        let url = try DatabaseManager.fileURL(for: "acct")
        XCTAssertEqual(url.lastPathComponent, "exponential-acct-v5.sqlite")
    }
}
