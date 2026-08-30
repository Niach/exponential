package com.exponential.app.ui.session

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.DeviceCommandDto
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.agentLoginCommand
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.api.worktreePruneCommand
import com.exponential.app.data.api.worktreeRemoveCommand
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceWorktreeEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The device-settings sheet's data + mutations (EXP-481): rename, team
// sharing, the server-authoritative launch-defaults edit — all of which work
// with the machine OFFLINE (the row is the truth; the machine converges) —
// and the worktree command queue (remove / prune), whose progress is polled
// off `devices.getCommand` while the material outcome arrives through the
// synced device_worktrees shape.
//
// EXP-490: name and defaults AUTO-SAVE. Every edit queues its value and the
// mutation fires once the user pauses — each `devices.*` write nudges the
// machine over the relay, so a call per keystroke or per picker tap is not an
// option — with a flush on field blur and on sheet dismiss.

/** One issued worktree command's UI state, keyed by [DeviceSettingsViewModel.commandStates]. */
sealed interface DeviceCommandUiState {
    data object Sending : DeviceCommandUiState
    /** Queued server-side; the machine is offline and runs it on return. */
    data object Queued : DeviceCommandUiState
    /** Delivered and being worked (pending + machine online). */
    data object Running : DeviceCommandUiState
    data class Done(val message: String?) : DeviceCommandUiState
    data class Failed(val message: String) : DeviceCommandUiState
}

/** The prune button's stable key in [DeviceSettingsViewModel.commandStates]. */
const val PRUNE_COMMAND_KEY = "__prune__"

/**
 * One agent's sign-in command key in [DeviceSettingsViewModel.commandStates]
 * (EXP-484) — the prefix is what tells a `Done` result apart from a worktree
 * command's plain-text summary, so its payload is parsed as a login URL.
 */
