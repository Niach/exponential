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
 * the dropped column. Every row survives. The teams offset survives too, but
 * the comments shape is marked for the atomic refetch (the inline
 * must-refetch recipe in [com.exponential.app.data.electric.ShapeClient]): an
 * older build may already have synced the new comments shape and dropped
 * every NULL-author reporter row it could not decode, and that handle never
 * 409s, so only a fresh snapshot brings those rows back. The old rows stay
 * visible until it lands.
 */
val MIGRATION_79_80: Migration = object : Migration(79, 80) {
    override fun migrate(db: SupportSQLiteDatabase) {
        rebuild(
            db, "comments", COMMENTS_V80, COMMENT_COLUMNS_V80, COMMENT_INDICES_V80,
            // The new NOT NULL column has no source column: select the default.
            selectOverrides = mapOf("audience" to "'team'"),
        )
        rebuild(db, "teams", TEAMS_V80, TEAM_COLUMNS_V80, emptyList())
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'comments'",
        )
    }
}

/**
 * EXP-1196/1218/1219: `devices.doctor` arrives — the machine's readiness
 * report, a nullable jsonb kept as raw text. A plain additive column (Room
 * reads a NULL-default TEXT column as the entity's nullable String). Every row
 * survives; the devices shape is marked for the atomic refetch so rows synced
 * before the column existed come back carrying it.
 */
val MIGRATION_80_81: Migration = object : Migration(80, 81) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `devices` ADD COLUMN `doctor` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'devices'",
        )
    }
}

/**
 * v82 (EXP-1231): `issues.draft_id`, the draft an issue was created from. The
 * nullable column lands empty, so the issues shape refetches (its offset
 * resets) and already-synced rows re-arrive carrying it.
 */
val MIGRATION_81_82: Migration = object : Migration(81, 82) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `issues` ADD COLUMN `draft_id` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'issues'",
        )
    }
}

/**
 * v83 (EXP-1247): `attachments.as_file`, set by the FILE/paperclip upload
 * paths. The column lands false, so the attachments shape refetches (its
 * offset resets) and already-synced rows re-arrive carrying it.
 */
val MIGRATION_82_83: Migration = object : Migration(82, 83) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `attachments` ADD COLUMN `as_file` INTEGER NOT NULL DEFAULT 0")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'attachments'",
        )
    }
}

/**
 * v84 (SLOP-17): automations are triggers everywhere. The synced
 * `coding_sessions.automation_id` is served as `trigger_id` now (the id of the
 * action trigger that fired the run), so the column is renamed. minSdk 26
 * predates SQLite's RENAME COLUMN, so `coding_sessions` is REBUILT like v79
 * did, copying `automation_id` into `trigger_id`; every row survives and the
 * coding-sessions shape refetches (its served column list changed). Any
 * orphaned pre-SLOP-2 `automations` table and its Electric offset (plus a
 * shape-repair marker naming it) drop too, idempotently.
 */
val MIGRATION_83_84: Migration = object : Migration(83, 84) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("DROP TABLE IF EXISTS `automations`")
        db.execSQL(
            "DELETE FROM `electric_offsets` WHERE `shape` = 'automations' OR " +
                "(substr(`shape`, 1, 8) = '_repair:' AND substr(`shape`, -12) = ':automations')",
        )
        rebuild(
            db, "coding_sessions", CODING_SESSIONS_V84, CODING_SESSION_COLUMNS_V84,
            CODING_SESSION_INDICES_V79,
            selectOverrides = mapOf("trigger_id" to "`automation_id`"),
        )
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'coding_sessions'",
        )
    }
}

/**
 * v85 (EXP-1284): threads — the unit of work. Two NEW tables (`threads`,
 * `thread_items`) for two new shapes; nothing existing changes, so every row
 * and every other shape's offset survives. The new shapes have no offset row
 * yet and snapshot from scratch. The DDL is verbatim from the generated
 * ExponentialDatabase_Impl (Room validates the tables against the entities).
 */
val MIGRATION_84_85: Migration = object : Migration(84, 85) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(THREADS_V85)
        for (index in THREAD_INDICES_V85) db.execSQL(index)
        db.execSQL(THREAD_ITEMS_V85)
        for (index in THREAD_ITEM_INDICES_V85) db.execSQL(index)
    }
}

/**
 * v86 (EXP-1284): `issue_drafts.thread_id`, the thread a "New issue in this
 * thread" draft files into. The nullable column lands empty, so the
 * issue-drafts shape refetches (its offset resets) and already-synced drafts
 * re-arrive carrying it.
 */
val MIGRATION_85_86: Migration = object : Migration(85, 86) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `issue_drafts` ADD COLUMN `thread_id` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'issue_drafts'",
        )
    }
}

