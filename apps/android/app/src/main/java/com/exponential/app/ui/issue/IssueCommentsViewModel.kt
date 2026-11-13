package com.exponential.app.ui.issue

import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AttachmentsApi
import com.exponential.app.data.api.CommentsApi
import com.exponential.app.data.api.CreateCommentResult
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.ComposerSend
import com.exponential.app.domain.ComposerDraft
import com.exponential.app.data.composer.ComposerDraftBinding
import com.exponential.app.data.composer.ComposerDrafts
import com.exponential.app.domain.DomainContract
import com.exponential.app.data.steer.SteerConnectionStore
import com.exponential.app.domain.MARKDOWN_PREVIEW_MAX_BYTES
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.MAX_COMMENT_ATTACHMENTS
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.MAX_FILE_UPLOAD_BYTES
import com.exponential.app.domain.MAX_IMAGE_UPLOAD_BYTES
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.commentComposerAfterBind
import com.exponential.app.domain.MediaPreparer
import com.exponential.app.domain.isInlineImage
import com.exponential.app.domain.isInlineMedia
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import dagger.hilt.android.lifecycle.HiltViewModel
import java.io.File
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

import com.exponential.app.domain.AgentSettings
import com.exponential.app.ui.components.actorRow
import kotlinx.coroutines.flow.flowOn

