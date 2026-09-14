package com.exponential.app.ui.drafts

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssueDraftsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.DraftRow
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.resolveDraftRows
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * The "Drafts" section of My Work (EXP-878): every unsent issue draft on the
 * active account, newest-edited first. ACCOUNT-WIDE, not team-scoped — a draft
 * is personal, and each row names the board it will be filed on — which is also
 * why the join drops a draft whose board no longer resolves locally.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class DraftsViewModel @Inject constructor(
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issueDraftsApi: IssueDraftsApi,
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
                ) { drafts, boards, statuses, teams ->
                    resolveDraftRows(
                        drafts = drafts,
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
    fun delete(draftId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { holder.database(forAccountId = accountId).issueDraftDao().deleteById(draftId) }
            runCatching { issueDraftsApi.delete(accountId, draftId) }
                .onFailure { android.util.Log.w("DraftsViewModel", "Draft delete failed", it) }
        }
    }
}
