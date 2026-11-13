package com.exponential.app.data.db

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/**
 * v105 (EXP-1459 sealed content): `sessions.seal_public_key`, `seal_epoch`
 * (the root's tree key) and `sealed_title` (a body envelope). Only the
 * sessions shape refetches so synced rows re-arrive with them.
 */
val MIGRATION_104_105: Migration = object : Migration(104, 105) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `seal_public_key` TEXT")
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `seal_epoch` INTEGER")
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `sealed_title` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'sessions'",
        )
    }
}

/**
 * v106 (#VAPP-186): `comments.via_app_id`, the app whose MCP tool wrote the
 * comment ("via <app>"). Only the comments shape refetches.
 */
val MIGRATION_105_106: Migration = object : Migration(105, 106) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `comments` ADD COLUMN `via_app_id` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'comments'",
        )
    }
}

/**
 * v107 (#EXP-1534): `notifications.device_id`, the device an
 * `agent_signed_out` row is about. Only the notifications shape refetches.
 */
val MIGRATION_106_107: Migration = object : Migration(106, 107) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `notifications` ADD COLUMN `device_id` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'notifications'",
        )
    }
}

/**
 * v108 (#EXP-1611): `pull_requests.checks_state` (the CI aggregate) and the
 * `pull_request_checks` table (one row per check at the PR's head). Only the
 * pull_requests shape refetches; the checks shape has no offset yet.
 */
val MIGRATION_107_108: Migration = object : Migration(107, 108) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `pull_requests` ADD COLUMN `checks_state` TEXT NOT NULL DEFAULT 'none'")
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `pull_request_checks` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
                "`pull_request_id` TEXT NOT NULL, `head_sha` TEXT NOT NULL, `name` TEXT NOT NULL, " +
                "`status` TEXT NOT NULL, `conclusion` TEXT, `required` INTEGER NOT NULL DEFAULT 0, " +
                "`started_at` TEXT, `completed_at` TEXT, `details_url` TEXT, `created_at` TEXT NOT NULL, " +
                "`updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
        )
        db.execSQL(
            "CREATE INDEX IF NOT EXISTS `index_pull_request_checks_pull_request_id` " +
                "ON `pull_request_checks` (`pull_request_id`)",
        )
        db.execSQL(
            "CREATE INDEX IF NOT EXISTS `index_pull_request_checks_team_id` ON `pull_request_checks` (`team_id`)",
        )
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'pull_requests'",
        )
    }
}

/**
 * v109 (#VAPP-187): `app_records.author_id` (whose record it is) and
 * `updated_by` (the last writer). Only the app_records shape refetches.
 */
val MIGRATION_108_109: Migration = object : Migration(108, 109) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `app_records` ADD COLUMN `author_id` TEXT")
        db.execSQL("ALTER TABLE `app_records` ADD COLUMN `updated_by` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'app_records'",
        )
    }
}

/**
 * v110 (#EXP-1661): the `skills` table (team skills, body server-only) and
 * `agents.skill_ids` (the agent's default skills). The agents shape
 * refetches; the new skills shape starts fresh.
 */
val MIGRATION_109_110: Migration = object : Migration(109, 110) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `agents` ADD COLUMN `skill_ids` TEXT")
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `skills` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
                "`name` TEXT NOT NULL, `description` TEXT NOT NULL, " +
                "`enabled_by_default` INTEGER NOT NULL DEFAULT 0, `created_by_id` TEXT, " +
                "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
        )
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_skills_team_id` ON `skills` (`team_id`)")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'agents'",
        )
    }
}

/**
 * v111 (eApps V1, #VAPP-210): `apps.published_version` (the version members
 * run; 0 / null = never published) and `apps.draft_package` (the head's
 * package while it is ahead). Only the apps shape refetches, so synced rows
 * re-arrive carrying them.
 */
val MIGRATION_110_111: Migration = object : Migration(110, 111) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `apps` ADD COLUMN `published_version` INTEGER")
        db.execSQL("ALTER TABLE `apps` ADD COLUMN `draft_package` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'apps'",
        )
    }
}

/**
 * v112 (#VAPP-213): `skills.builtin` (a shipped, read-only skill). Only the
 * skills shape refetches, so synced rows re-arrive carrying it.
 */
val MIGRATION_111_112: Migration = object : Migration(111, 112) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `skills` ADD COLUMN `builtin` INTEGER NOT NULL DEFAULT 0")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'skills'",
        )
    }
}

/**
 * v113 (#VAPP-201): `teams.time_zone` (an IANA name app hosts read dates in;
 * NULL = each device's own). Only the teams shape refetches, so synced rows
 * re-arrive carrying it.
 */
val MIGRATION_112_113: Migration = object : Migration(112, 113) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `teams` ADD COLUMN `time_zone` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'teams'",
        )
    }
}

/**
 * v114 (#VAPP-229): `issues.via_app_id`, `sessions.via_app_id` and
 * `app_records.via_app_id`, the app whose tool caused the row ("via <app>").
 * Only those three shapes refetch.
 */
val MIGRATION_113_114: Migration = object : Migration(113, 114) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `issues` ADD COLUMN `via_app_id` TEXT")
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `via_app_id` TEXT")
        db.execSQL("ALTER TABLE `app_records` ADD COLUMN `via_app_id` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` IN ('issues', 'sessions', 'app_records')",
        )
    }
}

