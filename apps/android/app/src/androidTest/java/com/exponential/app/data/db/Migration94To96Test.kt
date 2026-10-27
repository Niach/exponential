package com.exponential.app.data.db

import android.content.Context
import android.database.sqlite.SQLiteDatabase
import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * NEW-8: the production upgrade path 0.14.68 (Room v94) → v96, i.e.
 * [MIGRATION_94_95] then [MIGRATION_95_96], on a real version-94 file.
 *
 * The schema is exported nowhere (`exportSchema = false`), so [V94_SCHEMA] is
 * the `createAllTables` DDL VERBATIM from the ExponentialDatabase_Impl that
 * KSP generates at tag android-v0.14.68 (f22d428830d), identity hash included.
 * Room opens the file with the two migrations and NO destructive fallback, so
 * every table is validated against today's entities (what
 * MigrationTestHelper.runMigrationsAndValidate would check); a second case
 * opens it through [DatabaseHolder] exactly as the app does, fallback included,
 * and proves the rows survive (no silent wipe).
 */
@RunWith(AndroidJUnit4::class)
class Migration94To96Test {

    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() {
        context.deleteDatabase(TEST_DB)
        context.deleteDatabase(HOLDER_DB)
    }

    @After
    fun tearDown() = clean()

    @Test
    fun migrates94To96AndValidates() {
        createVersion94(TEST_DB)
        val db = Room.databaseBuilder(context, ExponentialDatabase::class.java, TEST_DB)
            .addMigrations(MIGRATION_94_95, MIGRATION_95_96, MIGRATION_96_97, MIGRATION_97_98, MIGRATION_98_99, MIGRATION_99_100, MIGRATION_100_101)
            .allowMainThreadQueries()
            .build()
        try {
            assertMigrated(db)
        } finally {
            db.close()
        }
    }

    @Test
    fun productionHolderKeepsTheRowsFrom94() {
        createVersion94(HOLDER_DB)
        val holder = DatabaseHolder(context)
        try {
            assertMigrated(holder.database(HOLDER_ACCOUNT))
        } finally {
            holder.close(HOLDER_ACCOUNT)
        }
    }

    private fun assertMigrated(db: ExponentialDatabase) {
        val sql = db.openHelper.writableDatabase
        assertEquals(101, sql.version)

        // sessions: every kept column carries its v94 value.
        val root = runBlocking { db.sessionDao().getById("s1") }!!
        assertEquals("Fix it", root.title)
        assertEquals("agent-1", root.agentId)
        assertEquals("app-1", root.appId)
        assertEquals("t1", root.teamId)
        assertEquals("s1", root.rootId)
        assertEquals("mac", root.deviceId)
        assertEquals("claude", root.cli)
        assertEquals("opus", root.model)
        assertEquals("repo-1", root.repositoryId)
        assertEquals("exp/fix", root.branch)
        assertEquals("idle", root.status)
        assertTrue(root.shared)
        assertTrue(root.planMode)
        assertEquals("[]", root.guide)
        val child = runBlocking { db.sessionDao().getById("s2") }!!
        assertEquals("s1", child.parentId)
        assertEquals(1, child.depth)
        assertTrue(child.busy)

        // session_issues: both links survive with their data.
        val links = runBlocking { db.sessionIssueDao().observeAll().first() }
        assertEquals(listOf("i1", "i2"), links.map { it.issueId }.sorted())
        val first = links.single { it.id == "si1" }
        assertEquals("user", first.source)
        assertEquals("u1", first.addedBy)
        assertEquals(1.0, first.sortOrder, 0.0)

        val tables = ArrayList<String>()
        sql.query("SELECT name FROM sqlite_master WHERE type = 'table'").use {
            while (it.moveToNext()) tables.add(it.getString(0))
        }
        assertFalse("actions" in tables)
        assertFalse(tables.any { it.endsWith("_slop3") })

        // pins: the issue + session pins survive, the action pin is gone.
        val pins = ArrayList<String>()
        sql.query("SELECT id, kind, issue_id, session_id FROM pins ORDER BY id").use {
            while (it.moveToNext()) pins.add("${it.getString(0)}|${it.getString(1)}|${it.getString(2)}|${it.getString(3)}")
        }
        assertEquals(listOf("p-issue|issue|i1|null", "p-session|session|null|s1"), pins)

        // messages: rebuilt empty; its shape refetches from scratch.
        sql.query("SELECT COUNT(*) FROM messages").use {
            it.moveToFirst()
            assertEquals(0, it.getInt(0))
        }
        val offsets = db.electricOffsetDao()
        val messages = runBlocking { offsets.get("messages") }!!
        assertEquals("", messages.handle)
        assertEquals("-1", messages.offset)
        assertFalse(messages.isLive)
        assertTrue(messages.needsRefetch)

        // actions: offset + repair marker gone; every other offset untouched.
        for (dropped in listOf("actions", "_repair:x:actions")) {
            assertNull(runBlocking { offsets.get(dropped) })
        }
        for (kept in listOf("session_issues", "pins", "_repair:x:issues")) { // sessions + issues refetch at v98 (#EXP-1383), conversations at v99
            val row = runBlocking { offsets.get(kept) }!!
            assertEquals("h-$kept", row.handle)
            assertEquals("7_3", row.offset)
            assertTrue(row.isLive)
            assertFalse(row.needsRefetch)
        }

        // the untouched tables keep their rows too.
        sql.query("SELECT title FROM issues WHERE id = 'i1'").use {
            assertTrue(it.moveToFirst())
            assertEquals("Crash on launch", it.getString(0))
        }
    }

