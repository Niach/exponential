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
            val db = Room.databaseBuilder(
                context,
                ExponentialDatabase::class.java,
                "exponential-$accountId.db",
            )
                .addMigrations(
                    ExponentialDatabase.MIGRATION_2_3,
                    ExponentialDatabase.MIGRATION_3_4,
                    ExponentialDatabase.MIGRATION_4_5,
                )
                .fallbackToDestructiveMigration()
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
            context.deleteDatabase("exponential-$accountId.db")
        }
    }
}
