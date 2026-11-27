package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.MemberSpacesApi
import com.exponential.app.data.api.MySpace
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DevicesList
import com.exponential.app.domain.MemberSpace
import com.exponential.app.domain.toSteerDevice
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.PickerValueRow
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.components.picker.DevicePicker
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.theme.TextEmphasis
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.launch

// #EXP-2161 (member-space.json, UI ×4; web `assistant-device-row.tsx`): the
// Assistant page's Runs on Device row: the member's own machines
// (`runsOnRows`), then one section per team with opted-in hosts of others
// (`spaceSections`), headed by the team's name. Picking one of those confirms
// (`copy.pick*`) and is `memberSpaces.request`; a bound space shows as its
// host's row. An Assistant without a device (its space ended) reads
// `copy.noDevice`; a space not let in yet reads `copy.waiting` under it.

/** What the Device row reads off the synced rows (a space resolves here,
 *  never in a list). */
data class AssistantSpaceRows(
    val picks: List<MemberSpace.Pick> = emptyList(),
    /** The picked row's value (a team-qualified host id). */
    val value: String? = null,
    /** The bound space's own row; null = not on a space. */
    val space: DeviceEntity? = null,
    /** The device the Account and Model rows read ([spaceDevice]). */
    val device: SteerDevice? = null,
    /** User id → display name (the pick confirm's `{owner}`). */
    val names: Map<String, String> = emptyMap(),
)

/** The device the Account and Model rows read: the bound space's own row
 *  (its logins are the host's), the host's while it reports none yet. */
