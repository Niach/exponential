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

/**
 * SLOP-4: a widget submission IS an issue and its reporter conversation is
 * comments, so the local comment row changes shape and the team row sheds the
 * Support switch:
 *  - `comments.author_id` goes NULLABLE (a reporter's comment, source
 *    `reporter`, has no users row) and `comments.audience` arrives
 *    (`team` | `reporter`, NOT NULL; every existing row is a team comment);
 *  - the EXP-180 team support switch column drops from `teams`.
 * SQLite cannot relax a NOT NULL constraint in place, so `comments` is REBUILT
 * exactly like v79 rebuilt `coding_sessions` (Room validates the table against
 * the entity after an explicit migration); `teams` takes the same route for
 * the dropped column. Every row survives, and so does every Electric offset —
 * the comments and teams shapes resume where they were.
 */
val MIGRATION_79_80: Migration = object : Migration(79, 80) {
    override fun migrate(db: SupportSQLiteDatabase) {
        rebuild(
            db, "comments", COMMENTS_V80, COMMENT_COLUMNS_V80, COMMENT_INDICES_V80,
            // The new NOT NULL column has no source column: select the default.
            selectOverrides = mapOf("audience" to "'team'"),
        )
        rebuild(db, "teams", TEAMS_V80, TEAM_COLUMNS_V80, emptyList())
    }
}

private fun rebuild(
    db: SupportSQLiteDatabase,
    table: String,
    create: String,
    columns: List<String>,
    indices: List<String>,
    /** Column → SQL expression to select INSTEAD of the old table's column of
     *  that name (a brand-new NOT NULL column has nothing to copy from). */
    selectOverrides: Map<String, String> = emptyMap(),
) {
    val tmp = "${table}_slop3"
    db.execSQL("DROP TABLE IF EXISTS `$tmp`")
    db.execSQL(create.replace("CREATE TABLE IF NOT EXISTS `$table`", "CREATE TABLE `$tmp`"))
    val list = columns.joinToString(", ") { "`$it`" }
    val select = columns.joinToString(", ") { selectOverrides[it] ?: "`$it`" }
    db.execSQL("INSERT INTO `$tmp` ($list) SELECT $select FROM `$table`")
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

// ── v80 schema, verbatim from the generated ExponentialDatabase_Impl ────────

private const val COMMENTS_V80 =
    "CREATE TABLE IF NOT EXISTS `comments` (`id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, " +
        "`team_id` TEXT NOT NULL, `board_id` TEXT, `author_id` TEXT, `parent_id` TEXT, " +
        "`source` TEXT, `audience` TEXT NOT NULL, `body` TEXT, `kind` TEXT NOT NULL, " +
        "`edited_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, " +
        "PRIMARY KEY(`id`))"

private val COMMENT_COLUMNS_V80 = listOf(
    "id", "issue_id", "team_id", "board_id", "author_id", "parent_id", "source", "audience",
    "body", "kind", "edited_at", "created_at", "updated_at",
)

private val COMMENT_INDICES_V80 = listOf(
    "CREATE INDEX IF NOT EXISTS `index_comments_issue_id` ON `comments` (`issue_id`)",
    "CREATE INDEX IF NOT EXISTS `index_comments_team_id` ON `comments` (`team_id`)",
    "CREATE INDEX IF NOT EXISTS `index_comments_parent_id` ON `comments` (`parent_id`)",
)

private const val TEAMS_V80 =
    "CREATE TABLE IF NOT EXISTS `teams` (`id` TEXT NOT NULL, `name` TEXT NOT NULL, " +
        "`slug` TEXT NOT NULL, `icon_url` TEXT, `yolo_mode` INTEGER NOT NULL, " +
        "`estimation_type` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, " +
        "PRIMARY KEY(`id`))"

private val TEAM_COLUMNS_V80 = listOf(
    "id", "name", "slug", "icon_url", "yolo_mode", "estimation_type", "created_at", "updated_at",
)