    /** A v94 file exactly as 0.14.68's Room creates it, with realistic rows. */
    private fun createVersion94(name: String) {
        val file = context.getDatabasePath(name)
        file.parentFile?.mkdirs()
        SQLiteDatabase.openOrCreateDatabase(file, null).use { db ->
            for (statement in V94_SCHEMA) db.execSQL(statement)

            // Every column of the v94 tables is filled from the table's own
            // shape so a NOT NULL column without a default can't be missed.
            insertRow(db, "teams", mapOf("id" to "t1", "name" to "Team", "slug" to "team"))
            insertRow(db, "boards", mapOf("id" to "b1", "team_id" to "t1"))
            insertRow(db, "issues", mapOf("id" to "i1", "board_id" to "b1", "title" to "Crash on launch"))
            insertRow(db, "issues", mapOf("id" to "i2", "board_id" to "b1", "title" to "Second"))
            insertRow(db, "actions", mapOf("id" to "a1", "team_id" to "t1", "name" to "Nightly"))
            insertRow(
                db, "sessions",
                mapOf(
                    "id" to "s1", "team_id" to "t1", "parent_id" to null, "root_id" to "s1", "depth" to 0,
                    "kind" to "agent", "user_id" to "u1", "device_id" to "mac", "cli" to "claude",
                    "account" to "me", "model" to "opus", "effort" to "high", "plan_mode" to 1,
                    "computer_use" to 0, "ultracode" to 0, "dispatch" to 1, "repository_id" to "repo-1",
                    "branch" to "exp/fix", "status" to "idle", "busy" to 0, "needs_input" to 0,
                    "blocked" to null, "started_at" to "s", "last_turn_at" to "l", "ended_at" to null,
                    "shared" to 1, "title" to "Fix it", "guide" to "[]", "action_id" to "a1",
                    "action_name" to "Nightly", "action_inputs" to "{\"k\":\"v\"}",
                    "spawned_by_session_id" to "s0", "name" to "Scout", "icon" to "bot", "color" to "blue",
                    "caption" to "Reading", "aside" to 0, "move_to_device_id" to "studio",
                    "triggers" to "[]", "sort_order" to 2.0, "created_at" to "c", "updated_at" to "u",
                    "agent_id" to "agent-1", "app_id" to "app-1",
                ),
            )
            insertRow(
                db, "sessions",
                mapOf(
                    "id" to "s2", "team_id" to "t1", "parent_id" to "s1", "root_id" to "s1", "depth" to 1,
                    "kind" to "thread", "user_id" to "u1", "device_id" to "mac", "status" to "running",
                    "busy" to 1, "aside" to 1, "caption" to "Editing", "spawned_by_session_id" to "s1",
                ),
            )
            insertRow(
                db, "session_issues",
                mapOf(
                    "id" to "si1", "session_id" to "s1", "issue_id" to "i1", "team_id" to "t1",
                    "source" to "user", "added_by" to "u1", "assigned_session_id" to "s2", "sort_order" to 1.0,
                ),
            )
            insertRow(
                db, "session_issues",
                mapOf(
                    "id" to "si2", "session_id" to "s2", "issue_id" to "i2", "team_id" to "t1",
                    "source" to "agent", "added_by_session_id" to "s1", "sort_order" to 2.0,
                ),
            )
            insertRow(db, "pins", mapOf("id" to "p-issue", "user_id" to "u1", "team_id" to "t1", "kind" to "issue", "issue_id" to "i1"))
            insertRow(db, "pins", mapOf("id" to "p-session", "user_id" to "u1", "team_id" to "t1", "kind" to "session", "session_id" to "s1"))
            insertRow(db, "pins", mapOf("id" to "p-action", "user_id" to "u1", "team_id" to "t1", "kind" to "action", "action_id" to "a1"))
            // v94 messages: keyed by the legacy session_id, no conversation_id.
            insertRow(
                db, "messages",
                mapOf(
                    "id" to "m1", "conversation_id" to null, "session_id" to "s1", "root_id" to "s1",
                    "team_id" to "t1", "author_kind" to "user", "author_id" to "u1", "body" to "hi",
                ),
            )
            insertRow(
                db, "messages",
                mapOf(
                    "id" to "m2", "conversation_id" to "cv1", "session_id" to "s1", "root_id" to "s1",
                    "team_id" to "t1", "author_kind" to "agent", "author_session_id" to "s1", "body" to "hello",
                ),
            )
            insertRow(db, "conversations", mapOf("id" to "cv1", "team_id" to "t1", "kind" to "session", "session_id" to "s1"))
            for (shape in listOf(
                "sessions", "session_issues", "pins", "issues", "conversations", "_repair:x:issues",
                "messages", "actions", "_repair:x:actions",
            )) {
                db.execSQL(
                    "INSERT INTO electric_offsets (shape, handle, `offset`, is_live, needs_refetch) " +
                        "VALUES (?, ?, '7_3', 1, 0)",
                    arrayOf(shape, "h-$shape"),
                )
            }
            db.version = 94
        }
    }

