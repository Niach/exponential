package com.exponential.app.ui.actions

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.toActionDto
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
import kotlinx.serialization.json.Json

// The Actions list (EXP-253): EVERY member team's action prompts (EXP-1186,
// cross-team like the Inbox; the screen bands them per team) LIVE from the
// synced actions shape (EXP-268 — the local Room flow, body-less by design;
// no client builtin is listed, EXP-431 / EXP-686). EXP-825: starts left for
// the Agent page composer. SLOP-2: an action carries its triggers on its own
// row, so this model reads nothing else — the triggers' rows, their writes
// and the action's runs live on the action page ([ActionDetailViewModel]).

data class ActionsState(
    val actions: List<ActionDto> = emptyList(),
    val loading: Boolean = true,
    val error: String? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ActionsViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val json: Json,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    // The selected team, for the screen's "New action" entry point (EXP-431).
    val selectedTeamId: StateFlow<String?> = selection.selectedId

    /** EXP-1186: the member teams (name order) the list bands by. */
    val teams: StateFlow<List<TeamEntity>> =
        dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * The machines a suggestion's trigger can be bound to: every synced device
     * that advertises the `automations` cap, ONLINE OR NOT (a trigger outlives
     * a machine's uptime — it fires whenever that machine is next up).
     */
    val triggerDevices: StateFlow<List<SteerDevice>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
        DeviceLiveness.ticker(),
        auth.userId,
    ) { rows, nowMs, userId ->
        rows.sortedWith(stableDeviceOrder(nowMs))
            .map { it.toSteerDevice(nowMs, userId) }
            .filter { it.canRunAutomations }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

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
                db.actionDao().observeAll().map { rows ->
                    ActionsState(
                        actions = rows.map { it.toActionDto(json) },
                        loading = false,
                    )
                }
            }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ActionsState())
}
