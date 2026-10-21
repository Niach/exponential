package com.exponential.app.ui.actions

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.routeTeamIdFlow
import com.exponential.app.data.api.PromptDto
import com.exponential.app.data.api.PromptFields
import com.exponential.app.data.api.PromptsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.DomainContract
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// NEW-2: reads + writes go through `prompts.get/update/delete` (the
// `actions` router is gone).
// The action editor's data layer (EXP-694 — editing stopped being
// web/desktop-only; SLOP-2: the action page's Prompt tab). The list metadata
// already rides the synced `actions` shape, but the ≤64KB markdown `body` is
// excluded from sync on purpose, so the tab fetches the row through tRPC
// `actions.get` when the page opens (the web form and the desktop editor do
// exactly this) and writes back through `actions.update`, which is
// OWNER-gated server-side — mirrored here so a member gets read-only fields
// instead of a refusal on submit.

data class ActionEditState(
    /** The `actions.get` round-trip is in flight (the prompt field parks). */
    val loading: Boolean = true,
    /** The fetched row, body included; null until it lands. */
    val action: PromptDto? = null,
    val saving: Boolean = false,
    /** FEED-79: a failed save (or a failed Retry tap), raised once as an error toast. */
    val failure: FailedAction? = null,
    /** The fetch failed: the prompt shows its empty state + Retry (no toast on open). */
    val loadFailed: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ActionEditViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val prompts: PromptsApi,
    selection: TeamSelection,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _state = MutableStateFlow(ActionEditState())
    val state: StateFlow<ActionEditState> = _state

    /**
     * Whether the caller OWNS the action's team (EXP-1186: the ACTION's, not
     * the selection — the Actions list is cross-team) — `actions.update` is
     * owner-gated, so a member sees the same fields disabled and no Save.
     */
    val isTeamOwner: StateFlow<Boolean> = combine(
        dbFlow,
        routeTeamIdFlow(dbFlow, savedStateHandle, selection.selectedId),
    ) { db, teamId ->
        db to teamId
    }.flatMapLatest { (db, teamId) ->
        if (db == null || teamId == null) {
            flowOf(emptyList())
        } else {
            db.teamMemberDao().observeByTeam(teamId)
        }
    }.combine(auth.userId) { members, userId ->
        userId != null &&
            members.firstOrNull { it.userId == userId }?.role == DomainContract.teamRoleOwner
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /**
     * Fetch [actionId]'s full row. Deliberately UNCONDITIONAL: the Prompt tab
     * calls this once per page visit (a `LaunchedEffect(actionId)`), so a
     * body edited elsewhere meanwhile is re-read and a failed first fetch
     * retries on the next open.
     */
    fun clearFailure() {
        _state.value = _state.value.copy(failure = null)
    }

    /** [retried] = the person tapped Retry: only then does a failure toast (FEED-79). */
    fun load(actionId: String, retried: Boolean = false) {
        _state.value = ActionEditState(loading = true)
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _state.value = ActionEditState(loading = false)
                return@launch
            }
            try {
                _state.value = ActionEditState(loading = false, action = prompts.get(accountId, actionId))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _state.value = ActionEditState(
                    loading = false,
                    loadFailed = true,
                    failure = if (!retried) null else FailedAction("load", "the prompt", trpcErrorMessage(t, "")) {
                        load(actionId, retried = true)
                    },
                )
            }
        }
    }

    /** Save the edited row (every field, inputs included); a refusal is
     *  the state's [ActionEditState.failure] (a toast with Retry). */
    fun save(actionId: String, fields: PromptFields) {
        if (_state.value.saving) return
        _state.value = _state.value.copy(saving = true, failure = null)
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _state.value = _state.value.copy(saving = false)
                return@launch
            }
            try {
                val saved = prompts.update(accountId, actionId, fields)
                _state.value = _state.value.copy(saving = false, action = saved)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _state.value = _state.value.copy(
                    saving = false,
                    failure = FailedAction("save", "the action", trpcErrorMessage(t, "")) {
                        save(actionId, fields)
                    },
                )
            }
        }
    }

    /** Owner-only, permanent; [onDeleted] leaves the page. */
    fun delete(actionId: String, onDeleted: () -> Unit) {
        if (_state.value.saving) return
        _state.value = _state.value.copy(saving = true, failure = null)
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            try {
                if (accountId != null) prompts.delete(accountId, actionId)
                _state.value = _state.value.copy(saving = false)
                onDeleted()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _state.value = _state.value.copy(
                    saving = false,
                    failure = FailedAction("delete", "the action", trpcErrorMessage(t, "")),
                )
            }
        }
    }
}
