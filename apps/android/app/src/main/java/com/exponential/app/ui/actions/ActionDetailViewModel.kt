package com.exponential.app.ui.actions

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.PromptDto
import com.exponential.app.data.api.toPromptDto
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.DomainContract
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

// NEW-2: the page reads the synced `prompts` row (route id = a prompt id).
// The action page's data (EXP-1291: an action is a SAVED PROMPT — no
// triggers, no runs tab): ONE action's synced row (name, icon, inputs) and
// whether the caller owns its team. The prompt body is the editor's own fetch
// ([ActionEditViewModel]).

data class ActionDetailState(
    /** The synced row; null while loading, or once the action is gone. */
    val action: PromptDto? = null,
    val loading: Boolean = true,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ActionDetailViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
) : ViewModel() {

    val actionId: String = savedStateHandle["actionId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val state: StateFlow<ActionDetailState> = dbFlow.flatMapLatest { db ->
        if (db == null) {
            flowOf(ActionDetailState(loading = false))
        } else {
            db.promptDao().observeById(actionId).map { row ->
                ActionDetailState(action = row?.toPromptDto(), loading = false)
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ActionDetailState())

    /** Whether the current user OWNS the action's team (owner-gated edits). */
    val isTeamOwner: StateFlow<Boolean> = combine(
        dbFlow,
        state.map { it.action?.teamId }.distinctUntilChanged(),
    ) { db, teamId -> db to teamId }
        .flatMapLatest { (db, teamId) ->
            if (db == null || teamId == null) {
                flowOf(emptyList())
            } else {
                db.teamMemberDao().observeByTeam(teamId)
            }
        }.combine(auth.userId) { members, userId ->
            userId != null &&
                members.firstOrNull { it.userId == userId }?.role == DomainContract.teamRoleOwner
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)
}