data class IssueCommentsState(
    val issue: IssueEntity? = null,
    val comments: List<CommentEntity> = emptyList(),
    val events: List<IssueEventEntity> = emptyList(),
    val usersById: Map<String, UserEntity> = emptyMap(),
    val labelsById: Map<String, LabelEntity> = emptyMap(),
    val currentUserId: String? = null,
    /** The issue's team statuses in render order (EXP-595) — the status-change
     *  rows' glyph + color resolve against the same vocabulary every other
     *  status surface uses. Empty while the shape is syncing (resolution
     *  degrades to the constructed builtin defaults). */
    val statuses: List<ResolvedIssueStatus> = emptyList(),
    /** The issue's team `estimation_type` (EXP-630) — the scale an
     *  `estimate_changed` row phrases on. Null until the team row syncs. */
    val estimationType: String? = null,
    /** #EXP-1383: every synced agent definition: an agent actor (event,
     *  comment, the creation row) and an agent assignee resolve against it
     *  (`AgentSettings.issueActor`). */
    val agents: List<AgentSettings.AgentRowLike> = emptyList(),
    /** #VAPP-186: the live synced apps' names by id, a comment's
     *  `via_app_id` resolves against it ("via <app>"). */
    val appNames: Map<String, String> = emptyMap(),
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class IssueCommentsViewModel @Inject constructor(
    private val holder: DatabaseHolder,
    private val commentsApi: CommentsApi,
    private val issueImagesApi: IssueImagesApi,
    private val attachmentsApi: AttachmentsApi,
    private val auth: AuthRepository,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
    // EXP-824: video / audio picks are normalised before they queue.
    private val mediaPreparer: MediaPreparer,
    // EXP-1543: a comment send outlives the screen (`launchDetached`).
    private val store: SteerConnectionStore,
    // #FEED-219: the composer's synced draft.
    private val composerDrafts: ComposerDrafts,
) : ViewModel() {

    // Reactive account scoping: all queries re-scope on account switch (no
    // constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val issueIdFlow = MutableStateFlow<String?>(null)

    // Comments + activity events + labels pre-combined into one flow so the
    // outer combine stays within the 5-arg typed overload. Labels feed the
    // event rows' "added label X" phrases (EXP-169) — cross-team list,
    // tiny table, same usage as the "My Issues" rows.
    private val commentsEventsLabels = combine(dbFlow, issueIdFlow) { db, id -> db to id }
        .flatMapLatest { (db, id) ->
            if (db == null || id == null) {
                flowOf(
                    Triple(
                        emptyList<CommentEntity>(),
                        emptyList<IssueEventEntity>(),
                        emptyList<LabelEntity>(),
                    ),
                )
            } else {
                combine(
                    db.commentDao().observeByIssue(id),
                    db.issueEventDao().observeByIssue(id),
                    db.labelDao().observeAll(),
                ) { comments, events, labels -> Triple(comments, events, labels) }
            }
        }

    // The issue's team vocabulary — its statuses (EXP-595) and its estimate
    // scale (EXP-630). The teamId comes off the issue's BOARD (issues don't
    // denormalize it), re-keyed on issue/account switches, feeding the
    // timeline's status-change glyphs and estimate phrases.
    private val issueTeamId: Flow<String?> =
        combine(dbFlow, issueIdFlow) { db, id -> db to id }
            .flatMapLatest { (db, id) ->
                if (db == null || id == null) {
                    flowOf(null)
                } else {
                    combine(db.issueDao().observeById(id), db.boardDao().observeAll()) { issue, boards ->
                        boards.firstOrNull { it.id == issue?.boardId }?.teamId
                    }
                }
            }
            .distinctUntilChanged()

    private val teamVocabulary: Flow<Pair<List<IssueStatusEntity>, TeamEntity?>> =
        combine(dbFlow, issueTeamId) { db, teamId -> db to teamId }
            .flatMapLatest { (db, teamId) ->
                if (db == null || teamId == null) {
                    flowOf(emptyList<IssueStatusEntity>() to null)
                } else {
                    combine(
                        db.issueStatusDao().observeByTeam(teamId),
                        db.teamDao().observeById(teamId),
                    ) { rows, team -> rows to team }
                }
            }

    val state: StateFlow<IssueCommentsState> = combine(
        combine(dbFlow, issueIdFlow) { db, id -> db to id }
            .flatMapLatest { (db, id) ->
                if (db == null || id == null) flowOf(null) else db.issueDao().observeById(id)
            },
        commentsEventsLabels,
        combine(
            dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
            dbFlow.scopedQuery(emptyList()) { it.agentDefinitionDao().observeAll() },
            dbFlow.scopedQuery(emptyList()) { it.appDao().observeAll() },
        ) { users, agents, apps -> Triple(users, agents.map { it.actorRow() }, apps.associate { it.id to it.name }) },
        auth.userId,
        teamVocabulary,
    ) { issue, (comments, events, labels), (users, agents, appNames), userId, (statusRows, team) ->
        IssueCommentsState(
            issue = issue,
            comments = comments,
            events = events,
            usersById = users.associateBy { it.id },
            labelsById = labels.associateBy { it.id },
            currentUserId = userId,
            statuses = IssueStatusResolver.teamStatuses(statusRows),
            estimationType = team?.estimationType,
            agents = agents,
            appNames = appNames,
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueCommentsState())

    fun bind(issueId: String) {
        if (issueIdFlow.value != issueId) {
            _replyTarget.value = null
            _reporterAudience.value = false
        }
        // F34: the queued files stay with the issue they were typed on (the
        // Work screen's Stack swap rebinds this screen-level VM); the TEXT is
        // the issue's synced draft (#FEED-219, [draftBinding]).
        val carry = commentComposerAfterBind(issueIdFlow.value, issueId, _draft.value, _pendingAttachments.value)
        _pendingAttachments.value = carry.pending
        issueIdFlow.value = issueId
    }

    // ── Reply to reporter (SLOP-4) ───────────────────────────────────────────

    /**
     * The docked composer's "Reply to reporter" pill: OFF by default, reset
     * on a send, a collapse and an issue switch. The screen offers the pill
     * only while the issue's submission has a reporter email; a reply under
     * a card ignores it (replies are team-only).
     */
    private val _reporterAudience = MutableStateFlow(false)
    val reporterAudience: StateFlow<Boolean> = _reporterAudience

    fun setReporterAudience(on: Boolean) {
        _reporterAudience.value = on
    }

    // ── Reply target (EXP-741) ───────────────────────────────────────────────

    /**
     * The comment the docked composer replies to: set by a card's "Leave a
     * reply…" row, cleared by the bar's ✕, a collapse, or a successful send.
     */
    private val _replyTarget = MutableStateFlow<CommentReplyTarget?>(null)
    val replyTarget: StateFlow<CommentReplyTarget?> = _replyTarget

    fun setReplyTarget(target: CommentReplyTarget?) {
        // #FEED-219: leaving a reply (the composer folds) keeps its draft.
        _replyTarget.value = target
    }

    /** The reply bar's ✕ = the reply's Cancel: its draft is discarded
     *  (composer-draft.json), then the composer is the issue's again. */
    fun cancelReply() {
        if (_replyTarget.value != null) draftBinding.discard()
        _replyTarget.value = null
    }

    // ── Comment attachments (EXP-554) ────────────────────────────────────────

    /**
     * This issue's synced attachment rows that belong to a COMMENT, grouped by
     * comment id and oldest first. Comment attachments are never inlined into
     * the markdown body — this map is the only thing that renders them.
     */
    val commentAttachments: StateFlow<Map<String, List<AttachmentEntity>>> =
        combine(dbFlow, issueIdFlow) { db, id -> db to id }
            .flatMapLatest { (db, id) ->
                if (db == null || id == null) flowOf(emptyList())
                else db.attachmentDao().observeByIssue(id)
            }
            .map { rows ->
                rows.filter { it.commentId != null }
                    .sortedBy { it.createdAt }
                    .groupBy { it.commentId!! }
            }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /** Files queued for the NEXT comment — uploaded on send, never before. */
    private val _pendingAttachments = MutableStateFlow<List<PendingAttachment>>(emptyList())
    val pendingAttachments: StateFlow<List<PendingAttachment>> = _pendingAttachments

    /** Files queued for the comment currently being EDITED. Only one comment
     *  is editable at a time (the comment list keeps a single `editingId`), so one
     *  list covers the whole comment list. */
    private val _editAttachments = MutableStateFlow<List<PendingAttachment>>(emptyList())
    val editAttachments: StateFlow<List<PendingAttachment>> = _editAttachments

    fun addPendingAttachment(uri: Uri) {
        addAttachment(_pendingAttachments, uri, keptCount = 0)
    }

    fun removePendingAttachment(index: Int) {
        _pendingAttachments.value = _pendingAttachments.value.filterIndexed { i, _ -> i != index }
    }

    /** [keptCount] = attachments the edit is keeping, so the 10-per-comment cap
     *  counts them too. */
    fun addEditAttachment(uri: Uri, keptCount: Int) {
        addAttachment(_editAttachments, uri, keptCount)
    }

    fun removeEditAttachment(index: Int) {
        _editAttachments.value = _editAttachments.value.filterIndexed { i, _ -> i != index }
    }

    fun clearEditAttachments() {
        _editAttachments.value = emptyList()
    }

    // Read the picked bytes off the main thread (an OpenDocument pick can come
    // from a cloud-backed provider that streams over the network) and refuse
    // locally anything the server would reject anyway.
    private fun addAttachment(
        target: MutableStateFlow<List<PendingAttachment>>,
        uri: Uri,
        keptCount: Int,
    ) {
        viewModelScope.launch {
            if (target.value.size + keptCount >= MAX_COMMENT_ATTACHMENTS) {
                _commentError.value =
                    "A comment can carry at most $MAX_COMMENT_ATTACHMENTS attachments"
                return@launch
            }
            val contentType = withContext(Dispatchers.IO) {
                canonicalContentType(
                    MarkdownMediaUtils.guessMimeType(
                        appContext,
                        uri,
                        fallback = "application/octet-stream",
                    ),
                )
            }
            // EXP-824: a video is transcoded to 720p H.264/AAC (poster +
            // probe alongside) BEFORE it queues, so the strip shows the poster
            // and the size cap applies to what will actually upload.
            if (isInlineMedia(contentType)) {
                val filename = withContext(Dispatchers.IO) {
                    MarkdownMediaUtils.guessFilename(appContext, uri)
                }
                val prepared = try {
                    mediaPreparer.prepare(uri, filename, contentType)
                } catch (cancel: CancellationException) {
                    throw cancel
                } catch (t: Throwable) {
                    android.util.Log.w("IssueCommentsViewModel", "Media prepare failed", t)
                    _commentFailure.value = FailedAction("attach", "the file", t.message)
                    return@launch
                }
                if (prepared.bytes.size > MAX_FILE_UPLOAD_BYTES) {
                    _commentError.value =
                        "Videos must be ${MAX_FILE_UPLOAD_BYTES / (1024 * 1024)} MB or smaller"
                    return@launch
                }
                target.value = target.value + PendingAttachment(
                    uri = uri,
                    bytes = prepared.bytes,
                    filename = prepared.filename,
                    contentType = prepared.contentType,
                    isImage = false,
                    isMedia = true,
                    poster = prepared.poster,
                    width = prepared.width,
                    height = prepared.height,
                    durationMs = prepared.durationMs,
                )
                return@launch
            }
            val picked = withContext(Dispatchers.IO) {
                val bytes = MarkdownMediaUtils.readBytes(appContext, uri) ?: return@withContext null
                PendingAttachment(
                    uri = uri,
                    bytes = bytes,
                    filename = MarkdownMediaUtils.guessFilename(appContext, uri),
                    contentType = contentType,
                    isImage = isInlineImage(contentType),
                )
            }
            if (picked == null) {
                _commentError.value = "That file could not be read"
                return@launch
            }
            val cap = if (picked.isImage) MAX_IMAGE_UPLOAD_BYTES else MAX_FILE_UPLOAD_BYTES
            if (picked.bytes.size > cap) {
                _commentError.value = if (picked.isImage) {
                    "Images must be ${cap / (1024 * 1024)} MB or smaller"
                } else {
                    "Files must be ${cap / (1024 * 1024)} MB or smaller"
                }
                return@launch
            }
            target.value = target.value + picked
        }
    }

    /**
     * Upload everything in [items] that hasn't landed yet, sequentially,
     * stamping each entry's `uploadedId` on the live pick in [target] (by
     * key: one added or removed meanwhile is left alone, EXP-1543) so a
     * retry after a mid-batch failure never re-uploads. Returns [items]
     * stamped, or null when an upload failed (the pending list and the
     * draft survive, so Send retries).
     */
    private suspend fun uploadPendingAttachments(
        accountId: String,
        issueId: String,
        target: MutableStateFlow<List<PendingAttachment>>,
        items: List<PendingAttachment> = target.value,
    ): List<PendingAttachment>? {
        val stamped = mutableListOf<PendingAttachment>()
        for (item in items) {
            if (item.uploadedId != null) {
                stamped += item
                continue
            }
            val uploadedId = try {
                if (item.isImage) {
                    issueImagesApi.upload(
                        accountId, issueId, item.bytes, item.filename, item.contentType,
                    ).id
                } else {
                    attachmentsApi.upload(
                        accountId, issueId, item.bytes, item.filename, item.contentType,
                        // EXP-824: the media parts ride along for video / audio.
                        poster = item.poster,
                        width = item.width,
                        height = item.height,
                        durationMs = item.durationMs,
                    ).id
                }
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (t: Throwable) {
                // The 412 body's billing copy never reaches the UI — the API
                // already replaced it (EXP-216).
                _commentFailure.value = FailedAction("upload", "the attachment", trpcErrorMessage(t, ""))
                return null
            }
            val entry = item.copy(uploadedId = uploadedId)
            stamped += entry
            target.value = target.value.map { if (it.key == item.key) entry else it }
        }
        return stamped
    }

    /**
     * Fetch an attachment's bytes into the per-id cache directory so another
     * app can open them. Attachments are immutable, so a cached file of the
     * expected size is served straight back.
     */
    suspend fun downloadToCache(attachment: AttachmentEntity): File? {
        val accountId = auth.activeAccountId.value ?: return null
        val target = attachmentCacheFile(appContext, attachment)
        if (target.isFile && target.length() == attachment.sizeBytes) return target
        return try {
            val bytes = attachmentsApi.download(accountId, attachment.url)
            withContext(Dispatchers.IO) {
                target.parentFile?.mkdirs()
                target.writeBytes(bytes)
            }
            target
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (t: Throwable) {
            reportFailure(t, "download", "the file")
            null
        }
    }

    /**
     * EXP-1003: the markdown preview's loader for comment attachments —
     * throws (keeping the HTTP status) so the sheet shows its own copy.
     */
    suspend fun loadAttachmentBytes(attachment: AttachmentEntity): ByteArray =
        fetchAttachmentBytes(
            appContext,
            attachmentsApi,
            auth.activeAccountId.value,
            attachment,
            maxBytes = MARKDOWN_PREVIEW_MAX_BYTES,
        )

    // Composer draft + in-flight flag, hoisted here (EXP-240) so the expanding
    // bottom-bar composer keeps its text across collapse/expand, rotation, and
    // the comment list/bar being separate composables sharing this screen-level VM.
    private val _draft = MutableStateFlow("")
    val draft: StateFlow<String> = _draft

    // #FEED-219 (composer-draft.json): the field is the synced draft of its
    // target, `comment:<issueId>` or, under a card, `reply:<topCommentId>`.
    private val draftBinding = ComposerDraftBinding(viewModelScope, composerDrafts, _draft) { _draft.value = it }

    init {
        viewModelScope.launch {
            combine(auth.activeAccountId, issueIdFlow, issueTeamId, _replyTarget) { account, issueId, teamId, reply ->
                Triple(account, commentDraftTarget(issueId, reply), teamId)
            }.collect { (account, target, teamId) -> draftBinding.bind(account, target, teamId) }
        }
    }

    override fun onCleared() {
        draftBinding.flush()
        super.onCleared()
    }

    private val _sending = MutableStateFlow(false)
    val sending: StateFlow<Boolean> = _sending

    // A picked attachment refused before it queues (count/size caps, an
    // unreadable file): a plain toast in the issue screen (web parity).
    private val _commentError = MutableStateFlow<String?>(null)
    val commentError: StateFlow<String?> = _commentError

    fun consumeCommentError() {
        _commentError.value = null
    }

    // FEED-79: a comment send / edit / delete / upload the server refused
    // (REV2-50): the comment list is driven by Electric sync, so a failed write left
    // the composer looking like a dead button. An error toast in the screen.
    private val _commentFailure = MutableStateFlow<FailedAction?>(null)
    val commentFailure: StateFlow<FailedAction?> = _commentFailure

    fun consumeCommentFailure() {
        _commentFailure.value = null
    }

    private fun reportFailure(t: Throwable, verb: String, thing: String) {
        if (t is CancellationException) throw t
        _commentFailure.value = FailedAction(verb, thing, trpcErrorMessage(t, ""))
    }

    fun updateDraft(text: String) {
        _draft.value = text
    }

    /**
     * Post the draft as Send finds it. EXP-1543 (composer-send.json): the
     * field stays editable while the comment is in flight; when it lands the
     * text clears only if the field still equals what was sent and only the
     * sent files leave ([ComposerSend.settle]); a failed upload or post keeps
     * everything (already-uploaded files hold their id, so a retry uploads
     * only what is left). Runs on the store's scope, not the ViewModel's:
     * backing out right after Send never cancels it. [onSent] receives the
     * server's `reporterEmailed` (SLOP-4): true/false = a reporter-audience
     * reply the screen toasts, null = a team comment.
     */
    fun send(onSent: (reporterEmailed: Boolean?) -> Unit = {}) {
        val sentText = _draft.value
        val sent = _pendingAttachments.value
        val body = sentText.trim()
        if ((body.isEmpty() && sent.isEmpty()) || _sending.value) return
        val issueId = issueIdFlow.value ?: return
        val accountId = auth.activeAccountId.value ?: return
        // EXP-741: a reply is an ordinary comment with parentId; the target
        // is read at send time and cleared with the draft.
        val parentId = _replyTarget.value?.parentId
        // SLOP-4: the pill only ever applies to a TOP-LEVEL comment.
        val audience = if (parentId == null && _reporterAudience.value) {
            DomainContract.commentAudienceReporter
        } else {
            null
        }
        val sentDraft = draftBinding.bound
        store.launchDetached {
            _sending.value = true
            try {
                val result = createComment(accountId, issueId, body, sent, parentId, audience)
                    ?: return@launchDetached
                // A rebind to another issue (or target) already swapped the
                // draft out; the binding clears the sent one's row then.
                if (draftBinding.bound?.id == sentDraft?.id) {
                    val after = ComposerSend.settle(
                        ComposerSend.Outcome.Sent, sentText, sent, _draft.value, _pendingAttachments.value,
                    )
                    _draft.value = after.text
                    _pendingAttachments.value = after.attachments
                }
                // #FEED-219: then the draft the send leaves is written.
                draftBinding.settleSent(sentDraft, sentText)
                if (issueIdFlow.value == issueId) {
                    if (_replyTarget.value?.parentId == parentId) _replyTarget.value = null
                    _reporterAudience.value = false
                }
                onSent(result.reporterEmailed)
            } finally {
                _sending.value = false
            }
        }
    }

    // Returns the server's answer only when the comment was actually posted
    // (null otherwise), so the composer keeps the draft (and the pending
    // attachments) when an upload or the request fails.
    private suspend fun createComment(
        accountId: String,
        issueId: String,
        body: String,
        pending: List<PendingAttachment>,
        parentId: String?,
        audience: String?,
    ): CreateCommentResult? {
        // Upload on send, sequentially — a comment is only created once every
        // attachment it links has a row.
        val attachmentIds = uploadPendingAttachments(accountId, issueId, _pendingAttachments, pending)
            ?.mapNotNull { it.uploadedId }
            ?: return null
        // Attachment-only comments are allowed; an empty one is not.
        if (body.isEmpty() && attachmentIds.isEmpty()) return null
        return runCatching {
            commentsApi.create(
                accountId, issueId, body, attachmentIds.ifEmpty { null }, parentId, audience,
            )
        }
            .onFailure { reportFailure(it, "post", "the comment") }
            .getOrNull()
    }

    /**
     * Returns true when the edit was saved — the editor stays open otherwise.
     *
     * [keptAttachmentIds] are the already-linked attachments the edit keeps;
     * together with the freshly uploaded ones they form the FULL desired set
     * the server syncs to (rows missing from it are hard-deleted).
     */
    suspend fun updateComment(
        id: String,
        text: String,
        keptAttachmentIds: List<String>,
    ): Boolean {
        val issueId = issueIdFlow.value ?: return false
        val accountId = auth.activeAccountId.value ?: return false
        val body = text.trim()
        val uploadedIds = uploadPendingAttachments(accountId, issueId, _editAttachments)
            ?.mapNotNull { it.uploadedId }
            ?: return false
        val attachmentIds = keptAttachmentIds + uploadedIds
        if (body.isEmpty() && attachmentIds.isEmpty()) return false
        return runCatching {
            commentsApi.update(accountId, id, body, attachmentIds)
        }
            .onFailure { reportFailure(it, "save", "the comment") }
            .isSuccess
            .also { if (it) _editAttachments.value = emptyList() }
    }

    suspend fun deleteComment(id: String) {
        val accountId = auth.activeAccountId.value ?: return
        runCatching { commentsApi.delete(accountId, id) }
            .onFailure { reportFailure(it, "delete", "the comment") }
    }

}

/** #FEED-219: the docked composer's draft target — the reply's top comment
 *  under a card, else the issue's comment composer. */
internal fun commentDraftTarget(issueId: String?, reply: CommentReplyTarget?): ComposerDraft.Target? = when {
    reply != null -> ComposerDraft.Target(ComposerDraft.Kind.Reply, reply.parentId)
    issueId != null -> ComposerDraft.Target(ComposerDraft.Kind.Comment, issueId)
    else -> null
}

/** EXP-741: the comment the docked composer replies to. */
data class CommentReplyTarget(val parentId: String, val authorName: String)