/**
 * v87 (EXP-1291 Threads v2): ONE session tree. The old run/thread tables
 * (`coding_sessions`, `threads`, `thread_items`) drop with their Electric
 * offsets (and any shape-repair marker naming them); the five new tables
 * (`sessions`, `session_issues`, `messages`, `pull_requests`,
 * `pull_request_issues`) are created EMPTY and snapshot from scratch — no row
 * migration. `issues` loses its pr_* / branch columns (PR state lives in
 * `pull_requests` now) and `actions` its `triggers` / `repository_id`
 * (saved prompts only); both are rebuilt in place, every row and every other
 * shape's offset survives. DDL verbatim from the generated
 * ExponentialDatabase_Impl (Room validates the tables against the entities).
 */
val MIGRATION_86_87: Migration = object : Migration(86, 87) {
    override fun migrate(db: SupportSQLiteDatabase) {
        for (table in DROPPED_TABLES_V87) {
            db.execSQL("DROP TABLE IF EXISTS `$table`")
            db.execSQL(
                "DELETE FROM `electric_offsets` WHERE `shape` = '$table' OR " +
                    "(substr(`shape`, 1, 8) = '_repair:' AND substr(`shape`, -${table.length + 1}) = ':$table')",
            )
        }
        rebuild(db, "issues", ISSUES_V87, ISSUE_COLUMNS_V87, ISSUE_INDICES_V87)
        rebuild(db, "actions", ACTIONS_V87, ACTION_COLUMNS_V87, ACTION_INDICES_V87)
        for (statement in SESSION_TREE_V87) db.execSQL(statement)
    }
}

/**
 * v88 (dispatcher roots): `sessions.dispatch`, true on every new root (thread
 * or team agent). The column lands false, so the sessions shape refetches (its
 * offset resets) and already-synced rows re-arrive carrying it.
 */
val MIGRATION_87_88: Migration = object : Migration(87, 88) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `dispatch` INTEGER NOT NULL DEFAULT 0")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'sessions'",
        )
    }
}

/**
 * v89 (EXP-1323): `sessions.caption` + `aside`, `messages.author_session_id`
 * and the NEW `session_reads` table (a per-user shape that snapshots from
 * scratch). The sessions + messages shapes refetch (their offsets reset) so
 * already-synced rows re-arrive carrying the new columns. DDL verbatim from
 * the generated ExponentialDatabase_Impl.
 */
val MIGRATION_88_89: Migration = object : Migration(88, 89) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `caption` TEXT")
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `aside` INTEGER NOT NULL DEFAULT 0")
        db.execSQL("ALTER TABLE `messages` ADD COLUMN `author_session_id` TEXT")
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `session_reads` (`id` TEXT NOT NULL, `user_id` TEXT NOT NULL, " +
                "`session_id` TEXT NOT NULL, `read_at` TEXT NOT NULL, `created_at` TEXT NOT NULL, " +
                "`updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
        )
        db.execSQL(
            "CREATE INDEX IF NOT EXISTS `index_session_reads_session_id` ON `session_reads` (`session_id`)",
        )
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` IN ('sessions', 'messages')",
        )
    }
}

/**
 * v90 (EXP-1341): `session_issues.assigned_session_id`, the tree session a
 * root's issue is assigned to (titles a child by its issues). The
 * session_issues shape refetches so already-synced rows re-arrive carrying it.
 */
val MIGRATION_89_90: Migration = object : Migration(89, 90) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `session_issues` ADD COLUMN `assigned_session_id` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'session_issues'",
        )
    }
}

/**
 * v91 (EXP-1252): `sessions.move_to_device_id`, a pending move of the session
 * to another device. The sessions shape refetches so already-synced rows
 * re-arrive carrying it.
 */
val MIGRATION_90_91: Migration = object : Migration(90, 91) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `move_to_device_id` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'sessions'",
        )
    }
}

/**
 * v92 (NEW-2): agents are DEFINITIONS (`agents`), conversations are sessions
 * with `agent_id`; prompts replace actions; events. `sessions.agent_id` lands
 * NULL, so the sessions shape refetches; the three new tables are created
 * EMPTY and snapshot from scratch. DDL verbatim from the generated
 * ExponentialDatabase_Impl.
 */
val MIGRATION_91_92: Migration = object : Migration(91, 92) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `agent_id` TEXT")
        for (statement in NEW_TABLES_V92) db.execSQL(statement)
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'sessions'",
        )
    }
}

private val NEW_TABLES_V92 = listOf(
    "CREATE TABLE IF NOT EXISTS `agents` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
        "`name` TEXT NOT NULL, `icon` TEXT, `color` TEXT, `triggers` TEXT, `device_id` TEXT NOT NULL, " +
        "`cli` TEXT, `account` TEXT, `model` TEXT, `owner_id` TEXT NOT NULL, `sort_order` REAL NOT NULL, " +
        "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
    "CREATE INDEX IF NOT EXISTS `index_agents_team_id` ON `agents` (`team_id`)",
    "CREATE TABLE IF NOT EXISTS `events` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
        "`name` TEXT NOT NULL, `payload` TEXT, `emitted_by` TEXT NOT NULL, `session_id` TEXT, " +
        "`user_id` TEXT, `created_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
    "CREATE INDEX IF NOT EXISTS `index_events_team_id` ON `events` (`team_id`)",
    "CREATE TABLE IF NOT EXISTS `prompts` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
        "`name` TEXT NOT NULL, `icon` TEXT, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, " +
        "`updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
    "CREATE INDEX IF NOT EXISTS `index_prompts_team_id` ON `prompts` (`team_id`)",
)

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