/**
 * v115 (#VAPP-235): the offline outbox, `app_record_outbox` (local only, no
 * shape: the app host's pending record writes survive a restart). Nothing
 * refetches.
 */
val MIGRATION_114_115: Migration = object : Migration(114, 115) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `app_record_outbox` (`write_id` TEXT NOT NULL, `seq` INTEGER NOT NULL, " +
                "`app_id` TEXT NOT NULL, `call` TEXT NOT NULL, `writes` TEXT NOT NULL, `at` TEXT NOT NULL, " +
                "`state` TEXT NOT NULL, `acked_at` TEXT, PRIMARY KEY(`write_id`))",
        )
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_app_record_outbox_app_id` ON `app_record_outbox` (`app_id`)")
    }
}

/**
 * v116 (#VAPP-233, eApps R8): a version's record sweep progress on
 * `app_versions` (apps.json `migrationCases`). Only the app-versions shape
 * refetches, so synced rows re-arrive carrying it.
 */
val MIGRATION_115_116: Migration = object : Migration(115, 116) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `app_versions` ADD COLUMN `migration_state` TEXT")
        db.execSQL("ALTER TABLE `app_versions` ADD COLUMN `migration_total` INTEGER DEFAULT 0")
        db.execSQL("ALTER TABLE `app_versions` ADD COLUMN `migration_done` INTEGER DEFAULT 0")
        db.execSQL("ALTER TABLE `app_versions` ADD COLUMN `migration_error` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'app_versions'",
        )
    }
}

/**
 * v117 (#EXP-1682): `messages.attachment_ids` (the files a message carries in
 * the clear; the chat row draws them as embeds). Only the messages shape
 * refetches, so synced rows re-arrive carrying it.
 */
val MIGRATION_116_117: Migration = object : Migration(116, 117) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `messages` ADD COLUMN `attachment_ids` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'messages'",
        )
    }
}

/**
 * v118 (#VAPP-241): `app_records.share_name`, the public link a guest's record
 * came through (app-share.json). Only the app_records shape refetches, so
 * synced rows re-arrive carrying it.
 */
val MIGRATION_117_118: Migration = object : Migration(117, 118) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `app_records` ADD COLUMN `share_name` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'app_records'",
        )
    }
}

/**
 * v119 (#EXP-1710, merge-train.json): the merge train's synced state —
 * `pull_requests.merge_state / merge_wait / merge_attempts / merge_hold /
 * merge_queued_since / merge_now / merge_position / merge_train_id` and
 * `teams.merge_mode`. Self-contained (renumbering = this object's two
 * versions + the database version): only the pull_requests and teams shapes
 * refetch, so synced rows re-arrive carrying the columns.
 */
val MIGRATION_118_119: Migration = object : Migration(118, 119) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `pull_requests` ADD COLUMN `merge_state` TEXT NOT NULL DEFAULT 'checks'")
        db.execSQL("ALTER TABLE `pull_requests` ADD COLUMN `merge_wait` TEXT")
        db.execSQL("ALTER TABLE `pull_requests` ADD COLUMN `merge_attempts` INTEGER")
        db.execSQL("ALTER TABLE `pull_requests` ADD COLUMN `merge_hold` TEXT")
        db.execSQL("ALTER TABLE `pull_requests` ADD COLUMN `merge_queued_since` TEXT")
        db.execSQL("ALTER TABLE `pull_requests` ADD COLUMN `merge_now` INTEGER NOT NULL DEFAULT 0")
        db.execSQL("ALTER TABLE `pull_requests` ADD COLUMN `merge_position` INTEGER")
        db.execSQL("ALTER TABLE `pull_requests` ADD COLUMN `merge_train_id` TEXT")
        db.execSQL("ALTER TABLE `teams` ADD COLUMN `merge_mode` TEXT NOT NULL DEFAULT 'auto'")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` IN ('pull_requests', 'teams')",
        )
    }
}

/**
 * v120 (#EXP-1715, composer-menu.json `teamsPick`): `sessions.scope_team_ids`,
 * the Assistant conversation root's Teams pick (NULL = every team). Only the
 * sessions shape refetches, so synced rows re-arrive carrying it.
 */
val MIGRATION_119_120: Migration = object : Migration(119, 120) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `scope_team_ids` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'sessions'",
        )
    }
}

