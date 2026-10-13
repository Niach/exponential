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
    private val auth: AuthRepository,
    holder: DatabaseHolder,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val sessionRows = dbFlow.scopedQuery(emptyList()) { it.sessionDao().observeAll() }

    private val sessions = sessionRows.map { rows -> rows.map(SessionTree::session) }

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

    /** Each listed row's glyph + identifier, keyed by root session id. */
    val rowMeta: StateFlow<Map<String, ThreadRowMeta>> = combine(groups, inputs, sessionRows) { g, input, rows ->
        threadRowMeta(g, input.sessions, input.links, input.issues, rows.associate { it.id to it.cli })
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /**
     * The phone home's Running band (`composer-home.json`: phones keep their
     * Running list of LIVE OWN sessions; never Recent): the caller's own
     * threads in Needs you then Running, every team, list order.
     */
    val running: StateFlow<List<SessionTree.ThreadsListRow>> = combine(groups, sessions, auth.userId) { g, s, me ->
        if (me == null) {
            emptyList()
        } else {
            val owner = s.associate { it.id to it.userId }
            g.filter { it.key == SessionTree.ThreadsGroupKey.NeedsYou || it.key == SessionTree.ThreadsGroupKey.Running }
                .flatMap { it.rows }
                .filter { owner[it.sessionId] == me }
        }
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

/** A Threads row's extras: the first linked issue's identifier, the root's
 *  agent (its brand mark) and the row's state glyph. */
data class ThreadRowMeta(
    val identifier: String?,
    val agent: String?,
    val state: SessionTree.SessionState,
)

/**
 * The extras each [SessionTree.ThreadsListRow] draws beside its title: the
 * mono identifier of the root's FIRST linked issue (thread order), the root's
 * agent and one state for its mark — needs you, then working (any live
 * session busy), else the root's own [SessionTree.sessionState].
 */
fun threadRowMeta(
    groups: List<SessionTree.ThreadsListGroup>,
    sessions: List<SessionTree.Session>,
    links: List<SessionTree.SessionIssue>,
    issues: List<SessionTree.Issue>,
    agentByRoot: Map<String, String?>,
): Map<String, ThreadRowMeta> {
    val byId = sessions.associateBy { it.id }
    val issueById = issues.associateBy { it.id }
    val linksByRoot = links.groupBy { it.sessionId }
    return groups.flatMap { it.rows }.associate { row ->
        val first = linksByRoot[row.sessionId].orEmpty()
            .sortedWith(compareBy<SessionTree.SessionIssue>({ it.sortOrder }, { it.issueId }))
            .firstNotNullOfOrNull { issueById[it.issueId] }
        val root = byId[row.sessionId]
        val state = when {
            row.needsYou -> SessionTree.SessionState.NeedsYou
            row.busy -> SessionTree.SessionState.Working
            root != null -> SessionTree.sessionState(root)
            else -> SessionTree.SessionState.Waiting
        }
        row.sessionId to ThreadRowMeta(first?.identifier, agentByRoot[row.sessionId], state)
    }
}
