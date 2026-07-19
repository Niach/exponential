package com.exponential.app.ui.share

import com.exponential.app.data.TeamSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn

data class TeamBoards(val team: TeamEntity, val boards: List<BoardEntity>)

data class ShareTargetState(
    val groups: List<TeamBoards> = emptyList(),
    val recentBoardId: String? = null,
    val isLoading: Boolean = true,
)

/**
 * Data source for the single-screen share composer (`share-compose`): the
 * active account's teams → boards, with the most recently opened board
 * surfaced as the default. Consumed by [com.exponential.app.ui.issue.CreateIssueScreen]
 * in share mode, which renders the "Share to" destination selector at the top
 * of the form (EXP-60), backed by [ShareBoardPickerSheet].
 */
@HiltViewModel
class ShareTargetPickerViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
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
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ShareTargetState())
}
