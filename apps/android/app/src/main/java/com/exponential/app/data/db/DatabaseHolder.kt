package com.exponential.app.data.db

import android.content.Context
import androidx.room.Room
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

/// Multi-account database holder. Maintains one open Room instance per
/// signed-in account (`exponential-<accountId>-v2.db`), keyed by accountId.
@Singleton
class DatabaseHolder @Inject constructor(
    @ApplicationContext private val context: Context,
) {
    private val lock = Any()
    private val instances = mutableMapOf<String, ExponentialDatabase>()

    /// Get (or open) the Room instance for the given account. Subsequent
    /// calls for the same accountId return the cached instance.
    fun database(forAccountId: String): ExponentialDatabase {
        val accountId = forAccountId
        synchronized(lock) {
            instances[accountId]?.let { return it }
            // Any device that ran a pre-consolidation build has an
            // `exponential-<account>.db` carrying the v5 schema. The
            // post-consolidation schema lives in `-v2.db`, so the legacy file
            // is unreachable forever — purge it on first launch so it doesn't
            // sit in the app's databases dir.
            context.deleteDatabase("exponential-$accountId.db")
            val db = Room.databaseBuilder(
                context,
                ExponentialDatabase::class.java,
                "exponential-$accountId-v2.db",
            )
                // SLOP-3: 78 → 79 and SLOP-4: 79 → 80 migrate in place (rows
                // and every shape's offset survive), as do EXP-1196's 80 → 81
                // (one added column), EXP-1284's 84 → 85 (two new tables),
                // EXP-1291's 86 → 87 (the session tree), EXP-1321's 87 → 88
                // (sessions.dispatch) and EXP-1323's 88 → 89 (sessions.caption
                // + aside, messages.author_session_id, the session_reads table).
                // Any OLDER schema still wipes and lets Electric resync.
                .addMigrations(MIGRATION_78_79, MIGRATION_79_80, MIGRATION_80_81, MIGRATION_81_82, MIGRATION_82_83, MIGRATION_83_84, MIGRATION_84_85, MIGRATION_85_86, MIGRATION_86_87, MIGRATION_87_88, MIGRATION_88_89, MIGRATION_89_90, MIGRATION_90_91, MIGRATION_91_92, MIGRATION_92_93, MIGRATION_93_94, MIGRATION_94_95, MIGRATION_95_96, MIGRATION_96_97, MIGRATION_97_98, MIGRATION_98_99)
                .fallbackToDestructiveMigration(dropAllTables = true)
                .build()
            instances[accountId] = db
            return db
        }
    }

    /// Close one account's Room instance. The underlying .db file stays on
    /// disk; use `deleteFiles(accountId)` to also wipe it.
    fun close(accountId: String) {
        synchronized(lock) {
            instances.remove(accountId)?.close()
        }
    }

    fun deleteFiles(accountId: String) {
        synchronized(lock) {
            close(accountId)
            context.deleteDatabase("exponential-$accountId-v2.db")
            // Also delete any pre-consolidation file if it survived an upgrade.
            context.deleteDatabase("exponential-$accountId.db")
        }
    }
}
