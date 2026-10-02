package com.exponential.app.data.db

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/**
 * SLOP-3: the FIRST explicit migration. Every older version still falls back
 * to the destructive wipe + Electric resync ([DatabaseHolder]); this one keeps
 * every row and every other shape's offset.
 *
 * The EXP-978 workflows are gone:
 *  - the `workflows` / `workflow_nodes` / `workflow_events` tables drop, and so
 *    do their Electric offsets (plus any shape-repair marker naming them), so
 *    nothing ever resumes those shapes;
 *  - `coding_sessions.workflow_id / workflow_node_id / workflow_role` leave
 *    the table. Room validates every entity against its table after an
 *    explicit migration, so it is REBUILT with exactly the schema Room's
 *    generated `createAllTables` emits (create new, copy the kept columns,
 *    drop, rename, recreate the indices).
 *
 * `issues` is untouched: `pr_base_branch` stays (the related-work badge reads
 * the stack edge), so v78's table already matches the v79 entity.
 */
val MIGRATION_78_79: Migration = object : Migration(78, 79) {
    override fun migrate(db: SupportSQLiteDatabase) {
        for (table in DROPPED_WORKFLOW_TABLES) {
            db.execSQL("DROP TABLE IF EXISTS `$table`")
        }
        val shapes = DROPPED_WORKFLOW_TABLES.joinToString(", ") { "'$it'" }
        val repairMarkers = DROPPED_WORKFLOW_TABLES.joinToString(" OR ") {
            "(substr(`shape`, 1, 8) = '_repair:' AND substr(`shape`, -${it.length + 1}) = ':$it')"
        }
        db.execSQL("DELETE FROM `electric_offsets` WHERE `shape` IN ($shapes) OR $repairMarkers")

        rebuild(db, "coding_sessions", CODING_SESSIONS_V79, CODING_SESSION_COLUMNS_V79, CODING_SESSION_INDICES_V79)
    }
}

/** The tables (and Electric shape names) v79 no longer has. */
internal val DROPPED_WORKFLOW_TABLES = listOf("workflows", "workflow_nodes", "workflow_events")

private fun rebuild(
    db: SupportSQLiteDatabase,
    table: String,
    create: String,
    columns: List<String>,
    indices: List<String>,
) {
    val tmp = "${table}_slop3"
    db.execSQL("DROP TABLE IF EXISTS `$tmp`")
    db.execSQL(create.replace("CREATE TABLE IF NOT EXISTS `$table`", "CREATE TABLE `$tmp`"))
    val list = columns.joinToString(", ") { "`$it`" }
    db.execSQL("INSERT INTO `$tmp` ($list) SELECT $list FROM `$table`")
    db.execSQL("DROP TABLE `$table`")
    db.execSQL("ALTER TABLE `$tmp` RENAME TO `$table`")
    for (index in indices) db.execSQL(index)
}

// ── v79 schema, verbatim from the generated ExponentialDatabase_Impl ────────

private const val CODING_SESSIONS_V79 =
    "CREATE TABLE IF NOT EXISTS `coding_sessions` (`id` TEXT NOT NULL, `issue_id` TEXT, " +
        "`team_id` TEXT NOT NULL, `board_id` TEXT, `user_id` TEXT NOT NULL, `device_label` TEXT, " +
        "`device_id` TEXT, `status` TEXT NOT NULL, `branch` TEXT, `agent` TEXT, `agent_account` TEXT, " +
        "`ended_by` TEXT, `resumed_from_id` TEXT, `parent_session_id` TEXT, " +
        "`needs_input` INTEGER NOT NULL, `agent_busy` INTEGER NOT NULL, `agent_caption` TEXT, " +
        "`agent_title` TEXT, `blocked` TEXT, `pending_question` TEXT, `results` TEXT, " +
        "`batch_issue_ids` TEXT, `action_id` TEXT, `action_name` TEXT, `started_reason` TEXT, " +
        "`automation_id` TEXT, `pr_url` TEXT, `pr_number` INTEGER, `pr_state` TEXT, " +
        "`started_at` TEXT NOT NULL, `ended_at` TEXT, `created_at` TEXT NOT NULL, " +
        "`updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))"

private val CODING_SESSION_COLUMNS_V79 = listOf(
    "id", "issue_id", "team_id", "board_id", "user_id", "device_label", "device_id", "status",
    "branch", "agent", "agent_account", "ended_by", "resumed_from_id", "parent_session_id",
    "needs_input", "agent_busy", "agent_caption", "agent_title", "blocked", "pending_question",
    "results", "batch_issue_ids", "action_id", "action_name", "started_reason", "automation_id",
    "pr_url", "pr_number", "pr_state", "started_at", "ended_at", "created_at", "updated_at",
)

private val CODING_SESSION_INDICES_V79 = listOf(
    "CREATE INDEX IF NOT EXISTS `index_coding_sessions_issue_id` ON `coding_sessions` (`issue_id`)",
    "CREATE INDEX IF NOT EXISTS `index_coding_sessions_team_id` ON `coding_sessions` (`team_id`)",
)
