package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.PrFilesResult
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.WorkspaceRepo
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.ReleasesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SubscriptionsApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.ReleaseEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.WorkspacePermissions
import com.exponential.app.domain.releaseComparator
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.extractDescriptionMarkdown
import com.exponential.app.ui.markdown.stripDraftImages
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock

data class IssueDetailState(
    val issue: IssueEntity? = null,
    val project: ProjectEntity? = null,
    val workspaceLabels: List<LabelEntity> = emptyList(),
    val issueLabels: List<LabelEntity> = emptyList(),
    val users: List<UserEntity> = emptyList(),
    val assignee: UserEntity? = null,
)

@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
@HiltViewModel
class IssueDetailViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val labelsApi: LabelsApi,
    private val releasesApi: ReleasesApi,
    private val subscriptionsApi: SubscriptionsApi,
    private val issueImagesApi: IssueImagesApi,
    private val repositoriesApi: RepositoriesApi,
    private val steerApi: SteerApi,
    private val stats: SyncStats,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
) : ViewModel() {

    val issueId: String = savedStateHandle["issueId"] ?: ""

    // Account scoping is reactive: every query chain hangs off the active
    // account's DB flow, so an account switch re-scopes all live data without
    // the nav shell rebuilding this ViewModel.
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val issueFlow = dbFlow.scopedQuery<IssueEntity?>(null) { it.issueDao().observeById(issueId) }
    private val _project = MutableStateFlow<ProjectEntity?>(null)
    private val workspaceLabelsFlow = combine(dbFlow, _project) { db, project -> db to project }
        .flatMapLatest { (db, project) ->
            if (db == null || project == null) flowOf(emptyList())
            else db.labelDao().observeByWorkspace(project.workspaceId)
        }
    private val workspaceForProject = combine(dbFlow, _project) { db, project -> db to project }
        .flatMapLatest { (db, project) ->
            if (db == null || project == null) flowOf(null)
            else db.workspaceDao().observeById(project.workspaceId)
        }
    private val membersForWorkspace = combine(dbFlow, _project) { db, project -> db to project }
        .flatMapLatest { (db, project) ->
            if (db == null || project == null) flowOf(emptyList())
            else db.workspaceMemberDao().observeByWorkspace(project.workspaceId)
        }

    // EXP-50: the workspace's lone HUMAN member (agent users excluded) when it
    // has exactly one — else null. A solo workspace hides the assignee row in
    // the detail editor (mirrors CreateIssueScreen).
    val soloMemberId: StateFlow<String?> = combine(
        membersForWorkspace,
        dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
    ) { members, users ->
        val agentIds = users.filter { it.isAgent }.map { it.id }.toSet()
        members.map { it.userId }.filter { it !in agentIds }.singleOrNull()
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    val permissions: StateFlow<WorkspacePermissions> = combine(
        workspaceForProject,
        membersForWorkspace,
        auth.userId,
        auth.isAdmin,
    ) { workspace, members, userId, isAdmin ->
        WorkspacePermissions.resolve(
            workspace = workspace,
            currentUserId = userId,
            isAdmin = isAdmin,
            isMember = userId != null && members.any { it.userId == userId },
            memberRole = members.firstOrNull { it.userId == userId }?.role,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), WorkspacePermissions.Denied)

    // "Syncing workspace…" banner while an un-synced member's membership row is
    // still in flight (so read-only controls don't read as a silent denial).
    val syncBanner: StateFlow<SyncBanner> = combine(
        permissions,
        auth.activeAccountId,
        stats.state,
    ) { perms, accountId, all ->
        syncBannerFor(perms, all[accountId]?.get(MEMBERS_SHAPE))
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SyncBanner.None)

    // Canonical web URL for the share sheet: {base}/w/{ws}/projects/{proj}/issues/{id}.
    // Null until issue + project + workspace + instance URL are all resolved.
    val shareUrl: StateFlow<String?> = combine(
        issueFlow,
        _project,
        workspaceForProject,
        auth.instanceUrl,
    ) { issue, project, workspace, base ->
        if (issue == null || project == null || workspace == null || base.isNullOrBlank()) null
        else com.exponential.app.domain.WebLinks.issueUrl(
            base = base,
            workspaceSlug = workspace.slug,
            projectSlug = project.slug,
            identifier = issue.identifier,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    val state: StateFlow<IssueDetailState> = combine(
        issueFlow,
        _project,
        workspaceLabelsFlow,
        dbFlow.scopedQuery(emptyList()) { it.issueLabelDao().observeByIssue(issueId) },
        dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
    ) { issue, project, allLabels, joins, users ->
        val labelsById = allLabels.associateBy { it.id }
        IssueDetailState(
            issue = issue,
            project = project,
            workspaceLabels = allLabels,
            issueLabels = joins.mapNotNull { labelsById[it.labelId] },
            users = users,
            assignee = issue?.assigneeId?.let { id -> users.firstOrNull { it.id == id } },
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueDetailState())

    // ── Releases (EXP-56) ─────────────────────────────────────────────────────

    // The issue's workspace's releases in canonical order (unshipped by target
    // date first, shipped last) — the single-select release picker's options.
    val workspaceReleases: StateFlow<List<ReleaseEntity>> = combine(
        dbFlow, _project,
    ) { db, project -> db to project }
        .flatMapLatest { (db, project) ->
            if (db == null || project == null) flowOf(emptyList())
            else db.releaseDao().observeByWorkspace(project.workspaceId)
        }
        .map { it.sortedWith(releaseComparator) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The release this issue currently ships in (null = none). */
    val currentRelease: StateFlow<ReleaseEntity?> = combine(
        issueFlow, workspaceReleases,
    ) { issue, releases ->
        issue?.releaseId?.let { id -> releases.firstOrNull { it.id == id } }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** Single-select: move the issue into [releaseId], or out with null. */
    fun setRelease(releaseId: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { releasesApi.setIssueRelease(accountId, issueId, releaseId) }
        }
    }

    // Subscription state (separate StateFlow — the main combine is at the 5-arg
    // typed cap). Drives the Bell/BellOff toggle in the detail top bar.
    val isSubscribed: StateFlow<Boolean> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueSubscriberDao().observeByIssue(issueId) },
        auth.userId,
    ) { subs, userId ->
        userId != null && subs.any { it.userId == userId && !it.unsubscribed }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    fun toggleSubscribe() {
        val subscribed = isSubscribed.value
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                if (subscribed) subscriptionsApi.unsubscribe(accountId, issueId)
                else subscriptionsApi.subscribe(accountId, issueId)
            }
        }
    }

    // ── Steer: remote start + live session (masterplan §5b/§5c) ──────────────

    // The running coding session for this issue (synced coding_sessions shape);
    // multi-window desktops can run several — surface the most recent.
    val runningSession: StateFlow<CodingSessionEntity?> = dbFlow
        .scopedQuery(emptyList()) { it.codingSessionDao().observeByIssue(issueId) }
        .map { rows ->
            rows.filter { it.status == DomainContract.codingSessionStatusRunning }
                .maxByOrNull { it.startedAt }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // steer.config is env-derived and static per instance: null = still loading.
    private val _steerEnabled = MutableStateFlow<Boolean?>(null)
    val steerEnabled: StateFlow<Boolean?> = _steerEnabled

    // The caller's online desktops (relay presence). null = not loaded yet.
    private val _steerDevices = MutableStateFlow<List<SteerDevice>?>(null)
    val steerDevices: StateFlow<List<SteerDevice>?> = _steerDevices

    private val _startState = MutableStateFlow<SteerStartState>(SteerStartState.Idle)
    val startState: StateFlow<SteerStartState> = _startState

    fun startOnDesktop(device: SteerDevice) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _startState.value = SteerStartState.Sending
            try {
                steerApi.startSession(accountId, issueId, device.deviceId)
                _startState.value = SteerStartState.Sent(device.deviceLabel)
                // The desktop inserts the coding_sessions row when the launcher
                // spins up, which swaps the panel via Electric. Re-enable after
                // a grace window in case it never picks up.
                delay(30_000)
                if (_startState.value is SteerStartState.Sent) {
                    _startState.value = SteerStartState.Idle
                }
            } catch (t: Throwable) {
                // Surfaces PRECONDITION_FAILED reasons (device offline, no
                // linked repository, relay off) from the steer router.
                _startState.value = SteerStartState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    // ── Duplicate-of (masterplan §5e) ─────────────────────────────────────────

    /** The canonical issue this one duplicates (null when not marked / not visible). */
    val duplicateOf: StateFlow<IssueEntity?> =
        combine(dbFlow, issueFlow) { db, issue -> db to issue?.duplicateOfId }
            .flatMapLatest { (db, dupId) ->
                if (db == null || dupId == null) flowOf(null)
                else db.issueDao().observeById(dupId)
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** Candidate canonical issues: same workspace, not this issue, not archived. */
    val duplicateCandidates: StateFlow<List<IssueEntity>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.projectDao().observeAll() },
        _project,
    ) { issues, projects, project ->
        if (project == null) {
            emptyList()
        } else {
            val workspaceProjectIds = projects
                .filter { it.workspaceId == project.workspaceId }
                .map { it.id }
                .toSet()
            issues
                .filter { it.projectId in workspaceProjectIds && it.id != issueId && it.archivedAt == null }
                .sortedByDescending { it.updatedAt }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // ── Issue-reference pills (masterplan §5e) ────────────────────────────────

    /**
     * This workspace's synced issues, newest-first — drives inline
     * `#IDENTIFIER` pill resolution in the description + comments AND the
     * editors' #-autocomplete (identifier/title search, empty query = most
     * recent). Scoped to this issue's workspace (same-prefix identifiers from
     * another synced workspace never leak in), mirroring the web
     * IssueRefProvider.
     */
    val issueRefCandidates: StateFlow<List<IssueRefTarget>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.projectDao().observeAll() },
        _project,
    ) { issues, projects, project ->
        if (project == null) {
            emptyList()
        } else {
            val workspaceProjectIds = projects
                .filter { it.workspaceId == project.workspaceId }
                .map { it.id }
                .toSet()
            issues
                .filter { it.projectId in workspaceProjectIds }
                .sortedByDescending { it.createdAt }
                .map { IssueRefTarget(it.id, it.identifier, it.title) }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Atomically set duplicateOfId + status='duplicate'. */
    fun markDuplicate(canonicalId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { issuesApi.setDuplicateOf(accountId, issueId, canonicalId) }
        }
    }

    /** Clear the FK and restore a non-terminal status. */
    fun unmarkDuplicate() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { issuesApi.setDuplicateOf(accountId, issueId, null) }
        }
    }

    // Debounced description autosave: editing fires updateDescription() on every
    // keystroke, but we only hit the API after the user pauses (or on flush),
    // instead of one tRPC mutation per character.
    private val descriptionInput = MutableStateFlow<String?>(null)

    // Surfaced when a description save fails after retries (snackbar in the
    // screen). The draft stays in descriptionInput, so a later edit or flush
    // retries — the edit is never silently discarded.
    private val _descriptionSaveError = MutableStateFlow<String?>(null)
    val descriptionSaveError: StateFlow<String?> = _descriptionSaveError

    fun consumeDescriptionSaveError() {
        _descriptionSaveError.value = null
    }

    // The backing repo's full name (owner/name) for the project + issue coding
    // chips. repository_id rides on the synced projects shape; the name is a
    // server-only tRPC read, cached per (account, workspace) so the chip doesn't
    // refetch across recompositions or issue navigations. Declared BEFORE init:
    // the init coroutine below touches _repoName synchronously (Main.immediate +
    // collectLatest) during construction, so a later declaration leaves it null.
    private val _repoName = MutableStateFlow<String?>(null)
    val repoName: StateFlow<String?> = _repoName

    init {
        viewModelScope.launch {
            combine(dbFlow, issueFlow) { db, issue -> db to issue }
                .flatMapLatest { (db, issue) ->
                    if (db == null || issue == null) flowOf(null)
                    else db.projectDao().observeAll().map { projects ->
                        projects.firstOrNull { it.id == issue.projectId }
                    }
                }
                .collect { _project.value = it }
        }
        viewModelScope.launch {
            descriptionInput
                .filterNotNull()
                .debounce(800)
                .collect { saveDescription(it) }
        }
        // Resolve the backing repo's name for the project/coding chips whenever
        // the project (its repository_id) or active account changes.
        viewModelScope.launch {
            combine(auth.activeAccountId, _project) { a, p -> a to p }
                .collectLatest { (accountId, project) ->
                    val repoId = project?.repositoryId
                    if (accountId == null || project == null || repoId == null) {
                        _repoName.value = null
                        return@collectLatest
                    }
                    // Return null (not emptyList) on failure so a single transient
                    // network error is NOT memoized — the cache stays empty and the
                    // next resolve retries instead of pinning the chip null forever.
                    val repos = RepoRegistryCache.get(accountId, project.workspaceId) {
                        runCatching { repositoriesApi.list(accountId, project.workspaceId) }
                            .getOrNull()
                    }
                    if (repos != null) {
                        _repoName.value = repos.firstOrNull { it.id == repoId }?.fullName
                    }
                }
        }
        // Steer availability + device presence, re-fetched on account switch.
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _steerEnabled.value = null
                _steerDevices.value = null
                _startState.value = SteerStartState.Idle
                if (accountId == null) {
                    _steerEnabled.value = false
                    _steerDevices.value = emptyList()
                    return@collectLatest
                }
                val enabled = runCatching { steerApi.config(accountId).enabled }
                    .getOrDefault(false)
                _steerEnabled.value = enabled
                _steerDevices.value = if (enabled) {
                    runCatching { steerApi.myDevices(accountId).devices }
                        .getOrDefault(emptyList())
                } else {
                    emptyList()
                }
            }
        }
    }

    fun updateTitle(title: String) {
        if (title.isBlank()) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, title = title.trim()))
            }
        }
    }

    fun updateDescription(text: String) {
        descriptionInput.value = text
    }

    /** Persist the latest description immediately, e.g. when leaving the screen. */
    fun flushDescription() {
        val text = descriptionInput.value ?: return
        // Launched on a process-lifetime scope: this fires from onDispose while
        // navigation is about to clear the ViewModel, and viewModelScope
        // cancellation must not abort the final save mid-flight.
        descriptionFlushScope.launch { saveDescription(text) }
    }

    private suspend fun saveDescription(text: String) {
        val accountId = auth.activeAccountId.value ?: return
        // Never persist `draft://` placeholders: while an image upload is in
        // flight (or failed, awaiting retry) the editor's markdown contains
        // them; the editor emits the final markdown once the upload resolves.
        val sanitized = stripDraftImages(text)
        // Skip no-op saves (debounce can fire with the already-persisted value).
        if (sanitized == extractDescriptionMarkdown(state.value.issue?.description)) return
        // This can be the LAST chance to persist an edit (the leave-screen
        // flush), so a transient failure must not silently drop it: retry with
        // backoff, then surface the error instead of swallowing it.
        var attempt = 1
        while (true) {
            try {
                issuesApi.update(
                    accountId,
                    UpdateIssueInput(id = issueId, description = sanitized)
                )
                _descriptionSaveError.value = null
                return
            } catch (e: CancellationException) {
                throw e
            } catch (t: Throwable) {
                if (attempt >= DESCRIPTION_SAVE_ATTEMPTS) {
                    _descriptionSaveError.value =
                        trpcErrorMessage(t, "Description changes could not be saved")
                    return
                }
                delay(DESCRIPTION_SAVE_RETRY_DELAY_MS * attempt)
                attempt++
            }
        }
    }

    fun updateStatus(status: IssueStatus) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, status = status.wire))
            }
        }
    }

    fun updatePriority(priority: IssuePriority) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, priority = priority.wire))
            }
        }
    }

    fun updateDueDate(date: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, dueDate = date))
            }
        }
    }

    fun updateAssignee(userId: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, assigneeId = userId))
            }
        }
    }

    fun updateDueTime(time: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, dueTime = time))
            }
        }
    }

    fun updateEndTime(time: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, endTime = time))
            }
        }
    }

    fun updateRecurrence(interval: Int?, unit: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(
                    accountId,
                    UpdateIssueInput(
                        id = issueId,
                        recurrenceInterval = interval,
                        recurrenceUnit = unit,
                    )
                )
            }
        }
    }

    fun toggleLabel(labelId: String, isCurrentlyAssigned: Boolean) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                if (isCurrentlyAssigned) labelsApi.removeLabel(accountId, issueId, labelId)
                else labelsApi.addLabel(accountId, issueId, labelId)
            }
        }
    }

    fun createAndAssignLabel(name: String, color: String) {
        val workspaceId = _project.value?.workspaceId ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                val label = labelsApi.create(accountId, CreateLabelInput(workspaceId, name.trim(), color))
                labelsApi.addLabel(accountId, issueId, label.id)
                label
            }.onSuccess { label ->
                // Optimistic local upserts (the label + the issue join) so the
                // chip shows immediately; Electric re-delivers both on its next
                // poll, idempotent REPLACE.
                runCatching {
                    val db = holder.database(forAccountId = accountId)
                    db.labelDao().upsert(label)
                    db.issueLabelDao().upsert(
                        IssueLabelEntity(issueId = issueId, labelId = label.id, workspaceId = workspaceId)
                    )
                }
            }
        }
    }

    fun delete(onDeleted: () -> Unit) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { issuesApi.delete(accountId, issueId) }.onSuccess { onDeleted() }
        }
    }

    // Throws on upload failure (after logging) so the editor's per-row upload
    // state can surface the server's actual rejection ("Unsupported image
    // type", storage-limit, …) instead of an opaque retry badge (EXP-61).
    // Local read failures stay a benign null.
    suspend fun uploadImage(uri: android.net.Uri): String? {
        val accountId = auth.activeAccountId.value ?: return null
        val resolver = appContext.contentResolver
        val bytes = runCatching {
            resolver.openInputStream(uri)?.use { it.readBytes() }
        }.getOrNull() ?: return null
        val contentType = resolver.getType(uri) ?: "image/jpeg"
        val filename = run {
            resolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                if (cursor.moveToFirst() && idx >= 0) cursor.getString(idx) else null
            } ?: uri.lastPathSegment ?: "image"
        }
        try {
            return issueImagesApi.upload(accountId, issueId, bytes, filename, contentType).url
        } catch (cancel: kotlinx.coroutines.CancellationException) {
            throw cancel
        } catch (error: Throwable) {
            android.util.Log.w("IssueDetailViewModel", "Image upload failed (type=$contentType, ${bytes.size} bytes)", error)
            throw error
        }
    }

    // Middle Changes tier (masterplan §4.8): the exp/<IDENTIFIER> branch compared
    // against the repo default branch. Null when the branch was never pushed —
    // the view then falls through to the "being coded on <device>" tier.
    suspend fun loadBranchDiff(): PrFilesResult? {
        val accountId = auth.activeAccountId.value ?: return null
        return repositoriesApi.branchDiff(accountId, issueId)
    }
}

