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
