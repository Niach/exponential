package com.exponential.app.ui.actions

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.PromptDto
import com.exponential.app.data.api.PromptFields
import com.exponential.app.data.api.PromptsApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.toPromptDto
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.stableDeviceOrder
import com.exponential.app.domain.toSteerDevice
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.FailedAction
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch

// NEW-2: the list reads the synced `prompts` (the `actions` router is gone).
// The Actions list (EXP-253): EVERY member team's action prompts (EXP-1186,
// cross-team like the Inbox; the screen bands them per team) LIVE from the
// synced actions shape (EXP-268 — the local Room flow, body-less by design;
// no client builtin is listed, EXP-431 / EXP-686). EXP-825: starts left for
// the Agent page composer. SLOP-2: an action carries its triggers on its own
// row, so this model reads nothing else — the triggers' rows, their writes
// and the action's runs live on the action page ([ActionDetailViewModel]).

data class ActionsState(
    val actions: List<PromptDto> = emptyList(),
    val loading: Boolean = true,
    val error: String? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ActionsViewModel @Inject constructor(
    holder: DatabaseHolder,
    private val selection: TeamSelection,
    private val auth: AuthRepository,
    private val prompts: PromptsApi,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    // The selected team, for the screen's "New action" entry point (EXP-431).
    val selectedTeamId: StateFlow<String?> = selection.selectedId

    /** EXP-1186: a team band's "New action" creates in THAT team. */
    fun selectTeam(teamId: String) = selection.select(teamId)

    /** EXP-1186: the member teams (name order) the list bands by. */
    val teams: StateFlow<List<TeamEntity>> =
        dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // Live from the synced actions shape (EXP-268): the DAO orders by
    // sort_order then name. NEITHER client builtin is listed (EXP-431,
    // EXP-686): "Create action" lives behind the header's "New action" button,
    // and "Fix merge conflicts" is launched from Reviews / the start-coding
    // sheet (which builds its own list) — neither poses as a team action here.
    val state: StateFlow<ActionsState> =
        dbFlow.flatMapLatest { db ->
            if (db == null) {
                flowOf(ActionsState(loading = false))
            } else {
                db.promptDao().observeAll().map { rows ->
                    ActionsState(
                        actions = rows.map { it.toPromptDto() },
                        loading = false,
                    )
                }
            }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ActionsState())

    /** EXP-1296: whether the caller OWNS the selected team — `New action`
     *  creates there and `prompts.create` is owner-only. */
    val canCreate: StateFlow<Boolean> = combine(dbFlow, selection.selectedId) { db, teamId -> db to teamId }
        .flatMapLatest { (db, teamId) ->
            if (db == null || teamId == null) flowOf(emptyList()) else db.teamMemberDao().observeByTeam(teamId)
        }
        .combine(auth.userId) { members, userId ->
            userId != null && members.firstOrNull { it.userId == userId }?.role == DomainContract.teamRoleOwner
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    private val _creating = MutableStateFlow(false)
    val creating: StateFlow<Boolean> = _creating

    private val _failure = MutableStateFlow<FailedAction?>(null)
    /** FEED-79: a refused create, raised once as an error toast. */
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    /** `New action` → `prompts.create` in the selected team; [onCreated]
     *  gets the new prompt's id. */
    fun create(fields: PromptFields, onCreated: (String) -> Unit) {
        val teamId = selection.selectedId.value ?: return
        if (_creating.value) return
        _creating.value = true
        _failure.value = null
        viewModelScope.launch {
            try {
                val accountId = auth.activeAccountId.value ?: return@launch
                onCreated(prompts.create(accountId, teamId, fields).id)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("create", "the action", trpcErrorMessage(t, "")) {
                    create(fields, onCreated)
                }
            } finally {
                _creating.value = false
            }
        }
    }
}