// Description saves fired while leaving the issue screen must outlive the
// ViewModel: viewModelScope is cancelled when navigation clears it, which
// could abort the final flush mid-request. Process-lifetime, mirroring the
// SyncManager/PushTokenManager scopes.
private val descriptionFlushScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

private const val DESCRIPTION_SAVE_ATTEMPTS = 3
private const val DESCRIPTION_SAVE_RETRY_DELAY_MS = 500L

// Process-wide cache of a workspace's repos (server-only, no Electric shape).
// Keyed by "accountId:workspaceId"; the create-project picker and this chip both
// read the registry, but the chip must not block on a per-recomposition fetch.
private object RepoRegistryCache {
    private val byKey = mutableMapOf<String, List<WorkspaceRepo>>()
    private val mutex = kotlinx.coroutines.sync.Mutex()

    // `load` returns null to signal a failed fetch: only successful results are
    // cached, so a transient failure leaves the cache empty and the next call
    // retries (a failed load never becomes a permanent empty list).
    suspend fun get(
        accountId: String,
        workspaceId: String,
        load: suspend () -> List<WorkspaceRepo>?,
    ): List<WorkspaceRepo>? {
        val key = "$accountId:$workspaceId"
        mutex.withLock { byKey[key] }?.let { return it }
        val loaded = load() ?: return null
        mutex.withLock { byKey[key] = loaded }
        return loaded
    }
}
