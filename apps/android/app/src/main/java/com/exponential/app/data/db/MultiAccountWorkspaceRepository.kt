package com.exponential.app.data.db

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map

/// One section in the unified cross-server workspace picker: every signed-in
/// server contributes one group, headed by its hostname.
data class ServerWorkspaceGroup(
    val accountId: String,
    val hostname: String,
    val userEmail: String?,
    val workspaces: List<WorkspaceEntity>,
)

/// Aggregates each signed-in account's `workspaces` table into a single
/// `Flow<List<ServerWorkspaceGroup>>` for the cross-server picker.
///
/// Sources every per-account Room instance through `DatabaseHolder.database(accountId)`,
/// which is the same instance SyncManager writes to — so observations see
/// every Electric update for every server immediately.
@Singleton
class MultiAccountWorkspaceRepository @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
) {
    @OptIn(ExperimentalCoroutinesApi::class)
    val serverGroups: Flow<List<ServerWorkspaceGroup>> =
        combine(auth.accounts, auth.activeAccountId) { accounts, activeId ->
            accounts to activeId
        }
            .distinctUntilChanged()
            .flatMapLatest { (accounts, activeId) ->
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
                            holder.database(forAccountId = account.id)
                                .workspaceDao()
                                .observeAll()
                                .map { ws -> account to ws }
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
}