fun agentLoginCommandKey(agent: String): String = "login:$agent"

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class DeviceSettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val devicesApi: DevicesApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    // The devices ROW id the sheet is bound to (set from the composable —
    // idempotent, survives recomposition).
    private val boundRowId = MutableStateFlow<String?>(null)

    fun bind(deviceRowId: String?) {
        // The ViewModel outlives one sheet open — a pending edit must land on
        // the machine it was typed for, not on the next one bound here.
        val previous = boundRowId.value
        if (previous != null && previous != deviceRowId) flushPending()
        boundRowId.value = deviceRowId
    }

    /** The bound machine's synced worktree inventory. */
    val worktrees: StateFlow<List<DeviceWorktreeEntity>> = boundRowId
        .flatMapLatest { rowId ->
            if (rowId == null) {
                flowOf(emptyList())
            } else {
                dbFlow.scopedQuery(emptyList<DeviceWorktreeEntity>()) {
                    it.deviceWorktreeDao().observeByDevice(rowId)
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The caller's teams — the sharing picker's options. */
    val teams: StateFlow<List<TeamEntity>> =
        dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // Per-section busy flags + inline errors (EXP-323 idiom — errors caption
    // the triggering section, cleared by the next attempt).
    private val _nameBusy = MutableStateFlow(false)
    val nameBusy: StateFlow<Boolean> = _nameBusy
    private val _nameError = MutableStateFlow<String?>(null)
    val nameError: StateFlow<String?> = _nameError

    private val _shareBusy = MutableStateFlow(false)
    val shareBusy: StateFlow<Boolean> = _shareBusy
    private val _shareError = MutableStateFlow<String?>(null)
    val shareError: StateFlow<String?> = _shareError
    private val _defaultBusy = MutableStateFlow(false)
    val defaultBusy: StateFlow<Boolean> = _defaultBusy
    private val _defaultError = MutableStateFlow<String?>(null)
    val defaultError: StateFlow<String?> = _defaultError

    private val _defaultsBusy = MutableStateFlow(false)
    val defaultsBusy: StateFlow<Boolean> = _defaultsBusy
    private val _defaultsError = MutableStateFlow<String?>(null)
    val defaultsError: StateFlow<String?> = _defaultsError

    // Worktree command progress, keyed by "<repo> <branch>" (or
    // [PRUNE_COMMAND_KEY]); a terminal state stays until the next command on
    // the same key.
    private val _commandStates = MutableStateFlow<Map<String, DeviceCommandUiState>>(emptyMap())
    val commandStates: StateFlow<Map<String, DeviceCommandUiState>> = _commandStates

    // The pending auto-saves: deviceId to the value the user last left behind.
    // Nulled only by a successful save (reference-identity compareAndSet, so a
    // newer edit made mid-flight survives) — a FAILED save keeps its input so
    // the blur/dismiss flush retries it instead of dropping the edit.
    private val renameInput = MutableStateFlow<Pair<String, String>?>(null)
    private val defaultsInput = MutableStateFlow<Pair<String, DeviceLaunchDefaults>?>(null)

    // What [flushPending] already wrote. Clearing the input does not cancel a
    // debounce window that is still counting down, so without this the timer
    // fires afterwards and repeats the call (and its relay nudge).
    private var flushedRename: Pair<String, String>? = null
    private var flushedDefaults: Pair<String, DeviceLaunchDefaults>? = null

    @OptIn(FlowPreview::class)
    private fun collectAutoSaves() {
        viewModelScope.launch {
            renameInput.filterNotNull().debounce(AUTOSAVE_DEBOUNCE_MS)
                .collect { if (it !== flushedRename) saveRenameNow(it) }
        }
        viewModelScope.launch {
            defaultsInput.filterNotNull().debounce(AUTOSAVE_DEBOUNCE_MS)
                .collect { if (it !== flushedDefaults) saveDefaultsNow(it) }
        }
    }

    init {
        collectAutoSaves()
    }

    /** Queue a rename; null (blank or unchanged input) drops the pending one. */
    fun queueRename(deviceId: String, label: String?) {
        renameInput.value = label?.let { deviceId to it }
    }

    fun queueDefaults(deviceId: String, defaults: DeviceLaunchDefaults) {
        defaultsInput.value = deviceId to defaults
    }

    fun hasPendingRename(): Boolean = renameInput.value != null || nameBusy.value

    fun hasPendingDefaults(): Boolean = defaultsInput.value != null || defaultsBusy.value

    /**
     * Save what is queued right now, on a process-lifetime scope: this fires
     * from the sheet's dispose, and a dismiss-then-navigate-away must not
     * cancel the final write mid-request.
     */
    fun flushPending() {
        val rename = renameInput.value
        val defaults = defaultsInput.value
        flushedRename = rename
        flushedDefaults = defaults
        renameInput.value = null
        defaultsInput.value = null
        if (rename != null) deviceSettingsFlushScope.launch { saveRenameNow(rename) }
        if (defaults != null) deviceSettingsFlushScope.launch { saveDefaultsNow(defaults) }
    }

    private suspend fun saveRenameNow(pending: Pair<String, String>) {
        val accountId = auth.activeAccountId.value ?: return
        _nameBusy.value = true
        _nameError.value = null
        runCatching { devicesApi.rename(accountId, pending.first, pending.second) }
            .fold(
                onSuccess = { renameInput.compareAndSet(pending, null) },
                onFailure = { t ->
                    if (t is CancellationException) throw t
                    _nameError.value = trpcErrorMessage(t, "The machine could not be renamed")
                },
            )
        _nameBusy.value = false
    }

    private suspend fun saveDefaultsNow(pending: Pair<String, DeviceLaunchDefaults>) {
        val accountId = auth.activeAccountId.value ?: return
        _defaultsBusy.value = true
        _defaultsError.value = null
        runCatching { devicesApi.setLaunchDefaults(accountId, pending.first, pending.second) }
            .fold(
                onSuccess = { defaultsInput.compareAndSet(pending, null) },
                onFailure = { t ->
                    if (t is CancellationException) throw t
                    _defaultsError.value = trpcErrorMessage(t, "The defaults could not be saved")
                },
            )
        _defaultsBusy.value = false
    }

    fun setShared(deviceId: String, teamId: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _shareBusy.value = true
            _shareError.value = null
            runCatching { devicesApi.setShared(accountId, deviceId, teamId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _shareError.value = trpcErrorMessage(t, "The share could not be changed")
                }
            _shareBusy.value = false
        }
    }

    /**
     * EXP-622: flag/unflag this machine as the caller's default — the row every
     * device picker prefills. Written straight through (a single toggle, no
     * debounce); the server clears the flag on the caller's other machines and
     * the switch re-renders off the synced row.
     */
    fun setDefault(deviceId: String, isDefault: Boolean) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _defaultBusy.value = true
            _defaultError.value = null
            runCatching { devicesApi.setDefault(accountId, deviceId, isDefault) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _defaultError.value =
                        trpcErrorMessage(t, "The default machine could not be changed")
                }
            _defaultBusy.value = false
        }
    }

    fun removeWorktree(deviceId: String, worktree: DeviceWorktreeEntity, deviceOnline: Boolean) {
        issueCommand(
            key = "${worktree.repoFullName} ${worktree.branch}",
            command = worktreeRemoveCommand(deviceId, worktree.repoFullName, worktree.branch),
            deviceOnline = deviceOnline,
        )
    }

    /**
     * EXP-484: ask the machine to run [agent]'s own sign-in flow and publish
     * the login URL (plus codex's device code) back as the command result —
     * no credential ever travels. [switchAccount] signs the current account
     * out first (which, for codex, revokes the token server-side — the sheet
     * confirms before calling). Same durable queue as the worktree commands,
     * but only offered for an ONLINE machine that advertises the cap: a login
     * link parked until tomorrow would be expired anyway.
     */
    fun agentLogin(
        deviceId: String,
        agent: String,
        switchAccount: Boolean,
        deviceOnline: Boolean,
    ) {
        issueCommand(
            key = agentLoginCommandKey(agent),
            command = agentLoginCommand(deviceId, agent, switchAccount),
            deviceOnline = deviceOnline,
        )
    }

    fun pruneWorktrees(deviceId: String, deviceOnline: Boolean) {
        issueCommand(
            key = PRUNE_COMMAND_KEY,
            command = worktreePruneCommand(deviceId),
            deviceOnline = deviceOnline,
        )
    }

    // Queue the command, then poll its row until terminal (the machine also
    // re-reports its worktrees on completion, so the list updates through
    // sync). An OFFLINE machine's command parks server-side — the row stays
    // pending and the UI says "runs when it comes online" instead of spinning.
    private fun issueCommand(key: String, command: kotlinx.serialization.json.JsonObject, deviceOnline: Boolean) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _commandStates.value = _commandStates.value + (key to DeviceCommandUiState.Sending)
            val created = runCatching { devicesApi.createCommand(accountId, command) }
                .getOrElse { t ->
                    if (t is CancellationException) throw t
                    _commandStates.value = _commandStates.value +
                        (key to DeviceCommandUiState.Failed(
                            trpcErrorMessage(t, "The command could not be queued"),
                        ))
                    return@launch
                }
            if (!deviceOnline) {
                _commandStates.value = _commandStates.value + (key to DeviceCommandUiState.Queued)
                return@launch
            }
            _commandStates.value = _commandStates.value + (key to DeviceCommandUiState.Running)
            val deadline = System.currentTimeMillis() + COMMAND_POLL_DEADLINE_MS
            while (System.currentTimeMillis() < deadline) {
                delay(COMMAND_POLL_INTERVAL_MS)
                val row = runCatching { devicesApi.getCommand(accountId, created.id) }
                    .getOrNull() ?: continue
                when (row.status) {
                    DeviceCommandDto.STATUS_DONE -> {
                        _commandStates.value = _commandStates.value +
                            (key to DeviceCommandUiState.Done(row.result))
                        return@launch
                    }
                    DeviceCommandDto.STATUS_FAILED -> {
                        _commandStates.value = _commandStates.value +
                            (key to DeviceCommandUiState.Failed(
                                row.result ?: "The machine refused the command",
                            ))
                        return@launch
                    }
                }
            }
            // Still pending after the deadline — the row is durable, so the
            // honest caption is "queued", not a failure.
            _commandStates.value = _commandStates.value + (key to DeviceCommandUiState.Queued)
        }
    }
}

// Auto-saves fired while the sheet is closing must outlive the ViewModel:
// viewModelScope is cancelled when navigation clears it, which could abort the
// final flush mid-request. Process-lifetime, mirroring descriptionFlushScope.
private val deviceSettingsFlushScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

/** How long an edit rests before it is written (one relay nudge per write). */
private const val AUTOSAVE_DEBOUNCE_MS = 800L

private const val COMMAND_POLL_INTERVAL_MS = 2_000L
private const val COMMAND_POLL_DEADLINE_MS = 90_000L
