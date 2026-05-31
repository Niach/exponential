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

/// One workspace's block inside the cross-server Home tree.
data class WorkspaceBlock(
    val workspace: WorkspaceEntity,
    val projects: List<ProjectEntity>,
)

/// One server's block inside the cross-server Home tree.
data class ServerProjectGroup(
    val accountId: String,
    val hostname: String,
    val userEmail: String?,
    val workspaceBlocks: List<WorkspaceBlock>,
)

/// Combines every signed-in account's `workspaces` + `projects` tables into a
/// single `Flow<List<ServerProjectGroup>>` for the Home screen tree.
///
/// Sources every per-account Room instance through `DatabaseHolder.database(forAccountId:)`,
/// so observations see writes from SyncManager's parallel shape pipelines
/// immediately.
@Singleton
class MultiAccountProjectRepository @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
) {
    @OptIn(ExperimentalCoroutinesApi::class)
    val serverGroups: Flow<List<ServerProjectGroup>> =
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
                    val perAccount: List<Flow<Triple<ServerAccount, List<WorkspaceEntity>, List<ProjectEntity>>>> =
                        eligible.map { account ->
                            val db = holder.database(forAccountId = account.id)
                            combine(
                                db.workspaceDao().observeAll(),
                                db.projectDao().observeAll(),
                            ) { workspaces, projects ->
                                Triple(account, workspaces, projects)
                            }
                        }
                    combine(perAccount) { entries ->
                        entries.mapNotNull { (account, workspaces, projects) ->
                            val byWorkspace = projects.groupBy { it.workspaceId }
                            val blocks = workspaces
                                .sortedBy { it.name.lowercase() }
                                .map { ws ->
                                    val projs = (byWorkspace[ws.id] ?: emptyList())
                                        .filter { it.archivedAt == null }
                                    // Include every workspace, even ones with an empty or
                                    // all-archived project list — they render as a header
                                    // with no project rows (parity with iOS Home).
                                    WorkspaceBlock(workspace = ws, projects = projs)
                                }
                            if (blocks.isEmpty()) null
                            else ServerProjectGroup(
                                accountId = account.id,
                                hostname = account.displayHost,
                                userEmail = account.userEmail,
                                workspaceBlocks = blocks,
                            )
                        }
                    }
                }
            }
}
