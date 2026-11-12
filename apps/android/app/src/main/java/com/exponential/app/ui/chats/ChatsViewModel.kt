package com.exponential.app.ui.chats

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.MergeTrain
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.DomainContract
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.domain.Chats
import com.exponential.app.domain.NeedsYou
import com.exponential.app.data.db.PullRequestEntity
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
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
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/**
 * EXP-1291 → session 58cb32df CHATS: the Chats TAB's model.
 * chats.json: ONE active team drives every phone screen (no team bands), so
 * [SessionTree.partitionChats] runs over the ACTIVE team
 * ([TeamSelection.selectedId]); the phone list = [Chats.chatList] over its
 * groups, the Agents section = [Chats.agentChatRows] over
 * [SessionTree.listAgents], the Apps section (#EXP-1733) = [Chats.appChatRows]
 * over the active team's apps, the Needs you entry and page = [NeedsYou.needsYou]. The menus' calls (Stop = `sessions.end` on the
 * root, Delete = `sessions.delete`, the agent's turn Stop = `sessions.stop`,
 * Compact / New conversation = the builtin lines through `sessions.send`)
 * live here too.
 */
@OptIn(ExperimentalCoroutinesApi::class)
/**
 * #PERF-9: what only the Chats LIST reads (the grouped rows and everything
 * derived from them) stops the moment the list leaves the screen: the usual
 * 5 s grace kept the whole list recomputing on every sync tick of the chat
 * just opened, on the cold open's busiest CPU. The state keeps its last
 * value, so returning shows the last list at once while it recomputes; the
 * nav dots ([SessionTree.chatsNavDots]) are the only derivation that runs
 * under an open chat.
 */
private val LIST_SHARING = SharingStarted.WhileSubscribed(stopTimeoutMillis = 0)

