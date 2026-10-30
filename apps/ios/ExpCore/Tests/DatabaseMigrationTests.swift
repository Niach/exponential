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
// shape, + coding_sessions.automation_id) the nineteenth, and
// v21_device_is_default (EXP-622: devices.is_default, the owner's default
// machine every device picker prefills) the twentieth, and
// v22_coding_session_outcome (EXP-637: the agent's own close-out —
// coding_sessions.summary/ended_by + resumed_from_id) the
// twenty-first, and v23_agent_status (EXP-484: the machine's per-agent auth
// status + rate-limit usage on the devices shape, and the agent a run was
// launched with on the coding-sessions shape) the twenty-second, and
// v24_drop_coding_session_outcome (EXP-686: the self-reported run outcome is
// gone server-side, so the local column goes too) the twenty-third, and
// v25_board_default_branch (EXP-712: a board's own branch — what its coding
// sessions start from — on the boards shape) the twenty-fourth.
// These tests pin the fresh-install schema and the
// exact migration identifiers so a new incremental migration is a conscious
// decision, not an accident.
final class DatabaseMigrationTests: XCTestCase {
    private var tempDir: URL!

    /// The last migration before v68 dropped `coding_sessions` / `threads` /
    /// `thread_items` and the issue PR + action trigger columns — the tests of
    /// the migrations that shaped them stop there.
    private static let preSessionTree = "v67_issue_draft_thread"

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
             "v19_coding_session_device_id", "v20_automations",
             "v21_device_is_default", "v22_coding_session_outcome",
             "v23_agent_status", "v24_drop_coding_session_outcome",
             "v25_board_default_branch", "v26_coding_session_pr",
             "v27_issue_relations", "v28_comment_threads",
             "v29_device_acp_agents", "v30_coding_session_blocked",
             "v31_coding_session_parent", "v32_action_prompt_placeholder",
             "v33_pins", "v34_device_shared_team_ids",
             "v35_attachment_video_metadata",
             "v36_coding_session_agent_busy",
             "v37_coding_session_agent_caption",
             "v38_drop_coding_session_summary",
             "v39_issue_drafts",
             "v40_issue_pr_base_branch",
             "v41_coding_session_results",
             "v42_coding_session_batch_issue_ids",
             "v43_coding_session_agent_account",
             "v44_coding_session_agent_title",
             "v45_device_icon",
             "v46_notification_session_id",
             "v47_workflows",
             "v48_workflow_node_approval",
             "v49_workflow_node_checkpoint",
             "v50_workflow_node_review",
             "v51_automation_account",
             "v52_issue_estimate",
             "v53_invite_placeholder", "v54_invite_sent_at",
             "v55_workflow_session_membership_events", "v56_workflow_start_on_dropped",
             "v57_team_yolo_mode", "v58_workflows_gate_dropped",
             "v59_action_triggers", "v60_drop_workflows_and_stacks",
             "v61_one_path", "v62_device_doctor",
             "v63_issue_draft_id", "v64_attachment_as_file",
             "v65_trigger_id", "v66_threads",
             "v67_issue_draft_thread", "v68_session_tree",
             "v69_session_dispatch", "v70_session_talk", "v71_session_issue_assignee",
             "v72_session_move_to_device", "v73_issue_identifier_upper_index",
             "v74_agents_events_prompts", "v75_conversations", "v76_apps",
             "v77_message_conversation", "v78_drop_actions_and_legacy_session_columns",
             "v79_app_records", "v80_agent_actors", "v81_conversation_digest", "v82_apps_granted_tools",
             "v83_session_background", "v84_session_read_archived_at", "v85_agent_role",
             "v86_session_read_story_seen_at", "v87_session_seal"]
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
             "v19_coding_session_device_id", "v20_automations",
             "v21_device_is_default", "v22_coding_session_outcome",
             "v23_agent_status", "v24_drop_coding_session_outcome",
             "v25_board_default_branch", "v26_coding_session_pr",
             "v27_issue_relations", "v28_comment_threads",
             "v29_device_acp_agents", "v30_coding_session_blocked",
             "v31_coding_session_parent", "v32_action_prompt_placeholder",
             "v33_pins", "v34_device_shared_team_ids",
             "v35_attachment_video_metadata",
             "v36_coding_session_agent_busy",
             "v37_coding_session_agent_caption",
             "v38_drop_coding_session_summary",
             "v39_issue_drafts",
             "v40_issue_pr_base_branch",
             "v41_coding_session_results",
             "v42_coding_session_batch_issue_ids",
             "v43_coding_session_agent_account",
             "v44_coding_session_agent_title",
             "v45_device_icon",
             "v46_notification_session_id",
             "v47_workflows",
             "v48_workflow_node_approval",
             "v49_workflow_node_checkpoint",
             "v50_workflow_node_review",
             "v51_automation_account",
             "v52_issue_estimate",
             "v53_invite_placeholder", "v54_invite_sent_at",
             "v55_workflow_session_membership_events", "v56_workflow_start_on_dropped",
             "v57_team_yolo_mode", "v58_workflows_gate_dropped",
             "v59_action_triggers", "v60_drop_workflows_and_stacks",
             "v61_one_path", "v62_device_doctor",
             "v63_issue_draft_id", "v64_attachment_as_file",
             "v65_trigger_id", "v66_threads",
             "v67_issue_draft_thread", "v68_session_tree",
             "v69_session_dispatch", "v70_session_talk", "v71_session_issue_assignee",
             "v72_session_move_to_device", "v73_issue_identifier_upper_index",
             "v74_agents_events_prompts", "v75_conversations", "v76_apps",
             "v77_message_conversation", "v78_drop_actions_and_legacy_session_columns",
             "v79_app_records", "v80_agent_actors", "v81_conversation_digest", "v82_apps_granted_tools",
             "v83_session_background", "v84_session_read_archived_at", "v85_agent_role",
             "v86_session_read_story_seen_at", "v87_session_seal"]
        )
    }

    // v57 (EXP-1105): a pre-v57 store's `teams` lacks `yolo_mode` (simulated
    // by dropping the v1 column after migrating through v56); the guarded
    // ALTER adds it defaulting false, keeps rows, and resets the teams offset.
    func testTeamYoloModeAddedToExistingStore() throws {
        let pool = try makePool("team-yolo-mode")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v56_workflow_start_on_dropped")
        try pool.write { db in
            try db.alter(table: "teams") { t in
                t.drop(column: "yolo_mode")
            }
            try db.execute(sql: """
                INSERT INTO "teams" ("id", "name", "slug", "created_at", "updated_at")
                VALUES ('t1', 'Acme', 'acme', '2026-09-29', '2026-09-29')
                """)
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('teams', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertTrue(try columnNames(pool, "teams").contains("yolo_mode"))
        let yolo = try pool.read { db in
            try Bool.fetchOne(db, sql: #"SELECT "yolo_mode" FROM "teams" WHERE "id" = 't1'"#)
        }
        XCTAssertEqual(yolo, false)
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch"
                    FROM "electric_offsets" WHERE "shape" = 'teams'
                    """
            )
        }
        XCTAssertEqual(offset?["handle"] as String?, "")
        XCTAssertEqual(offset?["offset"] as String?, "-1")
        XCTAssertEqual(offset?["needs_refetch"] as Int?, 1)
        XCTAssertNoThrow(try migrator.migrate(pool))
    }

    // v56 (EXP-1066/EXP-1090): a store migrated through v55 whose
    // `workflows` still carries the dead `start_on` column (every pre-v56
    // install) drops it, keeping its rows; a second pass is a no-op. Only UP
    // TO v56: v60 (SLOP-3) drops the whole table.
    func testWorkflowStartOnDroppedFromExistingStore() throws {
        let pool = try makePool("workflow-start-on-drop")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v55_workflow_session_membership_events")
        try pool.write { db in
            try db.alter(table: "workflows") { t in
                t.add(column: "start_on", .text).notNull().defaults(to: "contract")
            }
            try db.execute(sql: """
                INSERT INTO "workflows" ("id", "team_id", "name", "start_on")
                VALUES ('wf-1', 't1', 'Ship it', 'pr_open')
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v56_workflow_start_on_dropped"))
        XCTAssertFalse(try columnNames(pool, "workflows").contains("start_on"))
        let name = try pool.read { db in
            try String.fetchOne(db, sql: #"SELECT "name" FROM "workflows" WHERE "id" = 'wf-1'"#)
        }
        XCTAssertEqual(name, "Ship it")
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v56_workflow_start_on_dropped"))
    }

    // v51 (EXP-995): a store migrated through v50 carries an `automations`
    // table without `account`; the guarded ALTER adds it and the automations
    // shape offset resets so already-synced rows re-arrive carrying the pin.
    // Migrated only UP TO v51: v59 (SLOP-2) drops the table again.
    func testAutomationAccountAddedToExistingStore() throws {
        let pool = try makePool("automation-account")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v50_workflow_node_review")
        try pool.write { db in
            XCTAssertFalse(try db.columns(in: "automations").contains { $0.name == "account" })
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('automations', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v51_automation_account"))
        XCTAssertTrue(try columnNames(pool, "automations").contains("account"))
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch"
                    FROM "electric_offsets" WHERE "shape" = 'automations'
                    """
            )
        }
        XCTAssertEqual(offset?["handle"] as String?, "")
        XCTAssertEqual(offset?["offset"] as String?, "-1")
        XCTAssertEqual(offset?["needs_refetch"] as Int?, 1)
        // The rest of the chain runs clean on top (v59 drops the table).
        XCTAssertNoThrow(try migrator.migrate(pool))
    }

    // v59 (SLOP-2): a store migrated through v58 carries `actions` without
    // `triggers`, the dead `actions.trigger` column and the `automations`
    // table with a live cursor. The migration adds `triggers` (NOT NULL,
    // default '[]'), drops the dead column and the table, forgets the retired
    // shape's cursor and resets the actions offset so already-synced rows
    // re-arrive carrying their triggers.
    func testActionTriggersReplaceAutomationsOnExistingStore() throws {
        let pool = try makePool("action-triggers")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v58_workflows_gate_dropped")
        try pool.write { db in
            XCTAssertFalse(try db.columns(in: "actions").contains { $0.name == "triggers" })
            XCTAssertTrue(try db.tableExists("automations"))
            try db.execute(sql: """
                INSERT INTO "actions" ("id", "team_id", "name", "created_at", "updated_at")
                VALUES ('a-1', 't-1', 'Digest', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
                """)
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('actions', 'h', '0_0', 0, 1),
                       ('automations', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let actionCols = try columnNames(pool, "actions")
        XCTAssertTrue(actionCols.contains("triggers"))
        XCTAssertFalse(actionCols.contains("trigger"))
        XCTAssertFalse(try pool.read { db in try db.tableExists("automations") })
        // The row that predates the column reads as "no triggers".
        let stored = try pool.read { db in
            try String.fetchOne(db, sql: "SELECT \"triggers\" FROM \"actions\" WHERE \"id\" = 'a-1'")
        }
        XCTAssertEqual(stored, "[]")
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'actions'
                    """
            )
        }
        XCTAssertEqual(offset?["handle"] as String?, "")
        XCTAssertEqual(offset?["offset"] as String?, "-1")
        XCTAssertEqual(offset?["needs_refetch"] as Int?, 1)
        XCTAssertEqual(offset?["is_live"] as Int?, 0)
        let retired = try pool.read { db in
            try Int.fetchOne(
                db,
                sql: "SELECT COUNT(*) FROM \"electric_offsets\" WHERE \"shape\" = 'automations'"
            )
        }
        XCTAssertEqual(retired, 0)
        // Idempotent: a second pass is a no-op, never a duplicate column.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    // v65 (SLOP-17): a store migrated through v64 carries
    // `coding_sessions.automation_id`. The migration renames it to
    // `trigger_id` in place (the synced value survives), sweeps a stray
    // `automations` table + cursor, and resets the coding-sessions offset so
    // rows re-arrive under the renamed column.
    func testTriggerIdRenamesAutomationIdOnExistingStore() throws {
        let pool = try makePool("trigger-id")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v64_attachment_as_file")
        try pool.write { db in
            let cols = Set(try db.columns(in: "coding_sessions").map(\.name))
            XCTAssertTrue(cols.contains("automation_id"))
            XCTAssertFalse(cols.contains("trigger_id"))
            try db.execute(sql: """
                INSERT INTO "coding_sessions" ("id", "issue_id", "team_id", "user_id",
                    "device_label", "status", "automation_id", "started_at",
                    "created_at", "updated_at")
                VALUES ('cs-1', 'i-1', 't-1', 'u-1', 'macbook', 'ended', 'trg-1',
                    '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')
                """)
            // A stray legacy table + cursor (v59 should have swept them).
            try db.execute(sql: "CREATE TABLE \"automations\" (\"id\" TEXT PRIMARY KEY)")
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1),
                       ('automations', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let sessionCols = try columnNames(pool, "coding_sessions")
        XCTAssertTrue(sessionCols.contains("trigger_id"))
        XCTAssertFalse(sessionCols.contains("automation_id"))
        let kept = try pool.read { db in
            try String.fetchOne(
                db, sql: "SELECT \"trigger_id\" FROM \"coding_sessions\" WHERE \"id\" = 'cs-1'"
            )
        }
        XCTAssertEqual(kept, "trg-1")
        XCTAssertFalse(try pool.read { db in try db.tableExists("automations") })
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'coding-sessions'
                    """
            )
        }
        XCTAssertEqual(offset?["handle"] as String?, "")
        XCTAssertEqual(offset?["offset"] as String?, "-1")
        XCTAssertEqual(offset?["needs_refetch"] as Int?, 1)
        XCTAssertEqual(offset?["is_live"] as Int?, 0)
        let retired = try pool.read { db in
            try Int.fetchOne(
                db,
                sql: "SELECT COUNT(*) FROM \"electric_offsets\" WHERE \"shape\" = 'automations'"
            )
        }
        XCTAssertEqual(retired, 0)
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    // v52 (EXP-630): a store migrated through v51 carries `issues` without
    // `estimate` and `teams` without `estimation_type`; the guarded ALTERs add
    // both and BOTH shapes' offsets reset so already-synced rows re-arrive
    // carrying the columns (shape keys 'issues' and 'teams').
    func testIssueEstimateColumnsAddedToExistingStore() throws {
        let pool = try makePool("issue-estimate")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v51_automation_account")
        try pool.write { db in
            // Model the pre-v52 state: today's v1 create already declares the
            // columns, which is exactly the overlap the guarded ALTER tolerates.
            if try db.columns(in: "issues").contains(where: { $0.name == "estimate" }) {
                try db.alter(table: "issues") { t in t.drop(column: "estimate") }
            }
            if try db.columns(in: "teams").contains(where: { $0.name == "estimation_type" }) {
                try db.alter(table: "teams") { t in t.drop(column: "estimation_type") }
            }
            for shape in ["issues", "teams"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    """, arguments: [shape])
            }
        }
        XCTAssertFalse(try columnNames(pool, "issues").contains("estimate"))
        XCTAssertFalse(try columnNames(pool, "teams").contains("estimation_type"))

        XCTAssertNoThrow(try migrator.migrate(pool))
        let estimate = try pool.read { db in
            try db.columns(in: "issues").first { $0.name == "estimate" }
        }
        XCTAssertNotNil(estimate)
        XCTAssertFalse(estimate?.isNotNull ?? true)
        let estimationType = try pool.read { db in
            try db.columns(in: "teams").first { $0.name == "estimation_type" }
        }
        XCTAssertNotNil(estimationType)
        XCTAssertFalse(estimationType?.isNotNull ?? true)
        for shape in ["issues", "teams"] {
            let reset = try pool.read { db in
                try Bool.fetchOne(
                    db,
                    sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                        + "AND \"is_live\" = 0 FROM \"electric_offsets\" WHERE \"shape\" = ?",
                    arguments: [shape]
                )
            }
            XCTAssertEqual(reset, true, shape)
        }
        // Idempotent: a second pass is a no-op, never a duplicate column.
        XCTAssertNoThrow(try migrator.migrate(pool))
    }

    // v52 (EXP-630): a fresh store declares both columns in v1, so the guarded
    // migration must NOT reset either shape's offset (a re-snapshot of every
    // issue for a store that already has the column is pure cost).
    func testIssueEstimateMigrationLeavesFreshStoreOffsetsAlone() throws {
        let pool = try makePool("issue-estimate-fresh")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v51_automation_account")
        try pool.write { db in
            for shape in ["issues", "teams"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    """, arguments: [shape])
            }
        }
        // Up to v78: v80 (#EXP-1383) adds new `issues` columns and resets it.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v78_drop_actions_and_legacy_session_columns"))
        for shape in ["issues", "teams"] {
            let untouched = try pool.read { db in
                try Row.fetchOne(
                    db,
                    sql: "SELECT \"handle\", \"needs_refetch\" FROM \"electric_offsets\" WHERE \"shape\" = ?",
                    arguments: [shape]
                )
            }
            XCTAssertEqual(untouched?["handle"] as String?, "h", shape)
            XCTAssertEqual(untouched?["needs_refetch"] as Bool?, false, shape)
        }
    }

    // v53 (EXP-630 placeholder members): a store migrated through v52 carries
    // `team_invites` without `placeholder_user_id`; the guarded ALTER adds it
    // and the team-invites shape offset resets so already-synced invites
    // re-arrive carrying the binding (shape key 'team-invites' WITH A DASH).
    func testInvitePlaceholderColumnAddedToExistingStore() throws {
        let pool = try makePool("invite-placeholder")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v52_issue_estimate")
        try pool.write { db in
            // Model the pre-v53 state: today's v1 create already declares the
            // column, which is exactly the overlap the guarded ALTER tolerates.
            if try db.columns(in: "team_invites").contains(where: { $0.name == "placeholder_user_id" }) {
                try db.alter(table: "team_invites") { t in t.drop(column: "placeholder_user_id") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('team-invites', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "team_invites").contains("placeholder_user_id"))

        XCTAssertNoThrow(try migrator.migrate(pool))
        let column = try pool.read { db in
            try db.columns(in: "team_invites").first { $0.name == "placeholder_user_id" }
        }
        XCTAssertNotNil(column)
        XCTAssertFalse(column?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" WHERE \"shape\" = 'team-invites'"
            )
        }
        XCTAssertEqual(reset, true)
        // Idempotent: a second pass is a no-op, never a duplicate column.
        XCTAssertNoThrow(try migrator.migrate(pool))
    }

    // v53 (EXP-630): a fresh store declares the column in v1, so the guarded
    // migration must NOT reset the shape's offset (a re-snapshot for a store
    // that already has the column is pure cost).
    func testInvitePlaceholderMigrationLeavesFreshStoreOffsetAlone() throws {
        let pool = try makePool("invite-placeholder-fresh")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v52_issue_estimate")
        try pool.write { db in
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('team-invites', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertNoThrow(try migrator.migrate(pool))
        let untouched = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "needs_refetch" FROM "electric_offsets"
                    WHERE "shape" = 'team-invites'
                    """
            )
        }
        XCTAssertEqual(untouched?["handle"] as String?, "h")
        XCTAssertEqual(untouched?["needs_refetch"] as Bool?, false)
    }

    // v54 (EXP-1076 "Not invited"): a store migrated through v53 carries
    // `team_invites` without `sent_at`; the guarded ALTER adds it and the
    // team-invites shape offset resets so already-synced invites re-arrive
    // carrying it (shape key 'team-invites' WITH A DASH).
    func testInviteSentAtColumnAddedToExistingStore() throws {
        let pool = try makePool("invite-sent-at")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v53_invite_placeholder")
        try pool.write { db in
            // Model the pre-v54 state: today's v1 create already declares the
            // column, which is exactly the overlap the guarded ALTER tolerates.
            if try db.columns(in: "team_invites").contains(where: { $0.name == "sent_at" }) {
                try db.alter(table: "team_invites") { t in t.drop(column: "sent_at") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('team-invites', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "team_invites").contains("sent_at"))

        XCTAssertNoThrow(try migrator.migrate(pool))
        let column = try pool.read { db in
            try db.columns(in: "team_invites").first { $0.name == "sent_at" }
        }
        XCTAssertNotNil(column)
        XCTAssertFalse(column?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" WHERE \"shape\" = 'team-invites'"
            )
        }
        XCTAssertEqual(reset, true)
        // Idempotent: a second pass is a no-op, never a duplicate column.
        XCTAssertNoThrow(try migrator.migrate(pool))
    }

    // v54 (EXP-1076): a fresh store declares the column in v1, so the guarded
    // migration must NOT reset the shape's offset.
    func testInviteSentAtMigrationLeavesFreshStoreOffsetAlone() throws {
        let pool = try makePool("invite-sent-at-fresh")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v53_invite_placeholder")
        try pool.write { db in
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('team-invites', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertNoThrow(try migrator.migrate(pool))
        let untouched = try pool.read { db in
            try Row.fetchOne(
                db, sql: "SELECT * FROM \"electric_offsets\" WHERE \"shape\" = 'team-invites'"
            )
        }
        XCTAssertEqual(untouched?["handle"] as String?, "h")
        XCTAssertEqual(untouched?["needs_refetch"] as Bool?, false)
    }

    // v2 (EXP-180, for the helpdesk (gone, SLOP-4)): a `-v5` store created before
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
             "v19_coding_session_device_id", "v20_automations",
             "v21_device_is_default", "v22_coding_session_outcome",
             "v23_agent_status", "v24_drop_coding_session_outcome",
             "v25_board_default_branch", "v26_coding_session_pr",
             "v27_issue_relations", "v28_comment_threads",
             "v29_device_acp_agents", "v30_coding_session_blocked",
             "v31_coding_session_parent", "v32_action_prompt_placeholder",
             "v33_pins", "v34_device_shared_team_ids",
             "v35_attachment_video_metadata",
             "v36_coding_session_agent_busy",
             "v37_coding_session_agent_caption",
             "v38_drop_coding_session_summary",
             "v39_issue_drafts",
             "v40_issue_pr_base_branch",
             "v41_coding_session_results",
             "v42_coding_session_batch_issue_ids",
             "v43_coding_session_agent_account",
             "v44_coding_session_agent_title",
             "v45_device_icon",
             "v46_notification_session_id",
             "v47_workflows",
             "v48_workflow_node_approval",
             "v49_workflow_node_checkpoint",
             "v50_workflow_node_review",
             "v51_automation_account",
             "v52_issue_estimate",
             "v53_invite_placeholder", "v54_invite_sent_at",
             "v55_workflow_session_membership_events", "v56_workflow_start_on_dropped",
             "v57_team_yolo_mode", "v58_workflows_gate_dropped",
             "v59_action_triggers", "v60_drop_workflows_and_stacks",
             "v61_one_path", "v62_device_doctor",
             "v63_issue_draft_id", "v64_attachment_as_file",
             "v65_trigger_id", "v66_threads",
             "v67_issue_draft_thread", "v68_session_tree",
             "v69_session_dispatch", "v70_session_talk", "v71_session_issue_assignee",
             "v72_session_move_to_device", "v73_issue_identifier_upper_index",
             "v74_agents_events_prompts", "v75_conversations", "v76_apps",
             "v77_message_conversation", "v78_drop_actions_and_legacy_session_columns",
             "v79_app_records", "v80_agent_actors", "v81_conversation_digest", "v82_apps_granted_tools",
             "v83_session_background", "v84_session_read_archived_at", "v85_agent_role",
             "v86_session_read_story_seen_at", "v87_session_seal"]
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
             "v19_coding_session_device_id", "v20_automations",
             "v21_device_is_default", "v22_coding_session_outcome",
             "v23_agent_status", "v24_drop_coding_session_outcome",
             "v25_board_default_branch", "v26_coding_session_pr",
             "v27_issue_relations", "v28_comment_threads",
             "v29_device_acp_agents", "v30_coding_session_blocked",
             "v31_coding_session_parent", "v32_action_prompt_placeholder",
             "v33_pins", "v34_device_shared_team_ids",
             "v35_attachment_video_metadata",
             "v36_coding_session_agent_busy",
             "v37_coding_session_agent_caption",
             "v38_drop_coding_session_summary",
             "v39_issue_drafts",
             "v40_issue_pr_base_branch",
             "v41_coding_session_results",
             "v42_coding_session_batch_issue_ids",
             "v43_coding_session_agent_account",
             "v44_coding_session_agent_title",
             "v45_device_icon",
             "v46_notification_session_id",
             "v47_workflows",
             "v48_workflow_node_approval",
             "v49_workflow_node_checkpoint",
             "v50_workflow_node_review",
             "v51_automation_account",
             "v52_issue_estimate",
             "v53_invite_placeholder", "v54_invite_sent_at",
             "v55_workflow_session_membership_events", "v56_workflow_start_on_dropped",
             "v57_team_yolo_mode", "v58_workflows_gate_dropped",
             "v59_action_triggers", "v60_drop_workflows_and_stacks",
             "v61_one_path", "v62_device_doctor",
             "v63_issue_draft_id", "v64_attachment_as_file",
             "v65_trigger_id", "v66_threads",
             "v67_issue_draft_thread", "v68_session_tree",
             "v69_session_dispatch", "v70_session_talk", "v71_session_issue_assignee",
             "v72_session_move_to_device", "v73_issue_identifier_upper_index",
             "v74_agents_events_prompts", "v75_conversations", "v76_apps",
             "v77_message_conversation", "v78_drop_actions_and_legacy_session_columns",
             "v79_app_records", "v80_agent_actors", "v81_conversation_digest", "v82_apps_granted_tools",
             "v83_session_background", "v84_session_read_archived_at", "v85_agent_role",
             "v86_session_read_story_seen_at", "v87_session_seal"]
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
        // EXP-712: the board's own branch (nullable — nil follows the repo).
        XCTAssertTrue(boardCols.contains("default_branch"))
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

    // v61 (SLOP-4 one path): a store migrated through v60 carries `comments`
    // with a NOT NULL `author_id` and no `audience`, and `teams` with the
    // flag of the helpdesk (gone, SLOP-4). The rebuild relaxes the author, adds
    // `audience` defaulting to team, keeps rows, resets the comments offset
    // (every row must re-arrive carrying `audience`), and drops the team
    // column. Today's v1 create already has the new shape — hand-build the
    // old one to model the pre-v61 store.
    func testOnePathRelaxesCommentAuthorAddsAudienceAndDropsTheTeamFlag() throws {
        let pool = try makePool("one-path")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v60_drop_workflows_and_stacks")
        try pool.write { db in
            try db.drop(table: "comments")
            try db.create(table: "comments") { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull().indexed()
                t.column("team_id", .text).notNull().indexed()
                t.column("author_id", .text).notNull()
                t.column("body", .text)
                t.column("kind", .text).notNull().defaults(to: "regular")
                t.column("edited_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
                t.column("parent_id", .text).indexed()
                t.column("source", .text)
            }
            try db.execute(sql: """
                INSERT INTO "comments" (
                    "id", "issue_id", "team_id", "author_id", "body", "kind",
                    "created_at", "updated_at", "source"
                )
                VALUES ('c1', 'i1', 'w1', 'u1', 'hello', 'regular',
                        '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'user')
                """)
            try db.alter(table: "teams") { t in
                t.add(column: "helpdesk_enabled", .boolean).notNull().defaults(to: false)
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('comments', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        let author = try pool.read { db in
            try db.columns(in: "comments").first { $0.name == "author_id" }
        }
        XCTAssertNotNil(author)
        XCTAssertFalse(author?.isNotNull ?? true)
        XCTAssertTrue(try columnNames(pool, "comments").contains("audience"))
        XCTAssertFalse(try columnNames(pool, "teams").contains("helpdesk_enabled"))
        // The rebuild copies rows (audience defaults to team), it doesn't drop them.
        let existing = try pool.read { db in
            try Row.fetchOne(db, sql: "SELECT \"body\", \"audience\" FROM \"comments\" WHERE \"id\" = 'c1'")
        }
        let body: String? = existing?["body"]
        let audience: String? = existing?["audience"]
        XCTAssertEqual(body, "hello")
        XCTAssertEqual(audience, "team")
        // A reporter's reply (null author) now persists instead of throwing.
        XCTAssertNoThrow(try pool.write { db in
            try db.execute(sql: """
                INSERT INTO "comments" (
                    "id", "issue_id", "team_id", "author_id", "body", "kind",
                    "created_at", "updated_at", "source", "audience"
                )
                VALUES ('c2', 'i1', 'w1', NULL, 'still broken', 'regular',
                        '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'reporter', 'reporter')
                """)
        })
        // The rebuild must force a re-snapshot of the comments shape.
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'comments'
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

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
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

        // Only UP TO v18: v59 (SLOP-2) drops the dead `actions.trigger` again.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v18_action_automations"))
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

        // Only UP TO v20: v59 (SLOP-2) drops the table again.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v20_automations"))
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
    // v33 (EXP-778 pins): a store created before the `pins` table existed
    // must gain it on upgrade. A brand-new shape has no offset row, so no
    // shape offset is touched.
    func testPinsTableAddedToExistingStore() throws {
        let pool = try makePool("pins")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v32_action_prompt_placeholder")
        try pool.write { db in
            XCTAssertFalse(try db.tableExists("pins"))
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('issues', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v77_message_conversation"))
        XCTAssertTrue(try pool.read { db in try db.tableExists("pins") })
        XCTAssertEqual(
            try columnNames(pool, "pins"),
            ["id", "user_id", "team_id", "kind", "issue_id", "session_id",
             "action_id", "sort_order", "created_at", "updated_at"]
        )
        let indexed = try pool.read { db in
            try db.indexes(on: "pins").contains { $0.columns == ["user_id", "team_id"] }
        }
        XCTAssertTrue(indexed)
        // Other shapes' offsets are untouched.
        let untouched = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "needs_refetch" FROM "electric_offsets" WHERE "shape" = 'issues'
                    """
            )
        }
        let handle: String? = untouched?["handle"]
        let needsRefetch: Bool? = untouched?["needs_refetch"]
        XCTAssertEqual(handle, "h")
        XCTAssertEqual(needsRefetch, false)
    }

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

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
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

    // v22 (EXP-637 agent run close-outs): a store created before
    // `coding_sessions.summary` / `ended_by` / `resumed_from_id` existed must
    // gain all three via the guarded ALTERs and get the coding-sessions shape
    // offset reset (the key has A DASH — the proxy route name, not the table
    // name) so already-synced rows re-arrive carrying the agent's close-out.
    // EXP-686 dropped `outcome` from the set — v24 drops it again anyway.
    func testCodingSessionCloseOutColumnsAddedToExistingStore() throws {
        let pool = try makePool("session-close-out")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v21_device_is_default")
        try pool.write { db in
            // Hand-build the pre-v22 state: drop the columns today's v1 create
            // already declares (that overlap is exactly what the guarded
            // ALTERs have to tolerate) + a live offset row.
            let sessionCols = Set(try db.columns(in: "coding_sessions").map(\.name))
            for column in ["summary", "ended_by", "resumed_from_id"]
            where sessionCols.contains(column) {
                try db.alter(table: "coding_sessions") { t in t.drop(column: column) }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let columns = try columnNames(pool, "coding_sessions")
        // v38 (EXP-864) drops `summary` again at the end of the chain.
        XCTAssertFalse(columns.contains("summary"), "summary must be dropped by v38")
        for column in ["ended_by", "resumed_from_id"] {
            XCTAssertTrue(columns.contains(column), "missing \(column)")
            let added = try pool.read { db in
                try db.columns(in: "coding_sessions").first { $0.name == column }
            }
            // Every one of them is NULL on a run that ended any other way.
            XCTAssertFalse(added?.isNotNull ?? true, "\(column) must be nullable")
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

    // v39 (EXP-878 issue drafts): a store created before the `issue_drafts`
    // table existed must gain it on upgrade. A brand-new shape has no offset
    // row, so no shape offset is touched.
    func testIssueDraftsTableAddedToExistingStore() throws {
        let pool = try makePool("issue-drafts")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v38_drop_coding_session_summary")
        try pool.write { db in
            XCTAssertFalse(try db.tableExists("issue_drafts"))
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('issues', 'h', '0_0', 0, 1)
                """)
        }

        // Up to v78: v80 (#EXP-1383) adds new `issues` columns and resets it.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v78_drop_actions_and_legacy_session_columns"))
        XCTAssertTrue(try pool.read { db in try db.tableExists("issue_drafts") })
        XCTAssertEqual(
            try columnNames(pool, "issue_drafts"),
            ["id", "user_id", "team_id", "board_id", "title", "description",
             "status_id", "priority", "assignee_id", "label_ids", "due_date",
             "created_at", "updated_at", "session_id"]
        )
        let indexed = try pool.read { db in
            Set(try db.indexes(on: "issue_drafts").map(\.columns))
        }
        XCTAssertTrue(indexed.contains(["user_id"]))
        XCTAssertTrue(indexed.contains(["team_id"]))
        XCTAssertTrue(indexed.contains(["board_id"]))
        // Other shapes' offsets are untouched.
        let untouched = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "needs_refetch" FROM "electric_offsets" WHERE "shape" = 'issues'
                    """
            )
        }
        XCTAssertEqual(untouched?["handle"] as String?, "h")
        XCTAssertEqual(untouched?["needs_refetch"] as Bool?, false)
    }

    // The end-state schema must expose the tables + key columns sync writes to,
    // so a green migration can't silently produce the wrong shape. This pins
    // the EXP-180 rename: teams/boards/team_members/team_invites exist, the
    // workspace/project-era names do NOT.
    // v40 (EXP-897): the PR stack edge — `issues.pr_base_branch`. A store
    // created before it existed must gain the column via the guarded ALTER and
    // get the issues offset reset (shape key 'issues'), or the sync apply path
    // would keep dropping the wire column the local schema lacks and no client
    // could ever see a stack.
    func testIssuePrBaseBranchColumnAddedToExistingStore() throws {
        let pool = try makePool("issue-pr-base-branch")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v39_issue_drafts")
        try pool.write { db in
            // Model the pre-v40 state: today's v1 create already declares the
            // column, which is exactly the overlap the guarded ALTER tolerates.
            if try db.columns(in: "issues").contains(where: { $0.name == "pr_base_branch" }) {
                try db.alter(table: "issues") { t in
                    t.drop(column: "pr_base_branch")
                }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('issues', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let column = try pool.read { db in
            try db.columns(in: "issues").first { $0.name == "pr_base_branch" }
        }
        XCTAssertNotNil(column)
        XCTAssertFalse(column?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" WHERE \"shape\" = 'issues'"
            )
        }
        XCTAssertEqual(reset, true)
    }

    // v63 (EXP-1231): `issues.draft_id`, the draft an issue was created from.
    // A store created before it must gain the column via the guarded ALTER
    // and get the issues offset reset, or a draft page could never see its
    // draft was created elsewhere; a fresh store already has it from v1.
    func testIssueDraftIdColumnAddedToExistingStore() throws {
        let pool = try makePool("issue-draft-id")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v62_device_doctor")
        try pool.write { db in
            // Model the pre-v63 state: today's v1 create already declares it.
            if try db.columns(in: "issues").contains(where: { $0.name == "draft_id" }) {
                try db.alter(table: "issues") { t in
                    t.drop(column: "draft_id")
                }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('issues', 'h', '0_0', 0, 1)
                ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                    "needs_refetch" = 0, "is_live" = 1
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        let column = try pool.read { db in
            try db.columns(in: "issues").first { $0.name == "draft_id" }
        }
        XCTAssertNotNil(column)
        XCTAssertFalse(column?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" WHERE \"shape\" = 'issues'"
            )
        }
        XCTAssertEqual(reset, true)
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v66 (EXP-1284 threads): a store created before threads existed must
    // gain both tables (+ their indexes) on upgrade. Two brand-new shapes
    // have no offset rows, so no other shape's offset is touched.
    func testThreadTablesAddedToExistingStore() throws {
        let pool = try makePool("threads")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v65_trigger_id")
        try pool.write { db in
            XCTAssertFalse(try db.tableExists("threads"))
            XCTAssertFalse(try db.tableExists("thread_items"))
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        XCTAssertTrue(try pool.read { db in try db.tableExists("threads") })
        XCTAssertTrue(try pool.read { db in try db.tableExists("thread_items") })
        XCTAssertEqual(
            try columnNames(pool, "threads"),
            ["id", "team_id", "title", "created_by", "results", "created_at", "updated_at"]
        )
        XCTAssertEqual(
            try columnNames(pool, "thread_items"),
            ["id", "thread_id", "kind", "issue_id", "session_id", "team_id", "board_id",
             "added_by", "added_by_session_id", "source", "sort_order",
             "created_at", "updated_at"]
        )
        let threadIndexed = try pool.read { db in
            Set(try db.indexes(on: "threads").map(\.columns))
        }
        XCTAssertTrue(threadIndexed.contains(["team_id"]))
        let itemIndexed = try pool.read { db in
            Set(try db.indexes(on: "thread_items").map(\.columns))
        }
        XCTAssertTrue(itemIndexed.contains(["thread_id"]))
        XCTAssertTrue(itemIndexed.contains(["team_id"]))
        XCTAssertTrue(itemIndexed.contains(["issue_id"]))
        XCTAssertTrue(itemIndexed.contains(["session_id"]))
        // The nullable columns stay nullable (a NULL title = derived; run
        // items carry no issue/board, issue items no session).
        let nullable = try pool.read { db -> [String: Bool] in
            var out: [String: Bool] = [:]
            for col in try db.columns(in: "threads") { out["threads.\(col.name)"] = !col.isNotNull }
            for col in try db.columns(in: "thread_items") { out["thread_items.\(col.name)"] = !col.isNotNull }
            return out
        }
        XCTAssertEqual(nullable["threads.title"], true)
        XCTAssertEqual(nullable["threads.results"], true)
        XCTAssertEqual(nullable["thread_items.issue_id"], true)
        XCTAssertEqual(nullable["thread_items.session_id"], true)
        XCTAssertEqual(nullable["thread_items.board_id"], true)
        XCTAssertEqual(nullable["thread_items.kind"], false)
        // Other shapes' offsets are untouched.
        let untouched = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "needs_refetch" FROM "electric_offsets" WHERE "shape" = 'coding-sessions'
                    """
            )
        }
        XCTAssertEqual(untouched?["handle"] as String?, "h")
        XCTAssertEqual(untouched?["needs_refetch"] as Bool?, false)
        // Idempotent: a re-run converges (ifNotExists everywhere).
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    // v68 (EXP-1291 Threads v2): a store migrated through v67 drops
    // `coding_sessions` / `threads` / `thread_items` with their cursors (hard
    // break, no data carried over), drops the issue PR + action trigger
    // columns in place keeping the rows and their cursors, and gains the five
    // empty session-tree tables. A re-run converges.
    func testSessionTreeReplacesRunsAndThreads() throws {
        let pool = try makePool("session-tree")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: Self.preSessionTree)
        try pool.write { db in
            try db.execute(sql: """
                INSERT INTO "issues" ("id", "board_id", "title", "status", "priority",
                    "pr_url", "pr_number", "pr_state", "branch", "pr_base_branch",
                    "created_at", "updated_at")
                VALUES ('i-1', 'b-1', 'Kept', 'backlog', 'none',
                    'https://github.com/acme/repo/pull/1', 1, 'open', 'exp/EXP-1', 'main', '', '')
                """)
            try db.execute(sql: """
                INSERT INTO "actions" ("id", "team_id", "name", "repository_id", "triggers",
                    "created_at", "updated_at")
                VALUES ('a-1', 't-1', 'Ship', 'r-1', '[]', '', '')
                """)
            try db.execute(sql: """
                INSERT INTO "threads" ("id", "team_id", "created_at", "updated_at")
                VALUES ('th-1', 't-1', '', '')
                """)
            try db.execute(sql: """
                INSERT INTO "issue_drafts" ("id", "user_id", "team_id", "board_id", "title", "thread_id")
                VALUES ('d-1', 'u-1', 't-1', 'b-1', 'Draft', 'th-1')
                """)
            for shape in ["coding-sessions", "threads", "thread-items", "issues", "actions", "issue-drafts"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v77_message_conversation"))
        try pool.read { db in
            for table in ["coding_sessions", "threads", "thread_items"] {
                XCTAssertFalse(try db.tableExists(table), "\(table) must be gone")
            }
            for table in ["sessions", "session_issues", "messages", "pull_requests", "pull_request_issues"] {
                XCTAssertTrue(try db.tableExists(table), "missing \(table)")
            }
            let shapes = try String.fetchAll(db, sql: "SELECT \"shape\" FROM \"electric_offsets\" ORDER BY 1")
            XCTAssertEqual(shapes, ["actions", "issue-drafts", "issues"])
            let handles = try String.fetchAll(
                db, sql: "SELECT \"handle\" FROM \"electric_offsets\" ORDER BY \"shape\""
            )
            // `issue_drafts.thread_id` → `session_id` re-snapshots the drafts.
            XCTAssertEqual(handles, ["h", "", "h"])
            XCTAssertEqual(
                try String.fetchOne(db, sql: "SELECT \"session_id\" FROM \"issue_drafts\" WHERE \"id\" = 'd-1'"),
                "th-1"
            )
            XCTAssertEqual(
                try String.fetchOne(db, sql: "SELECT \"title\" FROM \"issues\" WHERE \"id\" = 'i-1'"),
                "Kept"
            )
            XCTAssertEqual(
                try String.fetchOne(db, sql: "SELECT \"name\" FROM \"actions\" WHERE \"id\" = 'a-1'"),
                "Ship"
            )
        }
        let issueCols = try columnNames(pool, "issues")
        for column in ["pr_url", "pr_number", "pr_state", "branch", "pr_merged_at", "pr_base_branch"] {
            XCTAssertFalse(issueCols.contains(column), "issues.\(column)")
        }
        let actionCols = try columnNames(pool, "actions")
        XCTAssertFalse(actionCols.contains("repository_id"))
        XCTAssertFalse(actionCols.contains("triggers"))
        XCTAssertFalse(actionCols.contains("description"))
        XCTAssertFalse(try columnNames(pool, "issue_drafts").contains("thread_id"))
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v67 (EXP-1284): `issue_drafts.thread_id`. A store whose drafts table
    // predates it gains the nullable column via the guarded ALTER, keeps its
    // rows, and gets the issue-drafts offset reset (so synced drafts re-arrive
    // carrying their thread); other shapes' offsets are untouched.
    func testIssueDraftThreadColumnAddedToExistingStore() throws {
        let pool = try makePool("issue-draft-thread")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v66_threads")
        try pool.write { db in
            XCTAssertFalse(try db.columns(in: "issue_drafts").contains { $0.name == "thread_id" })
            try db.execute(sql: """
                INSERT INTO "issue_drafts" ("id", "user_id", "team_id", "board_id", "title")
                VALUES ('d1', 'u1', 't1', 'b1', 'Kept')
                """)
            for shape in ["issue-drafts", "issues"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let column = try pool.read { db in
            try db.columns(in: "issue_drafts").first { $0.name == "thread_id" }
        }
        XCTAssertNotNil(column)
        XCTAssertFalse(column?.isNotNull ?? true)
        let kept = try pool.read { db in
            try String.fetchOne(db, sql: "SELECT \"title\" FROM \"issue_drafts\" WHERE \"id\" = 'd1'")
        }
        XCTAssertEqual(kept, "Kept")
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" WHERE \"shape\" = 'issue-drafts'"
            )
        }
        XCTAssertEqual(reset, true)
        let untouched = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "needs_refetch" FROM "electric_offsets" WHERE "shape" = 'issues'
                    """
            )
        }
        XCTAssertEqual(untouched?["handle"] as String?, "h")
        XCTAssertEqual(untouched?["needs_refetch"] as Bool?, false)
        // Idempotent: the guard makes a re-run a no-op.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    func testFreshStoreHasIssueDraftId() throws {
        let pool = try makePool("issue-draft-id-fresh")
        try DatabaseManager.runMigrations(on: pool)
        XCTAssertTrue(try columnNames(pool, "issues").contains("draft_id"))
    }

    func testMigratedSchemaHasSyncTables() throws {
        let pool = try makePool("schema")
        try DatabaseManager.runMigrations(on: pool)
        for table in ["teams", "boards", "issues", "labels", "issue_labels",
                      "users", "team_members", "team_invites", "comments",
                      "attachments", "notifications", "issue_subscribers",
                      "issue_events", "issue_statuses", "pins",
                      "issue_drafts", "sessions", "session_issues", "messages",
                      "pull_requests", "pull_request_issues", "electric_offsets"] {
            let exists = try pool.read { db in try db.tableExists(table) }
            XCTAssertTrue(exists, "missing table \(table)")
        }
        // The renamed-away tables must be gone on a fresh install.
        // SLOP-2: `automations` too — triggers live on their action now.
        // SLOP-3: and the workflow tables. EXP-1291: runs and threads became
        // the session tree.
        for table in ["workspaces", "projects", "workspace_members", "workspace_invites", "releases",
                      "automations", "workflows", "workflow_nodes", "workflow_events",
                      "coding_sessions", "threads", "thread_items", "actions"] {
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
                      "sessions"] {
            let cols = try columnNames(pool, table)
            XCTAssertTrue(cols.contains("team_id"), "\(table) missing team_id")
            XCTAssertFalse(cols.contains("workspace_id"), "\(table) still has workspace_id")
        }

        XCTAssertTrue(try columnNames(pool, "issues").contains("duplicate_of_id"))
        // EXP-1291: PR and branch state left `issues` for `pull_requests`.
        for column in ["pr_url", "pr_number", "pr_state", "branch", "pr_merged_at", "pr_base_branch"] {
            XCTAssertFalse(try columnNames(pool, "issues").contains(column), "issues.\(column)")
        }
        // EXP-630: story points ride the issues shape, the scale the teams one.
        XCTAssertTrue(try columnNames(pool, "issues").contains("estimate"))
        XCTAssertTrue(try columnNames(pool, "teams").contains("estimation_type"))
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
        XCTAssertFalse(try columnNames(pool, "issues").contains("recurrence_interval"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("recurrence_unit"))
        // Due date is date-only (REV2-49): the time-of-day columns are deleted
        // from the product, `due_date` stays.
        XCTAssertTrue(try columnNames(pool, "issues").contains("due_date"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("due_time"))
        XCTAssertFalse(try columnNames(pool, "issues").contains("end_time"))

        // EXP-778: pins are per-user (21st shape) — team_id + kind + the
        // nullable target columns (NEW-8: `action_id` left with actions).
        let pinCols = try columnNames(pool, "pins")
        XCTAssertEqual(
            pinCols,
            ["id", "user_id", "team_id", "kind", "issue_id", "session_id",
             "sort_order", "created_at", "updated_at"]
        )

        // EXP-878: issue drafts are per-user (22nd shape) — the board the
        // draft files into, the fields the compose page owns, and the uuid[]
        // label list stored as text like devices.shared_team_ids.
        let draftCols = try columnNames(pool, "issue_drafts")
        XCTAssertEqual(
            draftCols,
            ["id", "user_id", "team_id", "board_id", "title", "description",
             "status_id", "priority", "assignee_id", "label_ids", "due_date",
             "created_at", "updated_at", "session_id", "assignee_agent_id"]
        )

        // status_id is NULLABLE: NULL means the team's Backlog builtin.
        let draftStatusId = try pool.read { db in
            try db.columns(in: "issue_drafts").first { $0.name == "status_id" }
        }
        XCTAssertNotNil(draftStatusId)
        XCTAssertFalse(draftStatusId?.isNotNull ?? true)

        // EXP-1291: the session tree's five tables carry exactly the shape
        // allowlists — never the server-only visibility/board mirrors.
        XCTAssertEqual(
            try columnNames(pool, "sessions"),
            ["id", "team_id", "parent_id", "root_id", "depth", "user_id",
             "device_id", "cli", "account", "model", "effort", "plan_mode",
             "computer_use", "ultracode", "repository_id", "branch", "status", "busy",
             "needs_input", "blocked", "started_at", "last_turn_at", "ended_at",
             "shared", "title", "guide", "created_at", "updated_at", "agent_id", "app_id",
             "agent_instructions_at", "started_by_agent_id", "background",
             "seal_public_key", "seal_epoch", "sealed_title"]
        )
        // NEW-2: agent definitions, events and prompts carry exactly the
        // shape allowlists (`instructions`, `trigger_state`, `body` never).
        XCTAssertEqual(
            try columnNames(pool, "agents"),
            ["id", "team_id", "name", "icon", "color", "triggers", "device_id", "cli",
             "account", "model", "owner_id", "sort_order", "created_at", "updated_at",
             "instructions_updated_at", "role"]
        )
        XCTAssertEqual(
            try columnNames(pool, "events"),
            ["id", "team_id", "name", "payload", "emitted_by", "session_id", "user_id", "created_at"]
        )
        XCTAssertEqual(
            try columnNames(pool, "prompts"),
            ["id", "team_id", "name", "icon", "sort_order", "created_at", "updated_at"]
        )
        XCTAssertEqual(
            try columnNames(pool, "session_issues"),
            ["id", "session_id", "issue_id", "team_id", "source", "added_by",
             "added_by_session_id", "sort_order", "created_at", "updated_at"]
        )
        XCTAssertEqual(
            try columnNames(pool, "messages"),
            ["id", "root_id", "team_id", "author_kind", "author_id",
             "body", "card", "answered_at", "consumed_at", "created_at", "updated_at",
             "author_session_id", "conversation_id"]
        )
        XCTAssertEqual(
            try columnNames(pool, "session_reads"),
            ["id", "user_id", "session_id", "read_at", "archived_at", "created_at", "updated_at", "story_seen_at"]
        )
        XCTAssertEqual(
            try columnNames(pool, "pull_requests"),
            ["id", "team_id", "root_session_id", "session_id", "repository_id",
             "number", "url", "title", "head", "base", "base_pull_request_id",
             "state", "merged_at", "created_at", "updated_at"]
        )
        XCTAssertEqual(
            try columnNames(pool, "pull_request_issues"),
            ["id", "pull_request_id", "issue_id", "team_id", "created_at", "updated_at"]
        )
        // Every bool is declared BOOLEAN so partial updates coerce "t"/"f".
        let sessionBools = try pool.read { db in
            Set(try db.columns(in: "sessions").filter { $0.type.uppercased().contains("BOOL") }.map(\.name))
        }
        XCTAssertEqual(
            sessionBools,
            ["plan_mode", "computer_use", "ultracode", "busy", "needs_input", "shared", "background"]
        )

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
        // The flag of the helpdesk (gone, SLOP-4) left the teams shape and
        // the local column with it (v61).
        XCTAssertFalse(try columnNames(pool, "teams").contains("helpdesk_enabled"))
        // SLOP-4: a reporter's reply has no author; `audience` rides along.
        let commentAuthor = try pool.read { db in
            try db.columns(in: "comments").first { $0.name == "author_id" }
        }
        XCTAssertNotNil(commentAuthor)
        XCTAssertFalse(commentAuthor?.isNotNull ?? true)
        XCTAssertTrue(try columnNames(pool, "comments").contains("audience"))
        // EXP-1105: yolo mode hides Reviews; synced on the teams shape.
        XCTAssertTrue(try columnNames(pool, "teams").contains("yolo_mode"))
        // notifications.team_id (nullable): set on issue-less agent_message /
        // session_blocked rows so the inbox can name their team.
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

        // team_invites.placeholder_user_id (nullable, EXP-630): the roster row
        // an emailed invite created, so the member list can badge it "Invited"
        // until it is claimed. NULL for link invites.
        let invitePlaceholder = try pool.read { db in
            try db.columns(in: "team_invites").first { $0.name == "placeholder_user_id" }
        }
        XCTAssertNotNil(invitePlaceholder)
        XCTAssertFalse(invitePlaceholder?.isNotNull ?? true)

        // team_invites.sent_at (nullable, EXP-1076): when the link was issued;
        // NULL = an import seat nobody was ever invited ("Not invited").
        let inviteSentAt = try pool.read { db in
            try db.columns(in: "team_invites").first { $0.name == "sent_at" }
        }
        XCTAssertNotNil(inviteSentAt)
        XCTAssertFalse(inviteSentAt?.isNotNull ?? true)

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
            "launch_defaults_updated_at", "agent_accounts", "agent_usage",
            "agent_usage_at", "active_sessions", "last_seen_at",
            "shared_team_ids", "is_default", "icon", "update_requested_at",
            "created_at", "updated_at",
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

    // v21 (EXP-622): a store created before `devices.is_default` gains it via
    // the guarded ALTER, and the devices shape offset resets so the rows
    // already stored re-snapshot with the column.
    func testDeviceIsDefaultBackfillsAndResetsTheDevicesOffset() throws {
        let pool = try makePool("device-default")
        try pool.write { db in
            try db.create(table: "devices") { t in
                t.primaryKey("id", .text)
                t.column("user_id", .text).notNull()
                t.column("device_id", .text).notNull()
                t.column("label", .text).notNull().defaults(to: "")
            }
            try db.create(table: "electric_offsets") { t in
                t.primaryKey("shape", .text)
                t.column("handle", .text).notNull().defaults(to: "h1")
                t.column("offset", .text).notNull().defaults(to: "42")
                t.column("needs_refetch", .boolean).notNull().defaults(to: false)
                t.column("is_live", .boolean).notNull().defaults(to: true)
            }
            try db.execute(sql: "INSERT INTO \"electric_offsets\" (\"shape\") VALUES ('devices')")
        }
        try DatabaseManager.runMigrations(on: pool)
        XCTAssertTrue(try columnNames(pool, "devices").contains("is_default"))
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "FROM \"electric_offsets\" WHERE \"shape\" = 'devices'"
            )
        }
        XCTAssertEqual(reset, true)
    }

    // v23 (EXP-484 agent auth status + usage): a store created before the
    // devices `agent_accounts`/`agent_usage`/`agent_usage_at` columns and
    // `coding_sessions.agent` existed must gain all four via the guarded
    // ALTERs — and BOTH shapes rotate here, so the devices AND the
    // coding-sessions offsets reset (the latter key has A DASH: the proxy
    // route name, not the table name).
    func testAgentStatusColumnsBackfillAndResetBothOffsets() throws {
        let pool = try makePool("agent-status")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v22_coding_session_outcome")
        try pool.write { db in
            // Hand-build the pre-v23 state: drop the columns today's v1 create
            // already declares (that overlap is exactly what the guarded
            // ALTERs have to tolerate) + live offset rows for both shapes.
            let deviceCols = Set(try db.columns(in: "devices").map(\.name))
            for column in ["agent_accounts", "agent_usage", "agent_usage_at"]
            where deviceCols.contains(column) {
                try db.alter(table: "devices") { t in t.drop(column: column) }
            }
            if Set(try db.columns(in: "coding_sessions").map(\.name)).contains("agent") {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "agent") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('devices', 'h', '0_0', 0, 1), ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let deviceColumns = try columnNames(pool, "devices")
        for column in ["agent_accounts", "agent_usage", "agent_usage_at"] {
            XCTAssertTrue(deviceColumns.contains(column), "missing \(column)")
            let added = try pool.read { db in
                try db.columns(in: "devices").first { $0.name == column }
            }
            // A machine that never reported keeps them NULL forever.
            XCTAssertFalse(added?.isNotNull ?? true, "\(column) must be nullable")
        }
        XCTAssertTrue(try columnNames(pool, "coding_sessions").contains("agent"))
        let sessionAgent = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "agent" }
        }
        XCTAssertFalse(sessionAgent?.isNotNull ?? true, "agent must be nullable")

        // BOTH shapes must be marked for refetch — a devices-only reset would
        // strand every already-synced session row without its agent.
        for shape in ["devices", "coding-sessions"] {
            let offset = try pool.read { db in
                try Row.fetchOne(
                    db,
                    sql: """
                        SELECT "handle", "offset", "needs_refetch", "is_live"
                        FROM "electric_offsets" WHERE "shape" = ?
                        """,
                    arguments: [shape]
                )
            }
            let handle: String? = offset?["handle"]
            let offsetValue: String? = offset?["offset"]
            let needsRefetch: Bool? = offset?["needs_refetch"]
            let isLive: Bool? = offset?["is_live"]
            XCTAssertEqual(handle, "", shape)
            XCTAssertEqual(offsetValue, "-1", shape)
            XCTAssertEqual(needsRefetch, true, shape)
            XCTAssertEqual(isLive, false, shape)
        }
    }

    // v24 (EXP-686): a store created while `coding_sessions.outcome` still
    // existed must lose it via the guarded drop (today's v1 create no longer
    // declares it — hand-add it to model the old state). The rest of the
    // close-out trio must survive.
    func testCodingSessionOutcomeDroppedFromExistingStore() throws {
        let pool = try makePool("session-outcome-drop")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v23_agent_status")
        try pool.write { db in
            try db.alter(table: "coding_sessions") { t in
                t.add(column: "outcome", .text)
            }
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let columns = try columnNames(pool, "coding_sessions")
        XCTAssertFalse(columns.contains("outcome"))
        XCTAssertFalse(columns.contains("summary"), "summary must be dropped by v38")
        for column in ["ended_by", "resumed_from_id"] {
            XCTAssertTrue(columns.contains(column), "missing \(column)")
        }
    }

    // v25 (EXP-712 board branches): a store created before
    // `boards.default_branch` existed must gain it via the guarded ALTER and
    // get its boards shape offset reset, so the rows already synced re-arrive
    // carrying the branch.
    func testBoardDefaultBranchAddedToExistingStore() throws {
        let pool = try makePool("board-default-branch")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v24_drop_coding_session_outcome")
        try pool.write { db in
            // Model the pre-v25 state: today's v1 create already declares the
            // column, which is exactly the overlap the guarded ALTER tolerates.
            if try db.columns(in: "boards").contains(where: { $0.name == "default_branch" }) {
                try db.alter(table: "boards") { t in
                    t.drop(column: "default_branch")
                }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('boards', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        let column = try pool.read { db in
            try db.columns(in: "boards").first { $0.name == "default_branch" }
        }
        XCTAssertNotNil(column)
        XCTAssertFalse(column?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" WHERE \"shape\" = 'boards'"
            )
        }
        XCTAssertEqual(reset, true)
    }

    // v26 (EXP-734): a run parks its OWN pull request — the three PR columns
    // ride the coding-sessions shape. A store created before they existed must
    // gain all three via the guarded ALTERs and get the coding-sessions offset
    // reset (the shape key is 'coding-sessions' WITH A DASH), or the sync apply
    // path would keep dropping the wire columns the local schema lacks.
    func testCodingSessionPrColumnsAddedToExistingStore() throws {
        let pool = try makePool("session-pr-columns")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v25_board_default_branch")
        try pool.write { db in
            // Model the pre-v26 state: today's v1 create already declares the
            // columns, which is exactly the overlap the guarded ALTERs tolerate.
            for column in ["pr_url", "pr_number", "pr_state"] {
                if try db.columns(in: "coding_sessions").contains(where: { $0.name == column }) {
                    try db.alter(table: "coding_sessions") { t in
                        t.drop(column: column)
                    }
                }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let columns = try pool.read { db in try db.columns(in: "coding_sessions") }
        for name in ["pr_url", "pr_number", "pr_state"] {
            let added = columns.first { $0.name == name }
            XCTAssertNotNil(added, "missing column \(name)")
            XCTAssertFalse(added?.isNotNull ?? true)
        }
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'coding-sessions'"
            )
        }
        XCTAssertEqual(reset, true)
    }

    // v29 (EXP-749): a device reports WHICH of its runnable agents its ACP
    // engine drives. A store created before the column existed must gain it
    // via the guarded ALTER and get the devices offset reset, or the already
    // synced rows would never carry it (the v23 precedent).
    func testDeviceAcpAgentsColumnAddedToExistingStore() throws {
        let pool = try makePool("device-acp-agents")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v28_comment_threads")
        try pool.write { db in
            // Model the pre-v29 state: today's v1 create already declares the
            // column, which is exactly the overlap the guarded ALTER tolerates.
            if try db.columns(in: "devices").contains(where: { $0.name == "acp_agents" }) {
                try db.alter(table: "devices") { t in t.drop(column: "acp_agents") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('devices', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool))
        let added = try pool.read { db in
            try db.columns(in: "devices").first { $0.name == "acp_agents" }
        }
        XCTAssertNotNil(added)
        // A machine that never reports keeps it NULL — the "unknown" reading.
        XCTAssertFalse(added?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'devices'"
            )
        }
        XCTAssertEqual(reset, true)
    }

    // The `-v5` canonical file name + the legacy-file purge list are the wipe
    // mechanism for the rename — pin the suffix so a stray edit can't silently
    // strand every device on the old snapshot.
    // v32 (EXP-825 composer hint): a store created before
    // `actions.prompt_placeholder` existed must gain it and get the actions
    // shape offset reset so already-synced rows re-arrive with the column.
    func testActionPromptPlaceholderColumnAddedToExistingStore() throws {
        let pool = try makePool("action-prompt-placeholder")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v31_coding_session_parent")
        try pool.write { db in
            // Hand-build the pre-v32 state: drop the column if an earlier
            // create already declares it + a live offset row.
            let actionCols = Set(try db.columns(in: "actions").map(\.name))
            if actionCols.contains("prompt_placeholder") {
                try db.alter(table: "actions") { t in t.drop(column: "prompt_placeholder") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('actions', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "actions").contains("prompt_placeholder"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v77_message_conversation"))
        XCTAssertTrue(try columnNames(pool, "actions").contains("prompt_placeholder"))
        let column = try pool.read { db in
            try db.columns(in: "actions").first { $0.name == "prompt_placeholder" }
        }
        XCTAssertFalse(column?.isNotNull ?? true)
        // The ALTER must force a refetch of the actions shape.
        let offset = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: """
                    SELECT "handle", "offset", "needs_refetch", "is_live"
                    FROM "electric_offsets" WHERE "shape" = 'actions'
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

    // v34 (FEED-33): a store created before `devices.shared_team_ids` existed
    // must gain it and get the devices offset reset so the already-synced
    // rows re-arrive carrying the team set. The old `shared_team_id` column
    // stays declared (never dropped, never read).
    func testDeviceSharedTeamIdsColumnAddedToExistingStore() throws {
        let pool = try makePool("device-shared-team-ids")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v32_action_prompt_placeholder")
        try pool.write { db in
            if try db.columns(in: "devices").contains(where: { $0.name == "shared_team_ids" }) {
                try db.alter(table: "devices") { t in t.drop(column: "shared_team_ids") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('devices', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "devices").contains("shared_team_ids"))

        XCTAssertNoThrow(try migrator.migrate(pool))
        let cols = try columnNames(pool, "devices")
        XCTAssertTrue(cols.contains("shared_team_ids"))
        XCTAssertTrue(cols.contains("shared_team_id"))
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'devices'"
            )
        }
        XCTAssertEqual(reset, true)
    }

    // v35 (EXP-824): a store created before `attachments.duration_ms` /
    // `poster_storage_key` existed must gain both and get the attachments
    // offset reset so already-synced rows re-arrive carrying them.
    func testAttachmentVideoMetadataColumnsAddedToExistingStore() throws {
        let pool = try makePool("attachment-video-metadata")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v34_device_shared_team_ids")
        try pool.write { db in
            for column in ["duration_ms", "poster_storage_key"]
            where try db.columns(in: "attachments").contains(where: { $0.name == column }) {
                try db.alter(table: "attachments") { t in t.drop(column: column) }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('attachments', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "attachments").contains("duration_ms"))
        XCTAssertFalse(try columnNames(pool, "attachments").contains("poster_storage_key"))

        XCTAssertNoThrow(try migrator.migrate(pool))
        let cols = try columnNames(pool, "attachments")
        XCTAssertTrue(cols.contains("duration_ms"))
        XCTAssertTrue(cols.contains("poster_storage_key"))
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'attachments'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v36 (EXP-848): a store created before `coding_sessions.agent_busy`
    // existed must gain it NOT NULL DEFAULT false (the wire-bool mapping keys
    // on the boolean affinity) and get the coding-sessions offset reset so
    // already-synced rows re-arrive carrying it.
    func testCodingSessionAgentBusyColumnAddedToExistingStore() throws {
        let pool = try makePool("coding-session-agent-busy")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v35_attachment_video_metadata")
        try pool.write { db in
            if try db.columns(in: "coding_sessions").contains(where: { $0.name == "agent_busy" }) {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "agent_busy") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("agent_busy"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let added = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "agent_busy" }
        }
        XCTAssertNotNil(added)
        XCTAssertTrue(added?.isNotNull ?? false)
        XCTAssertTrue(added?.type.uppercased().contains("BOOL") ?? false)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'coding-sessions'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    // v37 (EXP-850 §8): a store created before `coding_sessions.agent_caption`
    // existed must gain it (nullable text — the caption is absent far more
    // often than not) and get the coding-sessions offset reset so already-synced
    // rows re-arrive carrying it.
    func testCodingSessionAgentCaptionColumnAddedToExistingStore() throws {
        let pool = try makePool("coding-session-agent-caption")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v36_coding_session_agent_busy")
        try pool.write { db in
            if try db.columns(in: "coding_sessions").contains(where: { $0.name == "agent_caption" }) {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "agent_caption") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("agent_caption"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let added = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "agent_caption" }
        }
        XCTAssertNotNil(added)
        XCTAssertFalse(added?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'coding-sessions'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    // v41 (EXP-879 session results): a store created before
    // `coding_sessions.results` existed must gain it (nullable text — most
    // runs publish nothing) and get the coding-sessions offset reset so
    // already-synced rows re-arrive carrying it.
    func testCodingSessionResultsColumnAddedToExistingStore() throws {
        let pool = try makePool("coding-session-results")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v40_issue_pr_base_branch")
        try pool.write { db in
            if try db.columns(in: "coding_sessions").contains(where: { $0.name == "results" }) {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "results") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("results"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let added = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "results" }
        }
        XCTAssertNotNil(added)
        XCTAssertFalse(added?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'coding-sessions'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    // v42 (EXP-876 batch run names): a store created before
    // `coding_sessions.batch_issue_ids` existed must gain it (nullable text)
    // and get the coding-sessions offset reset, so already-synced batch rows
    // re-arrive carrying the issues that NAME them.
    func testCodingSessionBatchIssueIdsColumnAddedToExistingStore() throws {
        let pool = try makePool("coding-session-batch-issue-ids")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v41_coding_session_results")
        try pool.write { db in
            if try db.columns(in: "coding_sessions")
                .contains(where: { $0.name == "batch_issue_ids" })
            {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "batch_issue_ids") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("batch_issue_ids"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let added = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "batch_issue_ids" }
        }
        XCTAssertNotNil(added)
        XCTAssertFalse(added?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'coding-sessions'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    // v43 (EXP-909 the run's account): a store created before
    // `coding_sessions.agent_account` existed must gain it (nullable text) and
    // get the coding-sessions offset reset, so already-synced runs re-arrive
    // naming the agent login they spend.
    func testCodingSessionAgentAccountColumnAddedToExistingStore() throws {
        let pool = try makePool("coding-session-agent-account")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v42_coding_session_batch_issue_ids")
        try pool.write { db in
            if try db.columns(in: "coding_sessions")
                .contains(where: { $0.name == "agent_account" })
            {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "agent_account") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("agent_account"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let added = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "agent_account" }
        }
        XCTAssertNotNil(added)
        XCTAssertFalse(added?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'coding-sessions'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    // v44 (EXP-905): a store created before `coding_sessions.agent_title`
    // existed must gain it (nullable text) and get the coding-sessions offset
    // reset, so already-synced chat rows re-arrive carrying their title.
    func testCodingSessionAgentTitleColumnAddedToExistingStore() throws {
        let pool = try makePool("coding-session-agent-title")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v43_coding_session_agent_account")
        try pool.write { db in
            if try db.columns(in: "coding_sessions")
                .contains(where: { $0.name == "agent_title" })
            {
                try db.alter(table: "coding_sessions") { t in t.drop(column: "agent_title") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "coding_sessions").contains("agent_title"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
        let added = try pool.read { db in
            try db.columns(in: "coding_sessions").first { $0.name == "agent_title" }
        }
        XCTAssertNotNil(added)
        XCTAssertFalse(added?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'coding-sessions'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: Self.preSessionTree))
    }

    // v45 (EXP-924): a store created before `devices.icon` existed must gain
    // it (nullable text — NULL is the kind default) and get the devices offset
    // reset, so already-synced rows re-arrive carrying their owner's pick.
    func testDeviceIconColumnAddedToExistingStore() throws {
        let pool = try makePool("device-icon")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v44_coding_session_agent_title")
        try pool.write { db in
            if try db.columns(in: "devices").contains(where: { $0.name == "icon" }) {
                try db.alter(table: "devices") { t in t.drop(column: "icon") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('devices', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "devices").contains("icon"))

        XCTAssertNoThrow(try migrator.migrate(pool))
        let added = try pool.read { db in
            try db.columns(in: "devices").first { $0.name == "icon" }
        }
        XCTAssertNotNil(added)
        XCTAssertFalse(added?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'devices'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v46 (EXP-980): a store created before `notifications.session_id` existed
    // must gain it (nullable text — the run a `session_blocked` row taps into)
    // and get the notifications offset reset, so already-synced rows re-arrive
    // carrying it.
    func testNotificationSessionIdColumnAddedToExistingStore() throws {
        let pool = try makePool("notif-session-id")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v45_device_icon")
        try pool.write { db in
            if try db.columns(in: "notifications").contains(where: { $0.name == "session_id" }) {
                try db.alter(table: "notifications") { t in t.drop(column: "session_id") }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('notifications', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "notifications").contains("session_id"))

        XCTAssertNoThrow(try migrator.migrate(pool))
        let added = try pool.read { db in
            try db.columns(in: "notifications").first { $0.name == "session_id" }
        }
        XCTAssertNotNil(added)
        XCTAssertFalse(added?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'notifications'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v47 (EXP-981 workflows): a store created before the `workflows` and
    // `workflow_nodes` tables existed must gain both. Brand-new shapes have no
    // offset row, so — unlike an added column — NOTHING resets here (the v20
    // `automations` precedent).
    func testWorkflowTablesAddedToExistingStore() throws {
        let pool = try makePool("workflows")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v46_notification_session_id")
        try pool.write { db in
            // Hand-build the pre-v47 state: neither table exists (the guarded
            // creates have to tolerate both forms).
            if try db.tableExists("workflow_nodes") { try db.drop(table: "workflow_nodes") }
            if try db.tableExists("workflows") { try db.drop(table: "workflows") }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('issues', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v47_workflows"))
        XCTAssertTrue(try pool.read { db in try db.tableExists("workflows") })
        XCTAssertTrue(try pool.read { db in try db.tableExists("workflow_nodes") })
        XCTAssertTrue(try columnNames(pool, "workflows").contains("metrics"))
        XCTAssertTrue(try columnNames(pool, "workflow_nodes").contains("wave"))
        // No shape gets re-snapshotted for a brand-new table.
        let untouched = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = 'h' AND \"needs_refetch\" = 0 "
                    + "FROM \"electric_offsets\" WHERE \"shape\" = 'issues'"
            )
        }
        XCTAssertEqual(untouched, true)
        // The rest of the chain runs clean on top (v60 drops both again).
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v48 (EXP-982): a store created before `workflow_nodes.approved_at` and
    // `.note` existed must gain both (nullable text) and get the workflow-nodes
    // offset reset, so already-synced nodes re-arrive carrying them.
    func testWorkflowNodeApprovalColumnsAddedToExistingStore() throws {
        let pool = try makePool("workflow-node-approval")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v47_workflows")
        try pool.write { db in
            for column in ["approved_at", "note"] {
                let present = try db.columns(in: "workflow_nodes").contains { $0.name == column }
                if present {
                    try db.alter(table: "workflow_nodes") { t in t.drop(column: column) }
                }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('workflow-nodes', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "workflow_nodes").contains("approved_at"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v48_workflow_node_approval"))
        let columns = try pool.read { db in try db.columns(in: "workflow_nodes") }
        for name in ["approved_at", "note"] {
            let added = columns.first { $0.name == name }
            XCTAssertNotNil(added, name)
            XCTAssertFalse(added?.isNotNull ?? true, name)
        }
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'workflow-nodes'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v49 (EXP-983): a store created before `workflow_nodes.checkpoint_at` and
    // `.after_node_ids` existed must gain both (nullable text — the node id
    // list is stored like `member_issue_ids`) and get the workflow-nodes offset
    // reset, so already-synced nodes re-arrive carrying them.
    func testWorkflowNodeCheckpointColumnsAddedToExistingStore() throws {
        let pool = try makePool("workflow-node-checkpoint")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v48_workflow_node_approval")
        try pool.write { db in
            for column in ["checkpoint_at", "after_node_ids"] {
                let present = try db.columns(in: "workflow_nodes").contains { $0.name == column }
                if present {
                    try db.alter(table: "workflow_nodes") { t in t.drop(column: column) }
                }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('workflow-nodes', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "workflow_nodes").contains("checkpoint_at"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v49_workflow_node_checkpoint"))
        let columns = try pool.read { db in try db.columns(in: "workflow_nodes") }
        for name in ["checkpoint_at", "after_node_ids"] {
            let added = columns.first { $0.name == name }
            XCTAssertNotNil(added, name)
            XCTAssertFalse(added?.isNotNull ?? true, name)
        }
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'workflow-nodes'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v50 (EXP-984): a store created before `workflow_nodes.review_round` and
    // `.review` existed must gain both — the counter NOT NULL with a 0 default
    // (a node no reviewer ever saw is at round 0), the verdict a nullable text
    // column holding the jsonb — and get the workflow-nodes offset reset, so
    // already-synced nodes re-arrive carrying them.
    func testWorkflowNodeReviewColumnsAddedToExistingStore() throws {
        let pool = try makePool("workflow-node-review")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v49_workflow_node_checkpoint")
        try pool.write { db in
            for column in ["review_round", "review"] {
                let present = try db.columns(in: "workflow_nodes").contains { $0.name == column }
                if present {
                    try db.alter(table: "workflow_nodes") { t in t.drop(column: column) }
                }
            }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('workflow-nodes', 'h', '0_0', 0, 1)
                """)
        }
        XCTAssertFalse(try columnNames(pool, "workflow_nodes").contains("review_round"))

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v50_workflow_node_review"))
        let columns = try pool.read { db in try db.columns(in: "workflow_nodes") }
        let round = columns.first { $0.name == "review_round" }
        XCTAssertNotNil(round)
        XCTAssertTrue(round?.isNotNull ?? false)
        let review = columns.first { $0.name == "review" }
        XCTAssertNotNil(review)
        XCTAssertFalse(review?.isNotNull ?? true)
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'workflow-nodes'"
            )
        }
        XCTAssertEqual(reset, true)
        // Re-running converges without a duplicate-column throw.
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v55 (EXP-1082 workflow contract): a store created before the session
    // membership columns + `pending_question` and the `workflow_events` table
    // must gain all of them and reset the coding-sessions offset.
    func testWorkflowSessionMembershipAndEventsAddedToExistingStore() throws {
        let pool = try makePool("workflow-session-membership")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v54_invite_sent_at")
        let columns = ["workflow_id", "workflow_node_id", "workflow_role", "pending_question"]
        try pool.write { db in
            let existing = Set(try db.columns(in: "coding_sessions").map(\.name))
            for column in columns where existing.contains(column) {
                try db.alter(table: "coding_sessions") { t in t.drop(column: column) }
            }
            if try db.tableExists("workflow_events") { try db.drop(table: "workflow_events") }
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('coding-sessions', 'h', '0_0', 0, 1)
                """)
        }

        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v55_workflow_session_membership_events"))
        let cols = try columnNames(pool, "coding_sessions")
        for column in columns {
            XCTAssertTrue(cols.contains(column), "missing coding_sessions.\(column)")
        }
        XCTAssertTrue(try pool.read { db in try db.tableExists("workflow_events") })
        let eventCols = try columnNames(pool, "workflow_events")
        for column in ["id", "workflow_id", "team_id", "node_id", "session_id", "at", "kind", "message"] {
            XCTAssertTrue(eventCols.contains(column), "missing workflow_events.\(column)")
        }
        let reset = try pool.read { db in
            try Bool.fetchOne(
                db,
                sql: "SELECT \"handle\" = '' AND \"offset\" = '-1' AND \"needs_refetch\" = 1 "
                    + "AND \"is_live\" = 0 FROM \"electric_offsets\" "
                    + "WHERE \"shape\" = 'coding-sessions'"
            )
        }
        XCTAssertEqual(reset, true)
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // v60 (SLOP-3): workflows and stacked starts are gone. A store migrated
    // through v59 loses the three workflow tables and their cursors and the
    // run's workflow membership columns, keeping `issues.pr_base_branch`
    // (the related-work badge reads it), while every surviving issue and coding_sessions row (and every
    // other shape's offset) stays exactly where it was; a re-run is a no-op.
    func testWorkflowsAndStacksDroppedKeepingExistingRows() throws {
        let pool = try makePool("drop-workflows-and-stacks")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v59_action_triggers")
        try pool.write { db in
            try db.execute(sql: """
                INSERT INTO "issues" ("id", "board_id", "title", "status", "priority",
                    "branch", "pr_base_branch", "created_at", "updated_at")
                VALUES ('i-1', 'b-1', 'Keep me', 'in_review', 'none',
                    'exp/EXP-2', 'exp/EXP-1', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')
                """)
            try db.execute(sql: """
                INSERT INTO "coding_sessions" ("id", "issue_id", "team_id", "user_id",
                    "device_label", "status", "workflow_id", "workflow_node_id",
                    "workflow_role", "pending_question", "started_at", "created_at",
                    "updated_at")
                VALUES ('cs-1', 'i-1', 't-1', 'u-1', 'macbook', 'running', 'wf-1',
                    'n-1', 'author', '{"question":"?"}', '2026-10-01T00:00:00Z',
                    '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')
                """)
            try db.execute(sql: """
                INSERT INTO "workflows" ("id", "team_id", "name")
                VALUES ('wf-1', 't-1', 'Ship it')
                """)
            for shape in ["issues", "coding-sessions", "workflows", "workflow-nodes", "workflow-events"] {
                try db.execute(
                    sql: """
                        INSERT INTO "electric_offsets"
                            ("shape", "handle", "offset", "needs_refetch", "is_live")
                        VALUES (?, 'h', '0_0', 0, 1)
                        """,
                    arguments: [shape]
                )
            }
        }

        // Only UP TO v60: v65 (SLOP-17) resets the coding-sessions cursor.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v60_drop_workflows_and_stacks"))
        for table in ["workflows", "workflow_nodes", "workflow_events"] {
            XCTAssertFalse(try pool.read { db in try db.tableExists(table) }, table)
        }
        XCTAssertTrue(try columnNames(pool, "issues").contains("pr_base_branch"))
        let sessionCols = try columnNames(pool, "coding_sessions")
        for column in ["workflow_id", "workflow_node_id", "workflow_role"] {
            XCTAssertFalse(sessionCols.contains(column), column)
        }
        XCTAssertTrue(sessionCols.contains("pending_question"))

        // The rows survive, columns intact.
        let issue = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: #"SELECT "title", "branch", "pr_base_branch" FROM "issues" WHERE "id" = 'i-1'"#
            )
        }
        XCTAssertEqual(issue?["title"] as String?, "Keep me")
        XCTAssertEqual(issue?["branch"] as String?, "exp/EXP-2")
        XCTAssertEqual(issue?["pr_base_branch"] as String?, "exp/EXP-1")
        let session = try pool.read { db in
            try Row.fetchOne(
                db,
                sql: #"SELECT "issue_id", "pending_question" FROM "coding_sessions" WHERE "id" = 'cs-1'"#
            )
        }
        XCTAssertEqual(session?["issue_id"] as String?, "i-1")
        XCTAssertEqual(session?["pending_question"] as String?, #"{"question":"?"}"#)

        // The retired shapes' cursors are gone; the survivors keep theirs.
        let shapes = try pool.read { db in
            try String.fetchAll(
                db,
                sql: #"SELECT "shape" FROM "electric_offsets" WHERE "handle" = 'h' ORDER BY "shape""#
            )
        }
        XCTAssertEqual(shapes, ["coding-sessions", "issues"])

        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // EXP-1323 v69: an existing session-tree store gains `sessions.caption`/
    // `aside` and `messages.author_session_id` (their cursors reset so synced
    // rows re-arrive carrying them) and the `session_reads` table.
    func testSessionTalkColumnsAndReadsTable() throws {
        let pool = try makePool("session-talk")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v68_session_tree")
        try pool.write { db in
            for shape in ["sessions", "messages", "issues"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v77_message_conversation"))
        try pool.read { db in
            let sessions = Set(try db.columns(in: "sessions").map(\.name))
            XCTAssertTrue(sessions.isSuperset(of: ["caption", "aside"]))
            XCTAssertTrue(try db.columns(in: "messages").contains { $0.name == "author_session_id" })
            XCTAssertTrue(try db.tableExists("session_reads"))
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["messages", "sessions"])
        }
    }

    // EXP-1252: v72 adds `sessions.move_to_device_id` and resets ONLY the
    // sessions cursor, so synced rows re-arrive carrying a pending move.
    func testSessionMoveToDeviceColumn() throws {
        let pool = try makePool("session-move")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v71_session_issue_assignee")
        try pool.write { db in
            for shape in ["sessions", "messages"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        // Up to v74 only: v75 resets the messages cursor of its own.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v74_agents_events_prompts"))
        try pool.read { db in
            XCTAssertTrue(try db.columns(in: "sessions").contains { $0.name == "move_to_device_id" })
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["sessions"])
        }
    }

    // NEW-2: v74 adds the `agents` / `events` / `prompts` tables and
    // `sessions.agent_id`, resetting ONLY the sessions cursor so synced rows
    // re-arrive carrying their agent.
    func testAgentsEventsPromptsTablesAndSessionAgentId() throws {
        let pool = try makePool("new2-agents")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v73_issue_identifier_upper_index")
        try pool.write { db in
            for shape in ["sessions", "messages", "actions"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        // Up to v74 only: v75 resets the messages cursor of its own.
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v74_agents_events_prompts"))
        try pool.read { db in
            XCTAssertTrue(try db.columns(in: "sessions").contains { $0.name == "agent_id" })
            XCTAssertTrue(try db.tableExists("agents"))
            XCTAssertTrue(try db.tableExists("events"))
            XCTAssertTrue(try db.tableExists("prompts"))
            // The legacy actions table stays (served empty until NEW-8).
            XCTAssertTrue(try db.tableExists("actions"))
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["sessions"])
        }
        // Idempotent: a second run is a no-op.
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // NEW-3: v75 adds the `conversations` table (the shape allowlist) and
    // `messages.conversation_id`, resetting ONLY the messages cursor so
    // synced rows re-arrive carrying their conversation.
    func testConversationsTableAndMessageConversationId() throws {
        let pool = try makePool("new3-conversations")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v74_agents_events_prompts")
        try pool.write { db in
            for shape in ["sessions", "messages"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v75_conversations"))
        XCTAssertEqual(
            try columnNames(pool, "conversations"),
            ["id", "team_id", "kind", "session_id", "title", "created_at", "updated_at"]
        )
        try pool.read { db in
            XCTAssertTrue(try db.columns(in: "messages").contains { $0.name == "conversation_id" })
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["messages"])
        }
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // VAPP-110: v76 adds `apps` + `app_versions` (the shape allowlists) and
    // `sessions.app_id`, resetting ONLY the sessions cursor.
    func testAppsTablesAndSessionAppId() throws {
        let pool = try makePool("vapp110-apps")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v75_conversations")
        try pool.write { db in
            for shape in ["sessions", "messages"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertEqual(
            try columnNames(pool, "apps"),
            ["id", "team_id", "slug", "name", "icon", "color", "description", "package",
             "head_version", "created_by", "deleted_at", "created_at", "updated_at",
             "granted_tools"]
        )
        XCTAssertEqual(
            try columnNames(pool, "app_versions"),
            ["id", "app_id", "team_id", "number", "note", "session_id", "created_by", "created_at"]
        )
        try pool.read { db in
            XCTAssertTrue(try db.columns(in: "sessions").contains { $0.name == "app_id" })
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["sessions"])
        }
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // VAPP-120: v82 adds `apps.granted_tools` (JSON text, '[]' = none) to
    // existing rows without a cursor reset; a synced row's jsonb list
    // decodes from the wire and round-trips through the store.
    func testAppsGrantedToolsColumn() throws {
        let pool = try makePool("vapp120-granted-tools")
        let migrator = DatabaseManager.makeMigrator()
        // A device on master's head (v81) upgrades to this build.
        try migrator.migrate(pool, upTo: "v81_conversation_digest")
        try pool.write { db in
            try db.execute(sql: """
                INSERT INTO "apps" ("id", "team_id", "slug", "name", "created_at", "updated_at")
                VALUES ('a0', 't1', 'old', 'Old', '2026-10-05T18:00:00Z', '2026-10-05T18:00:00Z')
                """)
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('apps', 'h', '0_0', 0, 1)
                ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                    "needs_refetch" = 0, "is_live" = 1
                """)
        }
        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertTrue(try columnNames(pool, "apps").contains("granted_tools"))
        try pool.read { db in
            let old = try AppEntity.fetchOne(db, key: "a0")
            XCTAssertEqual(old?.grantedTools, [])
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, [])
        }
        for (wireValue, expected) in [
            (#"["exponential.issues.update","exponential.sessions.send"]"#, ["exponential.issues.update", "exponential.sessions.send"]),
            (#""[\"exponential.issues.update\"]""#, ["exponential.issues.update"]),
            ("null", []),
        ] {
            let wire = Data("""
                {"id":"a1","team_id":"t1","slug":"hours","name":"Hours","granted_tools":\(wireValue),
                 "created_at":"2026-10-05T18:00:00Z","updated_at":"2026-10-05T18:00:00Z"}
                """.utf8)
            let decoded = try JSONDecoder().decode(AppEntity.self, from: wire)
            XCTAssertEqual(decoded.grantedTools, expected, wireValue)
            try pool.write { db in try decoded.save(db) }
            let stored = try pool.read { db in try AppEntity.fetchOne(db, key: "a1") }
            XCTAssertEqual(stored?.grantedTools, expected, wireValue)
        }
        let missing = try JSONDecoder().decode(AppEntity.self, from: Data(#"""
            {"id":"a2","team_id":"t1","created_at":"2026-10-05T18:00:00Z","updated_at":"2026-10-05T18:00:00Z"}
            """#.utf8))
        XCTAssertEqual(missing.grantedTools, [])
    }

    // VAPP-122: v79 adds `app_records` (the shape allowlist) with its
    // (app_id, kind) and team_id indexes; no cursor resets.
    func testAppRecordsTable() throws {
        let pool = try makePool("vapp122-app-records")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v78_drop_actions_and_legacy_session_columns")
        try pool.write { db in
            try db.execute(sql: """
                INSERT INTO "electric_offsets"
                    ("shape", "handle", "offset", "needs_refetch", "is_live")
                VALUES ('apps', 'h', '0_0', 0, 1)
                ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                    "needs_refetch" = 0, "is_live" = 1
                """)
        }
        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertEqual(
            try columnNames(pool, "app_records"),
            ["id", "app_id", "team_id", "kind", "data", "created_by", "created_at", "updated_at"]
        )
        try pool.read { db in
            let indexes = try db.indexes(on: "app_records").map(\.columns)
            XCTAssertTrue(indexes.contains(["app_id", "kind"]))
            XCTAssertTrue(indexes.contains(["team_id"]))
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, [])
        }
        // A synced row round-trips, its jsonb `data` as text.
        let wire = Data(#"""
            {"id":"r1","app_id":"a1","team_id":"t1","kind":"entry","data":{"hours":3,"done":false},
             "created_by":"u1","created_at":"2026-10-05T18:00:00Z","updated_at":"2026-10-05T18:00:00Z"}
            """#.utf8)
        let decoded = try JSONDecoder().decode(AppRecordEntity.self, from: wire)
        XCTAssertEqual(
            try JSONSerialization.jsonObject(with: Data(decoded.data.utf8)) as? NSDictionary,
            ["hours": 3, "done": false] as NSDictionary
        )
        try pool.write { db in try decoded.insert(db) }
        let stored = try pool.read { db in try AppRecordEntity.fetchOne(db, key: "r1") }
        XCTAssertEqual(stored, decoded)
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // NEW-8: v77 drops `messages.session_id` (a message's session is its
    // conversation's), dropping only rows without a conversation and then
    // resetting ONLY the messages cursor.
    func testMessageSessionIdDropped() throws {
        let pool = try makePool("new8-message-conversation")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v76_apps")
        try pool.write { db in
            for shape in ["sessions", "messages"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
            for (id, conversation) in [("m-kept", "c-1"), ("m-orphan", nil)] as [(String, String?)] {
                try db.execute(sql: """
                    INSERT INTO "messages" ("id", "session_id", "root_id", "team_id", "author_kind",
                        "created_at", "updated_at", "conversation_id")
                    VALUES (?, 's1', 's1', 't', 'user', 'x', 'x', ?)
                    """, arguments: [id, conversation])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v78_drop_actions_and_legacy_session_columns"))
        XCTAssertFalse(try columnNames(pool, "messages").contains("session_id"))
        try pool.read { db in
            XCTAssertEqual(try String.fetchAll(db, sql: "SELECT \"id\" FROM \"messages\""), ["m-kept"])
            XCTAssertTrue(try db.indexes(on: "messages").contains { $0.columns == ["conversation_id"] })
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["messages"])
        }
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // NEW-8: v78 drops the `actions` table and its cursor, the action pins
    // and `pins.action_id`, the legacy `sessions` columns and
    // `session_issues.assigned_session_id`; surviving rows and every other
    // cursor stay.
    func testActionsAndLegacySessionColumnsDropped() throws {
        let pool = try makePool("new8-drop-legacy")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v77_message_conversation")
        try pool.write { db in
            for shape in ["sessions", "actions", "pins"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
            try db.execute(sql: """
                INSERT INTO "actions" ("id", "team_id", "name", "created_at", "updated_at")
                VALUES ('a-1', 't', 'Ship', 'x', 'x')
                """)
            try db.execute(sql: """
                INSERT INTO "pins" ("id", "user_id", "team_id", "kind", "issue_id", "action_id",
                    "sort_order", "created_at", "updated_at")
                VALUES ('p-issue', 'u', 't', 'issue', 'i-1', NULL, 0, 'x', 'x'),
                       ('p-action', 'u', 't', 'action', NULL, 'a-1', 1, 'x', 'x')
                """)
            try db.execute(sql: """
                INSERT INTO "sessions" ("id", "team_id", "root_id", "kind", "user_id", "device_id",
                    "name", "caption", "aside", "dispatch", "move_to_device_id", "sort_order",
                    "created_at", "updated_at")
                VALUES ('s-1', 't', 's-1', 'agent', 'u', 'd', 'Caretaker', 'Reading', 1, 1, 'd2', 2,
                    'x', 'x')
                """)
            try db.execute(sql: """
                INSERT INTO "session_issues" ("id", "session_id", "issue_id", "team_id",
                    "assigned_session_id", "created_at", "updated_at")
                VALUES ('si-1', 's-1', 'i-1', 't', 's-2', 'x', 'x')
                """)
        }
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v78_drop_actions_and_legacy_session_columns"))
        let sessionCols = Set(try columnNames(pool, "sessions"))
        for (table, columns) in DatabaseManager.newEightDeadColumns {
            let cols = Set(try columnNames(pool, table))
            for column in columns {
                XCTAssertFalse(cols.contains(column), "\(table).\(column)")
            }
        }
        XCTAssertTrue(sessionCols.isSuperset(of: ["agent_id", "app_id", "device_id", "plan_mode"]))
        try pool.read { db in
            XCTAssertFalse(try db.tableExists("actions"))
            XCTAssertEqual(try String.fetchAll(db, sql: "SELECT \"id\" FROM \"pins\""), ["p-issue"])
            XCTAssertEqual(try String.fetchAll(db, sql: "SELECT \"id\" FROM \"sessions\""), ["s-1"])
            XCTAssertEqual(try String.fetchAll(db, sql: "SELECT \"id\" FROM \"session_issues\""), ["si-1"])
            XCTAssertEqual(
                try String.fetchAll(db, sql: "SELECT \"shape\" FROM \"electric_offsets\" ORDER BY \"shape\""),
                ["pins", "sessions"]
            )
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1'"
            )
            XCTAssertEqual(reset, [])
        }
        // The synced row still decodes and saves without the dropped columns.
        let row = try pool.read { db in try SessionEntity.fetchOne(db, key: "s-1") }
        XCTAssertEqual(row?.deviceId, "d")
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // #EXP-1383: v80 adds the agent-actor columns; a shape's cursor resets
    // only when one of its columns was added, every other cursor stays, and
    // a synced row decodes and saves carrying them.
    func testAgentActorColumnsAdded() throws {
        let pool = try makePool("exp1383-agent-actors")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v78_drop_actions_and_legacy_session_columns")
        try pool.write { db in
            for shape in ["issues", "comments", "issue-events", "pins"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool))
        for (table, _, columns) in DatabaseManager.agentActorColumns {
            let cols = Set(try columnNames(pool, table))
            for column in columns {
                XCTAssertTrue(cols.contains(column), "\(table).\(column)")
            }
        }
        try pool.read { db in
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["comments", "issue-events", "issues"])
        }
        let event = IssueEventEntity(
            id: "e-1", issueId: "i-1", teamId: "t", actorUserId: "u", type: "status_changed",
            payload: nil, createdAt: "x", updatedAt: "x", actorAgentId: "g-1"
        )
        try pool.write { db in try event.save(db) }
        let read = try pool.read { db in try IssueEventEntity.fetchOne(db, key: "e-1") }
        XCTAssertEqual(read?.actorAgentId, "g-1")
        // Re-running converges (the guarded ALTERs add nothing twice).
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // Thread e4a04d22: v81 adds `conversations.digest`, resetting ONLY the
    // conversations cursor; a synced row decodes the jsonb (object or text)
    // and saves it as JSON text.
    func testConversationDigestColumnAdded() throws {
        let pool = try makePool("e4a04d22-conversation-digest")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v80_agent_actors")
        try pool.write { db in
            for shape in ["conversations", "messages"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertTrue(try columnNames(pool, "conversations").contains("digest"))
        try pool.read { db in
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["conversations"])
        }
        let wire = Data("""
            {"id":"c-1","team_id":"t","kind":"session","session_id":"s-1","title":null,
             "created_at":"x","updated_at":"x","digest":{"agentAt":"2026-10-24T12:00:00Z","agentTimes":[]}}
            """.utf8)
        let row = try JSONDecoder().decode(ConversationEntity.self, from: wire)
        try pool.write { db in try row.save(db) }
        let read = try pool.read { db in try ConversationEntity.fetchOne(db, key: "c-1") }
        let digest = try XCTUnwrap(read?.digest)
        XCTAssertEqual(
            Chats.digestMessages(sessionId: "s-1", digest: digest).map(\.id),
            ["digest:s-1:agent"]
        )
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // EXP-1374: v83 adds `sessions.background` (false on existing rows),
    // resetting ONLY the sessions cursor; a synced row decodes the wire bool
    // (absent = false) and round-trips through the store.
    func testSessionBackgroundColumnAdded() throws {
        let pool = try makePool("exp1374-session-background")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v82_apps_granted_tools")
        try pool.write { db in
            for shape in ["sessions", "messages"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
            try db.execute(sql: """
                INSERT INTO "sessions" ("id", "team_id", "root_id", "user_id", "device_id", "created_at", "updated_at")
                VALUES ('s-0', 't', 's-0', 'u', 'd', 'x', 'x')
                """)
        }
        XCTAssertNoThrow(try migrator.migrate(pool))
        XCTAssertTrue(try columnNames(pool, "sessions").contains("background"))
        try pool.read { db in
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["sessions"])
            XCTAssertEqual(try SessionEntity.fetchOne(db, key: "s-0")?.background, false)
        }
        for (wireValue, expected) in [("true", true), (#""t""#, true), (#""f""#, false)] {
            let wire = Data("""
                {"id":"s-1","team_id":"t","root_id":"s-1","user_id":"u","device_id":"d","busy":true,
                 "background":\(wireValue),"created_at":"x","updated_at":"x"}
                """.utf8)
            let row = try JSONDecoder().decode(SessionEntity.self, from: wire)
            XCTAssertEqual(row.background, expected, wireValue)
            try pool.write { db in try row.save(db) }
            let read = try pool.read { db in try SessionEntity.fetchOne(db, key: "s-1") }
            XCTAssertEqual(read?.background, expected, wireValue)
        }
        let missing = try JSONDecoder().decode(SessionEntity.self, from: Data(#"""
            {"id":"s-2","team_id":"t","user_id":"u","device_id":"d","created_at":"x","updated_at":"x"}
            """#.utf8))
        XCTAssertFalse(missing.background)
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // S-3: v85 adds `session_reads.story_seen_at`, resetting ONLY the
    // session-reads cursor so synced read marks re-arrive with it.
    func testSessionReadStorySeenAtColumnAdded() throws {
        let pool = try makePool("s3-session-read-story-seen")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v84_session_read_archived_at")
        try pool.write { db in
            for shape in ["session-reads", "sessions"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v86_session_read_story_seen_at"))
        XCTAssertTrue(try columnNames(pool, "session_reads").contains("story_seen_at"))
        try pool.read { db in
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["session-reads"])
        }
    }

    // #EXP-1388: v84 adds `session_reads.archived_at`, resetting ONLY the
    // session-reads cursor so synced read marks re-arrive with it.
    func testSessionReadArchivedAtColumnAdded() throws {
        let pool = try makePool("exp1388-session-read-archived")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v83_session_background")
        try pool.write { db in
            for shape in ["session-reads", "sessions"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v86_session_read_story_seen_at"))
        XCTAssertTrue(try columnNames(pool, "session_reads").contains("archived_at"))
        try pool.read { db in
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["session-reads"])
        }
        let wire = Data("""
            {"id":"r-1","user_id":"u","session_id":"s-1","read_at":"2026-10-27T10:00:00Z",
             "archived_at":"2026-10-27T11:00:00Z","created_at":"x","updated_at":"x"}
            """.utf8)
        let row = try JSONDecoder().decode(SessionReadEntity.self, from: wire)
        try pool.write { db in try row.save(db) }
        let read = try pool.read { db in try SessionReadEntity.fetchOne(db, key: "r-1") }
        XCTAssertEqual(read?.archivedAt, "2026-10-27T11:00:00Z")
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // EXP-1458: v87 adds the tree key + sealed title to `sessions`,
    // resetting ONLY the sessions cursor so synced roots re-arrive with them.
    func testSessionSealColumnsAdded() throws {
        let pool = try makePool("exp1458-session-seal")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v86_session_read_story_seen_at")
        try pool.write { db in
            for shape in ["messages", "sessions"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v87_session_seal"))
        try pool.read { db in
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["sessions"])
        }
        let wire = Data("""
            {"id":"s-1","team_id":"t","user_id":"u","device_id":"d","created_at":"x","updated_at":"x",
             "seal_public_key":"NYBy1jZYgNGu6jKa35EhODhR7SGijjt16WXQ0s0WYlQ","seal_epoch":"2",
             "sealed_title":"exp-sealed:v1:2:abc"}
            """.utf8)
        let row = try JSONDecoder().decode(SessionEntity.self, from: wire)
        try pool.write { db in try row.save(db) }
        let read = try pool.read { db in try SessionEntity.fetchOne(db, key: "s-1") }
        XCTAssertEqual(read?.sealKey, TreeSealKey(publicKey: "NYBy1jZYgNGu6jKa35EhODhR7SGijjt16WXQ0s0WYlQ", epoch: 2))
        XCTAssertEqual(read?.sealedTitle, "exp-sealed:v1:2:abc")
        // Locked (no key here): the clear title stands in.
        XCTAssertEqual(read?.openedTitle, nil)
        try pool.write { db in try SealedRowsTouch.touch(db, rootIds: ["s-1"]) }
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    // S-3: v85 adds `agents.role` (existing rows = agent), resetting ONLY
    // the agents cursor so synced definitions re-arrive with it.
    func testAgentRoleColumnAdded() throws {
        let pool = try makePool("s3-agent-role")
        let migrator = DatabaseManager.makeMigrator()
        try migrator.migrate(pool, upTo: "v84_session_read_archived_at")
        try pool.write { db in
            try db.execute(sql: """
                INSERT INTO "agents" ("id", "team_id", "name", "created_at", "updated_at")
                VALUES ('old', 't', 'Old', 'x', 'x')
                """)
            for shape in ["agents", "sessions"] {
                try db.execute(sql: """
                    INSERT INTO "electric_offsets"
                        ("shape", "handle", "offset", "needs_refetch", "is_live")
                    VALUES (?, 'h', '0_0', 0, 1)
                    ON CONFLICT("shape") DO UPDATE SET "handle" = 'h', "offset" = '0_0',
                        "needs_refetch" = 0, "is_live" = 1
                    """, arguments: [shape])
            }
        }
        XCTAssertNoThrow(try migrator.migrate(pool, upTo: "v85_agent_role"))
        try pool.read { db in
            let reset = try String.fetchAll(
                db,
                sql: "SELECT \"shape\" FROM \"electric_offsets\" WHERE \"offset\" = '-1' ORDER BY \"shape\""
            )
            XCTAssertEqual(reset, ["agents"])
            XCTAssertEqual(try AgentDefinitionEntity.fetchOne(db, key: "old")?.role, "agent")
        }
        let wire = Data("""
            {"id":"as-1","team_id":"t","name":"Assistant","owner_id":"u","role":"assistant",
             "triggers":[],"sort_order":0,"created_at":"x","updated_at":"x"}
            """.utf8)
        let row = try JSONDecoder().decode(AgentDefinitionEntity.self, from: wire)
        try pool.write { db in try row.save(db) }
        let read = try pool.read { db in try AgentDefinitionEntity.fetchOne(db, key: "as-1") }
        XCTAssertEqual(read?.role, "assistant")
        XCTAssertEqual(read?.isAssistant, true)
        XCTAssertNoThrow(try DatabaseManager.runMigrations(on: pool))
    }

    func testFileURLUsesV5Suffix() throws {
        let url = try DatabaseManager.fileURL(for: "acct")
        XCTAssertEqual(url.lastPathComponent, "exponential-acct-v5.sqlite")
    }
}