/**
 * v121 (#EXP-1730 phase A, the one-tree model): `messages.session_id` (+ its
 * index; every message reader keys by it), `sessions.digest` (the list
 * digest, mirrored server-side from the conversation), `agents.defaults`
 * (the runner picks as one object) and `apps.agent_id`. The local
 * `conversations` table goes (no reader left) with its shape offset. The
 * messages, sessions, agents and apps shapes refetch, so synced rows
 * re-arrive carrying the columns.
 */
val MIGRATION_120_121: Migration = object : Migration(120, 121) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `messages` ADD COLUMN `session_id` TEXT")
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_messages_session_id` ON `messages` (`session_id`)")
        db.execSQL("ALTER TABLE `sessions` ADD COLUMN `digest` TEXT")
        db.execSQL("ALTER TABLE `agents` ADD COLUMN `defaults` TEXT")
        db.execSQL("ALTER TABLE `apps` ADD COLUMN `agent_id` TEXT")
        db.execSQL("DROP TABLE IF EXISTS `conversations`")
        db.execSQL("DELETE FROM `electric_offsets` WHERE `shape` = 'conversations'")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` IN ('messages', 'sessions', 'agents', 'apps')",
        )
    }
}

/**
 * v122 (#EXP-1793, agent-settings.json `mcpRow`): `agents.mcp_server_ids`,
 * the team MCP servers every run of the agent starts with. Only the agents
 * shape refetches, so synced rows re-arrive carrying it.
 */
val MIGRATION_121_122: Migration = object : Migration(121, 122) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `agents` ADD COLUMN `mcp_server_ids` TEXT")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'agents'",
        )
    }
}

// v123 (large chats): a chat's newest page (`observeNewestByRoot`) and its
// result cards (`observeResultCardStamps`) read the tree in transcript order:
// one (root_id, created_at, id) index makes both range reads; it covers the
// old (root_id) prefix, which goes. Local only: nothing refetches.
val MIGRATION_122_123: Migration = object : Migration(122, 123) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("DROP INDEX IF EXISTS `index_messages_root_id`")
        db.execSQL(
            "CREATE INDEX IF NOT EXISTS `index_messages_root_id_created_at_id` " +
                "ON `messages` (`root_id`, `created_at`, `id`)",
        )
    }
}

// v124 (#FEED-219, composer-draft.json): the `composer_drafts` table, every
// composer's synced draft. A new shape: nothing else refetches.
val MIGRATION_123_124: Migration = object : Migration(123, 124) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `composer_drafts` (`id` TEXT NOT NULL, `user_id` TEXT NOT NULL, " +
                "`team_id` TEXT NOT NULL, `key` TEXT NOT NULL, `text` TEXT NOT NULL, " +
                "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
        )
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_composer_drafts_key` ON `composer_drafts` (`key`)")
    }
}

// v125 (#EXP-1617 / #EXP-1804, ci-run.json + runner-settings.json):
// `devices.runner` (the owner's CI runner settings) and the `ci_runs` +
// `ci_jobs` tables (our own CI's run page). The devices shape refetches so
// synced rows re-arrive with the column; the two new shapes start fresh.
val MIGRATION_124_125: Migration = object : Migration(124, 125) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `devices` ADD COLUMN `runner` TEXT")
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `ci_runs` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
                "`repository_id` TEXT NOT NULL, `number` INTEGER, `name` TEXT NOT NULL, `sha` TEXT NOT NULL, " +
                "`ref` TEXT, `trigger` TEXT NOT NULL, `pull_request_id` TEXT, `state` TEXT NOT NULL, " +
                "`error` TEXT, `rerun_of` TEXT, `created_by_user_id` TEXT, `started_at` TEXT, " +
                "`completed_at` TEXT, `created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
        )
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_ci_runs_team_id` ON `ci_runs` (`team_id`)")
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `ci_jobs` (`id` TEXT NOT NULL, `team_id` TEXT NOT NULL, " +
                "`run_id` TEXT NOT NULL, `key` TEXT NOT NULL, `check` TEXT NOT NULL, `position` INTEGER, " +
                "`runs_on` TEXT, `needs` TEXT, `matrix` TEXT, `timeout_min` INTEGER, `status` TEXT NOT NULL, " +
                "`conclusion` TEXT, `reason` TEXT, `runner_device_id` TEXT, `steps` TEXT, `log_key` TEXT, " +
                "`log_bytes` INTEGER, `queued_at` TEXT, `started_at` TEXT, `completed_at` TEXT, " +
                "`created_at` TEXT NOT NULL, `updated_at` TEXT NOT NULL, PRIMARY KEY(`id`))",
        )
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_ci_jobs_run_id` ON `ci_jobs` (`run_id`)")
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_ci_jobs_team_id` ON `ci_jobs` (`team_id`)")
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_ci_jobs_runner_device_id` ON `ci_jobs` (`runner_device_id`)")
        db.execSQL(
            "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                "`needs_refetch` = 1 WHERE `shape` = 'devices'",
        )
    }
}
