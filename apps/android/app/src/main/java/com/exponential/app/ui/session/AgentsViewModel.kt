package com.exponential.app.ui.session

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DomainContract
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The Agents tab: every coding session currently running on the active
// account (synced coding_sessions shape joined to its issue). Pure read —
// the desktop remains the only session runner; this list is the front door
// to the existing watch/steer viewer.

data class AgentRow(
    val session: CodingSessionEntity,
    val issue: IssueEntity?,
)

data class AgentsState(
    val rows: List<AgentRow> = emptyList(),
    // steer.config is env-derived and static per instance: null = still
    // loading. Decides whether a row tap opens the live viewer directly or
    // falls back to the issue detail.
    val steerEnabled: Boolean? = null,
)

@HiltViewModel
class AgentsViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    private val steerApi: SteerApi,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _steerEnabled = MutableStateFlow<Boolean?>(null)

    val state: StateFlow<AgentsState> = combine(
        dbFlow.scopedQuery(emptyList()) {
            it.codingSessionDao().observeByStatus(DomainContract.codingSessionStatusRunning)
        },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        _steerEnabled,
    ) { sessions, issues, steerEnabled ->
        val issuesById = issues.associateBy { it.id }
        AgentsState(
            // issueId is null for batch multi-issue sessions — those rows
            // render without an issue link.
            rows = sessions.map { AgentRow(session = it, issue = it.issueId?.let(issuesById::get)) },
            steerEnabled = steerEnabled,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AgentsState())

    init {
        // Steer availability, re-fetched on account switch (mirrors the issue
        // detail's check).
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _steerEnabled.value = null
                if (accountId == null) {
                    _steerEnabled.value = false
                    return@collectLatest
                }
                _steerEnabled.value = runCatching { steerApi.config(accountId).enabled }
                    .getOrDefault(false)
            }
        }
    }
}
