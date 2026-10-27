package com.exponential.app.ui.threads

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.DomainContract
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.domain.Chats
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.steer.SessionSender
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * EXP-1291 Threads v2 → thread 58cb32df CHATS: the Chats TAB's model.
 * chats.json: ONE active team drives every phone screen (no team bands), so
 * [SessionTree.partitionThreads] runs over the ACTIVE team
 * ([TeamSelection.selectedId]); the phone list = [Chats.chatList] over its
 * groups, the header's agent bubbles = [Chats.agentBubbles] over
 * [SessionTree.listAgents]. The menus' calls (Stop = `sessions.end` on the
 * root, Delete = `sessions.delete`, the agent's turn Stop = `sessions.stop`,
 * Compact / New conversation = the builtin lines through `sessions.send`)
 * live here too.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ThreadsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val sessionsApi: SessionsApi,
    private val sender: SessionSender,
) : ViewModel() {

    /** chats.json: the ONE active team every phone list filters to. */
    private val activeTeam: StateFlow<String?> = selection.selectedId

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val sessionRows = dbFlow.scopedQuery(emptyList()) { it.sessionDao().observeAll() }

    private val sessions = sessionRows.map { rows -> rows.map(SessionTree::session) }

    private val inputs = combine(
        sessions,
        dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeAll() }.map { it.map(SessionTree::sessionIssue) },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() }.map { it.map(SessionTree::issue) },
        // #EXP-1380 / thread e4a04d22: the ACTIVE team's conversation
        // DIGESTS, never its messages — every list row, preview, unread count
        // and bubble ring reads the server-kept digest turned back into
        // messages (`Chats.digestMessages`, chats.json `digestCases`), so no
        // transcript loads here and a new message re-reads one row.
        activeTeam.flatMapLatest { team ->
            dbFlow.scopedQuery(emptyList()) {
                if (team == null) it.conversationDao().observeAll() else it.conversationDao().observeByTeam(team)
            }
        }.map { rows ->
            rows.flatMap { row ->
                val session = row.sessionId ?: return@flatMap emptyList()
                Chats.digestMessages(session, Chats.parseDigest(row.digest))
            }
        },
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

    /** The grouped thread list of the ACTIVE team (chats.json: one team per
     *  phone screen); EXP-1323: a live tree's children nest under their root
     *  (depth > 0; the phone list keeps the roots only). */
    val groups: StateFlow<List<SessionTree.ThreadsListGroup>> = combine(inputs, activeTeam) { input, team ->
        SessionTree.partitionThreads(
            input.sessions,
            input.links,
            input.issues,
            input.messages,
            teamId = team,
            reads = input.reads,
            // NEW-1: Recent's 24 h window (read on every input change).
            now = SessionTree.nowIso(),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** #EXP-1388: the Chats nav dot (`chatsUnread`): an unread chat outside
     *  the viewer's archive (one that needs you counts anyway). */
    val unread: StateFlow<Boolean> = combine(groups, inputs) { g, input -> Chats.chatsUnread(g, input.reads) }
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
     * NEW-41 (`threads-list.json`, web `rowLead`): each listed root's PR
     * glyph off the synced pull requests it opened, by root id: any open PR
     * (a stacked one too) = [ThreadRowPrLead.Open], else any merged one =
     * [ThreadRowPrLead.Merged]; none = absent.
     */
    val prLeads: StateFlow<Map<String, ThreadRowPrLead>> = combine(
        groups,
        dbFlow.scopedQuery(emptyList()) { it.pullRequestDao().observeAll() },
    ) { g, prs ->
        val byRoot = prs.filter { it.rootSessionId != null }.groupBy { it.rootSessionId }
        g.flatMap { it.rows }.filter { it.depth == 0 }.mapNotNull { row ->
            threadRowPrLead(byRoot[row.sessionId].orEmpty().map { it.state })?.let { row.sessionId to it }
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

    private val agentDefinitions = dbFlow.scopedQuery(emptyList()) { it.agentDefinitionDao().observeAll() }
        .map { rows -> rows.map(SessionTree::agentDefinition) }

    /** NEW-2: every member team's agent DEFINITIONS (rail / Settings order);
     *  busy / needs-you off the viewer's own conversation session. */
    val agents: StateFlow<List<SessionTree.AgentRow>> = combine(agentDefinitions, sessions, auth.userId, activeTeam) { defs, s, me, team ->
        SessionTree.listAgents(defs, s, teamId = team, me = me)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** chats.json `agentBubbles`: the header's bubbles (ring = needs you /
     *  unread), the active team's agents in list order. */
    val bubbles: StateFlow<List<Chats.AgentBubble>> = combine(agents, inputs, auth.userId) { rows, input, me ->
        Chats.agentBubbles(rows, input.sessions, input.messages, input.reads, me)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _filter = MutableStateFlow(Chats.ChatFilter.All)
    /** The selected filter chip. */
    val filter: StateFlow<Chats.ChatFilter> = _filter

    fun selectFilter(filter: Chats.ChatFilter) {
        _filter.value = filter
    }

    private val _view = MutableStateFlow(Chats.ChatListView.List)
    /** #EXP-1388: the chats, or the Archived page (its own pushed route). */
    val view: StateFlow<Chats.ChatListView> = _view

    fun setView(view: Chats.ChatListView) {
        _view.value = view
    }

    /** The list's clock: the time labels re-read once a minute. */
    private val minuteClock: Flow<Long> = flow {
        while (true) {
            emit(System.currentTimeMillis())
            delay(60_000)
        }
    }

    /** Each root's PR states (the lead's open / merged glyph). */
    private val prStates: Flow<Map<String, List<String>>> =
        dbFlow.scopedQuery(emptyList()) { it.pullRequestDao().observeAll() }.map { prs ->
            prs.filter { it.rootSessionId != null }.groupBy { it.rootSessionId!! }.mapValues { (_, rows) -> rows.map { it.state } }
        }

    /** chats.json `chatList`: the phone list (flat roots by activity, the
     *  preview, the time, the unread count) under the selected chip. */
    val chatList: StateFlow<Chats.ChatList> = combine(
        combine(groups, inputs) { g, i -> g to i },
        prStates,
        words,
        combine(_filter, _view) { f, v -> f to v },
        minuteClock,
    ) { (g, input), states, w, (filter, view), now ->
        Chats.chatList(
            groups = g,
            messages = input.messages,
            reads = input.reads,
            prStates = states,
            words = w,
            filter = filter,
            view = view,
            now = SessionTree.nowIso(now),
            utcOffsetMinutes = Chats.deviceUtcOffsetMinutes(now),
            // #EXP-1399 review: an ask mirror of a session I cannot see shows no text.
            visible = input.sessions.mapTo(HashSet()) { it.id },
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), Chats.ChatList(emptyList(), Chats.ChatCounts(0, 0, 0)))

    /** What a chat row's menu needs per root: its status, whether the
     *  viewer may steer it (shared, or the starter) and whether the viewer
     *  started it (with [isOwner], who may delete it). */
    data class RootFacts(val status: String, val canSteer: Boolean, val agent: String?, val mine: Boolean = false)

    val rootFacts: StateFlow<Map<String, RootFacts>> = combine(sessionRows, auth.userId) { rows, me ->
        rows.filter { it.isRoot }.associate { row ->
            row.id to RootFacts(
                row.status,
                row.shared || (me != null && row.userId == me),
                row.cli,
                mine = me != null && row.userId == me,
            )
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /** The viewer owns the active team (the agent menu's Agent settings). */
    val isOwner: StateFlow<Boolean> = combine(activeTeam, auth.userId) { team, me -> team to me }
        .flatMapLatest { (team, me) ->
            if (team == null || me == null) {
                flowOf(false)
            } else {
                dbFlow.scopedQuery(emptyList()) { it.teamMemberDao().observeByTeam(team) }
                    .map { members -> members.any { it.userId == me && it.role == "owner" } }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    // ── the menus' calls (chats.json MENUS) ─────────────────────────────────

    private val _failure = MutableStateFlow<FailedAction?>(null)
    /** A refused call, raised once as the fixture's toast (`copy.toasts`). */
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    private val _notice = MutableStateFlow<String?>(null)
    /** chats.json `copy.toasts.stopNotDelivered` (a stop no device got). */
    val notice: StateFlow<String?> = _notice

    fun clearNotice() {
        _notice.value = null
    }

    private fun call(
        verb: String,
        onDone: () -> Unit = {},
        thing: String = CHAT_THING,
        block: suspend (accountId: String) -> Unit,
    ) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                block(accountId)
                onDone()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                // `Couldn't stop the chat` / `Couldn't delete the chat`;
                // a partial Stop / Delete = `{n} of {m} chats couldn't be updated`.
                val partial = t as? PartialSelectionFailure
                _failure.value = FailedAction(
                    verb,
                    thing,
                    trpcErrorMessage(partial?.first ?: t, ""),
                    title = partial?.let { Chats.chatSelectionFailedTitle(it.failed, it.total) },
                )
            }
        }
    }

    /** Stop = `sessions.end` on the ROOT (the whole tree ends). */
    fun stopChat(rootId: String) = call("stop") { sessionsApi.end(it, rootId) }

    /** Stop and delete / Delete = `sessions.delete` (the server ends what is
     *  live, then deletes the tree). */
    fun deleteChat(rootId: String, onDeleted: () -> Unit = {}) =
        call("delete", onDeleted) { sessionsApi.delete(it, rootId) }

    // ── the selection bar's calls (chats.json SELECTION + ARCHIVE) ─────────

    /** One selection action; any refusal = `copy.toasts.selectionFailed`. */
    private fun selectionCall(block: suspend (accountId: String) -> Unit) =
        call(SELECTION_VERB, thing = SELECTION_THING, block = block)

    /** Mark read / unread = `sessions.setRead` (one batch of up to 200). */
    fun setRead(rootIds: List<String>, read: Boolean) = selectionCall { accountId ->
        rootIds.chunked(SELECTION_BATCH).forEach { sessionsApi.setRead(accountId, it, read) }
    }

    /** Archive / Unarchive = `sessions.setArchived` (per viewer). */
    fun setArchived(rootIds: List<String>, archived: Boolean) = selectionCall { accountId ->
        rootIds.chunked(SELECTION_BATCH).forEach { sessionsApi.setArchived(accountId, it, archived) }
    }

    /** Stop = `sessions.end` per chat (every one tried, then one toast). */
    fun stopChats(rootIds: List<String>) = selectionCall { accountId -> each(rootIds) { sessionsApi.end(accountId, it) } }

    /** Delete = `sessions.delete` per chat (after the confirm). */
    fun deleteChats(rootIds: List<String>) = selectionCall { accountId -> each(rootIds) { sessionsApi.delete(accountId, it) } }

    /** One chat at a time; every one is tried, then the failures counted. */
    private suspend fun each(ids: List<String>, block: suspend (String) -> Unit) {
        var first: Throwable? = null
        var failed = 0
        for (id in ids) {
            try {
                block(id)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                first = first ?: t
                failed += 1
            }
        }
        first?.let { throw PartialSelectionFailure(failed, ids.size, it) }
    }

    /** The agent menu's Stop = `sessions.stop` (the TURN interrupt). */
    fun stopTurn(sessionId: String) = call("stop") { accountId ->
        val result = sessionsApi.stop(accountId, sessionId)
        if (!result.delivered) _notice.value = Chats.Copy.TOAST_STOP_NOT_DELIVERED
    }

    /** Compact / New conversation = the builtin line, sent like a typed one
     *  into the viewer's conversation with [agentId]. */
    fun sendToAgent(agentId: String, text: String) = call("send", thing = "the message") {
        sender.send(SessionsSendInput.toAgent(agentId, text))
    }

    /** NEW-2: the viewer's conversation session per agent id (a row opens it). */
    val conversations: StateFlow<Map<String, String>> = combine(sessions, auth.userId) { s, me ->
        SessionTree.conversations(s, me)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

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

/** The thing every chat menu call names (`Couldn't stop the chat`). */
internal const val CHAT_THING = "the chat"

/** #EXP-1388 `copy.toasts.selectionFailed` = `Couldn't update every chat`. */
internal const val SELECTION_VERB = "update"
internal const val SELECTION_THING = "every chat"

/** A Stop / Delete selection that failed for [failed] of its [total] chats. */
private class PartialSelectionFailure(val failed: Int, val total: Int, val first: Throwable) : Exception(first)

/** `sessions.setRead` / `setArchived` take up to 200 ids per call. */
private const val SELECTION_BATCH = 200

/** A Threads row's extras: the first linked issue's identifier, the root's
 *  agent (its brand mark) and the root's device (the trailing icon). */
/** NEW-41: the PR glyph a Threads-tab row's lead wears (web `rowLead`). */
enum class ThreadRowPrLead { Open, Merged }

/** NEW-41 (web `rowLead`): `prs.some(open)` → emerald, else
 *  `prs.some(merged)` → violet, else none. */
fun threadRowPrLead(states: List<String>): ThreadRowPrLead? = when {
    states.any { it == DomainContract.prStateOpen } -> ThreadRowPrLead.Open
    states.any { it == DomainContract.prStateMerged } -> ThreadRowPrLead.Merged
    else -> null
}

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
