package com.exponential.app.data.db

import android.content.Context
import androidx.room.Room
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/// Multi-account database holder. Maintains one open Room instance per
/// signed-in account (`exponential-<accountId>-v2.db`), keyed by accountId.
///
/// New code should always call `database(forAccountId:)` so writes land in
/// the right per-server file. The transitional `current()` / `database`
/// (StateFlow) APIs resolve to a `lastUsedAccountId` mark and exist only so
/// existing UI ValueObservation / DAO facade plumbing keeps compiling during
/// the Phase B/C UI rework — they're removed once every caller carries an
/// accountId.
@Singleton
class DatabaseHolder @Inject constructor(
    @ApplicationContext private val context: Context,
) {
    private val lock = Any()
    private val instances = mutableMapOf<String, ExponentialDatabase>()
    private var lastUsedAccountId: String? = null

    // Transitional: published so the existing DAO facades' `flatMapLatest`
    // observers continue to auto-rebind whenever the active account changes
    // (i.e. switchTo / open is called for a different accountId). Once Phase B
    // routes carry accountId, callers move to `databaseFlow(forAccountId:)`
    // below and this single-active StateFlow goes away.
    private val _database = MutableStateFlow<ExponentialDatabase?>(null)
    val database: StateFlow<ExponentialDatabase?> = _database.asStateFlow()

    /// Get (or open) the Room instance for the given account. Subsequent
    /// calls for the same accountId return the cached instance.
    fun database(forAccountId: String): ExponentialDatabase {
        val accountId = forAccountId
        synchronized(lock) {
            instances[accountId]?.let {
                lastUsedAccountId = accountId
                _database.value = it
                return it
            }
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
            lastUsedAccountId = accountId
            _database.value = db
            return db
        }
    }

    /// **Transitional**: returns the most-recently-used account's instance.
    /// Existing UI code paths that don't yet thread an accountId through
    /// (project list filters, workspace settings observers) still rely on
    /// this. Throws if no instance has been opened.
    fun current(): ExponentialDatabase {
        return _database.value
            ?: error("DatabaseHolder accessed before database(forAccountId:) was called")
    }

    /// **Transitional**: legacy `switchTo` API kept so SyncManager's
    /// pre-multi-account callers and any UI code paths that still call into
    /// it keep working. New code calls `database(forAccountId:)` directly.
    fun switchTo(accountId: String) {
        database(forAccountId = accountId)
    }

    fun isOpen(): Boolean = _database.value != null

    /// Close one account's Room instance. The underlying .db file stays on
    /// disk; use `deleteFiles(accountId)` to also wipe it.
    fun close(accountId: String) {
        synchronized(lock) {
            instances.remove(accountId)?.close()
            if (lastUsedAccountId == accountId) {
                lastUsedAccountId = instances.keys.firstOrNull()
                _database.value = lastUsedAccountId?.let { instances[it] }
            }
        }
    }

    /// Close every open instance. App teardown / sign-out-all.
    fun close() {
        synchronized(lock) {
            instances.values.forEach { it.close() }
            instances.clear()
            lastUsedAccountId = null
            _database.value = null
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
