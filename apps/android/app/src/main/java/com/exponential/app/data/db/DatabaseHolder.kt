package com.exponential.app.data.db

import android.content.Context
import androidx.room.Room
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

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
                // 104 → 105 (EXP-1459), 105 → 106 (#VAPP-186) and 106 → 107 (#EXP-1534) migrate in place (rows and every
                // shape's offset survive). Any OLDER schema wipes and lets
                // Electric resync: every table but electric_offsets is a synced
                // shape, so nothing local is lost.
                .addMigrations(MIGRATION_104_105, MIGRATION_105_106, MIGRATION_106_107, MIGRATION_107_108, MIGRATION_108_109, MIGRATION_109_110, MIGRATION_110_111, MIGRATION_111_112, MIGRATION_112_113, MIGRATION_113_114, MIGRATION_114_115, MIGRATION_115_116, MIGRATION_116_117, MIGRATION_117_118, MIGRATION_118_119, MIGRATION_119_120, MIGRATION_120_121, MIGRATION_121_122, MIGRATION_122_123, MIGRATION_123_124)
                .fallbackToDestructiveMigration(dropAllTables = true)
                .build()
            instances[accountId] = db
            return db
        }
    }

    /// Close one account's Room instance. The underlying .db file stays on
    /// disk; use `deleteFiles(accountId)` to also wipe it.
    ///
    /// #EXP-1599: the instance leaves the map at once (the next `database()`
    /// opens a fresh one) but closes after [CLOSE_GRACE_MS]. Account-scoped
    /// flows run their queries off the main thread and only re-scope once the
    /// account change reaches them; closing under a query in flight threw
    /// "connection pool has been closed" and killed the app, on every login
    /// (the pending account's DB is swept then) and every account removal.
    fun close(accountId: String) {
        val db = synchronized(lock) { instances.remove(accountId) } ?: return
        closer.launch {
            delay(CLOSE_GRACE_MS)
            db.close()
        }
    }

    private val closer = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private companion object {
        const val CLOSE_GRACE_MS = 10_000L
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
