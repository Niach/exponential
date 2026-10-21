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
        // EXP-1323: the viewer's read marks (bold rows + the nav dot).
        dbFlow.scopedQuery(emptyList()) { it.sessionReadDao().observeAll() }.map { it.map(SessionTree::sessionRead) },
    ) { s, links, issues, messages, reads -> Inputs(s, links, issues, messages, reads) }

    private data class Inputs(
        val sessions: List<SessionTree.Session>,
        val links: List<SessionTree.SessionIssue>,
        val issues: List<SessionTree.Issue>,
        val messages: List<SessionTree.Message>,
        val reads: List<SessionTree.SessionRead>,
    )

    /** The grouped thread list across every member team (`teamId = null`);
     *  EXP-1323: a live tree's children nest under their root (depth > 0). */
    val groups: StateFlow<List<SessionTree.ThreadsListGroup>> = inputs.map { input ->
        SessionTree.partitionThreads(
            input.sessions,
            input.links,
            input.issues,
            input.messages,
            teamId = null,
            reads = input.reads,
            // NEW-1: Recent's 24 h window (read on every input change).
            now = SessionTree.nowIso(),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** EXP-1323: the Threads nav dot (`threadsUnread`): a thread has news. */
    val unread: StateFlow<Boolean> = groups.map(SessionTree::threadsUnread)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** Each listed row's glyph + identifier, keyed by root session id. */
    val rowMeta: StateFlow<Map<String, ThreadRowMeta>> = combine(
        groups,
        inputs,
        sessionRows,
        dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
    ) { g, input, rows, devices ->
        threadRowMeta(
            g,
            input.links,
            input.issues,
            rows.associate { it.id to it.cli },
            devices.associate { it.deviceId to ThreadRowDevice(it.label, it.icon, it.kind == "server") },
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /**
     * EXP-1309 (web `threadRowWord` ×4): each listed row's ONE quiet word off
     * the synced pull requests its root opened; a stack edge may cross
     * threads, so the row's whole TEAM's PRs are the stack context.
     */
    val words: StateFlow<Map<String, SessionTree.ThreadRowWord>> = combine(
        groups,
        dbFlow.scopedQuery(emptyList()) { it.pullRequestDao().observeAll() },
    ) { g, prs ->
        val byRoot = prs.filter { it.rootSessionId != null }.groupBy { it.rootSessionId }
        val byTeam = prs.groupBy { it.teamId }
        // EXP-1323: a nested child row carries no word (web + IDE).
        g.flatMap { it.rows }.filter { it.depth == 0 }.mapNotNull { row ->
            SessionTree.threadRowWord(
                busy = row.busy,
                needsYou = row.needsYou,
                prs = byRoot[row.sessionId].orEmpty(),
                teamPrs = byTeam[row.teamId].orEmpty(),
            )?.let { row.sessionId to it }
        }.toMap()
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
                // The threads themselves: nested children stay on the tab.
                .filter { it.depth == 0 && owner[it.sessionId] == me }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Every member team's agents (rail / Settings order). */
    val agents: StateFlow<List<SessionTree.AgentRow>> = sessions.map { SessionTree.listAgents(it, teamId = null) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The member teams, name order — the per-team bands. */
    val teams: StateFlow<List<TeamEntity>> = dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** EXP-1327: whether ANY thread runs (the tab's green dot, ×4). */
    val anyRunning: StateFlow<Boolean> = groups.map { list ->
        list.any { it.key == SessionTree.ThreadsGroupKey.Running && it.rows.isNotEmpty() }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** Whether ANY thread needs a person (the tab's amber dot). */
    val needsYou: StateFlow<Boolean> = groups.map { list ->
        list.any { it.key == SessionTree.ThreadsGroupKey.NeedsYou && it.rows.isNotEmpty() }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)
}

/** A Threads row's extras: the first linked issue's identifier, the root's
 *  agent (its brand mark) and the root's device (the trailing icon). */
data class ThreadRowMeta(
    val identifier: String?,
    val agent: String?,
    val device: ThreadRowDevice? = null,
)

/** EXP-1318: the synced device a root runs on — its name (the icon's
 *  label) and its icon pick, drawn like the Devices screen draws it. */
data class ThreadRowDevice(val name: String, val icon: String?, val isServer: Boolean)

/**
 * The extras each [SessionTree.ThreadsListRow] draws beside its title: the
 * mono identifier of the root's FIRST linked issue (thread order) and the
 * root's agent; the mark's state = [SessionTree.threadRowMarkState] (EXP-1312).
 */
fun threadRowMeta(
    groups: List<SessionTree.ThreadsListGroup>,
    links: List<SessionTree.SessionIssue>,
    issues: List<SessionTree.Issue>,
    agentByRoot: Map<String, String?>,
    /** Synced devices by steer `device_id`; a root on an unknown one draws none. */
    devices: Map<String, ThreadRowDevice> = emptyMap(),
): Map<String, ThreadRowMeta> {
    val issueById = issues.associateBy { it.id }
    val linksByRoot = links.groupBy { it.sessionId }
    return groups.flatMap { it.rows }.associate { row ->
        val first = linksByRoot[row.sessionId].orEmpty()
            .sortedWith(compareBy<SessionTree.SessionIssue>({ it.sortOrder }, { it.issueId }))
            .firstNotNullOfOrNull { issueById[it.issueId] }
        row.sessionId to ThreadRowMeta(
            first?.identifier,
            agentByRoot[row.sessionId],
            row.deviceId?.let { devices[it] },
        )
    }
}
