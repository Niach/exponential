package com.exponential.app.ui.session

import android.os.SystemClock
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.DeviceLatestVersions
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.agentProfileRemoveCommand
import com.exponential.app.data.api.agentProfileSignOutCommand
import com.exponential.app.data.api.agentUsageRefreshCommand
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.domain.AgentAccountsRows
import com.exponential.app.domain.AgentProfileUsageRow
import com.exponential.app.domain.DeviceFreshness
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionDeviceMove
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.domain.stableDeviceOrder
import com.exponential.app.domain.toSteerDevice
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.steer.steerDeviceFlow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The Devices tab's model: the caller's machines (EXP-403 registry) plus the
// signed-in user's OWN coding sessions — running (synced coding_sessions shape
// joined to its issue) and finished (EXP-746). EXP-825: the sessions render on
// the Agent page (which reuses this model) and every start goes through the
// composer there, so the remote-start launcher this model used to carry is
// gone; the desktop remains the only session runner.
//
// The machine list is the SYNCED devices shape (EXP-481 — the EXP-403
// registry became server-authoritative synced state): own rows plus (EXP-432)
// teammates' server machines shared with the selected team, with online-ness
// derived client-side from last_seen_at freshness on a 30s ticker. The 15s
// devices.list polling died with the shape, and EXP-485 retired the procedure
// itself — only the informational `devices.latestVersions` query is still
// fetched (one per account).

