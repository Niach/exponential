package com.exponential.app.ui.issue

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.PrFilesApi
import com.exponential.app.data.api.PullFile
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.TeamPermissions
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.ui.steer.SteerLaunchDelegate
import com.exponential.app.ui.steer.SteerRunCaptionRow
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassCard
import com.exponential.app.ui.theme.glassGroup
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The dedicated diff + PR-review page (EXP-34, EXP-156): summary header
// (branch, PR state, totals) + per-file expandable unified patches, with the
// review actions (Merge / Close / GitHub) in a floating bottom bar cloning
// the BottomNavBar treatment (EXP-248 — uniform with web/iOS). Opened from
// the issue detail's AgentPrCard on both the PR tier (issues.prFiles) and the
// pushed-branch tier (repositories.branchDiff). Horizontal scrolling stays
// inside each file's code block — the page itself never scrolls sideways.

sealed interface ChangesLoadState {
    data object Loading : ChangesLoadState
    data class Failed(val message: String) : ChangesLoadState
    data class Loaded(val files: List<PullFile>) : ChangesLoadState
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ChangesViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val prFilesApi: PrFilesApi,
    private val repositoriesApi: RepositoriesApi,
    private val issuesApi: IssuesApi,
    private val steerLaunch: SteerLaunchDelegate,
) : ViewModel() {

    val issueId: String = savedStateHandle["issueId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val issue: StateFlow<IssueEntity?> =
        dbFlow.scopedQuery<IssueEntity?>(null) { it.issueDao().observeById(issueId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // Membership resolution for the merge/close controls (mirrors
    // IssueDetailViewModel): issue → board → team + members + auth.
    private val boardFlow = combine(dbFlow, issue) { db, iss -> db to iss }
        .flatMapLatest { (db, iss) ->
            if (db == null || iss == null) flowOf(null)
            else db.boardDao().observeAll().map { boards -> boards.firstOrNull { it.id == iss.boardId } }
        }
    private val teamForBoard = combine(dbFlow, boardFlow) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(null)
            else db.teamDao().observeById(board.teamId)
        }
    private val membersForTeam = combine(dbFlow, boardFlow) { db, board -> db to board }
        .flatMapLatest { (db, board) ->
            if (db == null || board == null) flowOf(emptyList())
            else db.teamMemberDao().observeByTeam(board.teamId)
        }
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

    private val _load = MutableStateFlow<ChangesLoadState>(ChangesLoadState.Loading)
    val load: StateFlow<ChangesLoadState> = _load

    // PR review actions (EXP-156 — merge/close moved here off the issue detail).
    // No local writes: the Electric echo flips prState and the controls vanish.
    private val _merging = MutableStateFlow(false)
    val merging: StateFlow<Boolean> = _merging
    private val _closing = MutableStateFlow(false)
    val closing: StateFlow<Boolean> = _closing
    private val _actionError = MutableStateFlow<String?>(null)
    val actionError: StateFlow<String?> = _actionError

    /** Which action produced [actionError] — merge and close share the caption. */
    enum class PrAction { Merge, Close }

    // The "Fix conflicts" run rebases, force-pushes and then MERGES the PR, so
    // it may only be offered after a failed MERGE: a user who asked to CLOSE a
    // pull request must never be handed a button that merges it.
    private val _actionErrorFrom = MutableStateFlow<PrAction?>(null)
    val actionErrorFrom: StateFlow<PrAction?> = _actionErrorFrom

    // EXP-533: and only for a REAL conflict — a merge refused by branch
    // protection, a stale base or a dead connection is not something a rebase
    // run can resolve, so the button must stay away.
    private val _actionErrorIsConflict = MutableStateFlow(false)
    val actionErrorIsConflict: StateFlow<Boolean> = _actionErrorIsConflict

    // ── Remote start (EXP-323) ───────────────────────────────────────────────
    // A refused merge is usually a conflict, so the bar offers the builtin
    // "Fix merge conflicts" run right under the error — desktop parity.
    val steerEnabled: StateFlow<Boolean?> get() = steerLaunch.enabled
    val steerDevices: StateFlow<List<SteerDevice>?> get() = steerLaunch.devices
    val startCandidates: StateFlow<List<StartIssueOption>> get() = steerLaunch.startCandidates
    val runState: StateFlow<ActionRunState> get() = steerLaunch.runState
    val startedSessionId: StateFlow<String?> get() = steerLaunch.startedSessionId

    fun consumeStartedSession() = steerLaunch.consumeStartedSession()
    fun runAction(
        device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: Map<String, String>,
    ) = steerLaunch.runAction(device, action, options, inputs)
    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) =
        steerLaunch.startCoding(device, issueIds, options)

    init {
        steerLaunch.attach(viewModelScope)
        // Re-fetch when the diff source flips (a PR opens on a watched branch).
        viewModelScope.launch {
            issue.filterNotNull()
                .map { it.prUrl.isNullOrBlank() }
                .distinctUntilChanged()
                .collectLatest { refresh() }
        }
    }

    fun refresh() {
        viewModelScope.launch {
            _load.value = ChangesLoadState.Loading
            try {
                val accountId = auth.activeAccountId.value
                    ?: throw IllegalStateException("No active account")
                val hasPr = !issue.value?.prUrl.isNullOrBlank()
                val files = if (hasPr) {
                    prFilesApi.get(accountId, issueId).files
                } else {
                    repositoriesApi.branchDiff(accountId, issueId)?.files ?: emptyList()
                }
                _load.value = ChangesLoadState.Loaded(files)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _load.value = ChangesLoadState.Failed(trpcErrorMessage(t, "Failed to load changes"))
            }
        }
    }

    /** Squash-merge the issue's open PR via the GitHub App (batch PRs complete all linked issues). */
    fun mergePr() {
        if (_merging.value || _closing.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _merging.value = true
            _actionError.value = null
            _actionErrorFrom.value = null
            _actionErrorIsConflict.value = false
            runCatching { issuesApi.mergePr(accountId, issueId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    val failure = MergeFailure.from(t, "The pull request could not be merged")
                    _actionError.value = failure.message
                    _actionErrorFrom.value = PrAction.Merge
                    _actionErrorIsConflict.value = failure.isConflict
                }
            _merging.value = false
        }
    }

    /** Close the issue's open PR WITHOUT merging (EXP-100 reject path). */
    fun closePr() {
        if (_closing.value || _merging.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _closing.value = true
            _actionError.value = null
            _actionErrorFrom.value = null
            // Never a conflict offer: the recovery run ends in a MERGE, the
            // opposite of what a failed close asked for.
            _actionErrorIsConflict.value = false
            runCatching { issuesApi.closePr(accountId, issueId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _actionError.value = trpcErrorMessage(t, "The pull request could not be closed")
                    _actionErrorFrom.value = PrAction.Close
                }
            _closing.value = false
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChangesScreen(
    onBack: () -> Unit,
    onOpenSteer: (String) -> Unit,
    viewModel: ChangesViewModel = hiltViewModel(),
) {
    val issue by viewModel.issue.collectAsStateWithLifecycle()
    val load by viewModel.load.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val merging by viewModel.merging.collectAsStateWithLifecycle()
    val closing by viewModel.closing.collectAsStateWithLifecycle()
    val actionError by viewModel.actionError.collectAsStateWithLifecycle()
    val actionErrorFrom by viewModel.actionErrorFrom.collectAsStateWithLifecycle()
    val actionErrorIsConflict by viewModel.actionErrorIsConflict.collectAsStateWithLifecycle()

    // "Fix conflicts" (EXP-323): the launcher, its start feedback, and the
    // jump into the session the desktop reports back.
    val steerEnabled by viewModel.steerEnabled.collectAsStateWithLifecycle()
    val steerDevices by viewModel.steerDevices.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()
    val runState by viewModel.runState.collectAsStateWithLifecycle()
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
    var fixSheetOpen by remember { mutableStateOf(false) }
    LaunchedEffect(startedSessionId) {
        startedSessionId?.let {
            viewModel.consumeStartedSession()
            onOpenSteer(it)
        }
    }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Review") },
                navigationIcon = {
                    TopBarBackButton(onClick = onBack)
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

        // The summary header renders regardless of the diff fetch — and the
        // floating action bar comes from synced issue fields, so a
        // prFiles/branchDiff failure must NOT strand a member with no PR
        // actions. The per-file diff renders below it, per load state.
        val loadedFiles = (load as? ChangesLoadState.Loaded)?.files
        // Every file starts collapsed (EXP-248) — uniform with the web and
        // iOS review detail.
        val expanded = remember(loadedFiles) { mutableStateMapOf<String, Boolean>() }
        var mergeConfirmOpen by remember { mutableStateOf(false) }
        var closeConfirmOpen by remember { mutableStateOf(false) }
        // The floating bar grows when it captions a refusal (and the conflict
        // recovery run under it), so the list's bottom clearance follows the
        // bar's MEASURED height instead of the fixed pill-only inset — the
        // last file rows scroll clear of the notice instead of under it
        // (EXP-559). Never less than BottomBarInset, the shared floor.
        var barHeightPx by remember { mutableIntStateOf(0) }
        val barClearance = with(LocalDensity.current) { barHeightPx.toDp() } + 20.dp

        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(
                    start = 16.dp,
                    end = 16.dp,
                    top = 4.dp,
                    bottom = maxOf(BottomBarInset, barClearance),
                ),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                item(key = "__summary__") {
                    ChangesSummaryHeader(issue = issue, files = loadedFiles)
                }
                when (val state = load) {
                    ChangesLoadState.Loading -> item(key = "__loading__") {
                        Row(
                            modifier = Modifier.padding(vertical = 12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                            Text("Loading changes…", style = MaterialTheme.typography.bodySmall, color = secondary)
                        }
                    }
                    is ChangesLoadState.Failed -> item(key = "__failed__") {
                        Text(
                            "Couldn’t load changes: ${state.message}",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(vertical = 12.dp),
                        )
                    }
                    is ChangesLoadState.Loaded -> {
                        val files = state.files
                        if (files.isEmpty()) {
                            item(key = "__empty__") {
                                Text(
                                    "No changed files.",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = tertiary,
                                    modifier = Modifier.padding(vertical = 12.dp),
                                )
                            }
                        }
                        items(files, key = { it.filename }) { file ->
                            FileSection(
                                file = file,
                                expanded = expanded[file.filename] == true,
                                onToggle = {
                                    expanded[file.filename] = expanded[file.filename] != true
                                },
                            )
                        }
                    }
                }
            }

            ChangesBottomBar(
                issue = issue,
                isMember = permissions.isMember,
                merging = merging,
                closing = closing,
                actionError = actionError,
                runState = runState,
                // A REAL conflict only (EXP-533); the recovery run rebases the
                // PR's branch, so it needs one recorded (EXP-323). MERGE
                // failures only — the run ends in a merge, the opposite of what
                // a failed close asked for.
                canFixConflicts = actionErrorIsConflict &&
                    actionErrorFrom == ChangesViewModel.PrAction.Merge &&
                    steerEnabled == true &&
                    permissions.isMember &&
                    issue?.prState == DomainContract.prStateOpen &&
                    !issue?.branch.isNullOrBlank(),
                onMerge = { mergeConfirmOpen = true },
                onClosePr = { closeConfirmOpen = true },
                onFixConflicts = { fixSheetOpen = true },
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .onSizeChanged { barHeightPx = it.height },
            )
        }

        if (fixSheetOpen) {
            StartCodingSheet(
                devices = steerDevices ?: emptyList(),
                issues = startCandidates,
                preselectedIds = emptySet(),
                preselectedActionId = DomainContract.builtinFixConflictsId,
                preselectedPrIssueId = viewModel.issueId,
                onStart = viewModel::startCoding,
                onRunAction = viewModel::runAction,
                onDismiss = { fixSheetOpen = false },
            )
        }

        if (mergeConfirmOpen) {
            AlertDialog(
                onDismissRequest = { mergeConfirmOpen = false },
                title = { Text("Merge pull request?") },
                text = {
                    Text("Squash-merges PR #${issue?.prNumber ?: ""} via the GitHub App. Any live coding session for it closes.")
                },
                confirmButton = {
                    TextButton(onClick = {
                        mergeConfirmOpen = false
                        viewModel.mergePr()
                    }) { Text("Merge") }
                },
                dismissButton = {
                    TextButton(onClick = { mergeConfirmOpen = false }) { Text("Cancel") }
                },
            )
        }

        if (closeConfirmOpen) {
            AlertDialog(
                onDismissRequest = { closeConfirmOpen = false },
                title = { Text("Close pull request?") },
                text = {
                    Text(
                        "Closes the pull request on GitHub WITHOUT merging. Use this " +
                            "when the issue was dropped even though the work exists. " +
                            "The branch is kept and the PR can be reopened on GitHub.",
                    )
                },
                confirmButton = {
                    TextButton(onClick = {
                        closeConfirmOpen = false
                        viewModel.closePr()
                    }) {
                        Text("Close PR", color = MaterialTheme.colorScheme.error)
                    }
                },
                dismissButton = {
                    TextButton(onClick = { closeConfirmOpen = false }) { Text("Cancel") }
                },
            )
        }
    }
}

// The floating review-action bar (EXP-248), cloning the BottomNavBar /
// IssueDetailBottomBar treatment (near-opaque pill fill + hairline stroke):
// dismiss (icon), Merge (labeled, center), open on GitHub (icon). Merge and
// dismiss show for members on an OPEN PR; the GitHub circle whenever a PR
// exists. Renders nothing on the pushed-branch tier (no PR yet). No local
// write: the Electric echo flips prState and the bar disappears with it. A
// failed merge/close captions the bar, right where the user just tapped.
// No navigationBarsPadding here (unlike BottomNavBar, which lives outside a
// Scaffold): the Scaffold content padding this bar sits inside already carries
// the nav-bar inset, and applying it twice lifted the bar past the LazyColumn's
// BottomBarInset clearance so the last file section slid underneath.
@Composable
private fun ChangesBottomBar(
    issue: IssueEntity?,
    isMember: Boolean,
    merging: Boolean,
    closing: Boolean,
    actionError: String?,
    runState: ActionRunState,
    canFixConflicts: Boolean,
    onMerge: () -> Unit,
    onClosePr: () -> Unit,
    onFixConflicts: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val prUrl = issue?.prUrl
    val canReview = isMember &&
        !prUrl.isNullOrBlank() &&
        issue?.prState == DomainContract.prStateOpen
    if (!canReview && prUrl.isNullOrBlank()) return
    val busy = merging || closing
    Column(
        // EXP-627: the store slide's pop-out rect is measured off the review
        // bar (`PopRects`), iOS parity.
        modifier = modifier.testTag("pr-merge-bar").padding(horizontal = 16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (actionError != null) {
            ChangesRefusalNotice(message = actionError)
        }
        // Floating: this caption sits on top of the scrolling diff too.
        SteerRunCaptionRow(runState, modifier = Modifier.padding(top = 6.dp), floating = true)
        Row(
            modifier = Modifier.padding(top = 8.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (canReview) {
                ChangesBarCircle(onClick = onClosePr, enabled = !busy) {
                    if (closing) {
                        CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                    } else {
                        Icon(
                            ExpIcons.uiClose,
                            contentDescription = "Close PR without merging",
                            modifier = Modifier.size(20.dp),
                            tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                        )
                    }
                }
                // EXP-706: the primary action is a WHITE pill — the one solid
                // thing on the bar, so the review's outcome is unmistakable.
                // No hairline: a white fill needs no edge against the dim
                // circles beside it. A conflict-refused merge REPLACES it with
                // the recovery run (the notice above keeps only the message).
                Row(
                    modifier = Modifier
                        .height(52.dp)
                        .clip(RoundedCornerShape(percent = 50))
                        .background(Color.White)
                        .clickable(
                            enabled = !busy,
                            onClick = if (canFixConflicts) onFixConflicts else onMerge,
                        )
                        .padding(horizontal = 28.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    if (merging) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(18.dp),
                            strokeWidth = 2.dp,
                            // On white, the default primary tint disappears.
                            color = Color.Black,
                        )
                    } else {
                        Icon(
                            if (canFixConflicts) ExpIcons.uiBranch else ExpIcons.prMerged,
                            contentDescription = null,
                            modifier = Modifier.size(20.dp),
                            tint = Color.Black,
                        )
                    }
                    Text(
                        if (canFixConflicts) "Fix conflicts" else "Merge",
                        style = MaterialTheme.typography.titleSmall,
                        color = Color.Black,
                    )
                }
            }
            if (!prUrl.isNullOrBlank()) {
                ChangesBarCircle(onClick = {
                    runCatching {
                        val intent = android.content.Intent(
                            android.content.Intent.ACTION_VIEW,
                            android.net.Uri.parse(prUrl),
                        )
                        intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                        context.startActivity(intent)
                    }
                }) {
                    Icon(
                        ExpIcons.uiExternalLink,
                        contentDescription = "Open PR on GitHub",
                        modifier = Modifier.size(20.dp),
                        tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                    )
                }
            }
        }
    }
}

// The bar's refusal notice (EXP-559): a failed merge/close captions the bar
// that produced it. EXP-706: the MESSAGE only — the conflict-recovery run
// took the Merge pill's slot below, so it is no longer offered twice. It
// floats over the diff list, so it wears the
// SAME near-opaque pill fill + hairline as the circles below it and reads as
// the bar's own header — the lighter opaque card fill it used to have looked
// like a stray file row dropped on the changed files. Semantics come from a
// red warning glyph rather than a wall of red text: GitHub's refusals run to
// several sentences (stale base, stacked PR, rebase instructions), and those
// stay legible left-aligned in the primary text color. Wrap-width, so a short
// message is a compact centered chip and a long one grows to the bar's width.
@Composable
private fun ChangesRefusalNotice(
    message: String,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(GlassTokens.CardRadius)
    val stroke = GlassTokens.StrokeStrong
    Row(
        modifier = modifier
            .widthIn(max = 480.dp)
            .clip(shape)
            .background(GlassTokens.OpaqueCardFill, shape)
            .border(GlassTokens.Hairline, stroke, shape)
            .padding(start = 14.dp, end = 16.dp, top = 12.dp, bottom = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            ExpIcons.uiWarning,
            contentDescription = null,
            modifier = Modifier.padding(top = 1.dp).size(16.dp),
            tint = DesignTokens.Semantic.Red,
        )
        Text(
            message,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}

@Composable
private fun ChangesBarCircle(
    onClick: () -> Unit,
    enabled: Boolean = true,
    content: @Composable () -> Unit,
) {
    Box(
        modifier = Modifier
            .size(52.dp)
            .clip(CircleShape)
            .background(GlassTokens.OpaqueCardFill)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, CircleShape)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        content()
    }
}

@Composable
private fun ChangesSummaryHeader(
    issue: IssueEntity?,
    // Null while the diff fetch is still loading/failed — the file totals hide,
    // but the branch/PR-state (which come from the synced issue, not the fetch)
    // always render. The review actions — and their failures — live in the
    // floating bottom bar (EXP-248).
    files: List<PullFile>?,
) {
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(modifier = Modifier.fillMaxWidth().glassCard().padding(12.dp)) {
        val branch = issue?.branch
        if (!branch.isNullOrBlank()) {
            Text(
                branch,
                style = MaterialTheme.typography.labelMedium.copy(fontFamily = FontFamily.Monospace),
                color = secondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(8.dp))
        }
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            val prState = issue?.prState
            if (!prState.isNullOrBlank()) {
                Text(
                    prState.replaceFirstChar { it.uppercase() },
                    style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Medium),
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier
                        .glassButton()
                        .padding(horizontal = 8.dp, vertical = 3.dp),
                )
            }
            if (files != null) {
                Text(
                    "${files.size} ${if (files.size == 1) "file" else "files"}",
                    style = MaterialTheme.typography.labelMedium,
                    color = secondary,
                )
                Text(
                    "+${files.sumOf { it.additions }}",
                    color = DiffAddColor,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 12.sp,
                )
                Text(
                    "−${files.sumOf { it.deletions }}",
                    color = DiffDelColor,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 12.sp,
                )
            }
            Spacer(Modifier.weight(1f))
        }
    }
}

// One changed file: a tappable header (status letter, filename, +/− counts)
// over a collapsible unified patch with the shared line coloring.
@Composable
private fun FileSection(file: PullFile, expanded: Boolean, onToggle: () -> Unit) {
    val contextColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(modifier = Modifier.fillMaxWidth().glassGroup()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .testTag("changes-file-row")
                .clickable(onClick = onToggle)
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                statusLetter(file.status),
                color = statusColor(file.status),
                fontFamily = FontFamily.Monospace,
                fontSize = 12.sp,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.width(8.dp))
            Text(
                file.filename,
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            Text("+${file.additions}", color = DiffAddColor, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
            Spacer(Modifier.width(4.dp))
            Text("−${file.deletions}", color = DiffDelColor, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
            Spacer(Modifier.width(6.dp))
            // EXP-706: ONE glyph that turns, instead of two that swap — the
            // rotation reads as the section opening. Motion.standard() snaps
            // under the OS's reduce-motion setting (ui/theme/Motion.kt).
            val rotation by animateFloatAsState(
                targetValue = if (expanded) 180f else 0f,
                animationSpec = Motion.standard(),
                label = "file-chevron",
            )
            Icon(
                ExpIcons.uiChevronDown,
                contentDescription = if (expanded) "Collapse" else "Expand",
                modifier = Modifier.size(16.dp).rotate(rotation),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        if (expanded) {
            val patch = file.patch
            if (!patch.isNullOrEmpty()) {
                PatchLines(
                    lines = remember(patch) { patch.split("\n") },
                    contextColor = contextColor,
                    modifier = Modifier.padding(bottom = 8.dp),
                )
            } else {
                Text(
                    if (file.status == "renamed") "Renamed." else "No textual diff (binary or too large).",
                    style = MaterialTheme.typography.bodySmall,
                    color = contextColor,
                    modifier = Modifier.padding(horizontal = 12.dp).padding(bottom = 10.dp),
                )
            }
        }
    }
}

// GitHub file statuses: added / modified / removed / renamed / copied / changed.
private fun statusLetter(status: String): String = when (status) {
    "added" -> "A"
    "removed" -> "D"
    "renamed" -> "R"
    "copied" -> "C"
    else -> "M"
}

// EXP-706: the status letter reads as a STATUS, not as diff-line chrome —
// the shared semantic palette, web parity (M is amber, not muted grey).
private fun statusColor(status: String): Color = when (status) {
    "added" -> DesignTokens.Semantic.Green
    "removed" -> DesignTokens.Semantic.Red
    "renamed", "copied" -> DesignTokens.Semantic.Blue
    else -> DesignTokens.Semantic.Yellow
}
