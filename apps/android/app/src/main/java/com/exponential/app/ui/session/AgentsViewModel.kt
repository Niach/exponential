package com.exponential.app.ui.session

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
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

// The Agents tab: every coding session currently running on the active
// account (synced coding_sessions shape joined to its issue), plus a
// remote-start launcher against the user's online desktops (EXP-156). The
// desktop remains the only session runner — this tab lists live sessions and
// kicks off new (single or batch) runs on a picked desktop.

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
    private val selection: TeamSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    // The caller's online desktops (relay presence). null = not loaded yet.
    private val _devices = MutableStateFlow<List<SteerDevice>?>(null)
    val devices: StateFlow<List<SteerDevice>?> = _devices

    private val _startState = MutableStateFlow<SteerStartState>(SteerStartState.Idle)
    val startState: StateFlow<SteerStartState> = _startState

    val state: StateFlow<AgentsState> = combine(
        dbFlow.scopedQuery(emptyList()) {
            it.codingSessionDao().observeByStatuses(CodingSessionLiveness.liveStatuses)
        },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        _steerEnabled,
        // Heartbeat-stale rows render as absent (EXP-153); the ticker clears
        // them once the liveness window elapses without a sync delta.
        CodingSessionLiveness.minuteTicker(),
    ) { sessions, issues, steerEnabled, now ->
        val issuesById = issues.associateBy { it.id }
        AgentsState(
            // issueId is null for batch multi-issue sessions — those rows
            // render without an issue link.
            rows = sessions
                .filter { CodingSessionLiveness.isLive(it, now) }
                .map { AgentRow(session = it, issue = it.issueId?.let(issuesById::get)) },
            steerEnabled = steerEnabled,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AgentsState())

    // Issues the Start-coding sheet can queue, scoped to the SELECTED team
    // (no current-issue exemption here — this tab has no "current" issue):
    // repo-backed, non-archived boards; open issues, `updatedAt` desc.
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
                        it.archivedAt == null &&
                        it.deletedAt == null
                }
                .associateBy { it.id }
            issues
                .filter {
                    it.boardId in eligibleBoards.keys &&
                        it.archivedAt == null &&
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
        // Steer availability + device presence, re-fetched on account switch
        // (mirrors the issue detail's check).
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _steerEnabled.value = null
                _devices.value = null
                _startState.value = SteerStartState.Idle
                if (accountId == null) {
                    _steerEnabled.value = false
                    _devices.value = emptyList()
                    return@collectLatest
                }
                val enabled = runCatching { steerApi.config(accountId).enabled }
                    .getOrDefault(false)
                _steerEnabled.value = enabled
                _devices.value = if (enabled) {
                    runCatching { steerApi.myDevices(accountId).devices }.getOrDefault(emptyList())
                } else {
                    emptyList()
                }
            }
        }
    }

    /** Re-poll device presence (on tab resume) — no-op until steer resolves on. */
    fun refreshDevices() {
        if (_steerEnabled.value != true) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { steerApi.myDevices(accountId).devices }
                .onSuccess { _devices.value = it }
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
}

// Terminal issue statuses ineligible to start a new coding run.
private val TERMINAL_ISSUE_STATUSES = setOf("done", "cancelled", "duplicate")