data class AgentsState(
    // steer.config is env-derived and static per instance: null = still
    // loading. Decides whether a row tap opens the live viewer directly or
    // falls back to the issue detail, and whether the devices section shows.
    val steerEnabled: Boolean? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AgentsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val steerApi: SteerApi,
    private val devicesApi: DevicesApi,
    stats: SyncStats,
    private val selection: TeamSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** EXP-656: when our own `devices` shape last completed a poll — a cursor
     *  we haven't refreshed can only produce a FALSE offline, so a session on
     *  one must render unknown presence rather than "Paused". */
    private val devicesPolledAt = auth.activeAccountId.flatMapLatest { stats.devicesPolledAt(it) }

    // The machine rows every session row joins to (EXP-549/550: current labels
    // and last_seen_at freshness), paired with that freshness stamp.
    private val deviceRowsAndFreshness = combine(
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
        devicesPolledAt,
    ) { devices, polledAt -> devices to polledAt }

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    // The caller's registered machines, online AND offline, from the synced
    // devices shape — plus the selected team's shared servers. null until the
    // shape's initial snapshot has landed (offset is_live), so the section
    // shows nothing rather than a flash of "No machines yet".
    // EXP-1186: CROSS-TEAM like the Inbox — own machines plus every server
    // shared with ANY member team (the synced rows are exactly those).
    val devices: StateFlow<List<SteerDevice>?> =
        steerDeviceFlow(dbFlow, flowOf(null), auth.userId, allTeams = true)
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * FEED-149: the team a team owner's Remove drops a teammate's share from —
     * the SELECTED team, null when the caller does not OWN it. The list is
     * cross-team (EXP-1186), so a row only carries Remove when its
     * `sharedTeamIds` holds this team (web `my-machines.tsx`).
     */
    val ownedTeamId: StateFlow<String?> =
        combine(dbFlow, selection.selectedId) { db, teamId -> db to teamId }
            .flatMapLatest { (db, teamId) ->
                if (db == null || teamId == null) {
                    flowOf(null)
                } else {
                    combine(db.teamMemberDao().observeByTeam(teamId), auth.userId) { members, userId ->
                        teamId.takeIf {
                            userId != null &&
                                members.any { it.userId == userId && it.role == DomainContract.teamRoleOwner }
                        }
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // Informational CLIENT_LATEST_VERSION_* values behind the "update
    // available" hint on a machine row; both null until the first list lands.
    private val _latestVersions = MutableStateFlow(DeviceLatestVersions())
    val latestVersions: StateFlow<DeviceLatestVersions> = _latestVersions

    // Machine ids with a rename/remove/update mutation in flight — the row's
    // menu stays put but its actions disable until the refetch lands.
    private val _deviceBusy = MutableStateFlow<Set<String>>(emptySet())
    val deviceBusy: StateFlow<Set<String>> = _deviceBusy

    // FEED-79: a failed device write (rename, icon, remove, update) or login
    // command (sign out, remove) is ONE error toast. The Devices page and the
    // settings sheet may both watch this instance, so a host claims it first.
    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    /** True for the ONE caller that takes [failure] (it then toasts it). */
    fun claimFailure(failure: FailedAction): Boolean = _failure.compareAndSet(failure, null)

    // ── EXP-829/EXP-909: the logins under each machine row ─────────────────
    // EXP-909 folded the cross-device "Accounts" section away: a login belongs
    // to the machine that holds it, so the Devices page lists each device's
    // own logins beneath its row. null until the composed device list has
    // landed (the rows then render "Checking…", never a flash of "none").
    val deviceLogins: StateFlow<Map<String, List<AgentProfileUsageRow>>?> = devices
        .map { rows ->
            rows?.associate { device ->
                device.deviceId to AgentAccountsRows.deviceLoginRows(device)
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // Refreshes in flight, keyed by LOGIN row: the usage stamp it carried when
    // the command was queued — the device's re-report MOVES it, and that is
    // what clears the mark (web `refreshing`, desktop `RefreshMark`).
    private data class RefreshMark(val fetchedAt: String?, val at: Long)

    private val _refreshingAccounts = MutableStateFlow<Map<String, RefreshMark>>(emptyMap())
    val refreshingAccounts: StateFlow<Set<String>> = _refreshingAccounts
        .map { it.keys }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptySet())

    // EXP-817: when the page's OWN refresh last tried each login.
    private val autoAttempts = mutableMapOf<String, Long>()

    /**
     * Queue `agent_usage_refresh` on [row]'s machine. The answer arrives as a
     * synced `agent_usage` write, never as a command result, so the only local
     * state is the in-flight mark.
     *
     * EXP-862 retired the by-hand refresh and EXP-909 the section that carried
     * its error line, so this is only ever the page's OWN round: a refusal (a
     * command still queued from the last round is a CONFLICT) is swallowed and
     * the next tick simply looks again.
     */
    private fun refreshAccount(row: AgentProfileUsageRow) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _refreshingAccounts.value = _refreshingAccounts.value +
                (row.key to RefreshMark(row.usage?.fetchedAt, System.currentTimeMillis()))
            runCatching {
                devicesApi.createCommand(
                    accountId,
                    agentUsageRefreshCommand(row.deviceId, row.agent, row.profileId),
                )
            }.onFailure { t ->
                if (t is CancellationException) throw t
                _refreshingAccounts.value = _refreshingAccounts.value - row.key
            }
        }
    }

    /**
     * EXP-817: keep the machines list current while it is open — the screen
     * calls this on every login emission and on a 30s tick (web `useNow`).
     * First the in-flight marks the synced rows have answered (the stamp
     * moved) or that waited past [REFRESH_PENDING_MS] are dropped; then every
     * ELIGIBLE login (mine + online) whose report is past the floor
     * gets ONE refresh queued — never while one is in flight, never twice
     * inside [AUTO_REFRESH_RETRY_MS]. The floor is the device's own 429
     * budget, so this can never out-poll what the machine allows itself.
     */
    fun autoRefreshAccounts() {
        val byDevice = deviceLogins.value ?: return
        val rows = byDevice.values.flatten()
        val nowMs = System.currentTimeMillis()
        _refreshingAccounts.value = _refreshingAccounts.value.filter { (key, mark) ->
            val row = rows.firstOrNull { it.key == key } ?: return@filter false
            row.usage?.fetchedAt == mark.fetchedAt && nowMs - mark.at <= REFRESH_PENDING_MS
        }
        for (row in rows) {
            if (!AgentAccountsRows.canRefresh(row)) continue
            if (row.key in _refreshingAccounts.value) continue
            if (AgentAccountsRows.refreshAllowedAt(row.usage, nowMs) != null) continue
            val last = autoAttempts[row.key] ?: Long.MIN_VALUE
            if (nowMs - last < AUTO_REFRESH_RETRY_MS) continue
            autoAttempts[row.key] = nowMs
            refreshAccount(row)
        }
    }

    // ── EXP-849: the MACHINE rows' account repair (Devices, not Accounts) ───
    // The commands a device row runs by itself: sign-out and removal of a
    // login the device ALREADY holds. Sign-ins are NOT here — they round-trip
    // a link and a code, which the device-settings sheet owns (one
    // implementation, not one per surface). Keyed by machine × login
    // ([deviceLoginCommandKey]), so two logins — or two machines holding the
    // same one — caption independently.
    private val _accountCommandStates = MutableStateFlow<Map<String, DeviceCommandUiState>>(emptyMap())
    val accountCommandStates: StateFlow<Map<String, DeviceCommandUiState>> = _accountCommandStates

    /**
     * EXP-862 "Remove account" — the machine deletes ITS copy of the login
     * (`agent_profile_remove`): the profile's config dir and its index row. The
     * account itself is untouched, which is what the confirm says. Owner + online
     * + both caps, all of which the server re-checks.
     */
    fun removeAccountHere(device: SteerDevice, agent: String, profileId: String) {
        if (!device.isMine || !device.online) return
        val key = accountCommandKey(device.deviceId, agent, profileId)
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runDeviceCommand(
                devicesApi,
                accountId,
                agentProfileRemoveCommand(device.deviceId, agent, profileId),
                device.online,
            ) { state ->
                if (state is DeviceCommandUiState.Failed) _failure.value = FailedAction("remove", "the login", state.message)
                _accountCommandStates.value = _accountCommandStates.value + (key to state)
            }
        }
    }

    /**
     * EXP-1137 "Sign out" — the machine signs ONE login out
     * (`agent_profile_sign_out`) and keeps its row: claude's own `auth logout`
     * in that profile's config dir, codex's credential file deleted (never
     * `codex logout`). The account itself is untouched, which is what the
     * confirm says. Owner + online + both caps, all of which the server
     * re-checks.
     */
    fun signOutAccountHere(device: SteerDevice, agent: String, profileId: String) {
        if (!device.isMine || !device.online) return
        val key = accountCommandKey(device.deviceId, agent, profileId)
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runDeviceCommand(
                devicesApi,
                accountId,
                agentProfileSignOutCommand(device.deviceId, agent, profileId),
                device.online,
            ) { state ->
                if (state is DeviceCommandUiState.Failed) _failure.value = FailedAction("sign out", "the login", state.message)
                _accountCommandStates.value = _accountCommandStates.value + (key to state)
            }
        }
    }

    val state: StateFlow<AgentsState> = _steerEnabled
        .map { AgentsState(steerEnabled = it) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AgentsState())

    /**
     * EXP-1186: the member teams, name order — the Agent page bands its live
     * and recent runs per team once there is more than one.
     */
    val teams: StateFlow<List<TeamEntity>> =
        dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // The account steer.config was last resolved for. steer.config is
    // env-derived and static per INSTANCE, so a team switch must not re-run it
    // — that would blank `steerEnabled` and flicker the whole tab (EXP-432).
    private var configuredAccountId: String? = null

    init {
        // Steer availability + the machine registry, re-fetched on account
        // switch (mirrors the issue detail's check) and, since EXP-432, on
        // team switch — the shared machines the list carries belong to the
        // SELECTED team.
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _latestVersions.value = DeviceLatestVersions()
                if (accountId == null) {
                    configuredAccountId = null
                    _steerEnabled.value = false
                    return@collectLatest
                }
                if (configuredAccountId != accountId) {
                    _steerEnabled.value = null
                    _steerEnabled.value = runCatching { steerApi.config(accountId).enabled }
                        .getOrDefault(false)
                    configuredAccountId = accountId
                }
                // The rows come from sync — the only thing left to fetch is the
                // informational version floor (one query per account; a failure
                // just hides the update hint).
                if (_steerEnabled.value == true) {
                    runCatching { devicesApi.latestVersions(accountId) }
                        .onSuccess { _latestVersions.value = it }
                }
            }
        }
    }

    /** Rename a machine (its registry label wins over the relay's). */
    fun renameDevice(deviceId: String, label: String): Unit =
        mutateDevice(deviceId, "rename", "the device", retry = { renameDevice(deviceId, label) }) { accountId ->
            devicesApi.rename(accountId, deviceId, label.trim())
        }

    /**
     * EXP-924: the machine's display glyph, from the device icon set; null
     * resets it to the kind default. The pick is drawn OPTIMISTICALLY by the
     * caller (the write lands back through the devices shape), so a failure
     * hands [onFailure] back the revert and raises the error toast (FEED-79).
     */
    fun setDeviceIcon(deviceId: String, icon: String?, onFailure: () -> Unit = {}) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _deviceBusy.value = _deviceBusy.value + deviceId
            runCatching { devicesApi.setIcon(accountId, deviceId, icon) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    onFailure()
                    _failure.value = FailedAction("save", "the icon", trpcErrorMessage(t, ""))
                }
            _deviceBusy.value = _deviceBusy.value - deviceId
        }
    }

    /**
     * Drop the registry row. A machine whose daemon still runs re-registers
     * itself on its next heartbeat — the confirm dialog says so.
     */
    fun removeDevice(deviceId: String) =
        mutateDevice(deviceId, "remove", "the device") { accountId -> devicesApi.remove(accountId, deviceId) }

    /**
     * FEED-149: a team owner drops a TEAMMATE's machine from [teamId]. Keyed
     * by the synced row id; the shape delta removes the row, a failure is
     * the remove toast web raises (`failedAction("remove", "the device")`).
     */
    fun dropDeviceShare(device: SteerDevice, teamId: String) {
        val rowId = device.rowId ?: return
        mutateDevice(device.deviceId, "remove", "the device") { accountId ->
            devicesApi.dropShare(accountId, rowId, teamId)
        }
    }

    /** Ask a server daemon to self-update; it restarts when idle. */
    fun requestDeviceUpdate(deviceId: String): Unit =
        mutateDevice(deviceId, "update", "the device", retry = { requestDeviceUpdate(deviceId) }) { accountId ->
            devicesApi.requestUpdate(accountId, deviceId)
        }

    // Every row mutation follows the same shape: mark the row busy, run, and
    // let sync land the change (the mutations return txids the web awaits;
    // here the shape delta arrives within the long-poll).
    private fun mutateDevice(
        deviceId: String,
        verb: String,
        thing: String,
        retry: (() -> Unit)? = null,
        block: suspend (String) -> Unit,
    ) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _deviceBusy.value = _deviceBusy.value + deviceId
            runCatching { block(accountId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _failure.value = FailedAction(verb, thing, trpcErrorMessage(t, ""), retry = retry)
                }
            _deviceBusy.value = _deviceBusy.value - deviceId
        }
    }
}

