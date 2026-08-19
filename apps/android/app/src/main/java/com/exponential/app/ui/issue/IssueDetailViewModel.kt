package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.room.withTransaction
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.AttachmentsApi
import com.exponential.app.data.api.CreateLabelInput
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.NotificationsApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.WidgetSubmissionResult
import com.exponential.app.data.api.WidgetsApi
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.SubscriptionsApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.StartedRunKey
import com.exponential.app.domain.StartedRunMatch
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.MAX_FILE_UPLOAD_BYTES
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.TeamPermissions
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.isInlineImage
import com.exponential.app.domain.sanitizeFilename
import com.exponential.app.ui.markdown.AttachmentDims
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.extractDescriptionMarkdown
import com.exponential.app.ui.markdown.stripDraftImages
import dagger.hilt.android.lifecycle.HiltViewModel
import java.io.File
import java.util.UUID
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
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

data class IssueDetailState(
    val issue: IssueEntity? = null,
    val board: BoardEntity? = null,
    val teamLabels: List<LabelEntity> = emptyList(),
    val issueLabels: List<LabelEntity> = emptyList(),
    val users: List<UserEntity> = emptyList(),
    val assignee: UserEntity? = null,
)

/**
 * What to show while the issue isn't in the local cache. [Loading] is the
 * honest default (a push tap can beat sync by seconds); [Unavailable] is
 * reached only after the direct fetch failed — deleted, or not ours.
 */
enum class MissingIssueState { Loading, Unavailable }

