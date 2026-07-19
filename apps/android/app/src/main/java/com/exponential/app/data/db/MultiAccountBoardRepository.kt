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

/// One team's block inside the cross-server Home tree.
data class TeamBlock(
    val team: TeamEntity,
    val boards: List<BoardEntity>,
)

/// One server's block inside the cross-server Home tree.
data class ServerBoardGroup(
    val accountId: String,
    val hostname: String,
    val userEmail: String?,
    val teamBlocks: List<TeamBlock>,
)

/// Combines every signed-in account's `teams` + `boards` tables into a
/// single `Flow<List<ServerBoardGroup>>` for the Home screen tree.
///
/// Sources every per-account Room instance through `DatabaseHolder.database(forAccountId:)`,
/// so observations see writes from SyncManager's parallel shape pipelines
/// immediately.
@Singleton
class MultiAccountBoardRepository @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
) {
    @OptIn(ExperimentalCoroutinesApi::class)
    val serverGroups: Flow<List<ServerBoardGroup>> =
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
                    val perAccount: List<Flow<Triple<ServerAccount, List<TeamEntity>, List<BoardEntity>>>> =
                        eligible.map { account ->
                            val db = holder.database(forAccountId = account.id)
                            combine(
                                db.teamDao().observeAll(),
                                db.boardDao().observeAll(),
                            ) { teams, boards ->
                                Triple(account, teams, boards)
                            }
                        }
                    combine(perAccount) { entries ->
                        entries.mapNotNull { (account, teams, boards) ->
                            val byTeam = boards.groupBy { it.teamId }
                            val blocks = teams
                                .sortedBy { it.name.lowercase() }
                                .map { ws ->
                                    val projs = (byTeam[ws.id] ?: emptyList())
                                        .filter { it.archivedAt == null }
                                    // Include every team, even ones with an empty or
                                    // all-archived board list — they render as a header
                                    // with no board rows (parity with iOS Home).
                                    TeamBlock(team = ws, boards = projs)
                                }
                            if (blocks.isEmpty()) null
                            else ServerBoardGroup(
                                accountId = account.id,
                                hostname = account.displayName,
                                userEmail = account.userEmail,
                                teamBlocks = blocks,
                            )
                        }
                    }
                }
            }
}
