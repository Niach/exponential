package com.exponential.app.ui.chats

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.MergeTrain
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.DomainContract
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.domain.Chats
import com.exponential.app.domain.NavBadges
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
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/**
 * EXP-1291 → session 58cb32df CHATS: the Chats TAB's model.
 * #EXP-2109 (chats.json): no active team, so [SessionTree.partitionChats] runs
 * over every member team; the phone list = [Chats.chatList] over its
 * groups, the Agents section = [Chats.agentChatRows] over
 * [SessionTree.listAgents], the Apps section (#EXP-1733) = [Chats.appChatRows]
 * over every member team's apps, the Needs you entry and page = [needsYouRead]. The menus' calls (Stop = `sessions.end` on the
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
 * nav badge ([SessionTree.chatsNavBadge]) is the only derivation that runs
 * under an open chat.
 */
private val LIST_SHARING = SharingStarted.WhileSubscribed(stopTimeoutMillis = 0)

@HiltViewModel
class ChatsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val sessionsApi: SessionsApi,
    private val sender: SessionSender,
    private val agentsApi: com.exponential.app.data.api.AgentsApi,
    /** #EXP-1710: the Needs you stuck row's Retry / Merge now / Close PR. */
    private val pullRequestsApi: com.exponential.app.data.api.PullRequestsApi,
) : ViewModel() {

    /** S-3: the chats list makes sure the viewer has an Assistant (once
     *  per app run; #EXP-2109: no active team). */
    fun ensureAssistant() {
        viewModelScope.launch {
            com.exponential.app.data.api.AssistantEnsurer.ensureForAccount(agentsApi, auth.activeAccountId.value)
        }
    }

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val ownedTeams: Flow<Set<String>> =
        com.exponential.app.data.ownedTeamIdsFlow(dbFlow, auth.userId)

    // One shared query per table: the list, the nav badges and the
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
    // #ERR-28: only a changed row is mapped (and its guide / flags parsed) again.
    private val sessionCache = com.exponential.app.data.db.MappedRows(SessionEntity::id, SessionTree::session)
    private val sessions = sessionRows.map { rows -> sessionCache.read(rows) }
        .flowOn(Dispatchers.Default)
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    // #ERR-23: every session's digest turned back into messages ONCE per
    // sessions read (session id → its messages): the list's team slice and
    // the per-team badges both read it. Each parsed it on its own before,
    // which kept two to four copies of every digest's messages (and their
    // card JSON trees) on a large account.
    // #ERR-28: only a CHANGED digest is parsed again (Chats.DigestMessagesCache).
    private val digestCache = Chats.DigestMessagesCache()
    private val digestMessagesById = sessionRows.map { rows ->
        digestCache.read(rows.map { it.id to it.digest })
    }
        .flowOn(Dispatchers.Default)
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    private val issueCache = com.exponential.app.data.db.MappedRows<com.exponential.app.data.db.IssueEntity, SessionTree.Issue>(
        { it.id },
        { SessionTree.issue(it) },
    )

    private val inputs = combine(
        sessions,
        dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeAll() }.map { it.map(SessionTree::sessionIssue) },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() }.map { issueCache.read(it) },
        // #EXP-1380 / session e4a04d22: the session DIGESTS (#EXP-1730:
        // `sessions.digest`), never the messages — every list row, preview,
        // unread count and bubble ring reads the server-kept digest turned
        // back into messages (`Chats.digestMessages`, chats.json
        // `digestCases`), so no transcript loads here and a new message
        // re-reads one row. #EXP-2109: every member team's, plus the
        // viewer's personal team's (the Assistant's chat).
        sessionRows.combine(digestMessagesById) { rows, byId ->
            rows.flatMap { row -> byId[row.id].orEmpty() }
        },
        // EXP-1323: the viewer's read marks (bold rows + the badges).
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

    /** The grouped chat list of EVERY member team (chats.json PHONE SHELL);
     *  #EXP-1461: root rows only. */
    val groups: StateFlow<List<SessionTree.ChatsListGroup>> = inputs.map { input ->
        com.exponential.app.domain.perfTrace("ChatsViewModel.groups") { SessionTree.partitionChats(
            input.sessions,
            input.links,
            input.issues,
            input.messages,
            teamId = null,
            reads = input.reads,
            // NEW-1: Recent's 24 h window (read on every input change).
            now = SessionTree.nowIso(),
        ) }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyList())

    /**
     * #PERF-9 / #FEED-232: the Chats tab's count badge (`chatsBadge`), derived
     * on its own: the nav bar collects it under an open chat, so off [groups]
     * it kept the whole list recomputing on every sync tick of that chat;
     * [SessionTree.chatsNavBadge] reads the same rule without building it and
     * emits only on a change.
     */
    val chatsBadge: StateFlow<NavBadges.Badge> = inputs.map { input ->
        com.exponential.app.domain.perfTrace("ChatsViewModel.chatsBadge") {
            SessionTree.chatsNavBadge(
                input.sessions,
                input.messages,
                teamId = null,
                reads = input.reads,
                now = SessionTree.nowIso(),
            )
        }
    }.distinctUntilChanged().flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), NavBadges.NONE)

    /**
     * EXP-1309 (web `chatRowWord` ×4): each listed row's ONE quiet word off
     * the synced pull requests its root opened; a stack edge may cross
     * chats, so the row's whole TEAM's PRs are the stack context.
     */
    /**
     * #NEW-49 (chat-list.json `waiting`): what each listed IDLE root waits on
     * ([SessionTree.sessionWaiting]: its live children by the names their
     * cards show, then its watches), by root id; absent = nothing.
     */
    val waiting: StateFlow<Map<String, String>> = combine(groups, inputs) { g, input ->
        val roots = g.flatMap { it.rows }.filter { it.depth == 0 && !it.busy && !it.needsYou }.map { it.sessionId }.toSet()
        if (roots.isEmpty()) return@combine emptyMap()
        val byId = input.sessions.associateBy { it.id }
        // #ERR-28: the rows indexed once for every child's name, never per child.
        val titleOf by lazy { SessionTree.titler(input.links, input.issues, input.messages) }
        roots.mapNotNull { id ->
            val root = byId[id] ?: return@mapNotNull null
            SessionTree.sessionWaitingIn(root, input.sessions) { child -> titleOf(child) }?.let { id to it }
        }.toMap()
    }.distinctUntilChanged().flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

    val words: StateFlow<Map<String, SessionTree.ChatRowWord>> = combine(
        groups,
        pullRequestRows,
        waiting,
    ) { g, prs, waits ->
        val byRoot = prs.filter { it.rootSessionId != null }.groupBy { it.rootSessionId }
        val byTeam = prs.groupBy { it.teamId }
        g.flatMap { it.rows }.mapNotNull { row ->
            SessionTree.chatRowWord(
                busy = row.busy,
                needsYou = row.needsYou,
                prs = byRoot[row.sessionId].orEmpty(),
                teamPrs = byTeam[row.teamId].orEmpty(),
                wall = row.wall != null,
                waiting = waits[row.sessionId],
            )?.let { row.sessionId to it }
        }.toMap()
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

    /** #FEED-263: each listed row's wall (kind, its device's name) by root id. */
    val walls: StateFlow<Map<String, Pair<String, String?>>> = combine(
        groups,
        dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
    ) { g, devices ->
        val names = devices.associate { it.deviceId to it.label.takeIf { label -> label.isNotBlank() } }
        g.flatMap { it.rows }.mapNotNull { row ->
            row.wall?.let { wall -> row.sessionId to (wall.kind to wall.deviceId?.let { names[it] }) }
        }.toMap()
    }.distinctUntilChanged().flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

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
    ) { defs, s, me ->
        SessionTree.listAgents(defs, s, teamId = null, me = me)
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
     *  the viewer, ONE read over every member team and the Assistant's home
     *  ([needsYouRead]): the list, its entry and the team rows alike. */
    val needsYou: StateFlow<NeedsYouRead> = combine(
        inputs,
        agentDefinitions,
        pullRequestRows,
        combine(
            dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() },
            ownedTeams,
            dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
            ::Triple,
        ),
        combine(auth.userId, minuteClock, ::Pair),
    ) { input, definitions, prs, (teamRows, owned, devices), (me, now) ->
        com.exponential.app.domain.perfTrace("ChatsViewModel.needsYou") {
            needsYouRead(
                sessions = input.sessions,
                links = input.links,
                issues = input.issues,
                messages = input.messages,
                definitions = definitions,
                pullRequests = prs,
                teams = teamRows,
                owned = owned,
                devices = devices,
                me = me,
                now = now,
            )
        }
    }.distinctUntilChanged().flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, NeedsYouRead())

    /** Every conversation's `rowBadge` by root session id ([Chats.rowBadges]):
     *  the agent rows, the placed session rows, the dock's Assistant disc. */
    val rowBadges: StateFlow<Map<String, NavBadges.Badge>> = inputs
        .map { Chats.rowBadges(it.sessions, it.messages, it.reads) }
        .distinctUntilChanged().flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

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
    /** #EXP-1990 (chats.json `chatPreview`): the viewer, and every synced
     *  member's name by id: a teammate's line reads under their name. */
    private val viewerAndMembers: Flow<Pair<String?, Map<String, String>>> = combine(
        auth.userId,
        dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() }
            .map { users -> users.mapNotNull { user -> user.name?.takeIf { it.isNotBlank() }?.let { user.id to it } }.toMap() }
            .distinctUntilChanged(),
    ) { me, names -> me to names }

    val chatList: StateFlow<Chats.ChatList> = combine(
        combine(groups, inputs) { g, i -> g to i },
        prStates,
        combine(words, waiting, walls) { w, waits, ws -> Triple(w, waits, ws) },
        combine(_view, viewerAndMembers) { view, names -> view to names },
        minuteClock,
    ) { (g, input), states, (w, waits, ws), (view, names), now ->
        Chats.chatList(
            groups = g,
            messages = input.messages,
            reads = input.reads,
            prStates = states,
            words = w,
            waiting = waits,
            walls = ws,
            view = view,
            now = SessionTree.nowIso(now),
            utcOffsetMinutes = Chats.deviceUtcOffsetMinutes(now),
            // #EXP-1399 review: an ask mirror of a session I cannot see shows no text.
            visible = input.sessions.mapTo(HashSet()) { it.id },
            me = names.first,
            members = names.second,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, Chats.ChatList(emptyList()))

    /** #EXP-1733: every member team's live apps (the dao hides trashed rows). */
    private val teamAppRows: Flow<List<com.exponential.app.data.db.AppEntity>> =
        dbFlow.scopedQuery(emptyList()) { it.appDao().observeAll() }
            .map { rows -> rows.sortedWith(compareBy({ it.name.lowercase() }, { it.id })) }

    private val teamApps: Flow<List<Chats.AppChatInput>> = teamAppRows.map { rows ->
        rows.map { Chats.AppChatInput(appId = it.id, slug = it.slug, name = it.name, icon = it.icon, color = it.color) }
    }

    /** S-3 (chats.json `agentChatRows`): the AGENT rows pinned on top of the
     *  list, [agents] order (the viewer's Assistant first); none on the
     *  Archived page. */
    val agentRows: StateFlow<List<Chats.AgentChatRow>> = combine(
        combine(agents, inputs) { a, i -> a to i },
        viewerAndMembers,
        _view,
        minuteClock,
        teamAppRows,
    ) { (rows, input), (me, members), view, now, apps ->
        Chats.agentChatRows(
            // #EXP-1849 (chats.json): an app's agent chat IS the app row.
            agents = Chats.sidebarAgents(rows, apps.map { it.agentId }) { it.agentId },
            sessions = input.sessions,
            messages = input.messages,
            reads = input.reads,
            me = me,
            view = view,
            now = SessionTree.nowIso(now),
            utcOffsetMinutes = Chats.deviceUtcOffsetMinutes(now),
            members = members,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyList())

    /** #EXP-1733 (chats.json `appChatRows`): the APP rows between the Agents
     *  and the sessions, by name; none on the Archived page. */
    val appRows: StateFlow<List<Chats.AppChatRow>> = combine(
        combine(teamApps, inputs) { a, i -> a to i },
        viewerAndMembers,
        _view,
        minuteClock,
    ) { (apps, input), (me, members), view, now ->
        Chats.appChatRows(
            apps = apps,
            sessions = input.sessions,
            messages = input.messages,
            reads = input.reads,
            me = me,
            view = view,
            now = SessionTree.nowIso(now),
            utcOffsetMinutes = Chats.deviceUtcOffsetMinutes(now),
            members = members,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyList())

    /** What a chat row's menu needs per root: its status, whether the
     *  viewer may steer it (shared, or the starter) and whether the viewer
     *  started it (with [isOwner], who may delete it). #EDE-98: its title's
     *  marks ([Chats.chatMarks]). */
    data class RootFacts(
        val status: String,
        val canSteer: Boolean,
        val agent: String?,
        val mine: Boolean = false,
        val marks: List<String> = emptyList(),
    )

    val rootFacts: StateFlow<Map<String, RootFacts>> = combine(sessionRows, auth.userId) { rows, me ->
        rows.filter { it.isRoot }.associate { row ->
            row.id to RootFacts(
                row.status,
                row.shared || (me != null && row.userId == me),
                row.cli,
                mine = me != null && row.userId == me,
                marks = Chats.chatMarks(row.shared, row.sealEpoch),
            )
        }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

    /** The teams the viewer owns (a row's delete, the agent menu's settings). */
    val owned: StateFlow<Set<String>> =
        ownedTeams.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptySet())

    /** #EXP-2109: a row's TEAM MARK by its root, agent or app id; a row
     *  outside the member teams (the viewer's Assistant) has none. */
    val rowTeams: StateFlow<Map<String, TeamEntity>> = combine(
        sessionRows,
        agentDefinitions,
        teamAppRows,
        dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() },
        auth.userId,
    ) { rows, defs, apps, teamRows, me ->
        val byId = teamRows.associateBy { it.id }
        buildMap {
            rows.forEach { row -> if (row.isRoot) byId[row.teamId]?.let { put(row.id, it) } }
            defs.forEach { def ->
                if (!(def.isAssistant && def.ownerId == me)) byId[def.teamId]?.let { put(def.id, it) }
            }
            apps.forEach { app -> byId[app.teamId]?.let { put(app.id, it) } }
        }
    }.distinctUntilChanged().flowOn(Dispatchers.Default).stateIn(viewModelScope, LIST_SHARING, emptyMap())

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

    /** NEW-2: the viewer's chat per agent id (a row opens it); #EXP-1990:
     *  a team agent's group chat. */
    val conversations: StateFlow<Map<String, String>> = combine(sessions, auth.userId, agentDefinitions) { s, me, defs ->
        SessionTree.conversations(s, me, defs.associate { it.id to it.groupChatId })
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /** The member teams, name order — the per-team bands. */
    val teams: StateFlow<List<TeamEntity>> = dbFlow.scopedQuery(emptyList()) { it.teamDao().observeAll() }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
}

/** #FEED-191: THE Needs you rows and each row's team. */
data class NeedsYouRead(
    val rows: List<NeedsYou.Row> = emptyList(),
    /** [rows]' session teams by row id; an Assistant's row's is no member team. */
    val teamOf: Map<String, String> = emptyMap(),
    /** The member teams' ids. */
    val teams: List<String> = emptyList(),
) {
    /** `needsYouCounts` (nav-badges.json); [installs] = [InstallTrustSnapshot.installs]. */
    fun counts(installs: Int): NavBadges.NeedsYouCounts =
        NavBadges.needsYouCounts(rows.map { teamOf[it.messageId] }, teams, installs)
}

/** [NeedsYou.needsYou] over the synced rows of every member team and the
 *  Assistant's home TOGETHER (the viewer's own sessions in no synced team): an
 *  ask mirrored into the Assistant's chat is one row with its origin. Each PR
 *  reads its own team's merge mode and owner flag. */
internal fun needsYouRead(
    sessions: List<SessionTree.Session>,
    links: List<SessionTree.SessionIssue>,
    issues: List<SessionTree.Issue>,
    messages: List<SessionTree.Message>,
    definitions: List<SessionTree.AgentDefinition>,
    pullRequests: List<PullRequestEntity>,
    /** The member teams. */
    teams: List<TeamEntity>,
    /** The ones the viewer owns. */
    owned: Set<String>,
    /** #EXP-1710: the synced devices (a stuck row's `Device offline`). */
    devices: List<DeviceEntity>,
    me: String?,
    now: Long,
): NeedsYouRead {
    val teamById = teams.associateBy { it.id }
    val mine = sessions.filter { it.teamId in teamById || (teamById.isNotEmpty() && it.userId == me) }
    val ids = mine.mapTo(HashSet()) { it.id }
    val titleOf = SessionTree.titler(links, issues, messages)
    val agentNames = definitions.associate { it.id to it.name }
    val sessionsById = sessions.associateBy { it.id }
    val devicesById = devices.associateBy { it.deviceId }
    val rows = NeedsYou.needsYou(
        now = now,
        me = me,
        sessions = mine.map { session ->
            NeedsYou.SessionInput(
                id = session.id,
                rootId = session.rootId,
                name = session.agentId?.let(agentNames::get) ?: titleOf(session),
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
        pullRequests = pullRequests.mapNotNull { pr ->
            val team = teamById[pr.teamId] ?: return@mapNotNull null
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
                mergeMode = team.mergeMode,
                owner = team.id in owned,
            )
        },
        mergeMode = null,
        // #EXP-1990 (needs-you.json): a team agent's group chat counts for every member.
        groupChatIds = definitions.mapNotNull { it.groupChatId?.takeIf { id -> id.isNotEmpty() } },
    )
    return NeedsYouRead(
        rows = rows,
        teamOf = rows.mapNotNull { row -> sessionsById[row.sessionId]?.let { row.messageId to it.teamId } }.toMap(),
        teams = teams.map { it.id },
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

/** NEW-41: the PR glyph a Chats-tab row's lead wears (web `rowLead`). */
enum class ChatRowPrLead { Open, Merged }

/** NEW-41 (web `rowLead`): `prs.some(open)` → emerald, else
 *  `prs.some(merged)` → violet, else none. */
fun chatRowPrLead(states: List<String>): ChatRowPrLead? = when {
    states.any { it == DomainContract.prStateOpen } -> ChatRowPrLead.Open
    states.any { it == DomainContract.prStateMerged } -> ChatRowPrLead.Merged
    else -> null
}
