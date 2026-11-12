package com.exponential.app.ui.chat

import com.exponential.app.data.db.keyedMessages
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AnswerInput
import kotlinx.serialization.json.JsonObject
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.PullRequestsApi
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.StartSuggestionInput
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.steer.SteerConnection
import com.exponential.app.data.steer.SteerConnectionStore
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.Dock
import com.exponential.app.domain.TaskQueueBackgroundInput
import com.exponential.app.domain.TaskQueueInput
import com.exponential.app.domain.TaskQueueTaskInput
import com.exponential.app.domain.TaskQueues
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.domain.openWaits
import kotlinx.coroutines.Job
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.steer.steerDeviceFlow
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.PullRequestIssueEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.InboxRoute
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.issueRefTarget
import com.exponential.app.ui.steer.SessionSender
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import com.exponential.app.domain.Chats
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** #EXP-1380: every session id a card of [messages] names (`sessionId` on
 *  the card or on one of its items), sorted — the sessions outside the tree
 *  a chat's cards draw (their rows + titles). */
internal fun referencedSessionIds(messages: List<SessionTree.Message>): List<String> {
    val out = sortedSetOf<String>()
    fun take(element: kotlinx.serialization.json.JsonElement?) {
        val obj = element as? JsonObject ?: return
        (obj["sessionId"] as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content
            ?.takeIf { it.isNotEmpty() }?.let(out::add)
    }
    for (message in messages) {
        val card = message.card ?: continue
        take(card)
        for (value in card.values) (value as? kotlinx.serialization.json.JsonArray)?.forEach(::take)
    }
    return out.toList()
}

/** S-3: the sessions the messages were posted by (`authorSessionId`), sorted —
 *  a row from ANOTHER session links it (its row + title). */
internal fun authorSessionIds(messages: List<SessionTree.Message>): List<String> =
    messages.mapNotNullTo(sortedSetOf()) { it.authorSessionId?.takeIf { id -> id.isNotEmpty() } }.toList()

/** #EXP-1399: the ORIGIN message ids the `ask` mirrors name (an agent's
 *  conversation watching another chat draws THAT card, read from its
 *  message), sorted. */
internal fun askOriginIds(messages: List<SessionTree.Message>): List<String> {
    val out = sortedSetOf<String>()
    for (message in messages) {
        val card = message.card ?: continue
        if ((card["kind"] as? kotlinx.serialization.json.JsonPrimitive)?.content != DomainContract.messageCardKindAsk) continue
        (card["messageId"] as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content
            ?.takeIf { it.isNotEmpty() }?.let(out::add)
    }
    return out.toList()
}

/** The thing a chat menu call names (`Couldn't stop the chat`). */
private const val CHAT_THING = "the chat"

/**
 * EXP-1291: ONE chat (a root session of kind `thread`) or ONE
 * agent channel (a root of kind `agent`) — the same conversation screen
 * (lane CHAT owns the UI). Keyed by the route's `sessionId` (the ROOT).
 * [view] = [SessionTree.buildSessionChat] over the synced tree, re-built
 * every second so the session cards' elapsed ladder ticks.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ChatViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val sessionsApi: SessionsApi,
    private val sender: SessionSender,
    private val issuesApi: IssuesApi,
    private val imagesApi: IssueImagesApi,
    private val pullRequestsApi: PullRequestsApi,
    private val steerStore: SteerConnectionStore,
) : ViewModel() {

    /** The ROOT session id (`chat/{sessionId}`). */
    val rootId: String = savedStateHandle["sessionId"] ?: ""

    /** The tab key the screen opens on (`chat/{id}?tab=guide`); null = the
     *  Conversation. */
    val initialTab: String? = savedStateHandle["tab"]

    /** #EXP-1662: the plan card a way in opened the plan tab on. */
    val initialPlan: String? = savedStateHandle["plan"]

    /** needs-you.json: the waiting card a way in centres in the Conversation. */
    val initialCard: String? = savedStateHandle["card"]

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The synced root row; null while loading or once it is gone. */
    val root: StateFlow<SessionEntity?> =
        dbFlow.scopedQuery<SessionEntity?>(null) { it.sessionDao().observeById(rootId) }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** Every session of the tree (the root + its children), creation order. */
    val tree: StateFlow<List<SessionEntity>> =
        dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeTree(rootId) }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The chat's linked issues, in the chat's order (the header chips). */
    val issues: StateFlow<List<IssueEntity>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeByRoot(rootId) },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
    ) { links, all ->
        val byId = all.associateBy { it.id }
        links.mapNotNull { byId[it.issueId] }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The chat's pull requests (opening order) — the Guide's PRs. */
    val pullRequests: StateFlow<List<PullRequestEntity>> =
        dbFlow.scopedQuery(emptyList<PullRequestEntity>()) { it.pullRequestDao().observeByRoot(rootId) }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The team's pull requests — the open-stack walk behind the Guide's
     *  `Merge stack` (a stack may reach PRs outside this chat). */
    val teamPullRequests: StateFlow<List<PullRequestEntity>> =
        combine(dbFlow, root.map { it?.teamId }.distinctUntilChanged()) { db, team -> db to team }
            .flatMapLatest { (db, team) ->
                if (db == null || team == null) flowOf(emptyList()) else db.pullRequestDao().observeByTeam(team)
            }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** #EXP-1611 (pr-checks.json CHECKS LIST): the Guide PR's synced CI
     *  checks (the header `…`'s Checks submenu, the Merge pill's confirm). */
    val guideChecks: StateFlow<List<com.exponential.app.data.db.PullRequestCheckEntity>> =
        combine(dbFlow, pullRequests.map { SessionTree.guidePullRequest(it)?.id }.distinctUntilChanged()) { db, id -> db to id }
            .flatMapLatest { (db, id) ->
                if (db == null || id == null) flowOf(emptyList()) else db.pullRequestCheckDao().observeByPullRequest(id)
            }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** S-3 (session-chat.json `chatModeCases`): an agent conversation
     *  paints chat bubbles (`chatMode`), a chat its transcript. */
    val isAgent: StateFlow<Boolean> = root.map {
        com.exponential.app.domain.SessionTree.chatMode(it?.agentId) ==
            com.exponential.app.domain.SessionTree.ChatMode.Bubbles
    }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** NEW-2: an agent conversation's DEFINITION row (`sessions.agent_id`):
     *  the header's coloured icon + name come from it, not the session. */
    val agentDefinition: StateFlow<com.exponential.app.data.db.AgentDefinitionEntity?> =
        root.map { it?.agentId?.takeIf { id -> id.isNotEmpty() } }
            .distinctUntilChanged()
            .flatMapLatest { id ->
                if (id == null) {
                    flowOf(null)
                } else {
                    dbFlow.scopedQuery<com.exponential.app.data.db.AgentDefinitionEntity?>(null) {
                        it.agentDefinitionDao().observeById(id)
                    }
                }
            }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** #EXP-1380 (TS `useNewestMessages`): the tree's NEWEST
     *  [Chats.CHAT_MESSAGE_LIMIT] messages (Load earlier widens it per chat)
     *  + every waiting card the tree's digests name, however old. */
    private val messagePage: Flow<Chats.MessagePage> = combine(
        Chats.messageLimit(rootId),
        tree.map { rows -> Chats.digestWaitingIds(rows.map { Chats.parseDigest(it.digest) }) }.distinctUntilChanged(),
    ) { limit, ids -> limit to ids }
        .flatMapLatest { (limit, ids) ->
            dbFlow.filterNotNull().flatMapLatest { db ->
                val newest = db.keyedMessages(db.messageDao().observeNewestByRoot(rootId, limit))
                val waiting = if (ids.isEmpty()) flowOf(emptyList()) else db.keyedMessages(db.messageDao().observeByIds(ids))
                combine(newest, waiting) { loaded, cards -> Chats.newestMessagesPage(loaded, cards, limit) }
            }
        }

    /** #EXP-1380 (chats.json PERF): the tree's messages, keyed through
     *  their conversations — the transcript reads THIS tree, never every
     *  synced message. */
    private val treeMessages: Flow<List<SessionTree.Message>> =
        // Only the account's REAL rows (never the signed-out placeholder): the
        // view's first build already holds the whole tree, so the window's
        // first paint pins on the loaded transcript (chats.json WINDOW).
        messagePage.map { it.rows }
            // One query + one card parse per change, shared by the view and
            // the referenced-session lookup.
            .flowOn(Dispatchers.Default)
            .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    /** THE chat's Load earlier: one `windowStep` more messages (TS `loadEarlierMessages`). */
    fun loadEarlierMessages() = Chats.loadEarlierMessages(rootId)

    /** Every message of the tree (the root's conversation + its children's):
     *  the SAME [treeMessages] the view builds from (review of #1218: the
     *  tree's rows are queried once, never twice). */
    val messages: StateFlow<List<SessionTree.Message>> =
        treeMessages.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** EXP-1341: the sessions the tree's cards name outside the tree (an
     *  agent channel's `thread_started` → another root): their rows + the
     *  card-less messages their titles read, by id. */
    // S-3: + the sessions the messages came from (a link to another session).
    private val referencedIds: Flow<List<String>> = combine(treeMessages, tree) { messages, treeRows ->
        val inTree = treeRows.mapTo(HashSet()) { it.id }
        (referencedSessionIds(messages) + authorSessionIds(messages)).distinct().sorted().filter { it !in inTree }
    }
        .distinctUntilChanged()

    /** S-3: the root's starter = the conversation of the agent that started
     *  it (`started_by_agent_id`, a root of the same user): its `origin`. */
    private val starterSessions: Flow<List<SessionEntity>> = root
        .map { r -> r?.startedByAgentId?.takeIf { it.isNotEmpty() }?.let { it to r.userId } }
        .distinctUntilChanged()
        .flatMapLatest { key ->
            if (key == null) {
                flowOf(emptyList())
            } else {
                dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeAgents() }
                    .map { rows -> rows.filter { it.agentId == key.first && it.userId == key.second } }
            }
        }

    private val linkedSessions: Flow<List<SessionEntity>> = combine(
        referencedIds.flatMapLatest { ids ->
            if (ids.isEmpty()) flowOf(emptyList()) else dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeByIds(ids) }
        },
        starterSessions,
    ) { refs, starters -> (refs + starters).distinctBy { it.id } }

    /** + the roots of those sessions (S-3: a link's agent mark reads its
     *  root's agent), once each. */
    private val referencedSessions: Flow<List<SessionEntity>> = linkedSessions.flatMapLatest { rows ->
        val have = rows.mapTo(HashSet()) { it.id }
        val roots = rows.map { it.rootId.ifEmpty { it.id } }.filter { it !in have && it != rootId }.distinct().sorted()
        if (roots.isEmpty()) {
            flowOf(rows)
        } else {
            dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeByIds(roots) }.map { rows + it }
        }
    }
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    /** S-3: the synced agent definitions by id (the links' marks). */
    val agentMarks: StateFlow<Map<String, com.exponential.app.data.db.AgentDefinitionEntity>> =
        dbFlow.scopedQuery(emptyList<com.exponential.app.data.db.AgentDefinitionEntity>()) { it.agentDefinitionDao().observeAll() }
            .map { rows -> rows.associateBy { it.id } }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    private val referencedTitleMessages: Flow<List<SessionTree.Message>> = referencedSessions
        .map { rows -> rows.map { it.id }.sorted() }
        .distinctUntilChanged()
        .flatMapLatest { ids ->
        if (ids.isEmpty()) {
            flowOf(emptyList())
        } else {
            dbFlow.scopedQuery(emptyList<SessionTree.Message>()) { it.keyedMessages(it.messageDao().observeTitleRows(ids)) }
        }
    }

    /** #EXP-1399: the origin messages the tree's `ask` mirrors name outside
     *  the tree (an escalated question / plan card), by id. */
    private val askOrigins: Flow<List<SessionTree.Message>> = treeMessages
        .map { messages ->
            val own = messages.mapTo(HashSet()) { it.id }
            askOriginIds(messages).filter { it !in own }
        }
        .distinctUntilChanged()
        .flatMapLatest { ids ->
            if (ids.isEmpty()) {
                flowOf(emptyList())
            } else {
                dbFlow.scopedQuery(emptyList<SessionTree.Message>()) { it.keyedMessages(it.messageDao().observeByIds(ids)) }
            }
        }

    /** The sessions the tree's CARDS name (their elapsed ladder ticks). */
    private val cardRefIds: Flow<Set<String>> = treeMessages.map { referencedSessionIds(it).toSet() }.distinctUntilChanged()

    /** chats.json PERF: the clock ticks only while a session the chat paints
     *  a CARD for is live (a child, a `thread_started` root: their elapsed
     *  ladder). The root never has a card, so an idle agent chat or a lone
     *  chat builds once per change, never once a second. */
    private val clock: Flow<Unit> = combine(tree, referencedSessions, cardRefIds) { treeRows, refs, cardIds ->
        // S-3: a linked session (an author, the starter) draws no card: no tick.
        treeRows.any { it.id != rootId && !it.isEnded } || refs.any { it.id != rootId && it.id in cardIds && !it.isEnded }
    }
        .distinctUntilChanged()
        .flatMapLatest { live ->
            if (!live) {
                flowOf(Unit)
            } else {
                flow {
                    while (true) {
                        emit(Unit)
                        delay(1_000)
                    }
                }
            }
        }

    /** EXP-1341: the links + issues the tree's (and its named sessions')
     *  titles and chips read. */
    private val linksFlow = dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeAll() }

    /** The OTHER roots the chat's cards name (an agent channel's
     *  `thread_started`), refreshed with every [view] build. */
    @Volatile private var otherRootIds: Set<String> = emptySet()

    /** Whether [sessionId] is ANOTHER root (an agent channel's
     *  `thread_started` card): it opens as a chat, not a raw session. */
    fun isOtherRoot(sessionId: String): Boolean = sessionId != rootId && sessionId in otherRootIds

    /** #EXP-1464: the sessions the last [view] build knew (the tree + the
     *  sessions its rows name + their roots), by id. */
    @Volatile private var knownSessions: Map<String, SessionEntity> = emptyMap()

    /** #EXP-1464: where a Conversation row's session opens (inbox-route.json
     *  `sessionTarget`, the S-3 rule); null = not synced. */
    fun sessionTarget(sessionId: String): InboxRoute.SessionTarget? {
        val session = knownSessions[sessionId] ?: return null
        val sessionRoot = session.rootId.ifEmpty { session.id }
        val root = knownSessions[sessionRoot]
        return InboxRoute.sessionTarget(
            id = session.id,
            rootId = sessionRoot,
            agentId = root?.agentId ?: session.agentId,
            userId = root?.userId ?: session.userId,
            viewerId = currentUserId.value,
        )
    }

    private val allIssues = dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() }

    /** EXP-1334: the `parent` relations a linked sub-issue folds by. */
    private val issueParents = dbFlow.scopedQuery(emptyList()) { it.issueRelationDao().observeAll() }
        .map(SessionTree::issueParents)

    /** EXP-1306: every PR ↔ issue link — the Guide stack rail names a PR by
     *  its representative issue. */
    val pullRequestIssues: StateFlow<List<PullRequestIssueEntity>> =
        dbFlow.scopedQuery(emptyList<PullRequestIssueEntity>()) { it.pullRequestIssueDao().observeAll() }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Every synced issue (the stack rail's row names). */
    val syncedIssues: StateFlow<List<IssueEntity>> =
        allIssues.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The previous build's rows by id: an unchanged row keeps its INSTANCE,
     *  so the transcript's items skip recomposition (chats.json PERF). */
    private var lastRows: Map<String, SessionTree.SessionChatRow> = emptyMap()

    /** THE chat view (tabs, rows, composer target); null until the root
     *  syncs. #EXP-1380: built off the main thread over the tree's own rows
     *  (+ the sessions its cards name), re-built per change and, while a
     *  session is live, per clock tick. */
    val view: StateFlow<SessionTree.ChatView?> = combine(
        combine(root, tree, referencedSessions) { r, t, refs -> Triple(r, t, refs) },
        combine(linksFlow, allIssues, issueParents) { l, i, p -> l to i.map { SessionTree.issue(it, p[it.id]) } },
        // A named session's title rows may already be the tree's own: once each.
        // #EXP-1399: + the escalated cards' origin messages.
        combine(treeMessages, referencedTitleMessages, askOrigins) { own, titles, origins ->
            if (titles.isEmpty() && origins.isEmpty()) own else (own + titles + origins).distinctBy { it.id }
        },
        pullRequests,
        clock,
    ) { (rootRow, treeRows, refs), (links, issueRows), messageRows, prs, _ ->
        val r = rootRow ?: return@combine null
        otherRootIds = refs.filter { it.isRoot && it.id != rootId }.mapTo(HashSet()) { it.id }
        val sessions = if (refs.isEmpty()) treeRows else (treeRows + refs).distinctBy { it.id }
        knownSessions = sessions.associateBy { it.id }
        val built = SessionTree.buildSessionChat(
            root = SessionTree.session(r),
            sessions = sessions.map(SessionTree::session),
            sessionIssues = links.map(SessionTree::sessionIssue),
            issues = issueRows,
            messages = messageRows,
            pullRequests = prs.mapNotNull(SessionTree::pullRequest),
            now = SessionTree.nowIso(System.currentTimeMillis()),
        )
        val previous = lastRows
        val rows = built.rows.map { row -> previous[row.id]?.takeIf { it == row } ?: row }
        lastRows = rows.associateBy { it.id }
        built.copy(rows = rows)
    }
        .flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _sending = MutableStateFlow(false)
    /** A [send] is in flight. */
    val sending: StateFlow<Boolean> = _sending

    private val _failure = MutableStateFlow<FailedAction?>(null)
    /** FEED-79: a refused write, raised once as an error toast. */
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    /**
     * The composer: a message to the ROOT (`sessions.send({sessionId, text,
     * attachmentIds})`; an idle root resumes, an ended chat continues
     * server-side). [onSent] fires only on success (clear the draft then).
     */
    fun send(text: String, attachmentIds: List<String> = emptyList(), onSent: () -> Unit = {}) {
        if (text.isBlank() && attachmentIds.isEmpty()) return
        run("send", "the message", { _sending.value = it }) {
            sender.send(SessionsSendInput.message(rootId, text, attachmentIds))
            onSent()
        }
    }

    /** results-tab.json: an inline surface's reply press goes to the session
     *  that posted the result. */
    fun sendTo(sessionId: String, text: String) =
        run("send", "the message") { sender.send(SessionsSendInput.message(sessionId, text)) }

    /** #EXP-1561 (waiting-cards.json DISMISS): dismiss a waiting question or
     *  plan card through THE answer path (`dismissed: true` alone). */
    fun dismissCard(messageId: String, settled: (Boolean) -> Unit = {}) =
        run("dismiss", "the card", settled = settled) { accountId ->
            sessionsApi.answer(accountId, AnswerInput.dismiss(messageId))
        }

    // ── EXP-1323: read marks, suggested tasks, the live tail ────────────────

    /** Stamp the viewer's read of the chat (its root); background, so a
     *  failure stays silent (feedback-states: background never toasts). */
    fun markRead() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                sessionsApi.markRead(accountId, rootId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
            }
        }
    }

    private val _suggestionPending = MutableStateFlow<Set<String>>(emptySet())
    /** In-flight suggestion keys (`suggestionKey`): their buttons spin. */
    val suggestionPending: StateFlow<Set<String>> = _suggestionPending

    private fun suggestionCall(key: String, verb: String, thing: String, call: suspend (accountId: String) -> Unit) {
        if (key in _suggestionPending.value) return
        run(verb, thing, busy = { on ->
            _suggestionPending.value = if (on) _suggestionPending.value + key else _suggestionPending.value - key
        }, call = call)
    }

    /** Start (a child of the posting session; Options: a device / a model). */
    fun startSuggestion(messageId: String, itemId: String, deviceId: String? = null, model: String? = null) =
        suggestionCall(suggestionKey(messageId, itemId), "start", "the task") { accountId ->
            sessionsApi.startSuggestion(
                accountId,
                StartSuggestionInput(messageId = messageId, itemId = itemId, where = "child", deviceId = deviceId, model = model),
            )
        }

    /** Do it here: the item goes to the posting session as a message. */
    fun suggestionHere(messageId: String, itemId: String) =
        suggestionCall(suggestionKey(messageId, itemId), "send", "the task") { accountId ->
            sessionsApi.startSuggestion(accountId, StartSuggestionInput(messageId = messageId, itemId = itemId, where = "here"))
        }

    fun startAllSuggestions(messageId: String) =
        suggestionCall(suggestionKey(messageId), "start", "the tasks") { accountId ->
            sessionsApi.startAllSuggestions(accountId, messageId)
        }

    fun dismissSuggestion(messageId: String, itemId: String) =
        suggestionCall(suggestionKey(messageId, itemId), "dismiss", "the task") { accountId ->
            sessionsApi.dismissSuggestion(accountId, messageId, itemId)
        }

    fun fileSuggestion(messageId: String, itemId: String) =
        suggestionCall(suggestionKey(messageId, itemId), "file", "the issue") { accountId ->
            sessionsApi.fileSuggestion(accountId, messageId, itemId)
        }

    /** Start on…: the team's ONLINE machines (the synced devices shape). */
    val startDevices: StateFlow<List<SteerDevice>> = steerDeviceFlow(dbFlow, teamIdFlow(), auth.userId)
        .map { devices -> devices.orEmpty().filter { it.online } }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private fun teamIdFlow(): Flow<String?> = root.map { it?.teamId?.takeIf { id -> id.isNotEmpty() } }.distinctUntilChanged()

    /** The relay connection behind the live tail, acquired while the root is
     *  live and released when it ends (and in [onCleared]). */
    private var tailConnection: SteerConnection? = null

    private fun releaseTail() {
        tailConnection?.let { steerStore.release(it.codingSessionId) }
        tailConnection = null
    }

    /**
     * EXP-1323: the live tail's relay state ([SessionTree.ChatView.liveSessionId]
     * = the root while it is not ended): the Tasks card + the status line the
     * session view draws. Null = no tail.
     */
    val liveActivity: StateFlow<ActivityFeedState?> = view
        .map { it?.liveSessionId }
        .distinctUntilChanged()
        .flatMapLatest { id ->
            if (id == null) {
                // The root ended (or went away): the tail's connection goes
                // back to the store, never held until the screen closes.
                releaseTail()
                flowOf(null)
            } else {
                val held = tailConnection?.takeIf { it.codingSessionId == id }
                val connection = if (held != null) {
                    held
                } else {
                    releaseTail()
                    steerStore.acquire(id).also { tailConnection = it }
                }
                connection.kick("chat-tail")
                connection.activity
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** #EXP-1545 / #EXP-1678 (dock.json ONE CHILD STREAM): the ONE held
     *  connection on the child [Dock.chatQueueProbe] names (was one per
     *  Live child: opening a chat dialled every child's feed at once, 4 to
     *  9 joins to one device, and the views failed to settle in bursts). */
    private val childConnections = mutableMapOf<String, SteerConnection>()

    private fun releaseChildren(keep: Set<String> = emptySet()) {
        val gone = childConnections.keys.filter { it !in keep }
        for (id in gone) {
            childConnections.remove(id)?.let { steerStore.release(it.codingSessionId) }
            childAttachedAt.remove(id)
        }
    }

    /** One Live child's Dock slice: its stream's last-known slots ([slots] =
     *  dock.json LOCAL FIRST: drawn while it (re)connects, never once over). */
    data class ChildDockState(
        val slots: Boolean,
        val activity: com.exponential.app.ui.session.AgentSessionViewModel.DockActivity,
    )

    private data class LiveChild(val id: String, val activeAt: Long?)

    /** What the probe reads off the held child's stream. */
    private data class HeldChild(val id: String, val phase: AgentPhase, val replayed: Boolean, val open: Boolean)

    /** A child read closed, keyed by its activeAt then (dock.json `closedQueueTtlMs`). */
    private val closedReads = mutableMapOf<String, Pair<Long?, Long>>()
    private val childAttachedAt = mutableMapOf<String, Long>()
    private val heldChild = MutableStateFlow<String?>(null)
    private val probeTick = MutableStateFlow(0)
    private var probeTimer: Job? = null

    private val liveChildren: Flow<List<LiveChild>> = tree
        .map { rows ->
            rows.filter { it.id != rootId && !it.isEnded }
                .map { LiveChild(it.id, (it.lastTurnAt ?: it.startedAt)?.let(WireTimestamps::parseEpochMs)) }
                .sortedBy { it.id }
        }
        .distinctUntilChanged()

    private val rootQueueOpen: Flow<Boolean> = liveActivity
        .map { it != null && queueOpen(it) }
        .flowOn(Dispatchers.Default)
        .distinctUntilChanged()

    private val held: Flow<HeldChild?> = heldChild.flatMapLatest { id ->
        val connection = id?.let(childConnections::get)
        if (id == null || connection == null) {
            flowOf(null)
        } else {
            combine(
                connection.phase,
                connection.replayed,
                connection.activity.map(::queueOpen).flowOn(Dispatchers.Default).distinctUntilChanged(),
            ) { phase, replayed, open -> HeldChild(id, phase, replayed, open) }
        }
    }.distinctUntilChanged()

    /** The probe: which ONE child to hold, side effects (acquire/release) included. */
    private fun probe(children: List<LiveChild>, rootOpen: Boolean, held: HeldChild?): String? {
        val now = System.currentTimeMillis()
        val probes = children.map { child ->
            val seen = closedReads[child.id]
            val remembered = seen != null && seen.first == child.activeAt && now - seen.second < Dock.CLOSED_QUEUE_TTL_MS
            val heldHere = held?.takeIf { it.id == child.id && child.id in childConnections }
            val stream = heldHere?.let {
                Dock.ChildStream(streamPhase(it.phase), it.replayed, now - (childAttachedAt[child.id] ?: now))
            }
            Dock.ChatQueueProbe(child.id, child.activeAt, Dock.chatQueueChildState(heldHere?.open == true, remembered, stream))
        }
        for (p in probes) {
            if (p.state != Dock.ChildQueueState.Closed) continue
            val seen = closedReads[p.sessionId]
            if (seen == null || seen.first != p.activeAt || now - seen.second >= Dock.CLOSED_QUEUE_TTL_MS) {
                closedReads[p.sessionId] = p.activeAt to now
            }
        }
        val wanted = Dock.chatQueueProbe(rootOpen, probes)
        releaseChildren(setOfNotNull(wanted))
        if (wanted != null && wanted !in childConnections) {
            childConnections[wanted] = steerStore.acquire(wanted).also { it.kick("chat-child-probe") }
            childAttachedAt[wanted] = now
            rearmProbe(Dock.PROBE_DEADLINE_MS)
        } else if (wanted == null && children.isNotEmpty() && !rootOpen) {
            // Every child read closed: read them again once the memory runs out.
            rearmProbe(Dock.CLOSED_QUEUE_TTL_MS)
        }
        heldChild.value = wanted
        return wanted
    }

    private fun rearmProbe(afterMs: Long) {
        probeTimer?.cancel()
        probeTimer = viewModelScope.launch {
            delay(afterMs)
            probeTick.value += 1
        }
    }

    /**
     * #EXP-1545 (dock.json CHAT QUEUE) + #EXP-1678 (ONE CHILD STREAM): the
     * held Live child's relay state, by id (at most one): the same
     * per-session connection the session view reads ([SteerConnectionStore]),
     * so its last-known Queue is there at once.
     */
    val childDocks: StateFlow<Map<String, ChildDockState>> =
        combine(liveChildren, rootQueueOpen, held, probeTick) { children, rootOpen, held, _ ->
            probe(children, rootOpen, held)
        }
            .distinctUntilChanged()
            .flatMapLatest { id ->
                val connection = id?.let(childConnections::get)
                if (id == null || connection == null) {
                    flowOf(emptyMap())
                } else {
                    combine(connection.phase, connection.activity) { phase, activity ->
                        mapOf(
                            id to ChildDockState(
                                slots = com.exponential.app.ui.session.dockStreamSlots(phase, sessionEnded = false),
                                activity = com.exponential.app.ui.session.dockActivityOf(activity),
                            ),
                        )
                    }.distinctUntilChanged()
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    override fun onCleared() {
        releaseTail()
        releaseChildren()
        super.onCleared()
    }

    /** Decide a `plan` card: [approved] (+ [fresh] = start in a fresh
     *  context), with optional [feedback]. */
    fun decidePlan(
        messageId: String,
        approved: Boolean,
        fresh: Boolean = false,
        feedback: String? = null,
        settled: (Boolean) -> Unit = {},
        answers: Map<String, List<String>>? = null,
        surfaceData: JsonObject? = null,
    ) =
        run("answer", "the plan", settled = settled) { accountId ->
            sessionsApi.answer(
                accountId,
                AnswerInput(
                    messageId = messageId,
                    decision = if (approved) "approved" else "rejected",
                    // ask-dock.json: only an approval starts fresh.
                    fresh = if (approved && fresh) true else null,
                    feedback = feedback?.takeIf { it.isNotBlank() },
                    // EXP-1280: the questions' picks (empty ones left out).
                    answers = answers?.filterValues { it.isNotEmpty() }?.takeIf { it.isNotEmpty() }?.let(AnswerInput::planAnswers),
                    surfaceData = surfaceData,
                ),
            )
        }

    // ── session 58cb32df: the chat's menu (chats.json MENUS) ─────────────────

    /** Delete = `sessions.delete`; [onDeleted] closes the chat's screen back
     *  to the list. S-3: no Stop/End. */
    fun deleteChat(onDeleted: () -> Unit) = run("delete", CHAT_THING, settled = { ok -> if (ok) onDeleted() }) {
        sessionsApi.delete(it, rootId)
    }

    private val _notice = MutableStateFlow<String?>(null)
    /** A non-error notice toast (`copy.toasts.stopNotDelivered`). */
    val notice: StateFlow<String?> = _notice

    fun clearNotice() {
        _notice.value = null
    }

    /** The viewer owns the chat's team (the agent menu's Agent settings). */
    val isOwner: StateFlow<Boolean> = combine(root.map { it?.teamId }.distinctUntilChanged(), auth.userId) { team, user -> team to user }
        .flatMapLatest { (team, user) ->
            if (team.isNullOrEmpty() || user == null) {
                flowOf(false)
            } else {
                dbFlow.scopedQuery(emptyList()) { it.teamMemberDao().observeByTeam(team) }
                    .map { members -> members.any { it.userId == user && it.role == "owner" } }
            }
        }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** The ROOT's device name (the header subtitle); null = none / unknown. */
    val rootDeviceLabel: StateFlow<String?> = combine(
        root.map { it?.deviceId?.takeIf { id -> id.isNotEmpty() } }.distinctUntilChanged(),
        dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
    ) { deviceId, devices ->
        deviceId?.let { id -> devices.firstOrNull { it.deviceId == id }?.label?.takeIf { label -> label.isNotBlank() } }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The header "+": link issues to the chat. */
    fun linkIssues(issueIds: List<String>) {
        if (issueIds.isEmpty()) return
        run("link", "the issues") { sessionsApi.linkIssues(it, rootId, issueIds) }
    }

    fun unlinkIssue(issueId: String) = run("unlink", "the issue") { sessionsApi.unlinkIssue(it, rootId, issueId) }

    // ── the people + the "+" (link / file an issue) ─────────────────────────

    /** The signed-in user (my queued messages can be revoked). */
    val currentUserId: StateFlow<String?> = auth.userId

    private val teamId: Flow<String?> = root.map { it?.teamId?.takeIf { id -> id.isNotEmpty() } }.distinctUntilChanged()

    /** The chat team's users: the chat rows' `@email` mention pills
     *  (LocalMentions), like the issue detail's comment list. */
    val teamUsers: StateFlow<List<UserEntity>> = teamId
        .flatMapLatest { team ->
            if (team == null) flowOf(emptyList())
            else dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeByTeam(team) }
        }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** EXP-1307: the viewer is a member of the chat's team (the header
     *  `…`'s Close PR, `MergeTrain.mergeMenuEntries`). */
    val isMember: StateFlow<Boolean> = combine(teamId, auth.userId) { team, user -> team to user }
        .flatMapLatest { (team, user) ->
            if (team == null || user == null) flowOf(false)
            else dbFlow.scopedQuery(emptyList()) { it.teamMemberDao().observeByTeam(team) }
                .map { members -> members.any { it.userId == user } }
        }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** EXP-1307: the Guide `…`'s Close PR (after its confirm), without
     *  merging; a failure toasts `Couldn't close the PR`. */
    fun closePullRequest(pullRequestId: String) =
        run("close", "the PR") { pullRequestsApi.close(it, pullRequestId) }

    /** #EXP-1710 (merge-train.json MENU): an owner's Merge now (after the
     *  `merge-now-pr` alert); a failure toasts `Couldn't merge the PR`. */
    fun mergePullRequestNow(pullRequestId: String) =
        run("merge", "the PR") { pullRequestsApi.mergeNow(it, pullRequestId) }

    /** #EXP-1710: Hold auto-merge ([hold] true) / Resume auto-merge. */
    fun holdPullRequest(pullRequestId: String, hold: Boolean) =
        run(if (hold) "hold" else "resume", "auto-merge") { pullRequestsApi.hold(it, pullRequestId, hold) }

    private val teamBoards: Flow<List<BoardEntity>> = teamId.flatMapLatest { team ->
        if (team == null) flowOf(emptyList())
        else dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeByTeam(team) }
    }

    /** The team's resolved statuses — the issue tabs' glyphs. */
    val statuses: StateFlow<List<ResolvedIssueStatus>> = teamId
        .flatMapLatest { team ->
            if (team == null) flowOf(emptyList())
            else dbFlow.scopedQuery(emptyList<IssueStatusEntity>()) { it.issueStatusDao().observeByTeam(team) }
        }
        .map { rows -> if (rows.isEmpty()) IssueStatusResolver.builtinDefaults else IssueStatusResolver.teamStatuses(rows) }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueStatusResolver.builtinDefaults)

    /** The team's issues as search rows — the "+"'s Link an issue list. */
    val linkCandidates: StateFlow<List<IssueRefTarget>> = combine(allIssues, teamBoards, statuses) { issues, boards, resolved ->
        val boardIds = boards.map { it.id }.toSet()
        issues.filter { it.boardId in boardIds }
            .sortedByDescending { it.createdAt }
            .map { issueRefTarget(it, resolved) }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Where the "+"'s New issue files: the first linked issue's board, else
     *  the team's first board; null = nowhere (the row disables). */
    val newIssueBoardId: StateFlow<String?> = combine(issues, teamBoards) { linked, boards ->
        linked.firstOrNull()?.boardId ?: boards.firstOrNull()?.id
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** EXP-892: `issues.search` over the team behind the local rows. */
    suspend fun searchIssues(query: String): List<IssueRefTarget> {
        val accountId = auth.activeAccountId.value ?: return emptyList()
        val team = root.value?.teamId?.takeIf { it.isNotEmpty() } ?: return emptyList()
        return try {
            issuesApi.search(accountId, team, query).map(::issueRefTarget)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            emptyList()
        }
    }

    private fun run(
        verb: String,
        thing: String,
        busy: ((Boolean) -> Unit)? = null,
        settled: ((Boolean) -> Unit)? = null,
        call: suspend (accountId: String) -> Unit,
    ) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                settled?.invoke(false)
                return@launch
            }
            busy?.invoke(true)
            try {
                call(accountId)
                settled?.invoke(true)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                settled?.invoke(false)
                _failure.value = FailedAction(verb, thing, trpcErrorMessage(t, ""))
            } finally {
                busy?.invoke(false)
            }
        }
    }
}

