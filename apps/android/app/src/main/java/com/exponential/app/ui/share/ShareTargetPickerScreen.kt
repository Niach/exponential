package com.exponential.app.ui.share

import com.exponential.app.data.TeamSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.SessionTree
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOn

data class TeamBoards(val team: TeamEntity, val boards: List<BoardEntity>)

/** EXP-1635: the Share to page's rows for the active team. */
data class ShareToState(
    val teams: List<TeamEntity> = emptyList(),
    val teamId: String? = null,
    val agents: List<SessionTree.AgentRow> = emptyList(),
)

data class ShareTargetState(
    val groups: List<TeamBoards> = emptyList(),
    val recentBoardId: String? = null,
    val isLoading: Boolean = true,
)

/**
 * Data source for the single-screen share composer (`share-compose`): the
 * active account's teams → boards, with the most recently opened board
 * surfaced as the default. Consumed by [com.exponential.app.ui.issue.IssueDraftScreen]
 * in share mode, whose board chip picks the destination (EXP-60/1170), backed
 * by [ShareBoardPickerSheet].
 */
@HiltViewModel
class ShareTargetPickerViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    private val selection: TeamSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    val state: StateFlow<ShareTargetState> = combine(
        dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        auth.activeAccountId,
    ) { teams, boards, accountId ->
        val byTeam = boards.groupBy { it.teamId }
        val groups = teams.mapNotNull { ws ->
            val ps = byTeam[ws.id].orEmpty()
            if (ps.isEmpty()) null else TeamBoards(ws, ps)
        }
        ShareTargetState(
            groups = groups,
            recentBoardId = accountId?.let { selection.lastBoard(it) },
            isLoading = false,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ShareTargetState())

    /** share-targets.json: the Agents section = the active team's agents in
     *  `orderedAgents` order (the Assistant first). */
    val shareTo: StateFlow<ShareToState> = combine(
        dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.agentDefinitionDao().observeAll() },
        selection.selectedId,
        auth.userId,
    ) { teams, agents, teamId, me ->
        val listed = SessionTree.listAgents(agents.map(SessionTree::agentDefinition), emptyList(), teamId, me)
        val row = com.exponential.app.ui.agent.composerAgentRowState(listed, me, null)
        ShareToState(
            teams = teams,
            teamId = teamId,
            // Already `orderedAgents`: the composer row's order is THE order.
            agents = if (teamId == null) emptyList() else row.agents,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ShareToState())

    fun selectTeam(teamId: String) {
        selection.select(teamId)
    }
}
