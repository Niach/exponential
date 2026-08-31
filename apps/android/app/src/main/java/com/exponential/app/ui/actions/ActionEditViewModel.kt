package com.exponential.app.ui.actions

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.ActionsApi
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

// The action editor's data layer (EXP-694 — editing stopped being
// web/desktop-only). The list metadata already rides the synced `actions`
// shape, but the ≤64KB markdown `body` is excluded from sync on purpose, so
// the sheet fetches the row through tRPC `actions.get` when it opens (the web
// dialog and the desktop editor do exactly this) and writes back through
// `actions.update`, which is OWNER-gated server-side — mirrored here so a
// member gets a read-only sheet instead of a refusal on submit.

data class ActionEditState(
    /** The `actions.get` round-trip is in flight (the prompt field parks). */
    val loading: Boolean = true,
    /** The fetched row, body included; null until it lands. */
    val action: ActionDto? = null,
    val saving: Boolean = false,
    val error: String? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ActionEditViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val actionsApi: ActionsApi,
    selection: TeamSelection,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _state = MutableStateFlow(ActionEditState())
    val state: StateFlow<ActionEditState> = _state

    /**
     * Whether the caller OWNS the selected team — `actions.update` is
     * owner-gated (ActionsViewModel.isTeamOwner's rule), so a member sees the
     * same sheet with every field disabled and no Save.
     */
    val isTeamOwner: StateFlow<Boolean> = combine(dbFlow, selection.selectedId) { db, teamId ->
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
     * Fetch [actionId]'s full row. Deliberately UNCONDITIONAL: this model is
     * nav-entry-scoped and outlives the sheet, so caching the last id would
     * re-show a stale body when the row was edited elsewhere meanwhile, and
     * would leave a failed first fetch with no retry. The sheet calls this
     * once per presentation (a `LaunchedEffect(actionId)` in a composition
     * that only exists while the sheet is up — iOS EditActionSheet's
     * per-presentation `.task { await load() }`), so there is no fetch loop.
     */
    fun load(actionId: String) {
        _state.value = ActionEditState(loading = true)
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _state.value = ActionEditState(loading = false)
                return@launch
            }
            try {
                _state.value = ActionEditState(loading = false, action = actionsApi.get(accountId, actionId))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _state.value = ActionEditState(
                    loading = false,
                    error = trpcErrorMessage(t, "The action could not be loaded"),
                )
            }
        }
    }

    /**
     * Drop the fetched row when the sheet goes away, so the NEXT presentation
     * starts empty instead of rendering the previous fetch for a frame (and
     * seeding its form from it) before [load]'s answer lands.
     */
    fun reset() {
        _state.value = ActionEditState()
    }

    /**
     * Save the edited row. Blank [description]/[icon]/[repositoryId] clear
     * those fields (explicit nulls on the wire); [onDone] fires only on
     * success, so a refusal leaves the sheet open with the server's message.
     */
    fun save(
        actionId: String,
        name: String,
        description: String,
        icon: String,
        repositoryId: String,
        body: String,
        onDone: () -> Unit,
    ) {
        if (_state.value.saving) return
        _state.value = _state.value.copy(saving = true, error = null)
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _state.value = _state.value.copy(saving = false)
                return@launch
            }
            try {
                val saved = actionsApi.update(
                    accountId,
                    id = actionId,
                    name = name.trim(),
                    description = description.trim().takeIf { it.isNotEmpty() },
                    icon = icon.takeIf { it.isNotEmpty() },
                    repositoryId = repositoryId.takeIf { it.isNotEmpty() },
                    body = body,
                )
                _state.value = _state.value.copy(saving = false, action = saved)
                onDone()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _state.value = _state.value.copy(
                    saving = false,
                    error = trpcErrorMessage(t, "The action could not be saved"),
                )
            }
        }
    }
}
