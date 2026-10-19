package com.exponential.app.ui.drafts

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssueDraftsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.DraftRow
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.activeTeamDrafts
import com.exponential.app.domain.resolveDraftRows
import com.exponential.app.ui.components.FailedAction
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * The "Drafts" section of My Work (EXP-878): the ACTIVE team's unsent issue
 * drafts on the active account, newest-edited first (web's Drafts page,
 * `useTeamDrafts(teamId)`; the Inbox dot counts the same rows). A draft is
 * personal and each row names the board it will be filed on — which is also
 * why the join drops a draft whose board no longer resolves locally.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class DraftsViewModel @Inject constructor(
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issueDraftsApi: IssueDraftsApi,
    private val teamSelection: TeamSelection,
) : ViewModel() {

    val drafts: StateFlow<List<DraftRow>> = accountDatabaseFlow(auth, holder)
        .flatMapLatest { db ->
            if (db == null) {
                flowOf(emptyList())
            } else {
                combine(
                    db.issueDraftDao().observeAll(),
                    db.boardDao().observeAll(),
                    db.issueStatusDao().observeAll(),
                    db.teamDao().observeAll(),
                    teamSelection.selectedId,
                ) { drafts, boards, statuses, teams, teamId ->
                    resolveDraftRows(
                        drafts = activeTeamDrafts(drafts, teamId),
                        boards = boards,
                        statusesByTeam = statuses
                            .groupBy { it.teamId }
                            .mapValues { (_, rows) -> IssueStatusResolver.teamStatuses(rows) },
                        teamsById = teams.associateBy { it.id },
                    )
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * Delete a draft from the list. Optimistic: the local row goes first so the
     * row leaves the list immediately, then the server; Electric re-delivers
     * the same delete on its next poll.
     */
    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    fun delete(draftId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { holder.database(forAccountId = accountId).issueDraftDao().deleteById(draftId) }
            runCatching { issueDraftsApi.delete(accountId, draftId) }
                .onFailure {
                    android.util.Log.w("DraftsViewModel", "Draft delete failed", it)
                    // FEED-79 (web drafts-list parity): a toast, no Retry.
                    _failure.value = FailedAction("discard", "the draft", trpcErrorMessage(it, ""))
                }
        }
    }
}