// ── v84 schema: v79's `coding_sessions` with `automation_id` → `trigger_id` ──

private val CODING_SESSIONS_V84 = CODING_SESSIONS_V79.replace("`automation_id`", "`trigger_id`")

private val CODING_SESSION_COLUMNS_V84 =
    CODING_SESSION_COLUMNS_V79.map { if (it == "automation_id") "trigger_id" else it }

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

// ── v85 schema, verbatim from the generated ExponentialDatabase_Impl ────────

private const val THREADS_V85 =
    "CREATE TABLE IF NOT EXISTS `threads` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
        "`title` TEXT, `created_by` TEXT, `results` TEXT, `created_at` TEXT NOT NULL, " +
        "`updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))"

private val THREAD_INDICES_V85 = listOf(
    "CREATE INDEX IF NOT EXISTS `index_threads_team_id` ON `threads` (`team_id`)",
)

private const val THREAD_ITEMS_V85 =
    "CREATE TABLE IF NOT EXISTS `thread_items` (`id` TEXT NOT NULL, `thread_id` TEXT NOT NULL, " +
        "`kind` TEXT NOT NULL, `issue_id` TEXT, `session_id` TEXT, `team_id` TEXT NOT NULL, " +
        "`board_id` TEXT, `added_by` TEXT, `added_by_session_id` TEXT, `source` TEXT NOT NULL, " +
        "`sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, " +
        "PRIMARY KEY(`id`))"

private val THREAD_ITEM_INDICES_V85 = listOf(
    "CREATE INDEX IF NOT EXISTS `index_thread_items_thread_id` ON `thread_items` (`thread_id`)",
    "CREATE INDEX IF NOT EXISTS `index_thread_items_issue_id` ON `thread_items` (`issue_id`)",
    "CREATE INDEX IF NOT EXISTS `index_thread_items_session_id` ON `thread_items` (`session_id`)",
    "CREATE INDEX IF NOT EXISTS `index_thread_items_team_id` ON `thread_items` (`team_id`)",
)

// ── v87 schema, verbatim from the generated ExponentialDatabase_Impl ────────

private val DROPPED_TABLES_V87 = listOf("coding_sessions", "threads", "thread_items")

private const val ISSUES_V87 =
    "CREATE TABLE IF NOT EXISTS `issues` (`id` TEXT NOT NULL, `board_id` TEXT NOT NULL, " +
        "`number` INTEGER NOT NULL, `identifier` TEXT NOT NULL, `title` TEXT NOT NULL, " +
        "`description` TEXT, `status` TEXT NOT NULL, `status_id` TEXT, `priority` TEXT NOT NULL, " +
        "`assignee_id` TEXT, `creator_id` TEXT, `source` TEXT, `due_date` TEXT, `estimate` INTEGER, " +
        "`sort_order` REAL NOT NULL, `completed_at` TEXT, `duplicate_of_id` TEXT, `draft_id` TEXT, " +
        "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))"

private val ISSUE_COLUMNS_V87 = listOf(
    "id", "board_id", "number", "identifier", "title", "description", "status", "status_id",
    "priority", "assignee_id", "creator_id", "source", "due_date", "estimate", "sort_order",
    "completed_at", "duplicate_of_id", "draft_id", "created_at", "updated_at",
)

private val ISSUE_INDICES_V87 = listOf(
    "CREATE INDEX IF NOT EXISTS `index_issues_board_id` ON `issues` (`board_id`)",
    "CREATE INDEX IF NOT EXISTS `index_issues_status` ON `issues` (`status`)",
    "CREATE INDEX IF NOT EXISTS `index_issues_assignee_id` ON `issues` (`assignee_id`)",
    "CREATE INDEX IF NOT EXISTS `index_issues_due_date` ON `issues` (`due_date`)",
)

private const val ACTIONS_V87 =
    "CREATE TABLE IF NOT EXISTS `actions` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
        "`name` TEXT NOT NULL, `description` TEXT, `icon` TEXT, `inputs` TEXT, " +
        "`prompt_placeholder` TEXT, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, " +
        "`updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))"

