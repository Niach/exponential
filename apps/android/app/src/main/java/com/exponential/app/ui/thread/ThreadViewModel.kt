package com.exponential.app.ui.thread

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
import com.exponential.app.data.steer.stopHandingBackQueue
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.ui.steer.steerDeviceFlow
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.PullRequestIssueEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.issueRefTarget
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
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * EXP-1291 Threads v2: ONE thread (a root session of kind `thread`) or ONE
 * agent channel (a root of kind `agent`) — the same conversation screen
 * (lane THREAD owns the UI). Keyed by the route's `sessionId` (the ROOT).
 * [view] = [SessionTree.buildSessionThread] over the synced tree, re-built
 * every second so the session cards' elapsed ladder ticks.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ThreadViewModel @Inject constructor(
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

    /** The ROOT session id (`thread/{sessionId}`). */
    val rootId: String = savedStateHandle["sessionId"] ?: ""

    /** The tab key the screen opens on (`thread/{id}?tab=guide`); null = the
     *  Conversation. */
    val initialTab: String? = savedStateHandle["tab"]

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The synced root row; null while loading or once it is gone. */
    val root: StateFlow<SessionEntity?> =
        dbFlow.scopedQuery<SessionEntity?>(null) { it.sessionDao().observeById(rootId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** Every session of the tree (the root + its children), creation order. */
    val tree: StateFlow<List<SessionEntity>> =
        dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeTree(rootId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The thread's linked issues, in the thread's order (the header chips). */
    val issues: StateFlow<List<IssueEntity>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeByRoot(rootId) },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
    ) { links, all ->
        val byId = all.associateBy { it.id }
        links.mapNotNull { byId[it.issueId] }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Every message of the tree (the root's conversation + its children's). */
    val messages: StateFlow<List<MessageEntity>> =
        dbFlow.scopedQuery(emptyList<MessageEntity>()) { it.messageDao().observeByRoot(rootId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The thread's pull requests (opening order) — the Guide's PRs. */
    val pullRequests: StateFlow<List<PullRequestEntity>> =
        dbFlow.scopedQuery(emptyList<PullRequestEntity>()) { it.pullRequestDao().observeByRoot(rootId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The team's pull requests — the open-stack walk behind the Guide's
     *  `Merge stack` (a stack may reach PRs outside this thread). */
    val teamPullRequests: StateFlow<List<PullRequestEntity>> =
        combine(dbFlow, root.map { it?.teamId }.distinctUntilChanged()) { db, team -> db to team }
            .flatMapLatest { (db, team) ->
                if (db == null || team == null) flowOf(emptyList()) else db.pullRequestDao().observeByTeam(team)
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** An agent root's channel (kind `agent`) rather than a thread. */
    val isAgent: StateFlow<Boolean> = root.map { it?.isAgent == true }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

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
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The root's raw Guide (`sessions.guide` jsonb TEXT; parse with
     *  `parseSessionResultGroups`). */
    val guide: StateFlow<String?> = root.map { it?.guide }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val clock: Flow<Long> = flow {
        while (true) {
            emit(System.currentTimeMillis())
            delay(1_000)
        }
    }

    /** EXP-1341: EVERY link + message, so a card naming another tree (an
     *  agent channel's `thread_started`) resolves its shared title too;
     *  [SessionTree.buildSessionThread] scopes both to this tree itself. */
    private val linksFlow = dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeAll() }
    private val titleMessages = dbFlow.scopedQuery(emptyList<SessionTree.Message>()) { it.keyedMessages(it.messageDao().observeAll()) }

    /** Every synced session: the tree, plus the roots an agent channel's
     *  `thread_started` cards point at (another tree each). */
    private val allSessions: StateFlow<List<SessionEntity>> =
        dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Whether [sessionId] is ANOTHER root (an agent channel's
     *  `thread_started` card): it opens as a thread, not a raw session. */
    fun isOtherRoot(sessionId: String): Boolean =
        sessionId != rootId && allSessions.value.firstOrNull { it.id == sessionId }?.isRoot == true
    private val allIssues = dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() }

    /** EXP-1334: the `parent` relations a linked sub-issue folds by. */
    private val issueParents = dbFlow.scopedQuery(emptyList()) { it.issueRelationDao().observeAll() }
        .map(SessionTree::issueParents)

    /** EXP-1306: every PR ↔ issue link — the Guide stack rail names a PR by
     *  its representative issue. */
    val pullRequestIssues: StateFlow<List<PullRequestIssueEntity>> =
        dbFlow.scopedQuery(emptyList<PullRequestIssueEntity>()) { it.pullRequestIssueDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Every synced issue (the stack rail's row names). */
    val syncedIssues: StateFlow<List<IssueEntity>> =
        allIssues.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** THE thread view (tabs, rows, composer target); null until the root syncs. */
    val view: StateFlow<SessionTree.ThreadView?> = combine(
        combine(root, allSessions) { r, t -> r to t },
        combine(linksFlow, allIssues, issueParents) { l, i, p -> l to i.map { SessionTree.issue(it, p[it.id]) } },
        titleMessages,
        pullRequests,
        clock,
    ) { (rootRow, treeRows), (links, issueRows), messageRows, prs, now ->
        val r = rootRow ?: return@combine null
        SessionTree.buildSessionThread(
            root = SessionTree.session(r),
            sessions = treeRows.map(SessionTree::session),
            sessionIssues = links.map(SessionTree::sessionIssue),
            issues = issueRows,
            messages = messageRows,
            pullRequests = prs.mapNotNull(SessionTree::pullRequest),
            now = SessionTree.nowIso(now),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

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
     * attachmentIds})`; an idle root resumes, an ended thread continues
     * server-side). [onSent] fires only on success (clear the draft then).
     */
    fun send(text: String, attachmentIds: List<String> = emptyList(), onSent: () -> Unit = {}) {
        if (text.isBlank() && attachmentIds.isEmpty()) return
        run("send", "the message", { _sending.value = it }) {
            sender.send(SessionsSendInput.message(rootId, text, attachmentIds))
            onSent()
        }
    }

    /** A CHILD session under [parentId] on [deviceId] (a follow-up). */
    fun startChild(parentId: String, deviceId: String, text: String) =
        run("start", "the session") { sender.send(SessionsSendInput.child(parentId, deviceId, text)) }

    /** Answer a `question` card ([answer]: an option or free text) in place;
     *  EXP-1280: [answers] = the picked labels (the rich card; [answer] = them
     *  joined, the compat string), [surfaceData] = the inline surface's data
     *  model. [settled] = whether the call went through. */
    fun answer(
        messageId: String,
        answer: String,
        settled: (Boolean) -> Unit = {},
        answers: List<String>? = null,
        surfaceData: JsonObject? = null,
    ) =
        run("answer", "the question", settled = settled) { accountId ->
            sessionsApi.answer(
                accountId,
                AnswerInput(
                    messageId = messageId,
                    answer = answer,
                    answers = answers?.let(AnswerInput::questionAnswers),
                    surfaceData = surfaceData,
                ),
            )
        }

    // ── EXP-1323: read marks, suggested tasks, the live tail ────────────────

    /** Stamp the viewer's read of the thread (its root); background, so a
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
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private fun teamIdFlow(): Flow<String?> = root.map { it?.teamId?.takeIf { id -> id.isNotEmpty() } }.distinctUntilChanged()

    /** The relay connection behind the live tail, acquired while the root is
     *  live and released when it ends (and in [onCleared]). */
    private var tailConnection: SteerConnection? = null

    private fun releaseTail() {
        tailConnection?.let { steerStore.release(it.codingSessionId) }
        tailConnection = null
    }

    /**
     * EXP-1323: the live tail's relay state ([SessionTree.ThreadView.liveSessionId]
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
                connection.kick("thread-tail")
                connection.activity
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    override fun onCleared() {
        releaseTail()
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

    /** Revoke one of my queued messages (the queue bar's ✕). */
    fun unqueue(messageId: String) = run("remove", "the message") { sessionsApi.unqueue(it, messageId) }

    /** Stop a session's current turn (it stays live). EXP-1342: the shown
     *  page's queued messages go back into its composer first, as the
     *  composer's own Stop does ([SteerConnection.interrupt]); the page's
     *  connection is the store's, shared with the session view. */
    fun stop(sessionId: String = rootId) = run("stop", "the session") { accountId ->
        stopHandingBackQueue(steerStore.peek(sessionId)) { sessionsApi.stop(accountId, sessionId) }
    }

    /** End a session (the ROOT ends the whole tree; agents refuse). */
    fun end(sessionId: String = rootId) = run("end", "the session") { sessionsApi.end(it, sessionId) }

    /** The header "+": link issues to the thread. */
    fun linkIssues(issueIds: List<String>) {
        if (issueIds.isEmpty()) return
        run("link", "the issues") { sessionsApi.linkIssues(it, rootId, issueIds) }
    }

    fun unlinkIssue(issueId: String) = run("unlink", "the issue") { sessionsApi.unlinkIssue(it, rootId, issueId) }

    // ── the people + the "+" (link / file an issue) ─────────────────────────

    /** The signed-in user (my queued messages can be revoked). */
    val currentUserId: StateFlow<String?> = auth.userId

    /** Every synced user by id — the message rows' author names + avatars. */
    val users: StateFlow<Map<String, UserEntity>> =
        dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeAll() }
            .map { rows -> rows.associateBy { it.id } }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    private val teamId: Flow<String?> = root.map { it?.teamId?.takeIf { id -> id.isNotEmpty() } }.distinctUntilChanged()

    /** EXP-1307: the viewer is a member of the thread's team (the Guide
     *  `…`'s Close PR, `SessionTree.guideMenuEntries`). */
    val isMember: StateFlow<Boolean> = combine(teamId, auth.userId) { team, user -> team to user }
        .flatMapLatest { (team, user) ->
            if (team == null || user == null) flowOf(false)
            else dbFlow.scopedQuery(emptyList()) { it.teamMemberDao().observeByTeam(team) }
                .map { members -> members.any { it.userId == user } }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** EXP-1307: the Guide `…`'s Close PR (after its confirm), without
     *  merging; a failure toasts `Couldn't close the PR`. */
    fun closePullRequest(pullRequestId: String) =
        run("close", "the PR") { pullRequestsApi.close(it, pullRequestId) }

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
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueStatusResolver.builtinDefaults)

    /** The team's issues as search rows — the "+"'s Link an issue list. */
    val linkCandidates: StateFlow<List<IssueRefTarget>> = combine(allIssues, teamBoards, statuses) { issues, boards, resolved ->
        val boardIds = boards.map { it.id }.toSet()
        issues.filter { it.boardId in boardIds }
            .sortedByDescending { it.createdAt }
            .map { issueRefTarget(it, resolved) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Where the "+"'s New issue files: the first linked issue's board, else
     *  the team's first board; null = nowhere (the row disables). */
    val newIssueBoardId: StateFlow<String?> = combine(issues, teamBoards) { linked, boards ->
        linked.firstOrNull()?.boardId ?: boards.firstOrNull()?.id
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

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