fun spaceDevice(space: DeviceEntity, host: DeviceEntity?, me: String?, nowMs: Long): SteerDevice {
    val own = space.toSteerDevice(nowMs, me)
    if (host == null || !own.agentAccounts.isNullOrEmpty()) return own
    val from = host.toSteerDevice(nowMs, me)
    return own.copy(
        agents = from.agents,
        unauthedAgents = from.unauthedAgents,
        acpAgents = from.acpAgents,
        agentAccounts = from.agentAccounts,
        agentUsage = from.agentUsage,
        agentUsageAt = from.agentUsageAt,
    )
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AssistantSpacesViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val api: MemberSpacesApi,
) : ViewModel() {
    private val dbFlow = accountDatabaseFlow(auth, holder)
    private val bound = MutableStateFlow<String?>(null)

    /** The Assistant's device id (null / empty = none). */
    fun bind(deviceId: String?) {
        bound.value = deviceId?.takeIf { it.isNotEmpty() }
    }

    val rows: StateFlow<AssistantSpaceRows> = combine(
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
        dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() },
        dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeAll() },
        combine(bound, auth.userId) { id, me -> id to me },
        DeviceLiveness.ticker(),
    ) { devices, teams, users, (boundId, me), now ->
        val viewer = me.orEmpty()
        // The sidebar's team order (by name; the personal team never syncs).
        val ordered = teams.sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.name })
            .map { MemberSpace.Team(it.id, it.name) }
        val picks = MemberSpace.picks(devices, ordered, boundId, viewer, now)
        val space = devices.firstOrNull { it.deviceId == boundId && MemberSpace.isSpace(it) }
        val host = space?.let { s -> devices.firstOrNull { it.id == s.spaceHostId } }
        AssistantSpaceRows(
            picks = picks.picks,
            value = picks.value,
            space = space,
            device = space?.let { spaceDevice(it, host, me, now) },
            names = users.associate { it.id to userDisplayName(it, it.id) },
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AssistantSpaceRows())

    /** Whether the bound space's install waits for this member's approval
     *  (`memberSpaces.mine`, read again until it is let in). */
    val waiting: StateFlow<Boolean> = rows.map { it.space?.id }.distinctUntilChanged()
        .transformLatest { spaceId ->
            emit(false)
            if (spaceId == null) return@transformLatest
            while (true) {
                val status = runCatching {
                    val accountId = auth.activeAccountId.value ?: return@runCatching null
                    api.mine(accountId).space?.takeIf { it.spaceId == spaceId }?.status
                }.onFailure { if (it is CancellationException) throw it }.getOrNull()
                emit(status == MySpace.WAITING)
                if (status == MySpace.READY) return@transformLatest
                delay(WAITING_INTERVAL_MS)
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure
    fun clearFailure() {
        _failure.value = null
    }

    /** `memberSpaces.request`; [onDone] gets the space's device id. */
    fun request(pick: MemberSpace.Pick, onDone: (String) -> Unit) {
        if (_busy.value) return
        val accountId = auth.activeAccountId.value ?: return
        _busy.value = true
        viewModelScope.launch {
            try {
                val result = api.request(accountId, pick.hostDeviceId, pick.teamId)
                bound.value = result.deviceId
                onDone(result.deviceId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("save", "the device", trpcErrorMessage(t, ""))
            } finally {
                _busy.value = false
            }
        }
    }

    private companion object {
        const val WAITING_INTERVAL_MS = 15_000L
    }
}

/**
 * The Assistant's Device row: [own] = its member's own machines (the
 * `runsOnRows` rows), then the team sections. [deviceId] = the Assistant's
 * device (null = none). [onPickOwn] is an own machine's pick (the existing
 * `agents.update`); [onPickedSpace] runs once `memberSpaces.request` went
 * through (the space's device id).
 */
@Composable
fun AssistantDeviceRow(
    deviceId: String?,
    own: List<DevicePickerDevice>,
    onPickOwn: (String) -> Unit,
    onPickedSpace: (String) -> Unit,
    enabled: Boolean = true,
    viewModel: AssistantSpacesViewModel = hiltViewModel(),
) {
    LaunchedEffect(deviceId) { viewModel.bind(deviceId) }
    val rows by viewModel.rows.collectAsStateWithLifecycle()
    val waiting by viewModel.waiting.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    var confirm by remember { mutableStateOf<MemberSpace.Pick?>(null) }

    val space = rows.space
    // A space whose host is gone from the list names itself.
    val orphan = if (space != null && rows.value == null) {
        listOf(DevicePickerDevice(id = space.deviceId, name = space.label.ifEmpty { space.deviceId }, icon = space.icon, isServer = true))
    } else {
        emptyList()
    }
    val ownRows = if (space != null) own.filterNot { it.id == deviceId } else own
    val teamRows = rows.picks.map { pick ->
        DevicePickerDevice(
            id = pick.value,
            name = pick.name,
            icon = pick.icon,
            isServer = true,
            description = if (pick.online) null else DevicesList.Copy.OFFLINE,
            disabled = pick.disabled,
            section = pick.section,
            checked = pick.selected,
        )
    }
    val devices = orphan + ownRows + teamRows
    val value = if (space != null) rows.value ?: space.deviceId else deviceId
    val shown = devices.firstOrNull { it.id == value }

    DevicePicker(
        devices = devices,
        value = value,
        onChange = { next ->
            val target = rows.picks.firstOrNull { it.value == next }
            when {
                target == null -> onPickOwn(next)
                !target.selected -> confirm = target
            }
        },
        title = DEVICE_LABEL,
        trigger = { open ->
            PickerValueRow(
                label = DEVICE_LABEL,
                // EXP-1554: the value is the machine alone (×4).
                value = shown?.name?.substringBefore(" — ") ?: MemberSpace.Copy.NO_DEVICE,
                onClick = open,
                enabled = enabled,
                modifier = Modifier.testTag("agent-settings-device"),
            )
        },
    )
    if (waiting) {
        Text(
            MemberSpace.Copy.WAITING,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp).testTag("assistant-space-waiting"),
        )
    }
    confirm?.let { pick ->
        PromptAlert(
            prompt = MemberSpace.pickPrompt(pick.name, rows.names[pick.ownerId].orEmpty()),
            onDismiss = { confirm = null },
            handlers = mapOf(
                MemberSpace.ACTION_PICK to {
                    viewModel.request(pick) { id ->
                        confirm = null
                        onPickedSpace(id)
                    }
                },
            ),
            loading = MemberSpace.ACTION_PICK.takeIf { busy },
            testTags = mapOf(MemberSpace.ACTION_PICK to "assistant-space-confirm"),
        )
    }
}

private val DEVICE_LABEL = com.exponential.app.domain.AgentSettings.FIELDS
    .first { it.key == com.exponential.app.domain.AgentSettings.FIELD_DEVICE }.label