/**
 * How long a queued usage refresh shows as in flight before the section gives
 * up on the machine answering (it answers by re-reporting on its next beat).
 * Web `REFRESH_PENDING_MS`.
 */
const val REFRESH_PENDING_MS = 45_000L

/**
 * EXP-817: the section's own refresh never re-tries one account faster than
 * this — a queued command the machine has not answered yet is a CONFLICT on
 * the server, and hammering it buys nothing. Web `AUTO_REFRESH_RETRY_MS`.
 */
const val AUTO_REFRESH_RETRY_MS = 60_000L

/**
 * EXP-849/EXP-909: one LOGIN row's command slot in
 * [AgentsViewModel.accountCommandStates]. The row already carries its device
 * ([AgentProfileUsageRow.key] is `<deviceId>:<agent>:<profileId>`), which is
 * the whole point: two machines holding the same login must not share one
 * spinner and one error.
 */
internal fun deviceLoginCommandKey(row: AgentProfileUsageRow): String =
    accountCommandKey(row.deviceId, row.agent, row.profileId)

/**
 * EXP-862: the same slot, addressed by its parts — a caller that names a
 * device × agent × profile without holding a row.
 */
internal fun accountCommandKey(deviceId: String, agent: String, profileId: String): String =
    "$deviceId:$agent:$profileId"

