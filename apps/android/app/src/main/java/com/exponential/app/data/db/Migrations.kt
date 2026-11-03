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