@HiltViewModel
class ChatsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val sessionsApi: SessionsApi,
    private val sender: SessionSender,
    private val agentsApi: com.exponential.app.data.api.AgentsApi,
    /** #EXP-1710: the Needs you stuck row's Retry / Merge now / Close PR. */
    private val pullRequestsApi: com.exponential.app.data.api.PullRequestsApi,
) : ViewModel() {

    /** S-3: the chats list makes sure the viewer has an Assistant (once per
     *  team per app run). */
    fun ensureAssistant() {
        val team = activeTeam.value
        viewModelScope.launch {
            com.exponential.app.data.api.AssistantEnsurer.ensure(agentsApi, auth.activeAccountId.value, team)
        }
    }

    /** chats.json: the ONE active team every phone list filters to. */
    private val activeTeam: StateFlow<String?> = selection.selectedId

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** #EXP-1714: the synced teams' ids — a session or conversation in none of
     *  them sits in the viewer's personal team (the global Assistant's chat). */
    private val syncedTeamIds: Flow<List<String>> =
        dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() }.map { rows -> rows.map { it.id } }

    // One shared query per table: the list, rowMeta, the nav dots and the
    // PR rows all read these, so they must not each re-run them (and the
    // digest parsing) on every sync tick.
    private val sessionRows = dbFlow.scopedQuery(emptyList()) { it.sessionDao().observeAll() }
        .flowOn(Dispatchers.Default)
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    private val pullRequestRows = dbFlow.scopedQuery(emptyList()) { it.pullRequestDao().observeAll() }
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    // EXP-1522: the list's inputs are mapped OFF the main thread and SHARED.
    // Six flows below read them; each used to re-run every query + mapping
    // (digest JSON parsing included) on the main thread per database write,
    // and a send writes a burst of rows right as the screen comes back.
    private val sessions = sessionRows.map { rows -> rows.map(SessionTree::session) }
        .flowOn(Dispatchers.Default)
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    private val inputs = combine(
        sessions,
        dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeAll() }.map { it.map(SessionTree::sessionIssue) },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() }.map { it.map(SessionTree::issue) },
        // #EXP-1380 / session e4a04d22: the ACTIVE team's session DIGESTS
        // (#EXP-1730: `sessions.digest`), never its messages — every list
        // row, preview, unread count and bubble ring reads the server-kept
        // digest turned back into messages (`Chats.digestMessages`,
        // chats.json `digestCases`), so no transcript loads here and a new
        // message re-reads one row.
        // #EXP-1714: plus the personal team's (the viewer's Assistant chat):
        // only its owner syncs it, so any session outside the synced teams is theirs.
        activeTeam.flatMapLatest { team ->
            if (team == null) {
                sessionRows
            } else {
                combine(sessionRows, syncedTeamIds) { rows, ids ->
                    rows.filter { it.teamId == team || (ids.isNotEmpty() && it.teamId !in ids) }
                }
            }
        }.map { rows ->
            rows.flatMap { row -> Chats.digestMessages(row.id, Chats.parseDigest(row.digest)) }
        },
        // EXP-1323: the viewer's read marks (bold rows + the nav dot).
        dbFlow.scopedQuery(emptyList()) { it.sessionReadDao().observeAll() }.map { it.map(SessionTree::sessionRead) },
    ) { s, links, issues, messages, reads -> Inputs(s, links, issues, messages, reads) }
        .flowOn(Dispatchers.Default)
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    private data class Inputs(
        val sessions: List<SessionTree.Session>,
        val links: List<SessionTree.SessionIssue>,
        val issues: List<SessionTree.Issue>,
        val messages: List<SessionTree.Message>,
        val reads: List<SessionTree.SessionRead>,
    )

    /** The grouped chat list of the ACTIVE team (chats.json: one team per
     *  phone screen); #EXP-1461: root rows only. */
    val groups: StateFlow<List<SessionTree.ChatsListGroup>> = combine(inputs, activeTeam) { input, team ->
        com.exponential.app.domain.perfTrace("ChatsViewModel.groups") { SessionTree.partitionChats(
            input.sessions,
            input.links,
            input.issues,
            input.messages,
            teamId = team,
            reads = input.reads,
            // NEW-1: Recent's 24 h window (read on every input change).
            now = SessionTree.nowIso(),
        ) }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyList())

    /**
     * #PERF-9: the Chats tab's three dots, derived on their own. The nav bar
     * collects them under an open chat, so off [groups] they kept the whole
     * list (titles included) recomputing on every sync tick of that chat;
     * [SessionTree.chatsNavDots] reads the same rule without building it and
     * emits only on a change.
     */
    private val dots: StateFlow<SessionTree.ChatsNavDots> = combine(inputs, activeTeam) { input, team ->
        com.exponential.app.domain.perfTrace("ChatsViewModel.dots") {
            SessionTree.chatsNavDots(
                input.sessions,
                input.messages,
                teamId = team,
                reads = input.reads,
                now = SessionTree.nowIso(),
            )
        }
    }.distinctUntilChanged().flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SessionTree.ChatsNavDots())

    /** #EXP-1388: the Chats nav dot (`chatsUnread`): an unread chat outside
     *  the viewer's archive (one that needs you counts anyway). */
    val unread: StateFlow<Boolean> = dots.map { it.unread }.distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** Each listed row's glyph + identifier, keyed by root session id. */
    val rowMeta: StateFlow<Map<String, ChatRowMeta>> = combine(
        groups,
        inputs,
        sessionRows,
        dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
    ) { g, input, rows, devices ->
        chatRowMeta(
            g,
            input.links,
            input.issues,
            rows.associate { it.id to it.cli },
            devices.associate { it.deviceId to ChatRowDevice(it.label, it.icon, it.kind == "server") },
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

    /**
     * EXP-1309 (web `chatRowWord` ×4): each listed row's ONE quiet word off
     * the synced pull requests its root opened; a stack edge may cross
     * chats, so the row's whole TEAM's PRs are the stack context.
     */
    val words: StateFlow<Map<String, SessionTree.ChatRowWord>> = combine(
        groups,
        pullRequestRows,
    ) { g, prs ->
        val byRoot = prs.filter { it.rootSessionId != null }.groupBy { it.rootSessionId }
        val byTeam = prs.groupBy { it.teamId }
        g.flatMap { it.rows }.mapNotNull { row ->
            SessionTree.chatRowWord(
                busy = row.busy,
                needsYou = row.needsYou,
                prs = byRoot[row.sessionId].orEmpty(),
                teamPrs = byTeam[row.teamId].orEmpty(),
                offline = row.offline,
            )?.let { row.sessionId to it }
        }.toMap()
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

    /**
     * NEW-41 (`chat-list.json`, web `rowLead`): each listed root's PR
     * glyph off the synced pull requests it opened, by root id: any open PR
     * (a stacked one too) = [ChatRowPrLead.Open], else any merged one =
     * [ChatRowPrLead.Merged]; none = absent.
     */
    val prLeads: StateFlow<Map<String, ChatRowPrLead>> = combine(
        groups,
        pullRequestRows,
    ) { g, prs ->
        val byRoot = prs.filter { it.rootSessionId != null }.groupBy { it.rootSessionId }
        g.flatMap { it.rows }.mapNotNull { row ->
            chatRowPrLead(byRoot[row.sessionId].orEmpty().map { it.state })?.let { row.sessionId to it }
        }.toMap()
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

    /**
     * The phone home's Running band (`composer-home.json`: phones keep their
     * Running list of LIVE OWN sessions; never Recent): the caller's own
     * chats in Needs you then Running, every team, list order.
     */
    val running: StateFlow<List<SessionTree.ChatsListRow>> = combine(groups, sessions, auth.userId) { g, s, me ->
        if (me == null) {
            emptyList()
        } else {
            val owner = s.associate { it.id to it.userId }
            g.filter { it.key == SessionTree.ChatsGroupKey.NeedsYou || it.key == SessionTree.ChatsGroupKey.Running }
                .flatMap { it.rows }
                .filter { owner[it.sessionId] == me }
        }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyList())

    private val agentDefinitions = dbFlow.scopedQuery(emptyList()) { it.agentDefinitionDao().observeAll() }
        .map { rows -> rows.map(SessionTree::agentDefinition) }

    /** NEW-2: every member team's agent DEFINITIONS (rail / Settings order);
     *  busy / needs-you off the viewer's own conversation session. */
    val agents: StateFlow<List<SessionTree.AgentRow>> = combine(
        agentDefinitions,
        sessions,
        auth.userId,
        activeTeam,
        // Its own query: `teams` below is initialised after this property.
        dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() },
    ) { defs, s, me, team, teamRows ->
        // #EXP-1714: `teamIds` brings the viewer's global Assistant along.
        SessionTree.listAgents(defs, s, teamId = team, me = me, teamIds = teamRows.map { it.id })
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

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
        pullRequestRows.map { prs ->
            prs.filter { it.rootSessionId != null }.groupBy { it.rootSessionId!! }.mapValues { (_, rows) -> rows.map { it.state } }
        }

    /** #FEED-191 (needs-you.json): every plan, question and ready PR waiting on
     *  the viewer in the ACTIVE team, newest first; web `useNeedsYou`'s input: a
     *  session named by THE session title, an agent conversation by its agent;
     *  only the digests' waiting kinds; the team's pull requests. */
    val needsYouRows: StateFlow<List<NeedsYou.Row>> = combine(
        combine(inputs, activeTeam, needsYouTeamFacts()) { input, team, facts -> Triple(input, team, facts) },
        agentDefinitions,
        pullRequestRows,
        auth.userId,
        minuteClock,
    ) { (input, team, facts), definitions, prs, me, now ->
        needsYouFold(
            input.sessions, input.links, input.issues, input.messages, definitions, prs, team, me, now,
            teamIds = facts.teamIds,
            mergeMode = facts.mergeMode,
            owner = facts.owner,
            devices = facts.devices,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyList())

    /** #EXP-1710: what the Needs you fold reads beyond the list's inputs. */
    private data class NeedsYouTeamFacts(
        val mergeMode: String,
        val owner: Boolean,
        val devices: List<DeviceEntity>,
        /** #EXP-1714: the synced teams' ids. */
        val teamIds: List<String> = emptyList(),
    )

    /** The ACTIVE team's merge mode (no active team = `auto`), whether the
     *  viewer owns it, and the synced devices (a stuck row's offline clock). */
    private fun needsYouTeamFacts(): Flow<NeedsYouTeamFacts> = combine(activeTeam, auth.userId) { team, me -> team to me }
        .flatMapLatest { (team, me) ->
            combine(
                dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() },
                if (team == null) flowOf(emptyList()) else dbFlow.scopedQuery(emptyList()) { it.teamMemberDao().observeByTeam(team) },
                dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
            ) { teams, members, devices ->
                NeedsYouTeamFacts(
                    mergeMode = teams.firstOrNull { it.id == team }?.mergeMode ?: MergeTrain.MODE_AUTO,
                    owner = me != null && members.any { it.userId == me && it.role == "owner" },
                    devices = devices,
                    teamIds = teams.map { it.id },
                )
            }
        }

    /** #EXP-1710: the synced pull requests a Needs you PR row acts on (the
     *  row's PR + its team's stack pool for THE pill's confirm and Merge now). */
    val needsYouPullRequests: StateFlow<List<PullRequestEntity>> =
        pullRequestRows.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** #EXP-1710: the Needs you stuck row's Retry (= Resume auto-merge). */
    fun retryPullRequest(pullRequestId: String) =
        call("resume", thing = "auto-merge") { pullRequestsApi.hold(it, pullRequestId, false) }

    /** #EXP-1710: the stuck row's Merge now (owners, after `merge-now-pr`). */
    fun mergePullRequestNow(pullRequestId: String) =
        call("merge", thing = "the PR") { pullRequestsApi.mergeNow(it, pullRequestId) }

    /** #EXP-1710: how many issues each PR links (close-pr.json's batchLine
     *  counts the OTHER ones). */
    fun linkedIssueCount(pullRequestId: String): Flow<Int> =
        dbFlow.scopedQuery(emptyList()) { it.pullRequestIssueDao().observeByPullRequest(pullRequestId) }.map { it.size }

    /** #EXP-1710: the stuck row's Close PR (after close-pr.json's confirm). */
    fun closePullRequest(pullRequestId: String) =
        call("close", thing = "the PR") { pullRequestsApi.close(it, pullRequestId) }

    /** chats.json `chatList`: the phone list (roots by activity, the preview,
     *  the time, the unread count); #FEED-191: no chips, the screen groups it
     *  under [Chats.chatSections]. */
    val chatList: StateFlow<Chats.ChatList> = combine(
        combine(groups, inputs) { g, i -> g to i },
        prStates,
        words,
        _view,
        minuteClock,
    ) { (g, input), states, w, view, now ->
        Chats.chatList(
            groups = g,
            messages = input.messages,
            reads = input.reads,
            prStates = states,
            words = w,
            view = view,
            now = SessionTree.nowIso(now),
            utcOffsetMinutes = Chats.deviceUtcOffsetMinutes(now),
            // #EXP-1399 review: an ask mirror of a session I cannot see shows no text.
            visible = input.sessions.mapTo(HashSet()) { it.id },
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, Chats.ChatList(emptyList()))

    /** S-3 (chats.json `agentChatRows`): the AGENT rows pinned on top of the
     *  list, [agents] order (the viewer's Assistant first); none on the
     *  Archived page. */
    val agentRows: StateFlow<List<Chats.AgentChatRow>> = combine(
        combine(agents, inputs) { a, i -> a to i },
        auth.userId,
        _view,
        minuteClock,
    ) { (rows, input), me, view, now ->
        Chats.agentChatRows(
            agents = rows,
            sessions = input.sessions,
            messages = input.messages,
            reads = input.reads,
            me = me,
            view = view,
            now = SessionTree.nowIso(now),
            utcOffsetMinutes = Chats.deviceUtcOffsetMinutes(now),
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyList())

    /** #EXP-1733: the ACTIVE team's live apps (the dao hides trashed rows). */
    private val teamApps: Flow<List<Chats.AppChatInput>> = activeTeam.flatMapLatest { team ->
        if (team == null) {
            flowOf(emptyList())
        } else {
            dbFlow.scopedQuery(emptyList()) { it.appDao().observeByTeam(team) }
        }
    }.map { rows ->
        rows.map { Chats.AppChatInput(appId = it.id, slug = it.slug, name = it.name, icon = it.icon, color = it.color) }
    }

    /** #EXP-1733 (chats.json `appChatRows`): the APP rows between the Agents
     *  and the sessions, by name; none on the Archived page. */
    val appRows: StateFlow<List<Chats.AppChatRow>> = combine(
        combine(teamApps, inputs) { a, i -> a to i },
        auth.userId,
        _view,
        minuteClock,
    ) { (apps, input), me, view, now ->
        Chats.appChatRows(
            apps = apps,
            sessions = input.sessions,
            messages = input.messages,
            reads = input.reads,
            me = me,
            view = view,
            now = SessionTree.nowIso(now),
            utcOffsetMinutes = Chats.deviceUtcOffsetMinutes(now),
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyList())

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
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

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
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

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
            try {
                // No account = a refusal like any other (it used to return
                // silently: the confirm closed and nothing happened).
                val accountId = auth.activeAccountId.value ?: throw NoActiveAccount()
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

    /** Delete = `sessions.delete` (the server ends what is live, then
     *  deletes the tree). S-3: no Stop. */
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

    /** Delete = `sessions.delete` per chat (after the confirm). */
    fun deleteChats(rootIds: List<String>) = selectionCall { accountId -> eachChat(rootIds) { sessionsApi.delete(accountId, it) } }

    /** Compact / New conversation = the builtin line, sent like a typed one
     *  into the viewer's conversation with [agentId]. */
    fun sendToAgent(agentId: String, text: String) = call("send", thing = "the message") {
        sender.send(SessionsSendInput.toAgent(agentId, text))
    }

    /** NEW-2: the viewer's conversation session per agent id (a row opens it). */
    val conversations: StateFlow<Map<String, String>> = combine(sessions, auth.userId) { s, me ->
        SessionTree.conversations(s, me)
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /** The member teams, name order — the per-team bands. */
    val teams: StateFlow<List<TeamEntity>> = dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** EXP-1327: whether ANY chat runs (the tab's green dot, ×4). */
    val anyRunning: StateFlow<Boolean> = dots.map { it.running }.distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** Whether ANY chat needs a person (the tab's amber dot). */
    val needsYou: StateFlow<Boolean> = dots.map { it.needsYou }.distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)
}

/** #FEED-191: [NeedsYou.needsYou] over the synced rows of [team] (null = every
 *  team, while none is active). */
internal fun needsYouFold(
    sessions: List<SessionTree.Session>,
    links: List<SessionTree.SessionIssue>,
    issues: List<SessionTree.Issue>,
    messages: List<SessionTree.Message>,
    definitions: List<SessionTree.AgentDefinition>,
    pullRequests: List<PullRequestEntity>,
    team: String?,
    me: String?,
    now: Long,
    /** #EXP-1714: the synced teams' ids — the viewer's own sessions in none
     *  of them (their personal team: the global Assistant) count in every team. */
    teamIds: Collection<String> = emptyList(),
    /** #EXP-1710: the team's contract teamMergeMode. */
    mergeMode: String = MergeTrain.MODE_AUTO,
    owner: Boolean = false,
    /** #EXP-1710: the synced devices (a stuck row's `Device offline`). */
    devices: List<DeviceEntity> = emptyList(),
): List<NeedsYou.Row> {
    val inTeam = sessions.filter {
        team == null || com.exponential.app.domain.AgentSettings.Assistant.inTeamOrPersonal(it.teamId, it.userId, team, teamIds, me)
    }
    val ids = inTeam.mapTo(HashSet()) { it.id }
    val titles = SessionTree.sessionTitles(sessions, links, issues, messages)
    val agentNames = definitions.associate { it.id to it.name }
    val sessionsById = sessions.associateBy { it.id }
    val devicesById = devices.associateBy { it.deviceId }
    return NeedsYou.needsYou(
        now = now,
        me = me,
        sessions = inTeam.map { session ->
            NeedsYou.SessionInput(
                id = session.id,
                rootId = session.rootId,
                name = session.agentId?.let(agentNames::get) ?: titles[session.id] ?: session.title.orEmpty(),
                status = session.status,
                agentId = session.agentId,
                userId = session.userId,
                busy = session.busy,
            )
        },
        messages = messages.mapNotNull { message ->
            val kind = (message.card?.get("kind") as? JsonPrimitive)?.contentOrNull
            if (message.sessionId !in ids || kind !in WAITING_KINDS) return@mapNotNull null
            NeedsYou.MessageInput(
                id = message.id,
                sessionId = message.sessionId,
                authorSessionId = message.authorSessionId,
                body = message.body,
                createdAt = message.createdAt,
                answeredAt = message.answeredAt,
                card = message.card,
            )
        },
        pullRequests = pullRequests.filter { team == null || it.teamId == team }.map { pr ->
            NeedsYou.PullRequestInput(
                id = pr.id,
                rootSessionId = pr.rootSessionId,
                number = pr.number?.toLong() ?: 0L,
                title = pr.title.orEmpty(),
                state = pr.state,
                checksState = pr.checksState,
                basePullRequestId = pr.basePullRequestId,
                createdAt = pr.createdAt,
                mergeState = MergeTrain.prMergeState(pr.state, pr.mergeState),
                holdReason = pr.mergeHold,
                deviceOfflineSince = openerOfflineSince(pr, sessionsById, devicesById, now),
            )
        },
        mergeMode = mergeMode,
        owner = owner,
    )
}

/**
 * #EXP-1710 (merge-train.json `offlineSince`, [MergeTrain.offlineSince]): the
 * opener = [pr]'s `sessionId` session, else (none, or not synced) its root; no opener (or one not
 * synced, or without a device) = null; its device online by THE presence
 * ([DeviceLiveness]) = null; else the row's `last_seen_at`, and an UNKNOWN
 * last-seen (no device row synced, or no heartbeat) = the PR's `updatedAt`
 * (offline since the PR went conflict/failing), never online.
 */
internal fun openerOfflineSince(
    pr: PullRequestEntity,
    sessions: Map<String, SessionTree.Session>,
    devices: Map<String, DeviceEntity>,
    now: Long,
): String? {
    val opener = pr.sessionId?.let { sessions[it] } ?: pr.rootSessionId?.let { sessions[it] }
    val deviceId = opener?.deviceId?.takeIf { it.isNotEmpty() }
    val row = deviceId?.let { devices[it] }
    val seen = row?.lastSeenAt?.takeIf { it.isNotBlank() && WireTimestamps.parseEpochMs(it) != null }
    return MergeTrain.offlineSince(
        opener = deviceId != null,
        device = row?.let { MergeTrain.OpenerDevice(online = DeviceLiveness.isOnline(seen, now), lastSeenAt = seen) },
        updatedAt = pr.updatedAt.ifBlank { pr.createdAt },
    )
}

/** The digests' card kinds a person acts on. */
private val WAITING_KINDS = setOf("plan", "question", "ask")

/** The thing every chat menu call names (`Couldn't stop the chat`). */
internal const val CHAT_THING = "the chat"

/** #EXP-1388 `copy.toasts.selectionFailed` = `Couldn't update every chat`. */
internal const val SELECTION_VERB = "update"
internal const val SELECTION_THING = "every chat"

/** `sessions.setRead` / `setArchived` take up to 200 ids per call. */
private const val SELECTION_BATCH = 200

/** A Chats row's extras: the first linked issue's identifier, the root's
 *  agent (its brand mark) and the root's device (the trailing icon). */
/** NEW-41: the PR glyph a Chats-tab row's lead wears (web `rowLead`). */
enum class ChatRowPrLead { Open, Merged }

/** NEW-41 (web `rowLead`): `prs.some(open)` → emerald, else
 *  `prs.some(merged)` → violet, else none. */
fun chatRowPrLead(states: List<String>): ChatRowPrLead? = when {
    states.any { it == DomainContract.prStateOpen } -> ChatRowPrLead.Open
    states.any { it == DomainContract.prStateMerged } -> ChatRowPrLead.Merged
    else -> null
}

data class ChatRowMeta(
    val identifier: String?,
    val agent: String?,
    val device: ChatRowDevice? = null,
)

/** EXP-1318: the synced device a root runs on — its name (the icon's
 *  label) and its icon pick, drawn like the Devices screen draws it. */
data class ChatRowDevice(val name: String, val icon: String?, val isServer: Boolean)

/**
 * The extras each [SessionTree.ChatsListRow] draws beside its title: the
 * mono identifier of the root's FIRST linked issue (chat order) and the
 * root's agent; the mark's state = [SessionTree.chatRowMarkState] (EXP-1312).
 */
fun chatRowMeta(
    groups: List<SessionTree.ChatsListGroup>,
    links: List<SessionTree.SessionIssue>,
    issues: List<SessionTree.Issue>,
    agentByRoot: Map<String, String?>,
    /** Synced devices by steer `device_id`; a root on an unknown one draws none. */
    devices: Map<String, ChatRowDevice> = emptyMap(),
): Map<String, ChatRowMeta> {
    val issueById = issues.associateBy { it.id }
    val linksByRoot = links.groupBy { it.sessionId }
    return groups.flatMap { it.rows }.associate { row ->
        val first = linksByRoot[row.sessionId].orEmpty()
            .sortedWith(compareBy<SessionTree.SessionIssue>({ it.sortOrder }, { it.issueId }))
            .firstNotNullOfOrNull { issueById[it.issueId] }
        row.sessionId to ChatRowMeta(
            first?.identifier,
            agentByRoot[row.sessionId],
            row.deviceId?.let { devices[it] },
        )
    }
}
