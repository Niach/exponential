package com.exponential.app.data.db

import android.content.Context
import androidx.room.Room
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/// Owns the currently-open ExponentialDatabase. Each server account gets its own
/// SQLite file (`exponential-<accountId>.db`) so switching servers keeps each
/// server's cache intact. DAO consumers should go through the facade DAOs in
/// DatabaseModule, which delegate to the holder so reads/writes always hit the
/// current database.
@Singleton
class DatabaseHolder @Inject constructor(
    @ApplicationContext private val context: Context,
) {
    private val _database = MutableStateFlow<ExponentialDatabase?>(null)
    val database: StateFlow<ExponentialDatabase?> = _database.asStateFlow()
    private val lock = Any()
    private var currentAccountId: String? = null

    fun current(): ExponentialDatabase {
        return _database.value
            ?: error("DatabaseHolder accessed before switchTo(accountId) was called")
    }

    fun isOpen(): Boolean = _database.value != null

    /// Switch to the per-account SQLite file. Closes the previously-open DB first.
    fun switchTo(accountId: String) {
        synchronized(lock) {
            if (currentAccountId == accountId && _database.value != null) return
            _database.value?.close()
            // Any device that ran a pre-consolidation build has an
            // `exponential-<account>.db` carrying the v5 schema (plus three
            // explicit Migration objects). The post-consolidation schema lives
            // in `-v2.db`, so the legacy file is unreachable forever — purge it
            // on first launch so it doesn't sit in the app's databases dir.
            context.deleteDatabase("exponential-$accountId.db")
            val db = Room.databaseBuilder(
                context,
                ExponentialDatabase::class.java,
                "exponential-$accountId-v2.db",
            )
                // The schema is canonical; if it ever drifts we wipe and let
                // Electric resync. No explicit Migration objects on purpose.
                .fallbackToDestructiveMigration(dropAllTables = true)
                .build()
            _database.value = db
            currentAccountId = accountId
        }
    }

    fun close() {
        synchronized(lock) {
            _database.value?.close()
            _database.value = null
            currentAccountId = null
        }
    }

    fun deleteFiles(accountId: String) {
        synchronized(lock) {
            if (currentAccountId == accountId) close()
            context.deleteDatabase("exponential-$accountId-v2.db")
            // Also delete any pre-consolidation file if it survived an upgrade.
            context.deleteDatabase("exponential-$accountId.db")
        }
    }
}
