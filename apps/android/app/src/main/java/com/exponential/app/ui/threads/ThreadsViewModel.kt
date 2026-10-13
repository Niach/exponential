package com.exponential.app.ui.threads

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.SessionTree
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

/**
 * EXP-1291 Threads v2: the Threads TAB's model (lane THREADS owns the UI).
 * Phones list EVERY member team (`teamId = null`, banded per team by the
 * screen): [SessionTree.partitionThreads] over the synced session tree —
 * Needs you · Running · Recent, empty groups omitted — plus the team agents
 * ([SessionTree.listAgents]).
 */
@HiltViewModel
class ThreadsViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val sessions = dbFlow.scopedQuery(emptyList()) { it.sessionDao().observeAll() }
        .map { rows -> rows.map(SessionTree::session) }

    private val inputs = combine(
        sessions,
        dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeAll() }.map { it.map(SessionTree::sessionIssue) },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() }.map { it.map(SessionTree::issue) },
        dbFlow.scopedQuery(emptyList()) { it.messageDao().observeAll() }.map { it.map(SessionTree::message) },
    ) { s, links, issues, messages -> Inputs(s, links, issues, messages) }

    private data class Inputs(
        val sessions: List<SessionTree.Session>,
        val links: List<SessionTree.SessionIssue>,
        val issues: List<SessionTree.Issue>,
        val messages: List<SessionTree.Message>,
    )

    /** The grouped thread list across every member team (`teamId = null`). */
    val groups: StateFlow<List<SessionTree.ThreadsListGroup>> = inputs.map { input ->
        SessionTree.partitionThreads(input.sessions, input.links, input.issues, input.messages, teamId = null)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Every member team's agents (rail / Settings order). */
    val agents: StateFlow<List<SessionTree.AgentRow>> = sessions.map { SessionTree.listAgents(it, teamId = null) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The member teams, name order — the per-team bands. */
    val teams: StateFlow<List<TeamEntity>> = dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Whether ANY thread needs a person (the tab's amber dot). */
    val needsYou: StateFlow<Boolean> = groups.map { list ->
        list.any { it.key == SessionTree.ThreadsGroupKey.NeedsYou && it.rows.isNotEmpty() }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)
}
