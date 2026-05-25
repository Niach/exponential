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
                // Schema is canonical; if it ever drifts we wipe and let
                // Electric resync. No explicit Migration objects on purpose.
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

    /// Close every open instance. App teardown / sign-out-all.
    fun close() {
        synchronized(lock) {
            instances.values.forEach { it.close() }
            instances.clear()
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