/** #EXP-1678: a connection's phase as the child probe reads it. */
private fun streamPhase(phase: AgentPhase): Dock.ChildStream.Phase = when (phase) {
    AgentPhase.Idle, AgentPhase.Connecting -> Dock.ChildStream.Phase.Connecting
    AgentPhase.Live -> Dock.ChildStream.Phase.Live
    AgentPhase.Starting -> Dock.ChildStream.Phase.Starting
    is AgentPhase.Ended -> Dock.ChildStream.Phase.Ended
    is AgentPhase.Closed -> if (phase.reconnecting) Dock.ChildStream.Phase.Connecting else Dock.ChildStream.Phase.Closed
}

/** #EXP-1678: whether a session's Queue has a todo item (dock.json `open`). */
private fun queueOpen(state: ActivityFeedState): Boolean {
    val now = System.currentTimeMillis()
    return TaskQueues.of(
        TaskQueueInput(
            tasks = state.taskList.map { TaskQueueTaskInput(it.content, it.status) },
            backgroundTasks = state.backgroundTasks.map {
                TaskQueueBackgroundInput(id = it.id, kind = it.kind, description = it.description, startedAt = it.startedAt ?: now)
            },
            waits = openWaits(state.feed),
            now = now,
        ),
    )?.todo?.items?.isNotEmpty() == true
}
