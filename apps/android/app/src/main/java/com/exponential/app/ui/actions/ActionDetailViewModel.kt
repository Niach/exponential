package com.exponential.app.ui.actions

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.ActionsApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.toActionDto
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.ActionTrigger
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.domain.actionTriggerWireJson
import com.exponential.app.domain.stableDeviceOrder
import com.exponential.app.domain.toSteerDevice
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

// The action page's data (SLOP-2): ONE action — its synced row (name, icon,
// inputs and the triggers it carries), the machines its triggers name, and
// every run it produced. The prompt body is the editor's own fetch
// ([ActionEditViewModel]); this model owns the Triggers and Runs tabs.
//
// Triggers have ONE write: `actions.update({id, triggers})`, a WHOLE-ARRAY
// replace built from the row's readable triggers. Electric echoes the row
// back, so a success needs no local write — but until that echo lands the
// mutation's own response is the newer truth ([triggerWriteBase]). Nothing
// here RUNS a trigger — the bound machine does, on its own clock.

data class ActionDetailState(
    /** The synced row; null while loading, or once the action is gone. */
    val action: ActionDto? = null,
    val loading: Boolean = true,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ActionDetailViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val actionsApi: ActionsApi,
    private val json: Json,
) : ViewModel() {

    val actionId: String = savedStateHandle["actionId"] ?: ""

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    val state: StateFlow<ActionDetailState> = dbFlow.flatMapLatest { db ->
        if (db == null) {
            flowOf(ActionDetailState(loading = false))
        } else {
            db.actionDao().observeById(actionId).map { row ->
                ActionDetailState(action = row?.toActionDto(json), loading = false)
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ActionDetailState())

    // The last trigger write's response. The busy flag clears when tRPC
    // returns, NOT when Electric echoes the row, so the synced row can still
    // be the pre-write one: this stays the base of the next whole-array write
    // (and of the rows shown) until the synced `updatedAt` catches up.
    private val lastWritten = MutableStateFlow<ActionDto?>(null)

    /** The action's readable triggers, in stored order (the Triggers rows). */
    val triggers: StateFlow<List<ActionTrigger>> = combine(state, lastWritten) { current, written ->
        current.action?.let { triggerWriteBase(it, written).parsedTriggers }.orEmpty()
    }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * EVERY synced device as a [SteerDevice], offline included — a trigger row
     * resolves its bound deviceId to a label + online dot, and an offline
     * machine must still be nameable. Online-ness derives from `last_seen_at`
     * freshness, recomputed on the 30s ticker (Room flows only re-emit on
     * writes).
     */
    val syncedDevices: StateFlow<List<SteerDevice>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
        DeviceLiveness.ticker(),
        auth.userId,
    ) { rows, nowMs, userId ->
        rows.sortedWith(stableDeviceOrder(nowMs)).map { it.toSteerDevice(nowMs, userId) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The machines a trigger can be bound to: every synced device that
     * advertises the `automations` cap, ONLINE OR NOT (a trigger outlives a
     * machine's uptime — it fires whenever that machine is next up). */
    val triggerDevices: StateFlow<List<SteerDevice>> = syncedDevices
        .map { devices -> devices.filter { it.canRunAutomations } }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Every run of this action, newest first — person-started and triggered
     * alike (the Runs tab). */
    val runs: StateFlow<List<CodingSessionEntity>> =
        dbFlow.scopedQuery(emptyList()) { it.codingSessionDao().observeByAction(actionId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * Whether the current user OWNS the action's team — the enabled switch,
     * "Add trigger", Edit and Delete are hidden or disabled otherwise (every
     * `actions.update` is owner-gated server-side; mirror it instead of
     * bouncing on submit).
     */
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

    // True while ANY trigger write is in flight — every switch and the form's
    // submit park together (one write at a time: each replaces the WHOLE
    // array, so two racing ones would drop each other's change).
    private val _triggerBusy = MutableStateFlow(false)
    val triggerBusy: StateFlow<Boolean> = _triggerBusy

    private val _triggerError = MutableStateFlow<String?>(null)
    val triggerError: StateFlow<String?> = _triggerError

    fun clearTriggerError() {
        _triggerError.value = null
    }

    /** Flip one trigger's paused flag; the rest of the array rides unchanged. */
    fun setTriggerEnabled(triggerId: String, enabled: Boolean) {
        writeTriggers("The trigger could not be updated") { existing ->
            existing.map { trigger ->
                (if (trigger.id == triggerId) trigger.copy(enabled = enabled) else trigger)
                    .toWireJson()
            }
        }
    }

    /**
     * Save the trigger form: [editing] null APPENDS a new trigger (no id — the
     * server mints it, enabled), else that element is replaced in place,
     * keeping its id and its enabled flag. [onDone] fires only on success, so
     * the sheet stays open (with the server's message) on a refusal.
     */
    fun saveTrigger(editing: ActionTrigger?, result: TriggerFormResult, onDone: () -> Unit) {
        val written = actionTriggerWireJson(
            id = editing?.id,
            enabled = editing?.enabled ?: true,
            deviceId = result.deviceId,
            agent = result.agent,
            account = result.account,
            model = result.model,
            effort = result.effort,
            whenPart = result.whenPart,
        )
        val fallback = if (editing == null) {
            "The trigger could not be added"
        } else {
            "The trigger could not be updated"
        }
        writeTriggers(fallback, onDone) { existing ->
            if (editing == null) {
                existing.map { it.toWireJson() } + written
            } else {
                existing.map { if (it.id == editing.id) written else it.toWireJson() }
            }
        }
    }

    /** Owner-only, permanent — the row vanishes when Electric echoes it. */
    fun deleteTrigger(triggerId: String) {
        writeTriggers("The trigger could not be deleted") { existing ->
            existing.filter { it.id != triggerId }.map { it.toWireJson() }
        }
    }

    // One write at a time, with the server's own refusal message surfaced
    // (its copy is the actionable one — required inputs, an incapable device,
    // a rejected model). [build] turns the CURRENT readable triggers (the
    // synced row's, or the last write's while its echo is pending) into the
    // array that replaces them.
    private fun writeTriggers(
        fallback: String,
        onDone: () -> Unit = {},
        build: (List<ActionTrigger>) -> List<JsonObject>,
    ) {
        if (_triggerBusy.value) return
        val action = state.value.action ?: return
        _triggerBusy.value = true
        _triggerError.value = null
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _triggerBusy.value = false
                return@launch
            }
            try {
                val base = triggerWriteBase(action, lastWritten.value)
                lastWritten.value =
                    actionsApi.setTriggers(accountId, id = action.id, triggers = build(base.parsedTriggers))
                onDone()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _triggerError.value = trpcErrorMessage(t, fallback)
            }
            _triggerBusy.value = false
        }
    }
}

/**
 * The row a trigger write builds on: the [synced] one, unless [written] (the
 * last write's response) is the same action and strictly newer — the Electric
 * echo has not landed yet, so the synced array is the PRE-write one and a
 * whole-array replace built from it would silently revert that write. An
 * unreadable stamp on the response falls back to the synced row; one on the
 * synced row keeps the response.
 */
internal fun triggerWriteBase(synced: ActionDto, written: ActionDto?): ActionDto {
    if (written == null || written.id != synced.id) return synced
    val writtenMs = WireTimestamps.parseEpochMs(written.updatedAt) ?: return synced
    val syncedMs = WireTimestamps.parseEpochMs(synced.updatedAt) ?: return written
    return if (syncedMs >= writtenMs) synced else written
}
