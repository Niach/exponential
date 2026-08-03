package com.exponential.app.ui.session

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.DeviceLatestVersions
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.issue.StartIssueOption
import com.exponential.app.ui.issue.SteerStartState
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The Agents tab: the signed-in user's OWN coding sessions currently running
// (synced coding_sessions shape joined to its issue), plus a remote-start
// launcher against the user's machines (EXP-156). The desktop remains the only
// session runner — this tab lists live sessions and kicks off new (single or
// batch) runs on a picked machine.
//
// The machine list is the EXP-403 registry (`devices.list`, polled while the
// tab is visible), not the relay's presence map: it carries offline machines,
// their kind (desktop IDE vs headless `exponential` server), their version and
// the rename/remove/update affordances. Every OTHER surface in the app only
// needs a picker, so those still read the presence-only `steer.myDevices`.

data class AgentRow(
    val session: CodingSessionEntity,
    val issue: IssueEntity?,
)

data class AgentsState(
    val rows: List<AgentRow> = emptyList(),
    // steer.config is env-derived and static per instance: null = still
    // loading. Decides whether a row tap opens the live viewer directly or
    // falls back to the issue detail, and whether the devices section shows.
    val steerEnabled: Boolean? = null,
)

@HiltViewModel
class AgentsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val steerApi: SteerApi,
    private val devicesApi: DevicesApi,
    private val issuesApi: IssuesApi,
    private val selection: TeamSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    // The caller's registered machines, online AND offline. null = not loaded yet.
    private val _devices = MutableStateFlow<List<SteerDevice>?>(null)
    val devices: StateFlow<List<SteerDevice>?> = _devices

    // Informational CLIENT_LATEST_VERSION_* values behind the "update
    // available" hint on a machine row; both null until the first list lands.
    private val _latestVersions = MutableStateFlow(DeviceLatestVersions())
    val latestVersions: StateFlow<DeviceLatestVersions> = _latestVersions

    // Machine ids with a rename/remove/update mutation in flight — the row's
    // menu stays put but its actions disable until the refetch lands.
    private val _deviceBusy = MutableStateFlow<Set<String>>(emptySet())
    val deviceBusy: StateFlow<Set<String>> = _deviceBusy

    private val _startState = MutableStateFlow<SteerStartState>(SteerStartState.Idle)
    val startState: StateFlow<SteerStartState> = _startState

    // Paired up front: the typed `combine` overloads stop at five flows, and
    // the state below already needs six.
    private val liveSessionsAndIssues = combine(
        dbFlow.scopedQuery(emptyList()) {
            it.codingSessionDao().observeByStatuses(CodingSessionLiveness.liveStatuses)
        },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
    ) { sessions, issues -> sessions to issues }

    val state: StateFlow<AgentsState> = combine(
        liveSessionsAndIssues,
        _steerEnabled,
        // Heartbeat-stale rows render as absent (EXP-153); the ticker clears
        // them once the liveness window elapses without a sync delta.
        CodingSessionLiveness.minuteTicker(),
        auth.userId,
        selection.selectedId,
    ) { (sessions, issues), steerEnabled, now, userId, teamId ->
        AgentsState(
            rows = agentRows(sessions, issues, userId, teamId, now),
            steerEnabled = steerEnabled,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AgentsState())

    // Issues the Start-coding sheet can queue, scoped to the SELECTED team
    // (no current-issue exemption here — this tab has no "current" issue):
    // repo-backed, live boards; open issues, `updatedAt` desc.
    val startCandidates: StateFlow<List<StartIssueOption>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        selection.selectedId,
    ) { issues, boards, teamId ->
        if (teamId == null) {
            emptyList()
        } else {
            val eligibleBoards = boards
                .filter {
                    it.teamId == teamId &&
                        it.repositoryId != null &&
                        it.deletedAt == null
                }
                .associateBy { it.id }
            issues
                .filter {
                    it.boardId in eligibleBoards.keys &&
                        it.status !in TERMINAL_ISSUE_STATUSES &&
                        it.prState != DomainContract.prStateMerged
                }
                .sortedByDescending { it.updatedAt }
                .map { issue ->
                    StartIssueOption(
                        id = issue.id,
                        identifier = issue.identifier,
                        title = issue.title,
                        repositoryId = eligibleBoards[issue.boardId]?.repositoryId,
                        status = issue.status,
                        priority = issue.priority,
                    )
                }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    init {
        // Steer availability + the machine registry, re-fetched on account
        // switch (mirrors the issue detail's check).
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _steerEnabled.value = null
                _devices.value = null
                _latestVersions.value = DeviceLatestVersions()
                _startState.value = SteerStartState.Idle
                if (accountId == null) {
                    _steerEnabled.value = false
                    _devices.value = emptyList()
                    return@collectLatest
                }
                val enabled = runCatching { steerApi.config(accountId).enabled }
                    .getOrDefault(false)
                _steerEnabled.value = enabled
                if (enabled) {
                    loadDevices(accountId)
                } else {
                    _devices.value = emptyList()
                }
            }
        }
    }

    private suspend fun loadDevices(accountId: String) {
        runCatching { devicesApi.list(accountId) }
            .onSuccess {
                _devices.value = it.devices
                _latestVersions.value = it.latestVersions
            }
            // A failed poll must not blank a list the user is reading; only the
            // very first failure resolves the loading state (to empty).
            .onFailure { _devices.value = _devices.value ?: emptyList() }
    }

    /**
     * Re-read the machine registry — the tab polls this while visible, and
     * every row mutation calls it straight after. No-op until steer resolves
     * on (the machines section is hidden without a relay, exactly like web).
     */
    fun refreshDevices() {
        if (_steerEnabled.value != true) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            loadDevices(accountId)
        }
    }

    /** Rename a machine (its registry label wins over the relay's). */
    fun renameDevice(deviceId: String, label: String) =
        mutateDevice(deviceId) { accountId -> devicesApi.rename(accountId, deviceId, label.trim()) }

    /**
     * Drop the registry row. A machine whose daemon still runs re-registers
     * itself on its next heartbeat — the confirm dialog says so.
     */
    fun removeDevice(deviceId: String) =
        mutateDevice(deviceId) { accountId -> devicesApi.remove(accountId, deviceId) }

    /** Ask a server daemon to self-update; it restarts when idle. */
    fun requestDeviceUpdate(deviceId: String) =
        mutateDevice(deviceId) { accountId -> devicesApi.requestUpdate(accountId, deviceId) }

    // Every row mutation follows the same shape: mark the row busy, run, then
    // refetch so the list reflects the change without waiting for the poll.
    private fun mutateDevice(deviceId: String, block: suspend (String) -> Unit) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _deviceBusy.value = _deviceBusy.value + deviceId
            runCatching { block(accountId) }
            loadDevices(accountId)
            _deviceBusy.value = _deviceBusy.value - deviceId
        }
    }

    /**
     * Remote-start on a picked desktop (EXP-156): [issueIds] of size 1 launches
     * a plain single session, 2+ a batch. Sent state re-enables after a grace
     * window in case the desktop never picks up (the coding_sessions row would
     * otherwise swap the list via Electric).
     */
    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) {
        if (issueIds.isEmpty()) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            val isBatch = issueIds.size >= 2
            _startState.value = SteerStartState.Sending
            try {
                if (isBatch) {
                    steerApi.startSession(accountId, issueIds, device.deviceId, options)
                } else {
                    steerApi.startSession(accountId, issueIds.first(), device.deviceId, options)
                }
                _startState.value = SteerStartState.Sent(device.deviceLabel, isBatch)
                delay(30_000)
                if (_startState.value is SteerStartState.Sent) {
                    _startState.value = SteerStartState.Idle
                }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _startState.value = SteerStartState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    /**
     * Remote-run a team action from the unified sheet's Actions tab (EXP-257)
     * with the full option set + filled inputs; the builtin "Create action"
     * id additionally rides its teamId (server-required there, forbidden
     * otherwise). Same Sent/grace-window handling as [startCoding].
     */
    fun runAction(
        device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: Map<String, String>,
    ) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _startState.value = SteerStartState.Sending
            try {
                steerApi.startActionSession(
                    accountId,
                    actionId = action.id,
                    deviceId = device.deviceId,
                    options = options,
                    // Required for EVERY builtin (there is no DB row to derive
                    // the team from), forbidden otherwise — the server rejects
                    // both mistakes.
                    teamId = action.teamId.takeIf { action.isBuiltin },
                    inputs = inputs.takeIf { it.isNotEmpty() },
                )
                _startState.value = SteerStartState.Sent(device.deviceLabel, isBatch = false)
                delay(30_000)
                if (_startState.value is SteerStartState.Sent) {
                    _startState.value = SteerStartState.Idle
                }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _startState.value = SteerStartState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    // ── Merge and close (EXP-358) ────────────────────────────────────────────
    // A merge alone leaves the session alive on `merged`; this is the explicit
    // "and close" variant — the server merges AND ends the session, so the row
    // drops off this list on its own once the `ended` flip syncs. Keyed by
    // issue id: several rows can be in flight at once.
    private val _merging = MutableStateFlow<Set<String>>(emptySet())
    val merging: StateFlow<Set<String>> = _merging

    // Rendered INLINE on the failing row (EXP-323 pattern — a snackbar hides
    // behind the floating bottom nav pill). Cleared by the next attempt.
    private val _mergeErrors = MutableStateFlow<Map<String, String>>(emptyMap())
    val mergeErrors: StateFlow<Map<String, String>> = _mergeErrors

    /**
     * Squash-merge the row's PR and END its coding session (EXP-358). For a
     * batch PR the server resolves [issueId] to ALL linked issues and completes
     * them together.
     */
    fun mergeAndClose(issueId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _mergeErrors.value = _mergeErrors.value - issueId
            _merging.value = _merging.value + issueId
            runCatching { issuesApi.mergePr(accountId, issueId, closeSessions = true) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    // Conflicts, branch protection and GitHub App errors are the
                    // common, persistent failures of a squash merge — same copy
                    // as Reviews and the issue Changes tab.
                    _mergeErrors.value = _mergeErrors.value +
                        (issueId to trpcErrorMessage(t, "The pull request could not be merged"))
                }
            _merging.value = _merging.value - issueId
        }
    }
}

/**
 * The Agents list: the signed-in user's OWN live sessions in the SELECTED team
 * only. A teammate's live session is neither viewable nor steerable (EXP-312),
 * so listing it just read as "computer not online" — and a session in another
 * team belongs under that team, matching web's `use-agents-data.ts`. The rows
 * stay SYNCED for the issue-detail badges and Reviews, they only leave this
 * list. Every session row carries a non-null synced `team_id` (denormalized by
 * trigger for issue rows, explicit on batch/action rows), so the scoping holds
 * for issueless runs too. Signed out (null [currentUserId]) or no team
 * selected (null [teamId]) lists nothing.
 */
fun agentRows(
    sessions: List<CodingSessionEntity>,
    issues: List<IssueEntity>,
    currentUserId: String?,
    teamId: String?,
    nowMs: Long,
): List<AgentRow> {
    if (currentUserId == null || teamId == null) return emptyList()
    val issuesById = issues.associateBy { it.id }
    return sessions
        .filter {
            it.userId == currentUserId &&
                it.teamId == teamId &&
                CodingSessionLiveness.isLive(it, nowMs)
        }
        // issueId is null for batch multi-issue sessions — those rows render
        // without an issue link.
        .map { AgentRow(session = it, issue = it.issueId?.let(issuesById::get)) }
}

// Terminal issue statuses ineligible to start a new coding run.
private val TERMINAL_ISSUE_STATUSES = setOf("done", "cancelled", "duplicate")