private val ACTION_COLUMNS_V87 = listOf(
    "id", "team_id", "name", "description", "icon", "inputs", "prompt_placeholder", "sort_order",
    "created_at", "updated_at",
)

private val ACTION_INDICES_V87 = listOf(
    "CREATE INDEX IF NOT EXISTS `index_actions_team_id` ON `actions` (`team_id`)",
)

private val SESSION_TREE_V87 = listOf(
    "CREATE TABLE IF NOT EXISTS `sessions` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
        "`parent_id` TEXT, `root_id` TEXT NOT NULL, `depth` INTEGER, `kind` TEXT NOT NULL, " +
        "`user_id` TEXT NOT NULL, `device_id` TEXT NOT NULL, `cli` TEXT, `account` TEXT, " +
        "`model` TEXT, `effort` TEXT, `plan_mode` INTEGER NOT NULL, `computer_use` INTEGER NOT NULL, " +
        "`ultracode` INTEGER NOT NULL, `repository_id` TEXT, `branch` TEXT, `status` TEXT NOT NULL, " +
        "`busy` INTEGER NOT NULL, `needs_input` INTEGER NOT NULL, `blocked` TEXT, `started_at` TEXT, " +
        "`last_turn_at` TEXT, `ended_at` TEXT, `shared` INTEGER NOT NULL, `title` TEXT, `guide` TEXT, " +
        "`action_id` TEXT, `action_name` TEXT, `action_inputs` TEXT, `spawned_by_session_id` TEXT, " +
        "`name` TEXT, `icon` TEXT, `color` TEXT, `triggers` TEXT, `sort_order` REAL NOT NULL, " +
        "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
    "CREATE INDEX IF NOT EXISTS `index_sessions_team_id` ON `sessions` (`team_id`)",
    "CREATE INDEX IF NOT EXISTS `index_sessions_root_id` ON `sessions` (`root_id`)",
    "CREATE INDEX IF NOT EXISTS `index_sessions_parent_id` ON `sessions` (`parent_id`)",
    "CREATE TABLE IF NOT EXISTS `session_issues` (`id` TEXT NOT NULL, `session_id` TEXT NOT NULL, " +
        "`issue_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `source` TEXT NOT NULL, `added_by` TEXT, " +
        "`added_by_session_id` TEXT, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, " +
        "`updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
    "CREATE INDEX IF NOT EXISTS `index_session_issues_session_id` ON `session_issues` (`session_id`)",
    "CREATE INDEX IF NOT EXISTS `index_session_issues_issue_id` ON `session_issues` (`issue_id`)",
    "CREATE INDEX IF NOT EXISTS `index_session_issues_team_id` ON `session_issues` (`team_id`)",
    "CREATE TABLE IF NOT EXISTS `messages` (`id` TEXT NOT NULL, `session_id` TEXT NOT NULL, " +
        "`root_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `author_kind` TEXT NOT NULL, " +
        "`author_id` TEXT, `body` TEXT NOT NULL, `card` TEXT, `answered_at` TEXT, `consumed_at` TEXT, " +
        "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
    "CREATE INDEX IF NOT EXISTS `index_messages_session_id` ON `messages` (`session_id`)",
    "CREATE INDEX IF NOT EXISTS `index_messages_root_id` ON `messages` (`root_id`)",
    "CREATE INDEX IF NOT EXISTS `index_messages_team_id` ON `messages` (`team_id`)",
    "CREATE TABLE IF NOT EXISTS `pull_requests` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
        "`root_session_id` TEXT, `session_id` TEXT, `repository_id` TEXT NOT NULL, `number` INTEGER, " +
        "`url` TEXT NOT NULL, `title` TEXT, `head` TEXT NOT NULL, `base` TEXT NOT NULL, " +
        "`base_pull_request_id` TEXT, `state` TEXT NOT NULL, `merged_at` TEXT, " +
        "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
    "CREATE INDEX IF NOT EXISTS `index_pull_requests_team_id` ON `pull_requests` (`team_id`)",
    "CREATE INDEX IF NOT EXISTS `index_pull_requests_root_session_id` ON `pull_requests` (`root_session_id`)",
    "CREATE INDEX IF NOT EXISTS `index_pull_requests_session_id` ON `pull_requests` (`session_id`)",
    "CREATE TABLE IF NOT EXISTS `pull_request_issues` (`id` TEXT NOT NULL, " +
        "`pull_request_id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
        "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
    "CREATE INDEX IF NOT EXISTS `index_pull_request_issues_pull_request_id` ON `pull_request_issues` (`pull_request_id`)",
    "CREATE INDEX IF NOT EXISTS `index_pull_request_issues_issue_id` ON `pull_request_issues` (`issue_id`)",
    "CREATE INDEX IF NOT EXISTS `index_pull_request_issues_team_id` ON `pull_request_issues` (`team_id`)",
)
