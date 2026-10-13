package com.exponential.app.ui.thread

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AnswerInput
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.steer.SessionSender
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flow
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
@HiltViewModel
class ThreadViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val sessionsApi: SessionsApi,
    private val sender: SessionSender,
) : ViewModel() {

    /** The ROOT session id (`thread/{sessionId}`). */
    val rootId: String = savedStateHandle["sessionId"] ?: ""

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

    /** An agent root's channel (kind `agent`) rather than a thread. */
    val isAgent: StateFlow<Boolean> = root.map { it?.isAgent == true }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

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

    private val linksFlow = dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeByRoot(rootId) }
    private val allIssues = dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() }

    /** THE thread view (tabs, rows, composer target); null until the root syncs. */
    val view: StateFlow<SessionTree.ThreadView?> = combine(
        combine(root, tree) { r, t -> r to t },
        combine(linksFlow, allIssues) { l, i -> l to i },
        messages,
        pullRequests,
        clock,
    ) { (rootRow, treeRows), (links, issueRows), messageRows, prs, now ->
        val r = rootRow ?: return@combine null
        SessionTree.buildSessionThread(
            root = SessionTree.session(r),
            sessions = treeRows.map(SessionTree::session),
            sessionIssues = links.map(SessionTree::sessionIssue),
            issues = issueRows.map(SessionTree::issue),
            messages = messageRows.map(SessionTree::message),
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

    /** Answer a `question` card ([answer]) in place. */
    fun answer(messageId: String, answer: String) =
        run("answer", "the question") { accountId ->
            sessionsApi.answer(accountId, AnswerInput(messageId = messageId, answer = answer))
        }

    /** Decide a `plan` card: [approved], with optional [feedback]. */
    fun decidePlan(messageId: String, approved: Boolean, feedback: String? = null) =
        run("answer", "the plan") { accountId ->
            sessionsApi.answer(
                accountId,
                AnswerInput(
                    messageId = messageId,
                    decision = if (approved) "approved" else "rejected",
                    feedback = feedback?.takeIf { it.isNotBlank() },
                ),
            )
        }

    /** Revoke one of my queued messages (the queue bar's ✕). */
    fun unqueue(messageId: String) = run("remove", "the message") { sessionsApi.unqueue(it, messageId) }

    /** Stop a session's current turn (it stays live). */
    fun stop(sessionId: String = rootId) = run("stop", "the session") { sessionsApi.stop(it, sessionId) }

    /** End a session (the ROOT ends the whole tree; agents refuse). */
    fun end(sessionId: String = rootId) = run("end", "the session") { sessionsApi.end(it, sessionId) }

    /** Rename the thread; null/blank = back to the derived title. */
    fun setTitle(title: String?) = run("rename", "the thread") { sessionsApi.setTitle(it, rootId, title) }

    /** The header "+": link issues to the thread. */
    fun linkIssues(issueIds: List<String>) {
        if (issueIds.isEmpty()) return
        run("link", "the issues") { sessionsApi.linkIssues(it, rootId, issueIds) }
    }

    fun unlinkIssue(issueId: String) = run("unlink", "the issue") { sessionsApi.unlinkIssue(it, rootId, issueId) }

    private fun run(
        verb: String,
        thing: String,
        busy: ((Boolean) -> Unit)? = null,
        call: suspend (accountId: String) -> Unit,
    ) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            busy?.invoke(true)
            try {
                call(accountId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction(verb, thing, trpcErrorMessage(t, ""))
            } finally {
                busy?.invoke(false)
            }
        }
    }
}
