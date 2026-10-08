package com.exponential.app.ui.issue

import android.net.Uri
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AttachmentsApi
import com.exponential.app.data.api.CreateIssueInput
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.IssueDraftAttachment
import com.exponential.app.data.api.IssueDraftsApi
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.api.UpsertIssueDraftInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssueDraftPage
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.MAX_FILE_UPLOAD_BYTES
import com.exponential.app.domain.PreparedMedia
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.TeamPermissions
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.isInlineImage
import com.exponential.app.domain.isInlineMedia
import com.exponential.app.domain.sanitizeFilename
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.issueRefTarget
import com.exponential.app.ui.markdown.markdownEmbedUrls
import com.exponential.app.ui.markdown.markdownImageUrls
import com.exponential.app.ui.markdown.removeMarkdownImagesByUrl
import com.exponential.app.ui.markdown.replaceMarkdownImageUrls
import com.exponential.app.ui.share.SharePrefill
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import io.ktor.http.HttpStatusCode

/** A pick held in memory until the create (parent / share mode). */
data class HeldFile(val key: String, val uri: Uri, val filename: String)

/** The New issue page's form (EXP-1170) — everything a draft row carries + the page's flags. */
data class IssueDraftUiState(
    val title: String = "",
    val description: String = "",
    val status: ResolvedIssueStatus =
        IssueStatusResolver.builtinDefaults.first { it.builtinKey == IssueStatus.Backlog },
    val priority: IssuePriority = IssuePriority.None,
    val assigneeId: String? = null,
    val dueDate: String? = null,
    val labelIds: Set<String> = emptySet(),
    val boardId: String? = null,
    /** Files already uploaded against the draft row. */
    val attachments: List<IssueDraftAttachment> = emptyList(),
    /** Eager file uploads in flight / failed. */
    val pendingUploads: List<PendingFileUpload> = emptyList(),
    /** Deferred picks (parent / share mode), uploaded after the create. */
    val heldFiles: List<HeldFile> = emptyList(),
    val rowExists: Boolean = false,
    val seeded: Boolean = false,
    val creating: Boolean = false,
    val loadFailed: Boolean = false,
    /** EXP-1212: the row's files are listed (or there is no row). Until then
     *  a reopened draft counts as content: it may be file-only. */
    val attachmentsKnown: Boolean = false,
)

/**
 * EXP-1170: the New issue PAGE — the phone issue face in draft mode. Every
 * opener mints the draft id at tap time (`drafts/{draftId}?board=&status=&parent=`);
 * this model seeds once from the synced draft row (if any), autosaves the FULL
 * row through ONE single-flight `issueDrafts.upsert` path, and files the issue.
 *
 * Parent mode (`parent`, the Sub-issues `+`) and share mode (no draft id, the
 * `share-compose` route) never write a draft row: their uploads wait for the
 * create, like EXP-1130 settled.
 */
