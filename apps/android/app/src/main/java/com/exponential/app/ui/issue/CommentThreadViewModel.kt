package com.exponential.app.ui.issue

import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AttachmentsApi
import com.exponential.app.data.api.CommentsApi
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
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.MAX_COMMENT_ATTACHMENTS
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.MAX_FILE_UPLOAD_BYTES
import com.exponential.app.domain.MAX_IMAGE_UPLOAD_BYTES
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.isInlineImage
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

data class CommentThreadState(
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
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class CommentThreadViewModel @Inject constructor(
    private val holder: DatabaseHolder,
    private val commentsApi: CommentsApi,
    private val issueImagesApi: IssueImagesApi,
    private val attachmentsApi: AttachmentsApi,
    private val auth: AuthRepository,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
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

    // The issue's team statuses (EXP-595) — the teamId comes off the issue's
    // BOARD (issues don't denormalize it), re-keyed on issue/account switches,
    // feeding the timeline's status-change glyphs.
    private val statusRows: Flow<List<IssueStatusEntity>> =
        combine(dbFlow, issueIdFlow) { db, id -> db to id }
            .flatMapLatest { (db, id) ->
                if (db == null || id == null) {
                    flowOf(emptyList())
                } else {
                    combine(db.issueDao().observeById(id), db.boardDao().observeAll()) { issue, boards ->
                        boards.firstOrNull { it.id == issue?.boardId }?.teamId
                    }
                        .distinctUntilChanged()
                        .flatMapLatest { teamId ->
                            if (teamId == null) flowOf(emptyList())
                            else db.issueStatusDao().observeByTeam(teamId)
                        }
                }
            }

    val state: StateFlow<CommentThreadState> = combine(
        combine(dbFlow, issueIdFlow) { db, id -> db to id }
            .flatMapLatest { (db, id) ->
                if (db == null || id == null) flowOf(null) else db.issueDao().observeById(id)
            },
        commentsEventsLabels,
        dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
        auth.userId,
        statusRows,
    ) { issue, (comments, events, labels), users, userId, statusRows ->
        CommentThreadState(
            issue = issue,
            comments = comments,
            events = events,
            usersById = users.associateBy { it.id },
            labelsById = labels.associateBy { it.id },
            currentUserId = userId,
            statuses = IssueStatusResolver.teamStatuses(statusRows),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), CommentThreadState())

    fun bind(issueId: String) {
        issueIdFlow.value = issueId
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
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /** Files queued for the NEXT comment — uploaded on send, never before. */
    private val _pendingAttachments = MutableStateFlow<List<PendingAttachment>>(emptyList())
    val pendingAttachments: StateFlow<List<PendingAttachment>> = _pendingAttachments

    /** Files queued for the comment currently being EDITED. Only one comment
     *  is editable at a time (the thread keeps a single `editingId`), so one
     *  list covers the whole thread. */
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
            val picked = withContext(Dispatchers.IO) {
                val bytes = MarkdownMediaUtils.readBytes(appContext, uri) ?: return@withContext null
                val contentType = canonicalContentType(
                    MarkdownMediaUtils.guessMimeType(
                        appContext,
                        uri,
                        fallback = "application/octet-stream",
                    ),
                )
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
     * Upload everything in [target] that hasn't landed yet, sequentially,
     * stamping each entry's `uploadedId` so a retry after a mid-batch failure
     * never re-uploads. Returns the ids in order, or null when an upload
     * failed (the pending list — and the draft — survive, so Send retries).
     */
    private suspend fun uploadPendingAttachments(
        accountId: String,
        issueId: String,
        target: MutableStateFlow<List<PendingAttachment>>,
    ): List<String>? {
        val items = target.value
        for ((index, item) in items.withIndex()) {
            if (item.uploadedId != null) continue
            val uploadedId = try {
                if (item.isImage) {
                    issueImagesApi.upload(
                        accountId, issueId, item.bytes, item.filename, item.contentType,
                    ).id
                } else {
                    attachmentsApi.upload(
                        accountId, issueId, item.bytes, item.filename, item.contentType,
                    ).id
                }
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (t: Throwable) {
                // The 412 body's billing copy never reaches the UI — the API
                // already replaced it (EXP-216).
                _commentError.value = trpcErrorMessage(t, "The attachment could not be uploaded")
                return null
            }
            target.value = target.value.mapIndexed { i, entry ->
                if (i == index) entry.copy(uploadedId = uploadedId) else entry
            }
        }
        return target.value.mapNotNull { it.uploadedId }
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
            reportFailure(t, "The file could not be downloaded")
            null
        }
    }

    // Composer draft + in-flight flag, hoisted here (EXP-240) so the expanding
    // bottom-bar composer keeps its text across collapse/expand, rotation, and
    // the thread/bar being separate composables sharing this screen-level VM.
    private val _draft = MutableStateFlow("")
    val draft: StateFlow<String> = _draft

    private val _sending = MutableStateFlow(false)
    val sending: StateFlow<Boolean> = _sending

    // Surfaced when a comment send / edit / delete is refused (snackbar in the
    // issue screen, REV2-50): the thread is driven by Electric sync, so a
    // failed write left the composer looking like a dead button.
    private val _commentError = MutableStateFlow<String?>(null)
    val commentError: StateFlow<String?> = _commentError

    fun consumeCommentError() {
        _commentError.value = null
    }

    private fun reportFailure(t: Throwable, fallback: String) {
        if (t is CancellationException) throw t
        _commentError.value = trpcErrorMessage(t, fallback)
    }

    fun updateDraft(text: String) {
        _draft.value = text
    }

    /**
     * Post the current draft. Clears it and invokes [onSent] only when the
     * comment actually lands — a declined/failed send keeps the draft AND the
     * pending attachments (already-uploaded ones hold their id, so a retry
     * uploads only what is left).
     */
    fun send(onSent: () -> Unit = {}) {
        val text = _draft.value.trim()
        if ((text.isEmpty() && _pendingAttachments.value.isEmpty()) || _sending.value) return
        viewModelScope.launch {
            _sending.value = true
            if (createComment(text)) {
                _draft.value = ""
                _pendingAttachments.value = emptyList()
                onSent()
            }
            _sending.value = false
        }
    }

    // Returns true only when the comment was actually posted, so the composer
    // keeps the draft (and the pending attachments) when the send is declined
    // (nothing to post) or an upload/the request fails.
    suspend fun createComment(text: String): Boolean {
        val issueId = issueIdFlow.value ?: return false
        val accountId = auth.activeAccountId.value ?: return false
        val body = text.trim()
        // Upload on send, sequentially — a comment is only created once every
        // attachment it links has a row.
        val attachmentIds = uploadPendingAttachments(accountId, issueId, _pendingAttachments)
            ?: return false
        // Attachment-only comments are allowed; an empty one is not.
        if (body.isEmpty() && attachmentIds.isEmpty()) return false
        return runCatching {
            commentsApi.create(accountId, issueId, body, attachmentIds.ifEmpty { null })
        }
            .onFailure { reportFailure(it, "The comment could not be posted") }
            .isSuccess
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
            ?: return false
        val attachmentIds = keptAttachmentIds + uploadedIds
        if (body.isEmpty() && attachmentIds.isEmpty()) return false
        return runCatching {
            commentsApi.update(accountId, id, body, attachmentIds)
        }
            .onFailure { reportFailure(it, "The comment could not be saved") }
            .isSuccess
            .also { if (it) _editAttachments.value = emptyList() }
    }

    suspend fun deleteComment(id: String) {
        val accountId = auth.activeAccountId.value ?: return
        runCatching { commentsApi.delete(accountId, id) }
            .onFailure { reportFailure(it, "The comment could not be deleted") }
    }

}
