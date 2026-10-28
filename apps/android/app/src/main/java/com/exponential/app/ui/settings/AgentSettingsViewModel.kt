package com.exponential.app.ui.settings

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.AgentCreateInput
import com.exponential.app.data.api.AgentsApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.UpdateAgentPatch
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.AgentTrigger
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.domain.parseAgentTriggers
import com.exponential.app.domain.unreadableAgentTriggers
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.steer.steerDeviceFlow
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
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

/**
 * EXP-1291 Threads v2 / NEW-2: Team settings → Agents (lane AGENTS owns the
 * UI; `agent-settings.json`). ONE model for the list (the selected team's
 * agent DEFINITIONS) and an agent page (`agentId` from the route; null =
 * `New agent`). Agents are managed like boards: owner writes, members read.
 *
 * Writes: `agents.create` ([create]), `agents.update` ([update]),
 * `agents.delete` ([delete]). The Triggers are a WHOLE-ARRAY replace through
 * `agents.update({triggers})` built from the row's readable triggers — until
 * the Electric echo lands, the last write is the base ([triggers]).
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AgentSettingsViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val agentsApi: AgentsApi,
) : ViewModel() {

    /** S-3: the viewer (an Assistant is edited by its owner only). */
    val me: StateFlow<String?> get() = auth.userId

    /** S-3: the Settings Agents group makes sure the viewer has an Assistant. */
    fun ensureAssistant() {
        val team = teamId.value
        viewModelScope.launch {
            com.exponential.app.data.api.AssistantEnsurer.ensure(agentsApi, auth.activeAccountId.value, team)
        }
    }

    /** The agent definition id (`settings/agent/{agentId}`); null = list / new. */
    val agentId: String? = savedStateHandle.get<String>("agentId")?.takeIf { it.isNotBlank() }

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The team the list shows and a new agent lands in (the selection). */
    val teamId: StateFlow<String?> = selection.selectedId

    private val sessionRows = dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeAgents() }
        .map { rows -> rows.map(SessionTree::session) }

    /** The selected team's agents, rail order ([SessionTree.listAgents]). */
    val agents: StateFlow<List<SessionTree.AgentRow>> = combine(
        dbFlow.scopedQuery(emptyList<AgentDefinitionEntity>()) { it.agentDefinitionDao().observeAll() },
        sessionRows,
        teamId,
        auth.userId,
    ) { defs, sessions, team, me ->
        if (team == null) emptyList() else SessionTree.listAgents(defs.map(SessionTree::agentDefinition), sessions, team, me)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * NEW-2: the selected team's Events band (web `AgentEventsBand`): newest
     * first, capped at [AgentSettings.EVENTS_CAP], each with who emitted it
     * ([AgentSettings.eventEmitterLabel]). null = still loading.
     */
    val events: StateFlow<List<AgentEventRow>?> = combine(teamId, dbFlow) { team, db -> team to db }
        .flatMapLatest { (team, db) ->
            if (team == null || db == null) {
                flowOf(null)
            } else {
                combine(
                    db.eventDao().observeByTeam(team),
                    db.sessionDao().observeAll(),
                    db.userDao().observeAll(),
                ) { events, sessions, users ->
                    val titles = sessions.associate { it.id to it.title }
                    val names = users.associate { it.id to it }
                    events.take(AgentSettings.EVENTS_CAP).map { event ->
                        AgentEventRow(
                            id = event.id,
                            name = event.name,
                            emitter = AgentSettings.eventEmitterLabel(
                                event.emittedBy,
                                event.sessionId,
                                event.userId,
                                sessionTitle = { id -> titles[id] },
                                userName = { id -> com.exponential.app.ui.components.userDisplayName(names[id], id) },
                            ),
                            createdAt = event.createdAt,
                        )
                    }
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The page's synced agent definition (null for `New agent` / while loading). */
    val agent: StateFlow<AgentDefinitionEntity?> = (
        agentId?.let { id ->
            dbFlow.scopedQuery<AgentDefinitionEntity?>(null) { it.agentDefinitionDao().observeById(id) }
        } ?: flowOf(null)
        ).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** NEW-2: the caller's own conversation session with the agent (the
     *  page's chat button opens it); null = none yet. */
    val conversationId: StateFlow<String?> = combine(sessionRows, auth.userId) { sessions, me ->
        agentId?.let { SessionTree.conversations(sessions, me)[it] }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** Whether the caller owns the team (owner-only controls hide otherwise). */
    val isTeamOwner: StateFlow<Boolean> = combine(
        dbFlow,
        combine(agent, teamId) { row, selected -> row?.teamId ?: selected },
    ) { db, team -> db to team }
        .flatMapLatest { (db, team) ->
            if (db == null || team == null) flowOf(emptyList()) else db.teamMemberDao().observeByTeam(team)
        }
        .combine(auth.userId) { members, userId ->
            userId != null && members.firstOrNull { it.userId == userId }?.role == DomainContract.teamRoleOwner
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** The Device picker (`agent-settings.json`): the ONLINE devices of the
     *  team the caller may start on (the caller's machines + the team's shared
     *  servers), plus the agent's CURRENT device even while it is offline so
     *  the field still shows its value. null = loading. */
    val devices: StateFlow<List<SteerDevice>?> = combine(
        steerDeviceFlow(dbFlow, combine(agent, teamId) { row, selected -> row?.teamId ?: selected }, auth.userId),
        agent,
    ) { list, row ->
        list?.filter { it.online || it.deviceId == row?.deviceId }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _instructions = MutableStateFlow<String?>(null)
    /** The server-only instructions (`agents.get`); null = not loaded. */
    val instructions: StateFlow<String?> = _instructions

    // The last trigger write (its array + when), the base of the next one
    // until the synced row's `updated_at` passes it.
    private val lastWritten = MutableStateFlow<Pair<Long, List<AgentTrigger>>?>(null)

    /** The agent's readable triggers, stored order (the Triggers rows). */
    val triggers: StateFlow<List<AgentTrigger>> = combine(agent, lastWritten) { row, written ->
        triggerBase(row, written)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _busy = MutableStateFlow(false)
    /** ONE write at a time (each trigger write replaces the whole array). */
    val busy: StateFlow<Boolean> = _busy

    private val _failure = MutableStateFlow<FailedAction?>(null)
    /** FEED-79: a refused write, raised once as an error toast. */
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    init {
        val id = agentId
        if (id != null) {
            viewModelScope.launch {
                val accountId = auth.activeAccountId.value ?: return@launch
                runCatching { agentsApi.get(accountId, id) }
                    .onSuccess { _instructions.value = it.instructions }
                    .onFailure { if (it is CancellationException) throw it }
            }
        } else {
            _instructions.value = ""
        }
    }

    /** Field errors for the form ([AgentSettings.validateAgentForm]). */
    fun validate(name: String, deviceId: String?, instructions: String): List<AgentSettings.FormError> =
        AgentSettings.validateAgentForm(AgentSettings.Form(name, deviceId, instructions))

    /** `New agent` → `agents.create`; [onCreated] gets the new agent's id. */
    fun create(
        name: String,
        icon: String,
        color: String,
        deviceId: String,
        account: String?,
        instructions: String,
        triggers: List<AgentTrigger>,
        onCreated: (String) -> Unit,
        /** The picked login's agent (an account key's `<agent>:` half). */
        cli: String? = null,
    ) {
        val team = teamId.value ?: return
        write("create", "the agent") { accountId ->
            val created = agentsApi.create(
                accountId,
                AgentCreateInput(
                    teamId = team,
                    name = name.trim(),
                    icon = icon,
                    color = color,
                    instructions = instructions,
                    triggers = kotlinx.serialization.json.JsonArray(triggers.map { it.toWireJson() }),
                    deviceId = deviceId,
                    cli = cli,
                    account = account,
                ),
            )
            onCreated(created.id)
        }
    }

    /** Owner-only patch of the page's agent (name, icon, color, device,
     *  account, model, instructions, sort order). */
    fun update(patch: UpdateAgentPatch, onDone: () -> Unit = {}) {
        val id = agentId ?: return
        write("save", "the agent") { accountId ->
            agentsApi.update(accountId, id, patch)
            patch.instructions?.let { _instructions.value = it }
            onDone()
        }
    }

    /** Owner-only, permanent (confirm with [AgentSettings.Copy.DELETE_CONFIRM]). */
    fun delete(onDeleted: () -> Unit) {
        val id = agentId ?: return
        write("delete", "the agent") { accountId ->
            agentsApi.delete(accountId, id)
            onDeleted()
        }
    }

    /** Flip one trigger's paused flag; the rest rides unchanged. */
    fun setTriggerEnabled(triggerId: String, enabled: Boolean) = writeTriggers { existing ->
        existing.map { if (it.id == triggerId) it.copy(enabled = enabled) else it }
    }

    /** Save the trigger form ([editing]'s id null = APPEND a new one). */
    fun saveTrigger(trigger: AgentTrigger, onDone: () -> Unit = {}) = writeTriggers(onDone) { existing ->
        if (trigger.id == null) existing + trigger else existing.map { if (it.id == trigger.id) trigger else it }
    }

    fun deleteTrigger(triggerId: String) = writeTriggers { existing -> existing.filter { it.id != triggerId } }

    private fun writeTriggers(onDone: () -> Unit = {}, build: (List<AgentTrigger>) -> List<AgentTrigger>) {
        val id = agentId ?: return
        write("update", "the triggers") { accountId ->
            val next = build(triggers.value)
            val wire: List<JsonObject> = next.map { it.toWireJson() } +
                unreadableAgentTriggers(agent.value?.triggers)
            agentsApi.update(accountId, id, UpdateAgentPatch(triggers = wire))
            lastWritten.value = System.currentTimeMillis() to next
            onDone()
        }
    }

    private fun write(verb: String, thing: String, call: suspend (accountId: String) -> Unit) {
        if (_busy.value) return
        _busy.value = true
        _failure.value = null
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            try {
                if (accountId != null) call(accountId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction(verb, thing, trpcErrorMessage(t, ""))
            } finally {
                _busy.value = false
            }
        }
    }
}

/**
 * The triggers a write builds on: the synced row's, unless the last write is
 * strictly newer than the row's `updated_at` (its Electric echo has not
 * landed yet — a whole-array replace built from the pre-write row would
 * silently revert it).
 */
internal fun triggerBase(row: AgentDefinitionEntity?, written: Pair<Long, List<AgentTrigger>>?): List<AgentTrigger> {
    val synced = parseAgentTriggers(row?.triggers)
    if (written == null) return synced
    val syncedMs = row?.updatedAt?.let { WireTimestamps.parseEpochMs(it) } ?: return written.second
    return if (syncedMs >= written.first) synced else written.second
}

/** NEW-2: one row of the Events band. */
data class AgentEventRow(
    val id: String,
    val name: String,
    /** Who emitted it (the session's title, the person, or the server). */
    val emitter: String,
    val createdAt: String,
)