    /**
     * INSERT [values] into [table]; every other NOT NULL column without a
     * default gets a type-appropriate filler ('x' / 0), read from PRAGMA.
     */
    private fun insertRow(db: SQLiteDatabase, table: String, values: Map<String, Any?>) {
        val row = LinkedHashMap<String, Any?>()
        db.rawQuery("PRAGMA table_info(`$table`)", null).use {
            while (it.moveToNext()) {
                val column = it.getString(1)
                val type = it.getString(2)
                val notNull = it.getInt(3) == 1
                val hasDefault = !it.isNull(4)
                if (column in values) {
                    row[column] = values[column]
                } else if (notNull && !hasDefault) {
                    row[column] = if (type == "TEXT") "x" else 0
                }
            }
        }
        val unknown = values.keys - row.keys
        assertTrue("$table has no $unknown", unknown.isEmpty())
        val columns = row.keys.joinToString(", ") { "`$it`" }
        val marks = row.keys.joinToString(", ") { "?" }
        db.execSQL("INSERT INTO `$table` ($columns) VALUES ($marks)", row.values.toTypedArray())
    }

    private companion object {
        const val TEST_DB = "new8-migration-v94.db"
        const val HOLDER_ACCOUNT = "new8-migration-v94-holder"
        const val HOLDER_DB = "exponential-$HOLDER_ACCOUNT-v2.db"

        /** createAllTables of ExponentialDatabase_Impl at android-v0.14.68 (v94). */
        val V94_SCHEMA = listOf(
            "CREATE TABLE IF NOT EXISTS `teams` (`id` TEXT NOT NULL, `name` TEXT NOT NULL, `slug` TEXT NOT NULL, `icon_url` TEXT, `yolo_mode` INTEGER NOT NULL, `estimation_type` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE TABLE IF NOT EXISTS `boards` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `name` TEXT NOT NULL, `slug` TEXT NOT NULL, `prefix` TEXT NOT NULL, `color` TEXT NOT NULL, `icon` TEXT, `repository_id` TEXT, `default_branch` TEXT, `sort_order` REAL NOT NULL, `deleted_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_boards_team_id` ON `boards` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `issues` (`id` TEXT NOT NULL, `board_id` TEXT NOT NULL, `number` INTEGER NOT NULL, `identifier` TEXT NOT NULL, `title` TEXT NOT NULL, `description` TEXT, `status` TEXT NOT NULL, `status_id` TEXT, `priority` TEXT NOT NULL, `assignee_id` TEXT, `creator_id` TEXT, `source` TEXT, `due_date` TEXT, `estimate` INTEGER, `sort_order` REAL NOT NULL, `completed_at` TEXT, `duplicate_of_id` TEXT, `draft_id` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_issues_board_id` ON `issues` (`board_id`)",
            "CREATE INDEX IF NOT EXISTS `index_issues_status` ON `issues` (`status`)",
            "CREATE INDEX IF NOT EXISTS `index_issues_assignee_id` ON `issues` (`assignee_id`)",
            "CREATE INDEX IF NOT EXISTS `index_issues_due_date` ON `issues` (`due_date`)",
            "CREATE TABLE IF NOT EXISTS `labels` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `name` TEXT NOT NULL, `color` TEXT NOT NULL, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_labels_team_id` ON `labels` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `issue_statuses` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `category` TEXT NOT NULL, `name` TEXT NOT NULL, `color` TEXT, `sort_order` REAL NOT NULL, `builtin_key` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_issue_statuses_team_id` ON `issue_statuses` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `issue_labels` (`issue_id` TEXT NOT NULL, `label_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `board_id` TEXT, PRIMARY KEY(`issue_id`, `label_id`))",
            "CREATE INDEX IF NOT EXISTS `index_issue_labels_label_id` ON `issue_labels` (`label_id`)",
            "CREATE INDEX IF NOT EXISTS `index_issue_labels_team_id` ON `issue_labels` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `issue_relations` (`id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, `related_issue_id` TEXT NOT NULL, `type` TEXT NOT NULL, `source` TEXT NOT NULL, `team_id` TEXT NOT NULL, `board_id` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_issue_relations_issue_id` ON `issue_relations` (`issue_id`)",
            "CREATE INDEX IF NOT EXISTS `index_issue_relations_related_issue_id` ON `issue_relations` (`related_issue_id`)",
            "CREATE INDEX IF NOT EXISTS `index_issue_relations_team_id` ON `issue_relations` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `users` (`id` TEXT NOT NULL, `name` TEXT, `email` TEXT NOT NULL, `image` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE TABLE IF NOT EXISTS `team_members` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `user_id` TEXT NOT NULL, `role` TEXT NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_team_members_team_id` ON `team_members` (`team_id`)",
            "CREATE INDEX IF NOT EXISTS `index_team_members_user_id` ON `team_members` (`user_id`)",
            "CREATE TABLE IF NOT EXISTS `team_invites` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `invited_by_id` TEXT, `role` TEXT NOT NULL, `token` TEXT, `email` TEXT, `placeholder_user_id` TEXT, `sent_at` TEXT, `expires_at` TEXT NOT NULL, `accepted_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_team_invites_team_id` ON `team_invites` (`team_id`)",
            "CREATE INDEX IF NOT EXISTS `index_team_invites_token` ON `team_invites` (`token`)",
            "CREATE TABLE IF NOT EXISTS `comments` (`id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `board_id` TEXT, `author_id` TEXT, `parent_id` TEXT, `source` TEXT, `audience` TEXT NOT NULL, `body` TEXT, `kind` TEXT NOT NULL, `edited_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_comments_issue_id` ON `comments` (`issue_id`)",
            "CREATE INDEX IF NOT EXISTS `index_comments_team_id` ON `comments` (`team_id`)",
            "CREATE INDEX IF NOT EXISTS `index_comments_parent_id` ON `comments` (`parent_id`)",
            "CREATE TABLE IF NOT EXISTS `attachments` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, `board_id` TEXT, `comment_id` TEXT, `uploader_id` TEXT, `filename` TEXT NOT NULL, `content_type` TEXT NOT NULL, `size_bytes` INTEGER NOT NULL, `storage_key` TEXT NOT NULL, `url` TEXT NOT NULL, `width` INTEGER, `height` INTEGER, `duration_ms` INTEGER, `poster_storage_key` TEXT, `as_file` INTEGER NOT NULL DEFAULT 0, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_attachments_issue_id` ON `attachments` (`issue_id`)",
            "CREATE INDEX IF NOT EXISTS `index_attachments_team_id` ON `attachments` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `notifications` (`id` TEXT NOT NULL, `user_id` TEXT NOT NULL, `issue_id` TEXT, `team_id` TEXT, `session_id` TEXT, `type` TEXT NOT NULL, `title` TEXT NOT NULL, `body` TEXT, `read_at` TEXT, `pushed_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_notifications_user_id_read_at` ON `notifications` (`user_id`, `read_at`)",
            "CREATE TABLE IF NOT EXISTS `issue_subscribers` (`id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, `user_id` TEXT, `email` TEXT, `team_id` TEXT NOT NULL, `source` TEXT NOT NULL, `unsubscribed` INTEGER NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_issue_subscribers_user_id` ON `issue_subscribers` (`user_id`)",
            "CREATE INDEX IF NOT EXISTS `index_issue_subscribers_team_id` ON `issue_subscribers` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `issue_events` (`id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `board_id` TEXT, `actor_user_id` TEXT, `type` TEXT NOT NULL, `payload` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_issue_events_issue_id` ON `issue_events` (`issue_id`)",
            "CREATE INDEX IF NOT EXISTS `index_issue_events_team_id` ON `issue_events` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `actions` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `name` TEXT NOT NULL, `description` TEXT, `icon` TEXT, `inputs` TEXT, `prompt_placeholder` TEXT, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_actions_team_id` ON `actions` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `devices` (`id` TEXT NOT NULL, `user_id` TEXT NOT NULL, `device_id` TEXT NOT NULL, `label` TEXT NOT NULL, `kind` TEXT NOT NULL, `platform` TEXT, `icon` TEXT, `version` TEXT, `agents` TEXT, `caps` TEXT, `unauthed_agents` TEXT, `acp_agents` TEXT, `launch_defaults` TEXT, `launch_defaults_updated_at` TEXT, `agent_accounts` TEXT, `agent_usage` TEXT, `agent_usage_at` TEXT, `doctor` TEXT, `active_sessions` INTEGER NOT NULL, `last_seen_at` TEXT, `shared_team_ids` TEXT NOT NULL, `is_default` INTEGER NOT NULL, `update_requested_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_devices_user_id` ON `devices` (`user_id`)",
            "CREATE TABLE IF NOT EXISTS `device_worktrees` (`id` TEXT NOT NULL, `device_row_id` TEXT NOT NULL, `repo_full_name` TEXT NOT NULL, `branch` TEXT NOT NULL, `issue_identifier` TEXT, `agents` TEXT, `dirty` TEXT NOT NULL, `busy` INTEGER NOT NULL, `reported_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_device_worktrees_device_row_id` ON `device_worktrees` (`device_row_id`)",
            "CREATE TABLE IF NOT EXISTS `pins` (`id` TEXT NOT NULL, `user_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `kind` TEXT NOT NULL, `issue_id` TEXT, `session_id` TEXT, `action_id` TEXT, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_pins_team_id` ON `pins` (`team_id`)",
            "CREATE INDEX IF NOT EXISTS `index_pins_issue_id` ON `pins` (`issue_id`)",
            "CREATE INDEX IF NOT EXISTS `index_pins_session_id` ON `pins` (`session_id`)",
            "CREATE INDEX IF NOT EXISTS `index_pins_action_id` ON `pins` (`action_id`)",
            "CREATE TABLE IF NOT EXISTS `issue_drafts` (`id` TEXT NOT NULL, `user_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `board_id` TEXT NOT NULL, `title` TEXT NOT NULL, `description` TEXT NOT NULL, `status_id` TEXT, `priority` TEXT NOT NULL, `assignee_id` TEXT, `label_ids` TEXT NOT NULL, `due_date` TEXT, `thread_id` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_issue_drafts_user_id` ON `issue_drafts` (`user_id`)",
            "CREATE INDEX IF NOT EXISTS `index_issue_drafts_team_id` ON `issue_drafts` (`team_id`)",
            "CREATE INDEX IF NOT EXISTS `index_issue_drafts_board_id` ON `issue_drafts` (`board_id`)",
            "CREATE TABLE IF NOT EXISTS `sessions` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `parent_id` TEXT, `root_id` TEXT NOT NULL, `depth` INTEGER, `kind` TEXT NOT NULL, `user_id` TEXT NOT NULL, `device_id` TEXT NOT NULL, `cli` TEXT, `account` TEXT, `model` TEXT, `effort` TEXT, `plan_mode` INTEGER NOT NULL, `computer_use` INTEGER NOT NULL, `ultracode` INTEGER NOT NULL, `dispatch` INTEGER NOT NULL DEFAULT 0, `repository_id` TEXT, `branch` TEXT, `status` TEXT NOT NULL, `busy` INTEGER NOT NULL, `needs_input` INTEGER NOT NULL, `blocked` TEXT, `started_at` TEXT, `last_turn_at` TEXT, `ended_at` TEXT, `shared` INTEGER NOT NULL, `title` TEXT, `guide` TEXT, `action_id` TEXT, `action_name` TEXT, `action_inputs` TEXT, `spawned_by_session_id` TEXT, `name` TEXT, `icon` TEXT, `color` TEXT, `caption` TEXT, `aside` INTEGER NOT NULL DEFAULT 0, `move_to_device_id` TEXT, `triggers` TEXT, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, `agent_id` TEXT, `app_id` TEXT, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_sessions_team_id` ON `sessions` (`team_id`)",
            "CREATE INDEX IF NOT EXISTS `index_sessions_root_id` ON `sessions` (`root_id`)",
            "CREATE INDEX IF NOT EXISTS `index_sessions_parent_id` ON `sessions` (`parent_id`)",
            "CREATE TABLE IF NOT EXISTS `session_issues` (`id` TEXT NOT NULL, `session_id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `source` TEXT NOT NULL, `added_by` TEXT, `added_by_session_id` TEXT, `assigned_session_id` TEXT, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_session_issues_session_id` ON `session_issues` (`session_id`)",
            "CREATE INDEX IF NOT EXISTS `index_session_issues_issue_id` ON `session_issues` (`issue_id`)",
            "CREATE INDEX IF NOT EXISTS `index_session_issues_team_id` ON `session_issues` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `messages` (`id` TEXT NOT NULL, `conversation_id` TEXT, `session_id` TEXT NOT NULL, `root_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `author_kind` TEXT NOT NULL, `author_id` TEXT, `author_session_id` TEXT, `body` TEXT NOT NULL, `card` TEXT, `answered_at` TEXT, `consumed_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_messages_session_id` ON `messages` (`session_id`)",
            "CREATE INDEX IF NOT EXISTS `index_messages_root_id` ON `messages` (`root_id`)",
            "CREATE INDEX IF NOT EXISTS `index_messages_team_id` ON `messages` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `pull_requests` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `root_session_id` TEXT, `session_id` TEXT, `repository_id` TEXT NOT NULL, `number` INTEGER, `url` TEXT NOT NULL, `title` TEXT, `head` TEXT NOT NULL, `base` TEXT NOT NULL, `base_pull_request_id` TEXT, `state` TEXT NOT NULL, `merged_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_pull_requests_team_id` ON `pull_requests` (`team_id`)",
            "CREATE INDEX IF NOT EXISTS `index_pull_requests_root_session_id` ON `pull_requests` (`root_session_id`)",
            "CREATE INDEX IF NOT EXISTS `index_pull_requests_session_id` ON `pull_requests` (`session_id`)",
            "CREATE TABLE IF NOT EXISTS `pull_request_issues` (`id` TEXT NOT NULL, `pull_request_id` TEXT NOT NULL, `issue_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_pull_request_issues_pull_request_id` ON `pull_request_issues` (`pull_request_id`)",
            "CREATE INDEX IF NOT EXISTS `index_pull_request_issues_issue_id` ON `pull_request_issues` (`issue_id`)",
            "CREATE INDEX IF NOT EXISTS `index_pull_request_issues_team_id` ON `pull_request_issues` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `session_reads` (`id` TEXT NOT NULL, `user_id` TEXT NOT NULL, `session_id` TEXT NOT NULL, `read_at` TEXT NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_session_reads_session_id` ON `session_reads` (`session_id`)",
            "CREATE TABLE IF NOT EXISTS `agents` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `name` TEXT NOT NULL, `icon` TEXT, `color` TEXT, `triggers` TEXT, `device_id` TEXT NOT NULL, `cli` TEXT, `account` TEXT, `model` TEXT, `owner_id` TEXT NOT NULL, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_agents_team_id` ON `agents` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `events` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `name` TEXT NOT NULL, `payload` TEXT, `emitted_by` TEXT NOT NULL, `session_id` TEXT, `user_id` TEXT, `created_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_events_team_id` ON `events` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `prompts` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `name` TEXT NOT NULL, `icon` TEXT, `sort_order` REAL NOT NULL, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_prompts_team_id` ON `prompts` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `conversations` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `kind` TEXT NOT NULL, `session_id` TEXT, `title` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_conversations_team_id` ON `conversations` (`team_id`)",
            "CREATE INDEX IF NOT EXISTS `index_conversations_session_id` ON `conversations` (`session_id`)",
            "CREATE TABLE IF NOT EXISTS `apps` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `slug` TEXT NOT NULL, `name` TEXT NOT NULL, `icon` TEXT, `color` TEXT, `description` TEXT NOT NULL, `package` TEXT, `head_version` INTEGER, `created_by` TEXT NOT NULL, `deleted_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_apps_team_id` ON `apps` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `app_versions` (`id` TEXT NOT NULL, `app_id` TEXT NOT NULL, `team_id` TEXT NOT NULL, `number` INTEGER, `note` TEXT, `session_id` TEXT, `created_by` TEXT, `created_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
            "CREATE INDEX IF NOT EXISTS `index_app_versions_app_id` ON `app_versions` (`app_id`)",
            "CREATE INDEX IF NOT EXISTS `index_app_versions_team_id` ON `app_versions` (`team_id`)",
            "CREATE TABLE IF NOT EXISTS `electric_offsets` (`shape` TEXT NOT NULL, `handle` TEXT NOT NULL, `offset` TEXT NOT NULL, `is_live` INTEGER NOT NULL, `needs_refetch` INTEGER NOT NULL, PRIMARY KEY(`shape`))",
            "CREATE TABLE IF NOT EXISTS room_master_table (id INTEGER PRIMARY KEY,identity_hash TEXT)",
            "INSERT OR REPLACE INTO room_master_table (id,identity_hash) VALUES(42, '3020246988d879eea8b8597995565111')",
        )
    }
}