@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
@HiltViewModel
class IssueDraftViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val labelsApi: LabelsApi,
    private val issueImagesApi: IssueImagesApi,
    private val issueDraftsApi: IssueDraftsApi,
    private val attachmentsApi: AttachmentsApi,
    @ApplicationContext private val appContext: android.content.Context,
) : ViewModel() {

    private val draftIdArg: String? = savedStateHandle.get<String>("draftId")?.takeIf { it.isNotBlank() }
    private val parentArg: String? = savedStateHandle.get<String>("parent")?.takeIf { it.isNotBlank() }
    private val statusArg: String? = savedStateHandle.get<String>("status")?.takeIf { it.isNotBlank() }

    /** Share mode: the `share-compose` route carries no draft id. */
    val shareMode: Boolean = draftIdArg == null
    val parentId: String? = parentArg.takeIf { !shareMode }
    val draftId: String = draftIdArg ?: UUID.randomUUID().toString()

    /** Uploads wait for the create and no draft row is ever written. */
    val deferUploads: Boolean = shareMode || parentId != null

    private val _state = MutableStateFlow(
        IssueDraftUiState(
            boardId = savedStateHandle.get<String>("board")?.takeIf { it.isNotBlank() },
            seeded = deferUploads,
            attachmentsKnown = deferUploads,
        ),
    )
    val state: StateFlow<IssueDraftUiState> = _state

    // `draft://` placeholder → the held pick (parent / share mode).
    private val pendingImages = mutableMapOf<String, Uri>()
    private val pendingMedia = mutableMapOf<String, PreparedMedia>()

    private val _message = MutableStateFlow<String?>(null)
    val message: StateFlow<String?> = _message
    fun consumeMessage() {
        _message.value = null
    }

    private val _busyIds = MutableStateFlow<Set<String>>(emptySet())
    val busyIds: StateFlow<Set<String>> = _busyIds

    // ── Vocabulary (keyed on the target board) ──────────────────────────────

    private val dbFlow = accountDatabaseFlow(auth, holder)
    private val boardIdFlow = _state.map { it.boardId }.distinctUntilChanged()
    private val allBoards = dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() }

    val board: StateFlow<BoardEntity?> = combine(allBoards, boardIdFlow) { boards, id ->
        id?.let { boards.firstOrNull { b -> b.id == it } }
    }.stateIn(viewModelScope, SharingStarted.Eagerly, null)

    private val teamId = board.map { it?.teamId }.distinctUntilChanged()

    private fun <T> teamQuery(empty: T, query: (com.exponential.app.data.db.ExponentialDatabase, String) -> kotlinx.coroutines.flow.Flow<T>) =
        combine(dbFlow, teamId) { db, team -> db to team }.flatMapLatest { (db, team) ->
            if (db == null || team == null) flowOf(empty) else query(db, team)
        }

    /** Same-team boards — the board chip shows with 2+. */
    val teamBoards: StateFlow<List<BoardEntity>> = combine(allBoards, teamId) { boards, team ->
        if (team == null) emptyList() else boards.filter { it.teamId == team }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val labels: StateFlow<List<LabelEntity>> = teamQuery(emptyList()) { db, team -> db.labelDao().observeByTeam(team) }
        .stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    val statuses: StateFlow<List<ResolvedIssueStatus>> =
        teamQuery(emptyList()) { db, team -> db.issueStatusDao().observeByTeam(team) }
            .map { rows ->
                if (rows.isEmpty()) IssueStatusResolver.builtinDefaults else IssueStatusResolver.teamStatuses(rows)
            }
            .stateIn(viewModelScope, SharingStarted.Eagerly, IssueStatusResolver.builtinDefaults)

    private val members = teamQuery(emptyList()) { db, team -> db.teamMemberDao().observeByTeam(team) }

    /** The team's member users — assignee picker + @-mention vocabulary (EXP-487). */
    val teamUsers: StateFlow<List<UserEntity>> = teamQuery(emptyList()) { db, team -> db.userDao().observeByTeam(team) }
        .stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    /** EXP-50: the lone member of a solo team — hides the assignee chip. */
    val soloMemberId: StateFlow<String?> = members
        .map { rows -> rows.map { it.userId }.singleOrNull() }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    val permissions: StateFlow<TeamPermissions> = combine(
        teamQuery(null) { db, team -> db.teamDao().observeById(team) },
        members,
        auth.userId,
        auth.isAdmin,
    ) { team, rows, userId, isAdmin ->
        TeamPermissions.resolve(
            team = team,
            currentUserId = userId,
            isAdmin = isAdmin,
            isMember = userId != null && rows.any { it.userId == userId },
            memberRole = rows.firstOrNull { it.userId == userId }?.role,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TeamPermissions.Denied)

    /** `#IDENTIFIER` autocomplete: the target team's issues, newest first. */
    val issueRefCandidates: StateFlow<List<IssueRefTarget>> = combine(
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
        teamBoards,
        statuses,
    ) { issues, boards, statusRows ->
        val ids = boards.map { it.id }.toSet()
        issues.filter { it.boardId in ids }
            .sortedByDescending { it.createdAt }
            .map { issueRefTarget(it, statusRows) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The issue this one is filed under (parent mode) — the parent line. */
    val parentIssue: StateFlow<IssueEntity?> = dbFlow
        .flatMapLatest { db -> if (db == null || parentId == null) flowOf(null) else db.issueDao().observeById(parentId) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    suspend fun searchIssueRefs(query: String): List<IssueRefTarget> {
        val accountId = auth.activeAccountId.value ?: return emptyList()
        val team = board.value?.teamId ?: return emptyList()
        return try {
            issuesApi.search(accountId, team, query).map(::issueRefTarget)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            emptyList()
        }
    }

    // ── Seed + vocabulary upkeep ────────────────────────────────────────────

    // The draft's status_id (or the route's `status`), held until the team's
    // rows resolve it.
    private var pendingStatusId: String? = statusArg
    private var userEdited = false

    // Declared before `init`, which starts the debounce collector.
    private val textEdits = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
    private val saveMutex = Mutex()
    private val dirtyAgain = AtomicBoolean(false)

    // No write may land after the issue was filed or the draft discarded.
    @Volatile private var sealed = false
    @Volatile private var leaving = false
    @Volatile private var left = false

    // The row as last written (or as seeded): an unchanged form writes nothing.
    @Volatile private var lastWritten: DraftSnapshot? = null

    // The row's attachments are KNOWN (listed, or there was no row): until
    // then a file-only draft looks empty and must never be deleted.
    @Volatile private var attachmentsKnown = false

    private val _uploadsInFlight = MutableStateFlow(0)
    /** Uploads (file, image, media) still running — Create waits for them. */
    val uploadsInFlight: StateFlow<Int> = _uploadsInFlight

    private val _createdIssueId = MutableStateFlow<String?>(null)
    /** One-shot: the filed issue the page navigates to. */
    val createdIssueId: StateFlow<String?> = _createdIssueId
    fun consumeCreated() {
        _createdIssueId.value = null
    }

    /** EXP-1231: another client consumed this draft — the page's own exit, never the leave prompt. */
    sealed interface Consumed {
        /** Created elsewhere: open [issueId]'s detail in this page's place, no prompt, no toast. */
        data class Created(val issueId: String) : Consumed

        /** Discarded elsewhere: toast [IssueDraftPage.DISCARDED_ELSEWHERE] and go Back. */
        data object Discarded : Consumed
    }

    private val _consumedElsewhere = MutableStateFlow<Consumed?>(null)
    /** One-shot, like [createdIssueId]: this draft was consumed by another client. */
    val consumedElsewhere: StateFlow<Consumed?> = _consumedElsewhere
    fun consumeConsumed() {
        _consumedElsewhere.value = null
    }

    // EXP-1231: a SEEN row is missing (discarded elsewhere, or a resync gap):
    // nothing is written until it returns or the grace concludes.
    @Volatile private var holdWrites = false
    private var graceJob: Job? = null
    // The store's latest verdict. Nothing is concluded while the page's own
    // Create is in flight, so a FAILED create judges this again (iOS
    // `judgeFate()` after the create failure): a row gone meanwhile holds
    // and starts its grace, a grace that ran out meanwhile is re-armed.
    @Volatile private var lastFate: IssueDraftPage.Fate? = null

    init {
        if (!deferUploads) {
            viewModelScope.launch {
                val db = dbFlow.filterNotNull().first()
                val row = db.issueDraftDao().observeById(draftId).first()
                if (row == null) {
                    attachmentsKnown = true
                    _state.update { it.copy(seeded = true, attachmentsKnown = true) }
                } else if (userEdited) {
                    // Typed before the row arrived: keep the typing, but the
                    // row is real (a leave may now owe a delete).
                    lastWritten = row.toSnapshot()
                    _state.update { it.copy(seeded = true, rowExists = true, boardId = it.boardId ?: row.boardId) }
                    loadAttachments()
                } else {
                    lastWritten = row.toSnapshot()
                    row.statusId?.let { pendingStatusId = it }
                    _state.update {
                        it.copy(
                            title = row.title,
                            description = row.description,
                            priority = IssuePriority.fromWire(row.priority),
                            assigneeId = row.assigneeId,
                            dueDate = row.dueDate,
                            labelIds = row.labelIds.toSet(),
                            boardId = row.boardId.ifBlank { null } ?: it.boardId,
                            rowExists = true,
                            seeded = true,
                        )
                    }
                    repointStatus(statuses.value)
                    loadAttachments()
                }
            }
        }
        viewModelScope.launch { statuses.collect(::repointStatus) }
        // EXP-50: a solo team pins the assignee to its lone member.
        viewModelScope.launch {
            soloMemberId.collect { solo -> if (solo != null) _state.update { it.copy(assigneeId = solo) } }
        }
        // Drop selections the loaded vocabulary does not know.
        viewModelScope.launch {
            labels.collect { rows ->
                if (rows.isEmpty()) return@collect
                val known = rows.map { it.id }.toSet()
                _state.update { it.copy(labelIds = it.labelIds.filter { id -> id in known }.toSet()) }
            }
        }
        viewModelScope.launch {
            teamUsers.collect { rows ->
                if (rows.isEmpty()) return@collect
                _state.update { s ->
                    if (s.assigneeId != null && rows.none { it.id == s.assigneeId }) s.copy(assigneeId = null) else s
                }
            }
        }
        // An unresolvable board (deleted, trashed, no access) ends the page.
        viewModelScope.launch {
            combine(allBoards, boardIdFlow) { boards, id -> boards.isNotEmpty() && id != null && boards.none { it.id == id } }
                .collect { missing -> if (missing && !shareMode) _state.update { it.copy(loadFailed = true) } }
        }
        if (!deferUploads) watchConsumption()
        // The text autosave: AUTOSAVE_DEBOUNCE_MS after the last edit.
        viewModelScope.launch {
            textEdits.debounce(IssueDraftPage.AUTOSAVE_DEBOUNCE_MS).collect { requestSave() }
        }
    }

    /**
     * EXP-1231 (×4: web `issue-draft-page.ts`, iOS, desktop): the same draft
     * may be open on several clients; whichever creates or discards it
     * consumes the row for all. Watches this draft's row and any issue
     * carrying its id ([IssueDraftPage.fate]). `seen` counts this page's own
     * upsert mirror too; it resets per account database.
     */
    private fun watchConsumption() {
        var seen = false
        viewModelScope.launch {
            dbFlow.filterNotNull().flatMapLatest { db ->
                combine(
                    db.issueDraftDao().observeById(draftId),
                    db.issueDao().observeByDraftId(draftId),
                ) { row, issue -> (row != null) to issue?.id }
                    .onStart { seen = false }
            }.collect { (present, createdIssueId) ->
                if (present) seen = true
                val fate = IssueDraftPage.fate(seen, present, createdIssueId)
                lastFate = fate
                onFate(fate)
            }
        }
    }

    // Nothing is concluded while the page's own Create is in flight, nor
    // after its own Create / Discard / leave.
    private fun concluded() = sealed || left || _state.value.creating

    private fun onFate(fate: IssueDraftPage.Fate) {
        when (fate) {
            is IssueDraftPage.Fate.Created -> {
                if (concluded()) return
                sealed = true
                left = true
                graceJob?.cancel()
                graceJob = null
                _consumedElsewhere.value = Consumed.Created(fate.issueId)
            }
            IssueDraftPage.Fate.Gone -> {
                if (concluded() || graceJob?.isActive == true) return
                holdWrites = true
                graceJob = viewModelScope.launch {
                    delay(IssueDraftPage.DISCARDED_GRACE_MS)
                    if (!holdWrites || concluded()) return@launch
                    sealed = true
                    left = true
                    _consumedElsewhere.value = Consumed.Discarded
                }
            }
            IssueDraftPage.Fate.Open -> {
                graceJob?.cancel()
                graceJob = null
                if (holdWrites) {
                    holdWrites = false
                    // Edits made while held wrote nothing: save them now.
                    if (!concluded()) requestSave()
                }
            }
        }
    }

    // Draft attachments are not in the attachments shape (it is issue-scoped),
    // so they come over tRPC; inline images and clips already render from the
    // description. Only a SUCCESSFUL list makes them known.
    private suspend fun loadAttachments() {
        val accountId = auth.activeAccountId.value ?: return
        val files = try {
            issueDraftsApi.listAttachments(accountId, draftId)
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (error: Throwable) {
            android.util.Log.w("IssueDraftViewModel", "Draft attachments load failed", error)
            return
        }.filterNot { isInlineImage(it.contentType) || isInlineMedia(it.contentType) }
        _state.update { s ->
            s.copy(attachments = files + s.attachments.filterNot { a -> files.any { it.id == a.id } }, attachmentsKnown = true)
        }
        attachmentsKnown = true
    }

    private fun repointStatus(rows: List<ResolvedIssueStatus>) {
        if (rows.isEmpty()) return
        val fromPending = pendingStatusId?.let { id -> rows.firstOrNull { it.rowId == id || it.id == id } }
        if (fromPending != null) {
            pendingStatusId = null
            _state.update { it.copy(status = fromPending) }
            return
        }
        _state.update { s ->
            val picked = s.status
            val next = rows.firstOrNull { it.id == picked.id }
                // A pick against the constructed fallback set re-keys through
                // its builtin key once the real rows land.
                ?: picked.builtinKey?.let { key -> rows.firstOrNull { it.builtinKey == key } }
                ?: rows.firstOrNull { it.builtinKey == IssueStatus.Backlog }
                ?: rows.firstOrNull { it.category == IssueStatusCategory.Backlog }
                ?: rows.first()
            s.copy(status = next)
        }
    }

    // ── Edits ───────────────────────────────────────────────────────────────

    fun onTitleChange(title: String) {
        userEdited = true
        _state.update { it.copy(title = title) }
        textEdits.tryEmit(Unit)
    }

    fun onDescriptionChange(description: String) {
        userEdited = true
        _state.update { it.copy(description = description) }
        textEdits.tryEmit(Unit)
    }

    /** A chip pick saves at once. */
    fun onChipChange(transform: (IssueDraftUiState) -> IssueDraftUiState) {
        _state.update(transform)
        requestSave()
    }

    fun setStatus(status: ResolvedIssueStatus) {
        pendingStatusId = null
        onChipChange { it.copy(status = status) }
    }

    /** Re-point the page at another board (the board chip / share mode). */
    fun setBoard(boardId: String) {
        if (boardId == _state.value.boardId) return
        onChipChange { it.copy(boardId = boardId) }
    }

    fun applySharePrefill(prefill: SharePrefill) {
        if (!shareMode || userEdited) return
        pendingImages.putAll(prefill.pendingImages)
        _state.update { it.copy(title = prefill.title, description = prefill.description) }
    }

    suspend fun createLabel(name: String, color: String): LabelEntity? {
        val team = board.value?.teamId ?: return null
        val accountId = auth.activeAccountId.value ?: return null
        return runCatching { labelsApi.create(accountId, CreateLabelInput(team, name.trim(), color)) }
            .onSuccess { created -> runCatching { holder.database(forAccountId = accountId).labelDao().upsert(created) } }
            .onFailure { error ->
                if (error is CancellationException) throw error
                _message.value = trpcErrorMessage(error, "Failed to create label")
            }
            .getOrNull()
    }

    // ── Autosave: ONE single-flight path ────────────────────────────────────

    private fun hasContent(s: IssueDraftUiState) =
        IssueDraftPage.hasContent(s.title, s.description, s.attachments.size)

    /** Coalesced save on the process-lifetime scope (a leave must outlive the VM). */
    private fun requestSave() {
        if (deferUploads) return
        dirtyAgain.set(true)
        draftFlushScope.launch { drain() }
    }

    // A snapshot arriving mid-request re-runs the save once more; the loop
    // re-checks after unlocking so a late flag can never be stranded.
    private suspend fun drain() {
        while (true) {
            if (!saveMutex.tryLock()) return
            try {
                while (dirtyAgain.getAndSet(false)) writeOnce()
            } finally {
                saveMutex.unlock()
            }
            if (!dirtyAgain.get()) return
        }
    }

    /**
     * Flush now and wait for every queued write, then SEAL inside the same
     * lock (before Create): no drain can upsert between the flush and the
     * create, which would resurrect the row the create deletes.
     */
    private suspend fun flushAndSeal() {
        if (deferUploads) {
            sealed = true
            return
        }
        dirtyAgain.set(true)
        saveMutex.withLock {
            while (dirtyAgain.getAndSet(false)) writeOnce()
            sealed = true
        }
    }

    /** One write; false = the form's content could not be saved. */
    private suspend fun writeOnce(): Boolean {
        if (sealed || holdWrites) return true
        val s = _state.value
        // Unseeded: the row is as it was, unless the user already typed.
        if (!s.seeded) return !userEdited
        val accountId = auth.activeAccountId.value ?: return false
        if (!hasContent(s)) {
            // Emptied: the row goes, but only on the way out, and only once
            // its attachments are known (a file-only draft is content).
            if (leaving && s.rowExists && attachmentsKnown) deleteRow(accountId)
            return true
        }
        val team = board.value?.teamId ?: return false
        val boardId = s.boardId ?: return false
        if (s.rowExists && snapshotOf(s, boardId) == lastWritten) return true
        return runCatching { upsert(accountId, team, boardId, s) }
            .onFailure { error ->
                if (error is CancellationException) throw error
                android.util.Log.w("IssueDraftViewModel", "Draft save failed", error)
            }
            // EXP-1231: CONFLICT = the draft was consumed elsewhere; the shape
            // delivers the outcome and the page leaves, never a save error.
            .fold(onSuccess = { true }, onFailure = ::isConsumedConflict)
    }

    /** The row as `issueDrafts.upsert` would write it. */
    private fun snapshotOf(s: IssueDraftUiState, boardId: String): DraftSnapshot {
        val known = labels.value.map { it.id }.toSet()
        // A `draft://` placeholder is never persisted.
        val placeholders = markdownEmbedUrls(s.description).filter { it.startsWith(DRAFT_URL_PREFIX) }
        return DraftSnapshot(
            boardId = boardId,
            title = s.title.trim(),
            description = removeMarkdownImagesByUrl(s.description, placeholders),
            // Never the constructed fallback: until the team's rows resolve
            // the pending (seeded) row id stands, else none.
            statusId = s.status.rowId ?: pendingStatusId?.takeIf { !it.startsWith("builtin:") },
            priority = s.priority.wire,
            assigneeId = s.assigneeId,
            labelIds = (if (known.isEmpty()) s.labelIds else s.labelIds.filter { it in known }).sorted(),
            dueDate = s.dueDate,
        )
    }

    private suspend fun upsert(accountId: String, team: String, boardId: String, s: IssueDraftUiState) {
        val snap = snapshotOf(s, boardId)
        val saved = issueDraftsApi.upsert(
            accountId,
            UpsertIssueDraftInput(
                id = draftId,
                teamId = team,
                boardId = snap.boardId,
                title = snap.title,
                description = snap.description,
                statusId = snap.statusId,
                priority = snap.priority,
                assigneeId = snap.assigneeId,
                // ALWAYS sent — an omitted list could never clear a selection.
                labelIds = snap.labelIds,
                dueDate = snap.dueDate,
            ),
        )
        lastWritten = snap
        _state.update { it.copy(rowExists = true) }
        runCatching { holder.database(forAccountId = accountId).issueDraftDao().upsert(saved) }
    }

    // Server first: a failed delete keeps the local row (the Drafts list
    // must not lose a draft the server still holds).
    private suspend fun deleteRow(accountId: String) {
        runCatching { issueDraftsApi.delete(accountId, draftId) }
            .onSuccess {
                lastWritten = null
                _state.update { it.copy(rowExists = false) }
                runCatching { holder.database(forAccountId = accountId).issueDraftDao().deleteById(draftId) }
            }
            .onFailure { android.util.Log.w("IssueDraftViewModel", "Draft delete failed", it) }
    }

    /** Title blur, description blur, ON_STOP, dispose. After [leave] the leave write already covers it. */
    fun flush() {
        if (!left) requestSave()
    }

    /** Leaving the page (Back, or the model clearing): one last write. */
    fun leave() {
        if (left) return
        left = true
        leaving = true
        requestSave()
    }

    /**
     * EXP-1212 "Keep as draft": the leave write, AWAITED. On failure the page
     * stays (the save error toasts) and the model is live again, so the next
     * edit, leave or keep writes once more. Runs on the process-lifetime
     * scope: the write outlives the page.
     */
    suspend fun keep(): Boolean {
        if (deferUploads) return false
        left = true
        leaving = true
        val ok = draftFlushScope.async {
            dirtyAgain.set(true)
            val saved = saveMutex.withLock {
                var all = true
                while (dirtyAgain.getAndSet(false)) all = writeOnce() && all
                all
            }
            // A save requested while this held the lock lost its tryLock: run it.
            if (dirtyAgain.get()) draftFlushScope.launch { drain() }
            saved
        }.await()
        if (!ok) {
            left = false
            leaving = false
            _message.value = "Couldn't save the draft"
        }
        return ok
    }

    override fun onCleared() {
        leave()
        super.onCleared()
    }

    /** Discard draft: the row goes (if any). The page asks first (EXP-1212). */
    fun discard() {
        left = true
        if (deferUploads) return
        sealed = true
        val accountId = auth.activeAccountId.value ?: return
        draftFlushScope.launch {
            saveMutex.withLock { if (_state.value.rowExists) deleteRow(accountId) }
        }
    }

    /**
     * The upload route 404s without the row: write it (as the form stands)
     * before the first eager upload. Under the save lock, so it never races
     * an autosave.
     */
    private suspend fun ensureDraft(): Boolean {
        // EXP-1231: a held (possibly consumed) draft takes no new row.
        if (holdWrites || sealed) return false
        if (_state.value.rowExists) return true
        val accountId = auth.activeAccountId.value ?: return false
        val team = board.value?.teamId ?: return false
        val boardId = _state.value.boardId ?: return false
        val ok = saveMutex.withLock {
            if (_state.value.rowExists) return@withLock true
            runCatching { upsert(accountId, team, boardId, _state.value) }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    // EXP-1231: consumed elsewhere — the page leaves, no error.
                    if (!isConsumedConflict(error)) _message.value = trpcErrorMessage(error, "Couldn't save the draft")
                }
                .isSuccess
        }
        // A save requested while this held the lock lost its tryLock: run it.
        if (dirtyAgain.get()) draftFlushScope.launch { drain() }
        return ok
    }

    private suspend fun <T> trackUpload(block: suspend () -> T): T {
        _uploadsInFlight.update { it + 1 }
        try {
            return block()
        } finally {
            _uploadsInFlight.update { it - 1 }
        }
    }

    // ── Uploads (eager on the draft path, held in parent / share mode) ──────

    suspend fun uploadImage(uri: Uri): String? {
        if (deferUploads) {
            val placeholder = "$DRAFT_URL_PREFIX${UUID.randomUUID()}"
            pendingImages[placeholder] = uri
            return placeholder
        }
        return trackUpload {
            if (!ensureDraft()) return@trackUpload null
            val accountId = auth.activeAccountId.value ?: return@trackUpload null
            val resolver = appContext.contentResolver
            val bytes = withContext(Dispatchers.IO) {
                runCatching { resolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull()
            } ?: return@trackUpload null
            val contentType = resolver.getType(uri) ?: "image/jpeg"
            val filename = displayName(uri) ?: "image"
            issueImagesApi.uploadDraft(accountId, draftId, bytes, filename, contentType).url
        }
    }

    suspend fun uploadMedia(media: PreparedMedia): String? {
        if (deferUploads) {
            val placeholder = "$DRAFT_URL_PREFIX${UUID.randomUUID()}"
            pendingMedia[placeholder] = media
            return placeholder
        }
        return trackUpload {
            if (!ensureDraft()) return@trackUpload null
            val accountId = auth.activeAccountId.value ?: return@trackUpload null
            issueImagesApi.uploadDraftMedia(accountId, draftId, media).url
        }
    }

    fun attachFile(uri: Uri) {
        if (deferUploads) {
            val name = sanitizeFilename(displayName(uri) ?: uri.lastPathSegment)
            _state.update { it.copy(heldFiles = it.heldFiles + HeldFile(UUID.randomUUID().toString(), uri, name)) }
            return
        }
        val key = UUID.randomUUID().toString()
        _state.update {
            it.copy(pendingUploads = it.pendingUploads + PendingFileUpload(key, sanitizeFilename(uri.lastPathSegment), uri))
        }
        runFileUpload(key)
    }

    fun retryFileUpload(key: String) {
        _state.update { s -> s.copy(pendingUploads = s.pendingUploads.map { if (it.key == key) it.copy(error = null) else it }) }
        runFileUpload(key)
    }

    fun dismissFileUpload(key: String) {
        _state.update { s -> s.copy(pendingUploads = s.pendingUploads.filterNot { it.key == key }) }
    }

    fun removeHeldFile(key: String) {
        _state.update { s -> s.copy(heldFiles = s.heldFiles.filterNot { it.key == key }) }
    }

    private fun runFileUpload(key: String) {
        viewModelScope.launch {
            val upload = _state.value.pendingUploads.firstOrNull { it.key == key } ?: return@launch
            val error = trackUpload { uploadDraftFile(upload.uri) }
            if (error == null) {
                dismissFileUpload(key)
                requestSave()
            } else {
                _state.update { s ->
                    s.copy(pendingUploads = s.pendingUploads.map { if (it.key == key) it.copy(error = error) else it })
                }
            }
        }
    }

    // Returns null on success (the row joins [IssueDraftUiState.attachments]),
    // else the reason shown on the pending row.
    private suspend fun uploadDraftFile(uri: Uri): String? {
        if (!ensureDraft()) return "Couldn't save the draft"
        val accountId = auth.activeAccountId.value ?: return "You are signed out"
        val resolver = appContext.contentResolver
        val filename = sanitizeFilename(displayName(uri) ?: uri.lastPathSegment)
        return try {
            val contentType = canonicalContentType(resolver.getType(uri))
            val bytes = withContext(Dispatchers.IO) { resolver.openInputStream(uri)?.use { it.readBytes() } }
                ?: return "Couldn't read $filename"
            if (bytes.size > MAX_FILE_UPLOAD_BYTES) {
                return "Over ${MAX_FILE_UPLOAD_BYTES / (1024 * 1024)} MB"
            }
            val uploaded = attachmentsApi.uploadDraft(accountId, draftId, bytes, filename, contentType)
            val row = IssueDraftAttachment(
                id = uploaded.id,
                filename = uploaded.filename,
                contentType = uploaded.contentType,
                sizeBytes = uploaded.sizeBytes,
                url = uploaded.url,
            )
            _state.update { it.copy(attachments = it.attachments + row) }
            null
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (error: Throwable) {
            android.util.Log.w("IssueDraftViewModel", "Draft file upload failed", error)
            trpcErrorMessage(error, "Upload failed")
        }
    }

    fun deleteAttachment(attachmentId: String) {
        _state.update { s -> s.copy(attachments = s.attachments.filterNot { it.id == attachmentId }) }
        val accountId = auth.activeAccountId.value ?: return
        draftFlushScope.launch {
            runCatching { attachmentsApi.delete(accountId, attachmentId) }
                .onFailure { android.util.Log.w("IssueDraftViewModel", "Draft attachment delete failed", it) }
        }
    }

    /** A draft file, fetched into the cache to open in another app. */
    suspend fun downloadToCache(file: IssueDraftAttachment): File? {
        val accountId = auth.activeAccountId.value ?: return null
        val dir = File(File(appContext.cacheDir, "attachments"), file.id)
        val target = File(dir, sanitizeFilename(file.filename))
        if (target.isFile && target.length() == file.sizeBytes) return target
        _busyIds.update { it + file.id }
        return try {
            val bytes = attachmentsApi.download(accountId, file.url)
            withContext(Dispatchers.IO) {
                dir.mkdirs()
                target.writeBytes(bytes)
            }
            target
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (error: Throwable) {
            _message.value = trpcErrorMessage(error, "Couldn't open ${file.filename}")
            null
        } finally {
            _busyIds.update { it - file.id }
        }
    }

    // ── Create ──────────────────────────────────────────────────────────────

    /**
     * Flush + seal, then file the issue on [viewModelScope] (a rotation never
     * cancels it). Success lands in [createdIssueId]; a failed or cancelled
     * create unseals and clears `creating`, so the page is never stranded.
     */
    fun create() {
        val s0 = _state.value
        if (s0.title.isBlank() || s0.creating || _uploadsInFlight.value > 0) return
        val boardId = s0.boardId ?: return
        _state.update { it.copy(creating = true) }
        viewModelScope.launch {
            var id: String? = null
            try {
                flushAndSeal()
                val s = _state.value
                val known = labels.value.map { it.id }.toSet()
                id = createIssueAwait(
                    boardId = boardId,
                    s = s,
                    labelIds = s.labelIds.filter { it in known },
                    // The server reparents the draft's attachments and deletes
                    // the row in the create's transaction — only for a row
                    // that EXISTS.
                    draftId = draftId.takeIf { !deferUploads && s.rowExists },
                )
            } finally {
                if (id == null) {
                    sealed = false
                    _state.update { it.copy(creating = false) }
                    // EXP-1231: a verdict the store delivered while the create
                    // was in flight was dropped by `concluded()`; a grace that
                    // fired meanwhile bailed and left the hold on. Judge again
                    // now that `creating` is off (the 3 s grace restarts).
                    lastFate?.let(::onFate)
                } else {
                    left = true
                    _createdIssueId.value = id
                }
            }
        }
    }

    private suspend fun createIssueAwait(
        boardId: String,
        s: IssueDraftUiState,
        labelIds: List<String>,
        draftId: String?,
    ): String? {
        val accountId = auth.activeAccountId.value ?: return null
        return try {
            val rawDescription = s.description.takeIf { it.isNotBlank() }
            // Upload only the held picks the description still references (REV-24).
            val referencedImages = rawDescription
                ?.let { md -> pendingImages.filterKeys { it in markdownImageUrls(md) } }
                .orEmpty()
            val referencedMedia = rawDescription
                ?.let { md -> pendingMedia.filterKeys { it in markdownEmbedUrls(md) } }
                .orEmpty()
            val strippedDescription = rawDescription
                ?.let { removeMarkdownImagesByUrl(it, referencedImages.keys + referencedMedia.keys) }
                ?.takeIf { it.isNotBlank() }

            val created = issuesApi.create(
                accountId,
                CreateIssueInput(
                    boardId = boardId,
                    title = s.title.trim(),
                    status = s.status.anchorWireOrNull(),
                    statusId = s.status.rowId,
                    priority = s.priority.wire,
                    description = strippedDescription,
                    assigneeId = s.assigneeId,
                    dueDate = s.dueDate,
                    labelIds = labelIds.takeIf { it.isNotEmpty() },
                    draftId = draftId,
                    parentId = parentId,
                ),
            )
            upsertCreatedLocally(accountId, created, labelIds)

            if (rawDescription != null && (referencedImages.isNotEmpty() || referencedMedia.isNotEmpty())) {
                val urlByPlaceholder = uploadPendingImages(accountId, created.id, referencedImages) +
                    uploadPendingMedia(accountId, created.id, referencedMedia)
                // A failed placeholder is stripped, never stored.
                val finalDescription = replaceMarkdownImageUrls(
                    markdown = removeMarkdownImagesByUrl(
                        rawDescription,
                        (referencedImages.keys + referencedMedia.keys).minus(urlByPlaceholder.keys),
                    ),
                    replacements = urlByPlaceholder,
                )
                if (finalDescription != strippedDescription.orEmpty() && finalDescription.isNotBlank()) {
                    val updated = issuesApi.update(accountId, UpdateIssueInput(id = created.id, description = finalDescription))
                    runCatching { holder.database(forAccountId = accountId).issueDao().upsert(updated) }
                }
            }
            // Held files last: a failed one never fails the committed create.
            if (s.heldFiles.isNotEmpty()) uploadHeldFiles(accountId, created.id, s.heldFiles)
            created.id
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (error: Throwable) {
            _message.value = trpcErrorMessage(error, "Failed to create issue")
            null
        }
    }

    /**
     * Mirror the new issue (and its label joins) into Room at once — the share
     * path cold-starts the process, so its shape may still be catching up
     * (EXP-19). The server deleted the draft in the create's transaction, so
     * the local row goes too (EXP-878). Best-effort, idempotent.
     */
    private suspend fun upsertCreatedLocally(accountId: String, issue: IssueEntity, labelIds: List<String>) {
        runCatching {
            val db = holder.database(forAccountId = accountId)
            db.issueDao().upsert(issue)
            db.issueDraftDao().deleteById(draftId)
            val team = db.boardDao().getActiveById(issue.boardId)?.teamId ?: board.value?.teamId
            if (team != null) {
                for (labelId in labelIds) {
                    db.issueLabelDao().upsert(IssueLabelEntity(issueId = issue.id, labelId = labelId, teamId = team))
                }
            }
        }
    }

    private suspend fun uploadPendingImages(accountId: String, issueId: String, pending: Map<String, Uri>): Map<String, String> {
        val out = mutableMapOf<String, String>()
        val resolver = appContext.contentResolver
        for ((placeholder, uri) in pending) {
            try {
                val bytes = withContext(Dispatchers.IO) { resolver.openInputStream(uri)?.use { it.readBytes() } } ?: continue
                val contentType = resolver.getType(uri) ?: "image/jpeg"
                out[placeholder] = issueImagesApi.upload(accountId, issueId, bytes, displayName(uri) ?: "image", contentType).url
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (error: Throwable) {
                // Stripped from the final description; logged (EXP-61).
                android.util.Log.w("IssueDraftViewModel", "Pending image upload failed", error)
            }
        }
        return out
    }

    private suspend fun uploadPendingMedia(accountId: String, issueId: String, pending: Map<String, PreparedMedia>): Map<String, String> {
        val out = mutableMapOf<String, String>()
        val failed = mutableListOf<String>()
        for ((placeholder, media) in pending) {
            try {
                out[placeholder] = issueImagesApi.uploadMedia(accountId, issueId, media).url
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (error: Throwable) {
                android.util.Log.w("IssueDraftViewModel", "Pending media upload failed", error)
                failed += media.filename
            }
        }
        reportFailed(failed)
        return out
    }

    private suspend fun uploadHeldFiles(accountId: String, issueId: String, files: List<HeldFile>) {
        val resolver = appContext.contentResolver
        val failed = mutableListOf<String>()
        for (file in files) {
            try {
                val contentType = canonicalContentType(resolver.getType(file.uri))
                if (isInlineImage(contentType)) continue
                val bytes = withContext(Dispatchers.IO) { resolver.openInputStream(file.uri)?.use { it.readBytes() } }
                if (bytes == null) {
                    failed += file.filename
                    continue
                }
                if (bytes.size > MAX_FILE_UPLOAD_BYTES) {
                    failed += "${file.filename} (over ${MAX_FILE_UPLOAD_BYTES / (1024 * 1024)} MB)"
                    continue
                }
                attachmentsApi.upload(accountId, issueId, bytes, file.filename, contentType)
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (error: Throwable) {
                android.util.Log.w("IssueDraftViewModel", "Held file upload failed", error)
                failed += file.filename
            }
        }
        reportFailed(failed)
    }

    // A file the user attached never disappears without a word.
    private fun reportFailed(failed: List<String>) {
        if (failed.isEmpty()) return
        _message.value = "Couldn't attach ${failed.joinToString(", ")}. " +
            "Add ${if (failed.size == 1) "it" else "them"} from the issue."
    }

    private fun displayName(uri: Uri): String? = runCatching {
        appContext.contentResolver
            .query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                if (cursor.moveToFirst() && idx >= 0) cursor.getString(idx) else null
            }
    }.getOrNull() ?: uri.lastPathSegment
}

/**
 * EXP-1231: `issueDrafts.upsert` refuses a draft id another client already
 * consumed (tRPC CONFLICT). Not a save failure: the synced store carries the
 * outcome and the page follows it.
 */
private fun isConsumedConflict(error: Throwable): Boolean {
    val trpc = error as? TrpcException ?: return false
    return trpc.code == "CONFLICT" || trpc.status == HttpStatusCode.Conflict
}

/** The placeholder scheme a held image / clip carries until the create uploads it. */
internal const val DRAFT_URL_PREFIX = "draft://"

// Draft writes fired while leaving the page must outlive the ViewModel —
// navigation clears it as the page pops. Process-lifetime, mirroring
// descriptionFlushScope.
private val draftFlushScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

/** A draft row's written fields — what an unchanged form compares against. */
private data class DraftSnapshot(
    val boardId: String,
    val title: String,
    val description: String,
    val statusId: String?,
    val priority: String,
    val assigneeId: String?,
    val labelIds: List<String>,
    val dueDate: String?,
)

private fun com.exponential.app.data.db.IssueDraftEntity.toSnapshot() = DraftSnapshot(
    boardId = boardId,
    title = title,
    description = description,
    statusId = statusId,
    priority = priority,
    assigneeId = assigneeId,
    labelIds = labelIds.sorted(),
    dueDate = dueDate,
)