@OptIn(ExperimentalCoroutinesApi::class, FlowPreview::class)
@HiltViewModel
class IssueDetailViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val labelsApi: LabelsApi,
    private val subscriptionsApi: SubscriptionsApi,
    private val issueImagesApi: IssueImagesApi,
    private val attachmentsApi: AttachmentsApi,
    private val notificationsApi: NotificationsApi,
    private val steerApi: SteerApi,
    private val devicesApi: DevicesApi,
    private val widgetsApi: WidgetsApi,
    private val stats: SyncStats,
    private val syncManager: SyncManager,
    @dagger.hilt.android.qualifiers.ApplicationContext
    private val appContext: android.content.Context,
) : ViewModel() {

    val issueId: String = savedStateHandle["issueId"] ?: ""

    // Account scoping is reactive: every query chain hangs off the active
    // account's DB flow, so an account switch re-scopes all live data without
    // the nav shell rebuilding this ViewModel.
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val issueFlow = dbFlow.scopedQuery<IssueEntity?>(null) { it.issueDao().observeById(issueId) }
    private val _board = MutableStateFlow<BoardEntity?>(null)
    private val teamLabelsFlow = combine(dbFlow, _board) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(emptyList())
            else db.labelDao().observeByTeam(board.teamId)
        }
    // The board team's status rows (EXP-314) — the detail picker's vocabulary
    // and the chip's label/glyph. Falls back to the constructed builtins until
    // the issue_statuses shape has synced.
    val teamStatuses: StateFlow<List<ResolvedIssueStatus>> =
        combine(dbFlow, _board) { db, board -> db to board }
            .flatMapLatest { (db, board) ->
                if (db == null || board == null) flowOf(emptyList())
                else db.issueStatusDao().observeByTeam(board.teamId)
            }
            .map { rows ->
                if (rows.isEmpty()) IssueStatusResolver.builtinDefaults
                else IssueStatusResolver.teamStatuses(rows)
            }
            .stateIn(
                viewModelScope,
                SharingStarted.WhileSubscribed(5_000),
                IssueStatusResolver.builtinDefaults,
            )

    private val teamForBoard = combine(dbFlow, _board) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(null)
            else db.teamDao().observeById(board.teamId)
        }
    private val membersForTeam = combine(dbFlow, _board) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(emptyList())
            else db.teamMemberDao().observeByTeam(board.teamId)
        }

    // EXP-487: the issue team's users — the assignee picker + @-mention
    // vocabulary. state.users stays account-wide so an ex-member assignee /
    // session owner still displays.
    val teamUsers: StateFlow<List<UserEntity>> =
        combine(dbFlow, _board) { db, board -> db to board }
            .flatMapLatest { (db, board) ->
                if (db == null || board == null) flowOf(emptyList())
                else db.userDao().observeByTeam(board.teamId)
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // EXP-312: live sessions are owner-only — the screen gates tap-to-watch
    // on the session row's userId matching this.
    val currentUserId: StateFlow<String?> = auth.userId

    // EXP-50: the team's lone member when it has exactly one — else null.
    // A solo team hides the assignee row in the detail editor (mirrors
    // CreateIssueScreen).
    val soloMemberId: StateFlow<String?> = membersForTeam
        .map { members -> members.map { it.userId }.singleOrNull() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    val permissions: StateFlow<TeamPermissions> = combine(
        teamForBoard,
        membersForTeam,
        auth.userId,
        auth.isAdmin,
    ) { team, members, userId, isAdmin ->
        TeamPermissions.resolve(
            team = team,
            currentUserId = userId,
            isAdmin = isAdmin,
            isMember = userId != null && members.any { it.userId == userId },
            memberRole = members.firstOrNull { it.userId == userId }?.role,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), TeamPermissions.Denied)

    // "Syncing team…" banner while an un-synced member's membership row is
    // still in flight (so read-only controls don't read as a silent denial).
    val syncBanner: StateFlow<SyncBanner> = combine(
        permissions,
        auth.activeAccountId,
        stats.state,
    ) { perms, accountId, all ->
        syncBannerFor(perms, all[accountId]?.get(MEMBERS_SHAPE))
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SyncBanner.None)

    // Canonical web URL for the share sheet: {base}/t/{team}/boards/{board}/issues/{identifier}.
    // Null until issue + board + team + instance URL are all resolved.
    val shareUrl: StateFlow<String?> = combine(
        issueFlow,
        _board,
        teamForBoard,
        auth.instanceUrl,
    ) { issue, board, team, base ->
        if (issue == null || board == null || team == null || base.isNullOrBlank()) null
        else com.exponential.app.domain.WebLinks.issueUrl(
            base = base,
            teamSlug = team.slug,
            boardSlug = board.slug,
            identifier = issue.identifier,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    val state: StateFlow<IssueDetailState> = combine(
        issueFlow,
        _board,
        teamLabelsFlow,
        dbFlow.scopedQuery(emptyList()) { it.issueLabelDao().observeByIssue(issueId) },
        dbFlow.scopedQuery(emptyList()) { it.userDao().observeAll() },
    ) { issue, board, allLabels, joins, users ->
        val labelsById = allLabels.associateBy { it.id }
        IssueDetailState(
            issue = issue,
            board = board,
            teamLabels = allLabels,
            issueLabels = joins.mapNotNull { labelsById[it.labelId] },
            users = users,
            assignee = issue?.assigneeId?.let { id -> users.firstOrNull { it.id == id } },
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), IssueDetailState())

    // Every synced attachment row of this issue — the ONE query behind both the
    // probed-dimension map (inline images) and the Files section (everything
    // else, EXP-297). Comment attachments belong to the same issue, so this
    // covers the description AND the whole thread.
    private val attachmentsFlow: StateFlow<List<AttachmentEntity>> =
        dbFlow.scopedQuery(emptyList()) { it.attachmentDao().observeByIssue(issueId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // Probed sizes of this issue's attachments (REV2-79) — read views pre-size
    // embedded images from them instead of measuring 0-height and jumping when
    // the bitmap lands.
    val attachmentDims: StateFlow<AttachmentDims> = attachmentsFlow
        .map { rows ->
            AttachmentDims(
                rows.mapNotNull { row ->
                    val width = row.width
                    val height = row.height
                    if (width == null || height == null) null
                    else row.id to (width to height)
                }.toMap()
            )
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AttachmentDims.Empty)

    // ── Files section (EXP-297) ──────────────────────────────────────────────

    /**
     * Non-inline-image attachments, oldest first. These never appear in the
     * markdown (only the five raster types are embeddable), so the Files
     * section is the only place they are visible — which is exactly why the
     * classification has to mirror the server's accepted-image set: anything
     * else, `image/tiff` included, lands here rather than nowhere.
     *
     * EXP-554: comment-linked rows are excluded — they render in their comment's
     * attachment strip, and listing them here too would double-list them.
     */
    val fileAttachments: StateFlow<List<AttachmentEntity>> = attachmentsFlow
        .map { rows ->
            rows.filter { it.commentId == null && !isInlineImage(it.contentType) }
                .sortedBy { it.createdAt }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _pendingFiles = MutableStateFlow<List<PendingFileUpload>>(emptyList())

    /**
     * In-flight / failed file uploads. An entry that already produced a server
     * row stays until that row lands via sync, so the list never flickers
     * empty between the 200 and the Electric delta — and is deduped by id, so
     * the synced row never renders twice.
     */
    val pendingFiles: StateFlow<List<PendingFileUpload>> =
        combine(_pendingFiles, attachmentsFlow) { pending, synced ->
            val syncedIds = synced.map { it.id }.toSet()
            pending.filterNot { it.uploadedId != null && it.uploadedId in syncedIds }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Attachment ids with a download or delete in flight (per-row spinner). */
    private val _busyAttachmentIds = MutableStateFlow<Set<String>>(emptySet())
    val busyAttachmentIds: StateFlow<Set<String>> = _busyAttachmentIds

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
            }.onFailure { t ->
                reportMutationFailure(
                    t,
                    if (subscribed) "You could not be unsubscribed"
                    else "You could not be subscribed",
                )
            }
        }
    }

    // ── Steer: remote start + live session (masterplan §5b/§5c) ──────────────

    // The running coding session for this issue (synced coding_sessions shape);
    // multi-window desktops can run several — surface the most recent.
    // Heartbeat-stale rows render as absent (EXP-153); the ticker clears the
    // panel once the liveness window elapses without a sync delta.
    val runningSession: StateFlow<CodingSessionEntity?> = combine(
        dbFlow.scopedQuery(emptyList()) { it.codingSessionDao().observeByIssue(issueId) },
        CodingSessionLiveness.minuteTicker(),
    ) { rows, now ->
        rows.filter { CodingSessionLiveness.isLive(it, now) }
            .maxByOrNull { it.startedAt }
    }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // steer.config is env-derived and static per instance: null = still loading.
    private val _steerEnabled = MutableStateFlow<Boolean?>(null)
    val steerEnabled: StateFlow<Boolean?> = _steerEnabled

    // EXP-496: the widget/agent submission metadata behind this issue
    // (widgets.submissionForIssue, server-only). null = loading, fetch failed
    // (non-member), or not a widget/agent issue — the card renders nothing in
    // every one of those states (web parity).
    private val _widgetSubmission = MutableStateFlow<WidgetSubmissionResult?>(null)
    val widgetSubmission: StateFlow<WidgetSubmissionResult?> = _widgetSubmission

    // The online machines a start can go to: the caller's own plus (EXP-432)
    // the board team's shared servers. null = not loaded yet.
    private val _steerDevices = MutableStateFlow<List<SteerDevice>?>(null)
    val steerDevices: StateFlow<List<SteerDevice>?> = _steerDevices

    private val _startState = MutableStateFlow<SteerStartState>(SteerStartState.Idle)
    val startState: StateFlow<SteerStartState> = _startState

    // EXP-536: the freshly-started run's session id — consumed exactly once
    // by the screen, which opens the live viewer on it.
    private val _startedSessionId = MutableStateFlow<String?>(null)
    val startedSessionId: StateFlow<String?> = _startedSessionId

    // The live rows the post-send watch scans — a start is only a command;
    // the desktop writes the coding_sessions row a moment later.
    private val liveSessionRows = dbFlow.scopedQuery(emptyList()) {
        it.codingSessionDao().observeByStatuses(CodingSessionLiveness.liveStatuses)
    }

    /**
     * Issues the Start-coding sheet can queue (EXP-156). Regular candidates need
     * a repo-backed, live board and to be open (status not
     * done/cancelled/duplicate, PR not merged), `updatedAt` desc. The CURRENT
     * issue is force-included and pinned first — exempt from the issue-level
     * rules AND the board-trash filter (a run can seed off a trashed board),
     * the same seed handling as desktop/iOS — but a repo-LESS current
     * issue stays OUT (nothing can host its run), so the sheet never seeds a
     * phantom id the batch logic can't back with a repository.
     */
    val startCandidates: StateFlow<List<StartIssueOption>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        _board,
    ) { issues, boards, board ->
        if (board == null) {
            emptyList()
        } else {
            // Every repo-backed board in the team (keyed by id), and the
            // subset that's also live. Seeds resolve against the former (repo is
            // the only hard requirement); regular candidates require the latter.
            val repoBoards = boards
                .filter { it.teamId == board.teamId && it.repositoryId != null }
                .associateBy { it.id }
            val liveRepoBoardIds = repoBoards.values
                .filter { it.deletedAt == null }
                .map { it.id }
                .toSet()
            // Force-include the current issue whenever its board has a repo,
            // regardless of the board being trashed or the issue's own state.
            val current = issues.firstOrNull {
                it.id == issueId && it.boardId in repoBoards.keys
            }
            val rest = issues
                .filter {
                    it.id != issueId &&
                        it.boardId in liveRepoBoardIds &&
                        it.status !in TERMINAL_ISSUE_STATUSES &&
                        it.prState != DomainContract.prStateMerged
                }
                .sortedByDescending { it.updatedAt }
            (listOfNotNull(current) + rest).map { issue ->
                StartIssueOption(
                    id = issue.id,
                    identifier = issue.identifier,
                    title = issue.title,
                    repositoryId = repoBoards[issue.boardId]?.repositoryId,
                    status = issue.status,
                    priority = issue.priority,
                )
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * Remote-start on the user's own desktop (EXP-156): [issueIds] of size 1
     * launches a plain single session, 2+ a batch (one Claude on one
     * `exp/batch-<id8>` branch spanning them). EXP-536: both then wait for
     * the desktop's coding_sessions row and surface it as [startedSessionId]
     * — the screen opens the live session rather than telling the user where
     * to find it.
     */
    fun startOnDesktop(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) {
        val key = StartedRunKey.forIssues(issueIds) ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _startState.value = SteerStartState.Sending
            try {
                if (issueIds.size >= 2) {
                    steerApi.startSession(accountId, issueIds, device.deviceId, options)
                } else {
                    steerApi.startSession(accountId, issueIds.first(), device.deviceId, options)
                }
                awaitStartedRun(key, device)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                // Surfaces PRECONDITION_FAILED reasons (device offline, no
                // linked repository, relay off) from the steer router.
                _startState.value = SteerStartState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    /**
     * Remote-run a team action from the unified sheet's Actions tab (EXP-257)
     * with the full option set + filled inputs; the builtin "Create action"
     * id additionally rides its teamId (server-required there, forbidden
     * otherwise). Same Sent/grace-window handling as [startOnDesktop].
     */
    fun runAction(
        device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: Map<String, String>,
    ) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _startState.value = SteerStartState.Sending
            try {
                steerApi.startActionSession(
                    accountId,
                    actionId = action.id,
                    deviceId = device.deviceId,
                    options = options,
                    // Required for EVERY builtin (there is no DB row to derive
                    // the team from), forbidden otherwise — the server rejects
                    // both mistakes.
                    teamId = action.teamId.takeIf { action.isBuiltin },
                    inputs = inputs.takeIf { it.isNotEmpty() },
                )
                awaitStartedRun(StartedRunKey.Action(action.name), device)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _startState.value = SteerStartState.Failed(
                    trpcErrorMessage(t, "The start command could not be delivered"),
                )
            }
        }
    }

    /**
     * EXP-536: hold a "waiting for the desktop" caption (the start circle's
     * spinner) until the run's synced row appears, then hand it to the
     * screen's navigation. The deadline surfaces as an error rather than a
     * silent fall back to Idle — a run the desktop REFUSED must not read the
     * same as one still starting.
     */
    private suspend fun awaitStartedRun(key: StartedRunKey, device: SteerDevice) {
        val label = device.deviceLabel.ifBlank { device.deviceId }
        _startState.value = SteerStartState.Sent(label)
        val userId = auth.userId.value
        val sessionId = if (userId == null) {
            null
        } else {
            StartedRunMatch.await(liveSessionRows, key, userId)
        }
        if (sessionId != null) {
            _startState.value = SteerStartState.Idle
            _startedSessionId.value = sessionId
        } else {
            _startState.value = SteerStartState.Failed(
                "$label never started this run. Open the Exponential desktop app " +
                    "there to see why.",
            )
        }
    }

    /** Clears the one-shot navigation signal once the screen has acted on it. */
    fun consumeStartedSession() {
        _startedSessionId.value = null
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

    /** Candidate canonical issues: same team, not this issue. */
    val duplicateCandidates: StateFlow<List<IssueEntity>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        _board,
    ) { issues, boards, board ->
        if (board == null) {
            emptyList()
        } else {
            val teamBoardIds = boards
                .filter { it.teamId == board.teamId }
                .map { it.id }
                .toSet()
            issues
                .filter { it.boardId in teamBoardIds && it.id != issueId }
                .sortedByDescending { it.updatedAt }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // ── Move to board (EXP-57) ──────────────────────────────────────────────

    /**
     * Same-team boards the issue can move to (the current board is
     * excluded; observeAll already filters trashed rows). Empty
     * hides the "Move to board" action — mirrors the web submenu, which
     * only renders with 2+ team boards.
     */
    val moveTargets: StateFlow<List<BoardEntity>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        _board,
    ) { boards, board ->
        if (board == null) emptyList()
        else boards.filter { it.teamId == board.teamId && it.id != board.id }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // Surfaced when a move fails (snackbar in the screen) — a silently
    // dropped move would read as the issue "staying put" for no reason.
    private val _moveError = MutableStateFlow<String?>(null)
    val moveError: StateFlow<String?> = _moveError

    fun consumeMoveError() {
        _moveError.value = null
    }

    // Surfaced when an inline label create fails (snackbar in the screen) —
    // e.g. the duplicate-name CONFLICT (EXP-254).
    private val _labelError = MutableStateFlow<String?>(null)
    val labelError: StateFlow<String?> = _labelError

    fun consumeLabelError() {
        _labelError.value = null
    }

    // One channel for the fire-and-forget property mutations (REV2-50):
    // title/status/priority/due date/assignee/labels, subscribe, duplicate,
    // delete. The screen is driven by Electric sync, which never moves on a
    // failed write — without this the tap simply did nothing, with no way to
    // tell "didn't register" from "server refused".
    private val _mutationError = MutableStateFlow<String?>(null)
    val mutationError: StateFlow<String?> = _mutationError

    fun consumeMutationError() {
        _mutationError.value = null
    }

    private fun reportMutationFailure(t: Throwable, fallback: String) {
        if (t is CancellationException) throw t
        _mutationError.value = trpcErrorMessage(t, fallback)
    }

    /**
     * Move the issue to [boardId] via `issues.move` (EXP-57). The issue
     * keeps its id (this screen observes by id, so it stays live) but gets a
     * new boardId + identifier; the returned row is upserted locally so the
     * identifier chip flips immediately — Electric re-delivers it idempotently.
     */
    fun moveToBoard(boardId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { issuesApi.move(accountId, issueId, boardId) }
                .onSuccess { moved ->
                    runCatching { holder.database(forAccountId = accountId).issueDao().upsert(moved) }
                }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _moveError.value = trpcErrorMessage(t, "The issue could not be moved")
                }
        }
    }

    // ── Issue-reference pills (masterplan §5e) ────────────────────────────────

    /**
     * This team's synced issues, newest-first — drives inline
     * `#IDENTIFIER` pill resolution in the description + comments AND the
     * editors' #-autocomplete (identifier/title search, empty query = most
     * recent). Scoped to this issue's team (same-prefix identifiers from
     * another synced team never leak in), mirroring the web
     * IssueRefProvider.
     */
    val issueRefCandidates: StateFlow<List<IssueRefTarget>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        _board,
        teamStatuses,
    ) { issues, boards, board, statuses ->
        if (board == null) {
            emptyList()
        } else {
            val teamBoardIds = boards
                .filter { it.teamId == board.teamId }
                .map { it.id }
                .toSet()
            issues
                .filter { it.boardId in teamBoardIds }
                .sortedByDescending { it.createdAt }
                // The chip's status glyph is precomputed here (EXP-423), the
                // same way the web provider does it: a status rename/recolor or
                // a reordered started clock produces a new candidate list, hence
                // a new handler, hence a repaint.
                .map {
                    IssueRefTarget(
                        it.id,
                        it.identifier,
                        it.title,
                        IssueStatusResolver.resolve(it, statuses),
                    )
                }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Atomically set duplicateOfId + status='duplicate'. */
    fun markDuplicate(canonicalId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { issuesApi.setDuplicateOf(accountId, issueId, canonicalId) }
                .onFailure { reportMutationFailure(it, "The issue could not be marked duplicate") }
        }
    }

    /** Clear the FK and restore a non-terminal status. */
    fun unmarkDuplicate() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { issuesApi.setDuplicateOf(accountId, issueId, null) }
                .onFailure { reportMutationFailure(it, "The duplicate mark could not be cleared") }
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

    /**
     * Drop not-yet-saved local description input — the screen applied a remote
     * value over it (live apply or banner reload), so a later dispose-time flush
     * would revert the teammate's edit.
     */
    fun discardPendingDescription() {
        descriptionInput.value = null
    }

    // Why the issue isn't on screen yet. Only consulted while the issue is
    // null; a bare "Loading…" that never resolved was the blank screen a push
    // tap landed on before the row synced (EXP-264).
    private val _missing = MutableStateFlow(MissingIssueState.Loading)
    val missing: StateFlow<MissingIssueState> = _missing

    /**
     * Make the issue exist locally, one way or another: kick sync (the row is
     * usually mid-flight), and if it hasn't landed shortly, read it straight
     * from the server and write it into Room. The Room insert — rather than a
     * screen-local copy — is deliberate: it only ever fills a hole (guarded
     * below — sync rows always win), it survives navigation, and it lights up
     * every derived flow (labels, timeline, share URL) with no parallel code
     * path.
     */
    private suspend fun fetchIfAbsent() {
        if (issueId.isEmpty()) {
            _missing.value = MissingIssueState.Unavailable
            return
        }
        _missing.value = MissingIssueState.Loading
        syncManager.kick("issue-detail")
        if (withTimeoutOrNull(SYNC_WAIT_MS) { issueFlow.filterNotNull().first() } != null) return

        val accountId = auth.activeAccountId.value
        if (accountId == null) {
            _missing.value = MissingIssueState.Unavailable
            return
        }
        try {
            val result = issuesApi.get(accountId, issueId)
            val db = holder.database(forAccountId = accountId)
            db.withTransaction {
                // Never clobber a synced row: sync may have delivered a NEWER
                // version while issues.get was in flight, and Electric won't
                // re-send it (its offset already advanced) — a REPLACE here
                // would leave the row stale until the next server-side change.
                // Skip the label joins too: an existing issue's joins arrive
                // via sync, and re-inserting the point read's could resurrect
                // a just-removed label. iOS parity: fetchIssueFallback's
                // fetchOne-nil guard inside the same pool.write.
                if (db.issueDao().exists(result.issue.id)) return@withTransaction
                db.issueDao().upsert(result.issue)
                for (labelId in result.labelIds) {
                    db.issueLabelDao().upsert(
                        IssueLabelEntity(
                            issueId = result.issue.id,
                            labelId = labelId,
                            teamId = result.teamId,
                            boardId = result.issue.boardId,
                        )
                    )
                }
            }
            // The write only helps if this screen observes that id (it does for
            // every real entry point); anything else would spin forever.
            if (withTimeoutOrNull(LOCAL_WRITE_WAIT_MS) { issueFlow.filterNotNull().first() } == null) {
                _missing.value = MissingIssueState.Unavailable
            }
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (t: Throwable) {
            android.util.Log.w("IssueDetailViewModel", "issues.get failed for $issueId: ${t.message}")
            _missing.value = MissingIssueState.Unavailable
        }
    }

    /** "Retry" on the unavailable state — same path, from the top. */
    fun retryFetch() {
        viewModelScope.launch { fetchIfAbsent() }
    }

    init {
        viewModelScope.launch { fetchIfAbsent() }
        // EXP-496: one-shot fetch of the submission metadata card's data.
        // Errors degrade to "no card" — non-members and older self-hosted
        // servers without the procedure both land there.
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                _widgetSubmission.value = widgetsApi.submissionForIssue(accountId, issueId)
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (t: Throwable) {
                android.util.Log.d(
                    "IssueDetailViewModel",
                    "widgets.submissionForIssue failed for $issueId: ${t.message}",
                )
            }
        }
        // Opening an issue clears its inbox notifications (EXP-92) — push taps
        // and app links never pass through the inbox's own mark-read.
        // Fire-and-forget; also tolerates older self-hosted servers without
        // the mutation.
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId != null && issueId.isNotEmpty()) {
                runCatching { notificationsApi.markReadByIssue(accountId, issueId) }
            }
        }
        viewModelScope.launch {
            combine(dbFlow, issueFlow) { db, issue -> db to issue }
                .flatMapLatest { (db, issue) ->
                    if (db == null || issue == null) flowOf(null)
                    else db.boardDao().observeAll().map { boards ->
                        boards.firstOrNull { it.id == issue.boardId }
                    }
                }
                .collect { _board.value = it }
        }
        viewModelScope.launch {
            descriptionInput
                .filterNotNull()
                .debounce(800)
                .collect { saveDescription(it) }
        }
        // Steer availability, re-fetched on account switch. It is env-derived
        // and static per instance, so it stays keyed to the account alone —
        // the device list below is what follows the team (EXP-432).
        viewModelScope.launch {
            auth.activeAccountId.collectLatest { accountId ->
                _steerEnabled.value = null
                _startState.value = SteerStartState.Idle
                _steerEnabled.value = if (accountId == null) {
                    false
                } else {
                    runCatching { steerApi.config(accountId).enabled }.getOrDefault(false)
                }
            }
        }
        // Device presence, re-fetched whenever the account, the board's team
        // (EXP-432 — the list carries the team's shared servers too) or steer
        // availability resolves. Filtered to ONLINE machines: the screen reads
        // an empty list as "no desktop online", which the registry's offline
        // rows would break.
        viewModelScope.launch {
            combine(
                auth.activeAccountId,
                _board.map { it?.teamId }.distinctUntilChanged(),
                _steerEnabled,
            ) { accountId, teamId, enabled -> Triple(accountId, teamId, enabled) }
                .collectLatest { (accountId, teamId, enabled) ->
                    if (enabled == null) {
                        // Still resolving — back to the loading state.
                        _steerDevices.value = null
                        return@collectLatest
                    }
                    _steerDevices.value = if (accountId == null || !enabled) {
                        emptyList()
                    } else {
                        runCatching {
                            devicesApi.list(accountId, teamId).devices.filter { it.online }
                        }.getOrDefault(emptyList())
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
            }.onFailure { reportMutationFailure(it, "The title could not be saved") }
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
                // Consume the pending input only if the user hasn't typed since —
                // otherwise a later dispose-time flush would re-save this now
                // already-persisted text over a teammate's subsequent remote edit.
                // The same String instance flows through descriptionInput, so the
                // reference-identity compareAndSet matches.
                descriptionInput.compareAndSet(text, null)
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

    /**
     * Write the picked status ROW (EXP-314). A constructed `builtin:<key>`
     * fallback (the statuses shape hasn't synced yet) goes out as the enum
     * anchor instead — the server takes `status` XOR `statusId`.
     */
    fun updateStatus(status: ResolvedIssueStatus) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(
                    accountId,
                    UpdateIssueInput(
                        id = issueId,
                        status = status.anchorWireOrNull(),
                        statusId = status.rowId,
                    ),
                )
            }.onFailure { reportMutationFailure(it, "The status could not be changed") }
        }
    }

    fun updatePriority(priority: IssuePriority) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, priority = priority.wire))
            }.onFailure { reportMutationFailure(it, "The priority could not be changed") }
        }
    }

    fun updateDueDate(date: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, dueDate = date))
            }.onFailure { reportMutationFailure(it, "The due date could not be changed") }
        }
    }

    fun updateAssignee(userId: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            // setAssignee, not update(): a null userId ("Unassigned") has to
            // reach the server as an explicit JSON null, and the shared Json
            // drops nulls — through update() this was a silent no-op.
            runCatching {
                issuesApi.setAssignee(accountId, issueId, userId)
            }.onFailure { reportMutationFailure(it, "The assignee could not be changed") }
        }
    }

    fun toggleLabel(labelId: String, isCurrentlyAssigned: Boolean) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                if (isCurrentlyAssigned) labelsApi.removeLabel(accountId, issueId, labelId)
                else labelsApi.addLabel(accountId, issueId, labelId)
            }.onFailure { reportMutationFailure(it, "The label could not be updated") }
        }
    }

    fun createAndAssignLabel(name: String, color: String) {
        val teamId = _board.value?.teamId ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                val label = labelsApi.create(accountId, CreateLabelInput(teamId, name.trim(), color))
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
                        IssueLabelEntity(issueId = issueId, labelId = label.id, teamId = teamId)
                    )
                }
            }.onFailure { t ->
                // Surface the server reject (e.g. the duplicate-name CONFLICT,
                // EXP-254) — otherwise the sheet just closes and nothing happens.
                _labelError.value = trpcErrorMessage(t, "Couldn't create the label")
            }
        }
    }

    fun delete(onDeleted: () -> Unit) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { issuesApi.delete(accountId, issueId) }
                .onSuccess { onDeleted() }
                // The confirm dialog is already dismissed: without this a
                // refused delete looks exactly like a successful one.
                .onFailure { reportMutationFailure(it, "The issue could not be deleted") }
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

    // ── File attachments (EXP-297) ───────────────────────────────────────────

    /**
     * Installed by the screen (EXP-327): where an image that reached the FILE
     * path goes instead of erroring — appended to the description editor, whose
     * model the screen owns. Null (no editor mounted) leaves the pick dropped.
     */
    var onInlineImagePicked: ((android.net.Uri, String) -> Unit)? = null

    /**
     * Upload a picked document as an issue attachment. Failures stay on the
     * pending row (with the server's reason and a Retry) rather than becoming a
     * snackbar that scrolls away — the file is only ever gone if the user
     * dismisses it.
     */
    fun uploadFile(uri: android.net.Uri) {
        val key = UUID.randomUUID().toString()
        // Placeholder name from the URI only — the real display name needs a
        // ContentResolver query, which runUpload does on Dispatchers.IO.
        _pendingFiles.value = _pendingFiles.value + PendingFileUpload(
            key = key,
            filename = sanitizeFilename(uri.lastPathSegment),
            uri = uri,
        )
        runUpload(key)
    }

    /** Retry a failed pending upload from its original content URI. */
    fun retryFileUpload(key: String) {
        val pending = _pendingFiles.value.firstOrNull { it.key == key } ?: return
        if (pending.error == null) return
        updatePending(key) { it.copy(error = null) }
        runUpload(key)
    }

    /** Drop a failed pending upload the user gave up on. */
    fun dismissFileUpload(key: String) {
        _pendingFiles.value = _pendingFiles.value.filterNot { it.key == key }
    }

    private fun runUpload(key: String) {
        viewModelScope.launch {
            val pending = _pendingFiles.value.firstOrNull { it.key == key } ?: return@launch
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                updatePending(key) { it.copy(error = "You are signed out") }
                return@launch
            }
            val resolver = appContext.contentResolver
            // ALL ContentResolver work rides Dispatchers.IO: a 50 MB pick from
            // a cloud-backed DocumentsProvider streams the bytes over the
            // network inside openInputStream/readBytes and would ANR the main
            // thread (viewModelScope launches on Main.immediate).
            val displayName = withContext(Dispatchers.IO) { queryDisplayName(pending.uri) }
            val filename = displayName?.let { sanitizeFilename(it) } ?: pending.filename
            if (filename != pending.filename) {
                updatePending(key) { it.copy(filename = filename) }
            }
            // Canonicalized before the classification guard AND the upload so
            // the stored row always exact-matches on every client (EXP-297).
            val contentType = canonicalContentType(
                withContext(Dispatchers.IO) { resolver.getType(pending.uri) }
            )
            if (isInlineImage(contentType)) {
                // An inline-image type uploaded through the Files flow would be
                // invisible everywhere: filtered out of every client's Files
                // section, referenced by no markdown, and eventually deleted by
                // the owner's unreferenced-image sweep. EXP-327: rather than
                // dead-ending the pick with an error telling the user to go use
                // the other button, put it where it belongs — the end of the
                // description. (The attach menu classifies picks up front, so
                // this only catches a URI whose type resolves differently here.)
                _pendingFiles.value = _pendingFiles.value.filterNot { it.key == key }
                onInlineImagePicked?.invoke(pending.uri, contentType)
                return@launch
            }
            val bytes = withContext(Dispatchers.IO) {
                runCatching {
                    resolver.openInputStream(pending.uri)?.use { it.readBytes() }
                }.getOrNull()
            }
            if (bytes == null) {
                updatePending(key) { it.copy(error = "The file could not be read") }
                return@launch
            }
            if (bytes.size > MAX_FILE_UPLOAD_BYTES) {
                // Refuse locally: a 50 MB body only to be rejected wastes the
                // user's data plan and minutes of waiting.
                updatePending(key) {
                    it.copy(
                        sizeBytes = bytes.size.toLong(),
                        error = "Files must be ${MAX_FILE_UPLOAD_BYTES / (1024 * 1024)} MB or smaller",
                    )
                }
                return@launch
            }
            updatePending(key) { it.copy(sizeBytes = bytes.size.toLong()) }
            try {
                val uploaded = attachmentsApi.upload(
                    accountId,
                    issueId,
                    bytes,
                    filename,
                    contentType,
                )
                // Keep the row until the synced attachment lands (pendingFiles
                // dedupes on this id) so the list never blinks empty.
                updatePending(key) { it.copy(uploadedId = uploaded.id, error = null) }
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (t: Throwable) {
                android.util.Log.w("IssueDetailViewModel", "File upload failed (type=$contentType)", t)
                updatePending(key) {
                    it.copy(error = trpcErrorMessage(t, "The file could not be uploaded"))
                }
            }
        }
    }

    private fun updatePending(key: String, transform: (PendingFileUpload) -> PendingFileUpload) {
        _pendingFiles.value = _pendingFiles.value.map { if (it.key == key) transform(it) else it }
    }

    private fun queryDisplayName(uri: android.net.Uri): String? = runCatching {
        appContext.contentResolver.query(
            uri,
            arrayOf(android.provider.OpenableColumns.DISPLAY_NAME),
            null,
            null,
            null,
        )?.use { cursor ->
            val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (cursor.moveToFirst() && idx >= 0) cursor.getString(idx) else null
        }
    }.getOrNull()

    /**
     * Delete an attachment (`attachments.delete`, member-level). The server
     * rewrites every markdown reference to a placeholder in the same
     * transaction; the local row is dropped optimistically so the list reacts
     * immediately instead of waiting for the Electric delta.
     */
    fun deleteAttachment(attachmentId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _busyAttachmentIds.value = _busyAttachmentIds.value + attachmentId
            try {
                attachmentsApi.delete(accountId, attachmentId)
                runCatching {
                    holder.database(forAccountId = accountId).attachmentDao().deleteById(attachmentId)
                }
            } catch (t: Throwable) {
                reportMutationFailure(t, "The file could not be deleted")
            } finally {
                _busyAttachmentIds.value = _busyAttachmentIds.value - attachmentId
            }
        }
    }

    /**
     * Fetch an attachment's bytes into the app cache so a viewer/share target
     * can read them through the FileProvider. One directory per attachment id
     * keeps names from colliding, and the filename is sanitized so a
     * server-side name can never escape it. Returns null on failure (the
     * reason is surfaced through [mutationError]).
     */
    suspend fun downloadToCache(attachment: AttachmentEntity): File? {
        val accountId = auth.activeAccountId.value ?: return null
        val dir = File(File(appContext.cacheDir, "attachments"), attachment.id)
        val target = File(dir, sanitizeFilename(attachment.filename))
        // Already cached at the expected size — attachments are immutable, so
        // re-downloading buys nothing.
        if (target.isFile && target.length() == attachment.sizeBytes) return target
        _busyAttachmentIds.value = _busyAttachmentIds.value + attachment.id
        return try {
            val bytes = attachmentsApi.download(accountId, attachment.url)
            withContext(Dispatchers.IO) {
                dir.mkdirs()
                target.writeBytes(bytes)
            }
            target
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (t: Throwable) {
            reportMutationFailure(t, "The file could not be downloaded")
            null
        } finally {
            _busyAttachmentIds.value = _busyAttachmentIds.value - attachment.id
        }
    }
}

