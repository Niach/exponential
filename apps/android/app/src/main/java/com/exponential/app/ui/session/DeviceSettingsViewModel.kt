package com.exponential.app.ui.session

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.DeviceCommandDto
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.DevicesApi
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
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
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

    private val _defaultsBusy = MutableStateFlow(false)
    val defaultsBusy: StateFlow<Boolean> = _defaultsBusy
    private val _defaultsError = MutableStateFlow<String?>(null)
    val defaultsError: StateFlow<String?> = _defaultsError
    /** Flips true after a successful save; cleared by the next edit. */
    private val _defaultsSaved = MutableStateFlow(false)
    val defaultsSaved: StateFlow<Boolean> = _defaultsSaved

    // Worktree command progress, keyed by "<repo> <branch>" (or
    // [PRUNE_COMMAND_KEY]); a terminal state stays until the next command on
    // the same key.
    private val _commandStates = MutableStateFlow<Map<String, DeviceCommandUiState>>(emptyMap())
    val commandStates: StateFlow<Map<String, DeviceCommandUiState>> = _commandStates

    fun clearDefaultsSaved() {
        _defaultsSaved.value = false
    }

    fun rename(deviceId: String, label: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _nameBusy.value = true
            _nameError.value = null
            runCatching { devicesApi.rename(accountId, deviceId, label.trim()) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _nameError.value = trpcErrorMessage(t, "The machine could not be renamed")
                }
            _nameBusy.value = false
        }
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

    fun saveDefaults(deviceId: String, defaults: DeviceLaunchDefaults) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _defaultsBusy.value = true
            _defaultsError.value = null
            _defaultsSaved.value = false
            runCatching { devicesApi.setLaunchDefaults(accountId, deviceId, defaults) }
                .fold(
                    onSuccess = { _defaultsSaved.value = true },
                    onFailure = { t ->
                        if (t is CancellationException) throw t
                        _defaultsError.value =
                            trpcErrorMessage(t, "The defaults could not be saved")
                    },
                )
            _defaultsBusy.value = false
        }
    }

    fun removeWorktree(deviceId: String, worktree: DeviceWorktreeEntity, deviceOnline: Boolean) {
        issueCommand(
            key = "${worktree.repoFullName} ${worktree.branch}",
            command = worktreeRemoveCommand(deviceId, worktree.repoFullName, worktree.branch),
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

private const val COMMAND_POLL_INTERVAL_MS = 2_000L
private const val COMMAND_POLL_DEADLINE_MS = 90_000L