/**
 * The synced devices rows → the tab's SteerDevice list (EXP-481): the
 * caller's own machines first, then the SELECTED team's shared servers with
 * their owner resolved from the synced users (a sharing owner is always a
 * member of the sharing team, hence inside the users shape). Each group in
 * [stableDeviceOrder] (EXP-623) — online-by-label first, so heartbeats can't
 * reorder the list. Signed out (null [currentUserId]) lists nothing.
 *
 * EXP-1186: [allTeams] (the Devices page) lists the servers shared with ANY
 * member team instead — the synced rows are exactly those (`user_id = me OR
 * shared_team_ids && member teams`) — once each, ignoring [teamId].
 */
fun composeDeviceList(
    rows: List<DeviceEntity>,
    users: List<UserEntity>,
    teamId: String?,
    currentUserId: String?,
    nowMs: Long,
    allTeams: Boolean = false,
): List<SteerDevice> {
    if (currentUserId == null) return emptyList()
    val usersById = users.associateBy { it.id }
    // #EXP-1440: scoped to a team, an own machine shared only with OTHER
    // teams is theirs and never listed (`deviceUsableInTeam`).
    val own = rows
        .filter { it.userId == currentUserId }
        .sortedWith(stableDeviceOrder(nowMs))
        .map { it.toSteerDevice(nowMs, currentUserId) }
        .filter { allTeams || teamId == null || SessionDeviceMove.deviceUsableInTeam(it, teamId) }
    val shared = rows
        .filter {
            it.userId != currentUserId &&
                (allTeams || (teamId != null && it.sharedTeamIds.contains(teamId))) &&
                it.kind == SteerDevice.KIND_SERVER
        }
        .distinctBy { it.id }
        .sortedWith(stableDeviceOrder(nowMs))
        .map { it.toSteerDevice(nowMs, currentUserId, usersById[it.userId]?.name) }
    return own + shared
}