/**
 * A file upload the user started that isn't a synced attachment row yet —
 * in flight, or failed and awaiting Retry.
 */
data class PendingFileUpload(
    val key: String,
    val filename: String,
    val uri: android.net.Uri,
    val sizeBytes: Long? = null,
    /** Set once the server accepted it; the row lives on until sync delivers it. */
    val uploadedId: String? = null,
    val error: String? = null,
)

// Terminal issue statuses that make an issue ineligible to start a NEW coding
// run (the current issue is exempt — see startCandidates). An ANCHOR set
// (EXP-314): every row still carries one of the 7 enum values, and a custom
// status inherits its anchor's eligibility.
private val TERMINAL_ISSUE_STATUSES = setOf("done", "cancelled", "duplicate")

// Description saves fired while leaving the issue screen must outlive the
// ViewModel: viewModelScope is cancelled when navigation clears it, which
// could abort the final flush mid-request. Process-lifetime, mirroring the
// SyncManager/PushTokenManager scopes.
private val descriptionFlushScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

private const val DESCRIPTION_SAVE_ATTEMPTS = 3
private const val DESCRIPTION_SAVE_RETRY_DELAY_MS = 500L

// How long a kicked sync gets to deliver the issue before we fetch it directly
// — long enough for a round-trip on mobile data, short enough that the fetch
// still beats the user's patience.
private const val SYNC_WAIT_MS = 1_500L
// A local Room write notifies its flows in milliseconds; this only bounds the
// pathological case where the fetched row isn't the one this screen observes.
private const val LOCAL_WRITE_WAIT_MS = 1_000L
