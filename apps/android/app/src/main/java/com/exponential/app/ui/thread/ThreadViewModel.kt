package com.exponential.app.ui.thread

import android.net.Uri
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AnswerInput
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
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
import com.exponential.app.domain.AgentComposerPrompt
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.isInlineImage
import com.exponential.app.domain.steerAttachmentRefusal
import com.exponential.app.domain.uploadedImageIds
import com.exponential.app.domain.uploadedSteerFiles
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
    fun setTitle(title: String?) = run("rename", "the title") { sessionsApi.setTitle(it, rootId, title) }

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

    // ── the composer's attachments ──────────────────────────────────────────

    private val _images = MutableStateFlow<List<PendingAttachment>>(emptyList())
    /** Picked files waiting for the next [sendDraft] (the steer caps apply). */
    val images: StateFlow<List<PendingAttachment>> = _images

    private val _imageError = MutableStateFlow<String?>(null)
    /** A refused pick: invalid input, under the field. */
    val imageError: StateFlow<String?> = _imageError

    fun addImage(uri: Uri, bytes: ByteArray, filename: String, contentType: String) {
        _imageError.value = null
        val canonical = canonicalContentType(contentType)
        val isImage = isInlineImage(canonical)
        val current = _images.value
        val refusal = steerAttachmentRefusal(
            imageCount = current.count { it.isImage },
            fileCount = current.count { !it.isImage },
            isImage = isImage,
            sizeBytes = bytes.size.toLong(),
        )
        if (refusal != null) {
            _imageError.value = refusal
            return
        }
        _images.value = current + PendingAttachment(
            uri = uri,
            bytes = bytes,
            filename = filename,
            contentType = canonical,
            isImage = isImage,
        )
    }

    fun refuseAttachment(message: String) {
        _imageError.value = message
    }

    fun removeImage(index: Int) {
        _images.value = _images.value.filterIndexed { i, _ -> i != index }
    }

    /**
     * The composer's send WITH its pending files: upload what has not landed
     * (each id stamped as it lands, so a retry uploads only the rest), embed
     * them in the text, then ONE [send]. [onSent] clears the draft; a refusal
     * keeps text + files for the retry.
     */
    fun sendDraft(text: String, onSent: () -> Unit = {}) {
        if (_sending.value) return
        val pending = _images.value
        if (text.isBlank() && pending.isEmpty()) return
        if (pending.isEmpty()) {
            send(text.trim(), emptyList(), onSent)
            return
        }
        val team = root.value?.teamId?.takeIf { it.isNotEmpty() } ?: return
        run("upload", "the attachment", { _sending.value = it }) { accountId ->
            var current = _images.value
            for ((index, image) in current.withIndex()) {
                if (image.uploadedId != null) continue
                val uploaded = imagesApi.uploadTeamSessionImage(accountId, team, image.bytes, image.filename, image.contentType)
                current = current.mapIndexed { i, entry ->
                    if (i == index) entry.copy(uploadedId = uploaded.id, uploadedName = uploaded.filename) else entry
                }
                _images.value = current
            }
            val body = AgentComposerPrompt.build(text, current.uploadedImageIds(), current.uploadedSteerFiles()).orEmpty()
            try {
                sender.send(SessionsSendInput.message(rootId, body, current.mapNotNull { it.uploadedId }))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("send", "the message", trpcErrorMessage(t, ""))
                return@run
            }
            _images.value = emptyList()
            onSent()
        }
    }

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
