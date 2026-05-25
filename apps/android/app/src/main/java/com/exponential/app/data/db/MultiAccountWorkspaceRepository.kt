package com.exponential.app.data.db

import android.content.Context
import androidx.room.Room
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map

// One section in the unified cross-server workspace picker: every signed-in
// server contributes one group, headed by its hostname.
data class ServerWorkspaceGroup(
    val accountId: String,
    val hostname: String,
    val userEmail: String?,
    val workspaces: List<WorkspaceEntity>,
)

/// Combines workspaces from every signed-in account's local Room database
/// into a single Flow<List<ServerWorkspaceGroup>> for the cross-server
/// workspace picker.
///
/// Active account: sourced via the injected `WorkspaceDao` facade (which
/// internally routes to `DatabaseHolder.current()`). Inactive signed-in
/// accounts: each gets its own Room instance pointing at
/// `exponential-<accountId>-v2.db`. We never write through these shadow
/// instances — only `observeAll()` is called on them.
@Singleton
class MultiAccountWorkspaceRepository @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    private val workspaceDao: WorkspaceDao,
) {
    private val shadowDbs = mutableMapOf<String, ExponentialDatabase>()
    private val lock = Any()

    @OptIn(ExperimentalCoroutinesApi::class)
    val serverGroups: Flow<List<ServerWorkspaceGroup>> =
        combine(auth.accounts, auth.activeAccountId) { accounts, activeId ->
            accounts to activeId
        }
            .distinctUntilChanged()
            .flatMapLatest { (accounts, activeId) ->
                // flatMapLatest cancels the previous inner Flow before this
                // transform runs, so any observation on a soon-to-be-closed
                // shadow has already been torn down by the time we close it.
                closeOrphanShadows(accounts, activeId)

                val eligible = accounts
                    .filter { it.token != null }
                    .sortedWith(
                        compareByDescending<ServerAccount> { it.id == activeId }
                            .thenByDescending { it.lastUsedAt },
                    )

                if (eligible.isEmpty()) {
                    flowOf(emptyList())
                } else {
                    val perAccount: List<Flow<Pair<ServerAccount, List<WorkspaceEntity>>>> =
                        eligible.map { account ->
                            workspacesFor(account, activeId).map { ws -> account to ws }
                        }
                    combine(perAccount) { entries ->
                        // Hide empty groups — a newly added server that hasn't
                        // delivered its first Electric shape yet would otherwise
                        // show a header with no rows.
                        entries.mapNotNull { (account, ws) ->
                            if (ws.isEmpty()) {
                                null
                            } else {
                                ServerWorkspaceGroup(
                                    accountId = account.id,
                                    hostname = account.displayHost,
                                    userEmail = account.userEmail,
                                    workspaces = ws.sortedBy { it.name.lowercase() },
                                )
                            }
                        }
                    }
                }
            }

    private fun workspacesFor(
        account: ServerAccount,
        activeId: String?,
    ): Flow<List<WorkspaceEntity>> {
        return if (account.id == activeId) {
            // The facade routes through DatabaseHolder, so we don't open a
            // second Room instance against the read-write file.
            workspaceDao.observeAll()
        } else {
            val shadow = openShadow(account.id) ?: return flowOf(emptyList())
            shadow.workspaceDao().observeAll()
        }
    }

    private fun openShadow(accountId: String): ExponentialDatabase? {
        synchronized(lock) {
            shadowDbs[accountId]?.let { return it }
            val path = context.getDatabasePath("exponential-$accountId-v2.db")
            // Account just added but has never finished an Electric sync — the
            // file doesn't exist yet, so there's nothing to read.
            if (!path.exists()) return null
            val db = Room.databaseBuilder(
                context,
                ExponentialDatabase::class.java,
                "exponential-$accountId-v2.db",
            )
                .fallbackToDestructiveMigration(dropAllTables = true)
                .build()
            shadowDbs[accountId] = db
            return db
        }
    }

    private fun closeOrphanShadows(accounts: List<ServerAccount>, activeId: String?) {
        synchronized(lock) {
            val desired = accounts
                .asSequence()
                .filter { it.token != null && it.id != activeId }
                .map { it.id }
                .toSet()
            shadowDbs.keys.toList().forEach { id ->
                if (id !in desired) {
                    shadowDbs.remove(id)?.close()
                }
            }
        }
    }
}
