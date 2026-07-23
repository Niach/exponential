package com.exponential.app.ui.issue

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Checklist
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.FilterList
import androidx.compose.material.icons.filled.Groups
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Remove
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Tag
import androidx.compose.material.icons.filled.UnfoldMore
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.TeamPermissions
import com.exponential.app.domain.WebLinks
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.priorityIcon
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.components.InitialsAvatar
import com.exponential.app.ui.components.LabelDot
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.home.HomeViewModel
import com.exponential.app.ui.home.BoardSwitcherSheet
import com.exponential.app.ui.onboarding.CreateBoardSheet
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.AccentIndigo
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow
import kotlinx.coroutines.delay

/**
 * How the issue list is mounted:
 * - [Root] — the Issues tab's home. No back button; the pinned header is the
 *   inline board switcher (current board name + expander glyph → the
 *   switcher sheet) plus the settings gear. The board swaps in place.
 * - [Pushed] — a pushed `board/{boardId}` destination (share target,
 *   deep link, search): back button, fixed board.
 */
enum class IssueListMode { Root, Pushed }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueListScreen(
    boardId: String?,
    mode: IssueListMode,
    onOpenIssue: (String) -> Unit,
    onBack: () -> Unit = {},
    onOpenSettings: () -> Unit = {},
    // Root zero-team empty state's "Join team" (EXP-188): hands the extracted
    // invite token to the existing invite/{token} route.
    onOpenInvite: (String) -> Unit = {},
    viewModel: IssueListViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val syncBanner by viewModel.syncBanner.collectAsStateWithLifecycle()
    var showFilters by remember { mutableStateOf(false) }
    var showSwitcher by remember { mutableStateOf(false) }
    var showCreateBoard by remember { mutableStateOf(false) }
    var showCreateTeam by remember { mutableStateOf(false) }
    var showJoinTeam by remember { mutableStateOf(false) }
    var collapsed by remember { mutableStateOf(emptySet<IssueStatus>()) }

    // Multi-select mode (EXP-239): long-press a row to enter, tap toggles,
    // the floating selection bar acts on the whole selection. Mode is active
    // exactly while the selection is non-empty.
    var selectedIds by remember { mutableStateOf(emptySet<String>()) }
    var showStartSheet by remember { mutableStateOf(false) }
    var noDesktopHint by remember { mutableStateOf(false) }
    // Which bulk-property sheet the selection bar has open (null = none).
    var bulkSheet by remember { mutableStateOf<BulkSheet?>(null) }
    // Inline single-issue status/priority edit fired from a list-row icon tap.
    var inlineEdit by remember { mutableStateOf<InlineEdit?>(null) }
    val selectionActive = selectedIds.isNotEmpty()
    val haptics = LocalHapticFeedback.current
    val soloMemberId by viewModel.soloMemberId.collectAsStateWithLifecycle()
    val steerEnabled by viewModel.steerEnabled.collectAsStateWithLifecycle()
    val steerDevices by viewModel.devices.collectAsStateWithLifecycle()
    val startState by viewModel.startState.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()

    // Selected rows resolved back to their entries — drives the selection
    // bar's shared status/priority glyphs and the bulk property sheets. A
    // single distinct value means every selected issue shares it.
    val selectedEntries = remember(state.groups, selectedIds) {
        state.groups.flatMap { it.issues }.filter { it.issue.id in selectedIds }
    }
    val sharedStatus = remember(selectedEntries) {
        selectedEntries.map { IssueStatus.fromWire(it.issue.status) }.distinct().singleOrNull()
    }
    val sharedPriority = remember(selectedEntries) {
        selectedEntries.map { IssuePriority.fromWire(it.issue.priority) }.distinct().singleOrNull()
    }
    val sharedAssigneeId = remember(selectedEntries) {
        selectedEntries.map { it.issue.assigneeId }.distinct().singleOrNull()
    }

    // Selection ids are board-scoped — a board swap drops them.
    LaunchedEffect(state.board?.id) { selectedIds = emptySet() }
    // Back gesture leaves selection mode before it pops the screen.
    BackHandler(enabled = selectionActive) { selectedIds = emptySet() }
    LaunchedEffect(noDesktopHint) {
        if (noDesktopHint) {
            delay(6_000)
            noDesktopHint = false
        }
    }

    // Root mode resolves the board outside the nav args (last-used → first),
    // so the ViewModel is re-pointed whenever the resolution changes.
    LaunchedEffect(boardId) { viewModel.setBoard(boardId.orEmpty()) }

    // The switcher tree + bootstrap live in the (old home) switcher ViewModel;
    // only the Root mount needs them. The mode of a mounted screen never
    // changes, so the conditional composable calls are stable.
    val homeViewModel: HomeViewModel? = if (mode == IssueListMode.Root) hiltViewModel() else null
    val homeState = homeViewModel?.state?.collectAsStateWithLifecycle()?.value
    val homeError = homeViewModel?.error?.collectAsStateWithLifecycle()?.value
    if (homeViewModel != null) {
        LaunchedEffect(Unit) { homeViewModel.bootstrap() }
    }

    // "Any signed-in account has a board" — gates the cross-account switcher
    // (so a boardless active account with a board-bearing sibling can still
    // switch to it). The spinner instead gates on the ACTIVE account
    // (activeAccountHasBoard), because only the active account's board can
    // resolve into the root list.
    val hasAnyBoard = homeState?.boardTree
        ?.any { group -> group.teamBlocks.any { it.boards.isNotEmpty() } } == true

    Scaffold(containerColor = Color.Transparent) { padding ->
        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
        Column(modifier = Modifier.fillMaxSize()) {
            // Pinned nav row. Pushed: circular back button + the filter
            // trigger trailing. Root: the inline board switcher control +
            // the filter trigger next to the settings gear (EXP-251 — the
            // filter button moved up from the removed tab-preset row); the
            // single add-issue affordance is the bottom bar's compose FAB.
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                when (mode) {
                    IssueListMode.Pushed -> {
                        CircleIconButton(Icons.AutoMirrored.Filled.ArrowBack, "Back", onClick = onBack)
                        Spacer(Modifier.weight(1f))
                        FilterButton(count = state.filters.count, onClick = { showFilters = true })
                    }
                    IssueListMode.Root -> {
                        BoardSwitcherControl(
                            name = state.board?.name,
                            enabled = hasAnyBoard,
                            onClick = { showSwitcher = true },
                        )
                        Spacer(Modifier.weight(1f))
                        // No filter state exists before a board resolves —
                        // the empty/create-board states have nothing to filter.
                        if (!boardId.isNullOrBlank()) {
                            FilterButton(count = state.filters.count, onClick = { showFilters = true })
                            Spacer(Modifier.width(8.dp))
                        }
                        CircleIconButton(Icons.Filled.Settings, "Settings", onClick = onOpenSettings)
                    }
                }
            }

            // In-flow start feedback + selection bar, pinned above the list
            // in the space the removed tab-preset row used to occupy
            // (EXP-251 — sticky at the top, no longer a floating bottom
            // overlay).
            val startNoticeVisible =
                startState is SteerStartState.Sent || startState is SteerStartState.Failed
            if (startNoticeVisible || noDesktopHint || selectionActive) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 4.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    when (val sent = startState) {
                        is SteerStartState.Sent -> NoticeChip(
                            text = (if (sent.isBatch) "Batch start sent to " else "Start sent to ") +
                                sent.deviceLabel.ifEmpty { "your desktop" } +
                                " — watch it in Agents.",
                            isError = false,
                            onClick = null,
                        )
                        is SteerStartState.Failed -> NoticeChip(
                            text = sent.message,
                            isError = true,
                            onClick = viewModel::dismissStartState,
                        )
                        else -> {}
                    }
                    if (noDesktopHint) {
                        NoticeChip(
                            text = "No desktop online — open the Exponential desktop app to run here.",
                            isError = true,
                            onClick = { noDesktopHint = false },
                        )
                    }
                    if (selectionActive) {
                        SelectionBar(
                            count = selectedIds.size,
                            sharedStatus = sharedStatus,
                            sharedPriority = sharedPriority,
                            // Assignee is meaningless in a solo team (one member).
                            showAssignee = soloMemberId == null,
                            // Only repo-backed boards can code, and only while the
                            // relay isn't known-off.
                            showStartCoding = state.board?.repositoryId != null && steerEnabled != false,
                            devicesLoading = steerDevices == null,
                            onClear = { selectedIds = emptySet() },
                            onStatus = { bulkSheet = BulkSheet.Status },
                            onPriority = { bulkSheet = BulkSheet.Priority },
                            onAssignee = { bulkSheet = BulkSheet.Assignee },
                            onLabels = { bulkSheet = BulkSheet.Labels },
                            onStartCoding = {
                                val online = steerDevices
                                when {
                                    online == null -> {} // presence still resolving
                                    online.isEmpty() -> noDesktopHint = true
                                    else -> showStartSheet = true
                                }
                            },
                        )
                    }
                }
            }

            SyncBannerRow(syncBanner, Modifier.padding(horizontal = 16.dp, vertical = 4.dp))

            if (mode == IssueListMode.Root && boardId.isNullOrBlank()) {
                // No board on this account yet (companion app: boards are
                // created on web/desktop). When the ACTIVE account already has a
                // board, the current one is still resolving — show a spinner,
                // not the empty copy. An active account with only boardless
                // teams (a fresh account) falls through to the empty state,
                // never a perpetual spinner.
                if (homeState?.activeAccountHasBoard == true) {
                    LoadingState()
                } else {
                    // Still syncing until the team resolve finishes AND
                    // the boards shape has reached up-to-date at least once —
                    // otherwise a companion account that DOES have boards would
                    // briefly flash "Create your first board" before its
                    // boards snapshot lands. Only a settled, genuinely empty
                    // account shows the create copy.
                    val stillSyncing = homeState == null ||
                        homeState.isSyncing ||
                        !homeState.activeAccountBoardsSynced
                    val hasTeam = homeState?.activeAccountHasTeam == true
                    if (!stillSyncing && !hasTeam) {
                        // Zero teams (EXP-188 — signups get no auto-created
                        // team): create-or-join, mirroring the onboarding
                        // choice. Kept visible even after an error so a failed
                        // create can simply be retried.
                        EmptyState(
                            message = homeError
                                ?: "You're not in a team yet. Create one, or join a teammate's with an invite link.",
                            icon = Icons.Filled.Groups,
                            action = {
                                Column(
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    verticalArrangement = Arrangement.spacedBy(8.dp),
                                ) {
                                    Button(onClick = { showCreateTeam = true }) {
                                        Text("Create team")
                                    }
                                    OutlinedButton(onClick = { showJoinTeam = true }) {
                                        Text("Join team")
                                    }
                                }
                            },
                        )
                    } else {
                        val syncingOrError = stillSyncing || homeError != null
                        EmptyState(
                            message = when {
                                homeError != null -> homeError
                                stillSyncing -> "Syncing…"
                                else -> "No boards yet. Create your first board to get started."
                            },
                            icon = Icons.Filled.UnfoldMore,
                            action = if (syncingOrError) null else {
                                {
                                    Button(onClick = { showCreateBoard = true }) {
                                        Text("Create board")
                                    }
                                }
                            },
                        )
                    }
                }
            } else {
                IssueListContent(
                    state = state,
                    permissions = permissions,
                    soloMemberId = soloMemberId,
                    collapsed = collapsed,
                    onToggleCollapsed = { status, isCollapsed ->
                        collapsed = if (isCollapsed) collapsed - status else collapsed + status
                    },
                    onOpenIssue = onOpenIssue,
                    onInlineStatus = { id -> inlineEdit = InlineEdit(id, InlineKind.Status) },
                    onInlinePriority = { id -> inlineEdit = InlineEdit(id, InlineKind.Priority) },
                    viewModel = viewModel,
                    selectedIds = selectedIds,
                    onToggleSelect = { id ->
                        selectedIds = if (id in selectedIds) selectedIds - id else selectedIds + id
                    },
                    onEnterSelection = { id ->
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        selectedIds = setOf(id)
                        noDesktopHint = false
                        // Resolve relay + device presence while the user is
                        // still picking, so Start coding is ready when tapped.
                        if (state.board?.repositoryId != null) viewModel.ensureSteerLoaded()
                    },
                )
            }
        }

        }
    }

    if (showStartSheet) {
        StartCodingSheet(
            devices = steerDevices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = selectedIds,
            onStart = { device, ids, options ->
                selectedIds = emptySet()
                viewModel.startCoding(device, ids, options)
            },
            onDismiss = { showStartSheet = false },
        )
    }

    // Bulk property sheets (EXP-247) — status/priority/assignee apply then
    // clear the selection; the label sheet stays open across tri-state toggles.
    when (bulkSheet) {
        BulkSheet.Status -> IssuePickerSheet(
            title = "Status",
            items = issueStatusOrder.filter { it != IssueStatus.Duplicate },
            selected = sharedStatus,
            labelOf = { it.label },
            leadingContent = { StatusIcon(it, size = 18.dp) },
            onSelect = {
                viewModel.bulkUpdateStatus(selectedIds, it)
                selectedIds = emptySet()
            },
            onDismiss = { bulkSheet = null },
        )
        BulkSheet.Priority -> IssuePickerSheet(
            title = "Priority",
            items = issuePriorityOrder,
            selected = sharedPriority,
            labelOf = { it.label },
            leadingContent = { PriorityIcon(it, size = 18.dp) },
            onSelect = {
                viewModel.bulkUpdatePriority(selectedIds, it)
                selectedIds = emptySet()
            },
            onDismiss = { bulkSheet = null },
        )
        BulkSheet.Assignee -> AssigneePickerSheet(
            users = state.users,
            selectedUserId = sharedAssigneeId,
            onSelect = {
                viewModel.bulkUpdateAssignee(selectedIds, it)
                selectedIds = emptySet()
            },
            onDismiss = { bulkSheet = null },
        )
        BulkSheet.Labels -> BulkLabelSheet(
            teamLabels = state.labels,
            selectedEntries = selectedEntries,
            onToggle = { labelId, allHave ->
                if (allHave) {
                    viewModel.bulkToggleLabel(selectedIds, labelId, add = false)
                } else {
                    val missing = selectedEntries
                        .filter { entry -> entry.labels.none { it.id == labelId } }
                        .map { it.issue.id }
                    viewModel.bulkToggleLabel(missing, labelId, add = true)
                }
            },
            onDismiss = { bulkSheet = null },
        )
        null -> {}
    }

    val edit = inlineEdit
    if (edit != null) {
        val editIssue = state.groups.flatMap { it.issues }
            .firstOrNull { it.issue.id == edit.issueId }?.issue
        when (edit.kind) {
            InlineKind.Status -> IssuePickerSheet(
                title = "Status",
                items = issueStatusOrder.filter { it != IssueStatus.Duplicate },
                selected = editIssue?.let { IssueStatus.fromWire(it.status) },
                labelOf = { it.label },
                leadingContent = { StatusIcon(it, size = 18.dp) },
                onSelect = { viewModel.updateStatus(edit.issueId, it) },
                onDismiss = { inlineEdit = null },
            )
            InlineKind.Priority -> IssuePickerSheet(
                title = "Priority",
                items = issuePriorityOrder,
                selected = editIssue?.let { IssuePriority.fromWire(it.priority) },
                labelOf = { it.label },
                leadingContent = { PriorityIcon(it, size = 18.dp) },
                onSelect = { viewModel.updatePriority(edit.issueId, it) },
                onDismiss = { inlineEdit = null },
            )
        }
    }

    if (showFilters) {
        IssueFilterSheet(
            filters = state.filters,
            labels = state.labels,
            onToggleStatus = viewModel::toggleStatus,
            onTogglePriority = viewModel::togglePriority,
            onToggleLabel = viewModel::toggleLabel,
            onClear = viewModel::clearFilters,
            onDismiss = { showFilters = false },
        )
    }

    if (showSwitcher && homeViewModel != null) {
        BoardSwitcherSheet(
            groups = homeState?.boardTree ?: emptyList(),
            onSelect = { accountId, pickedBoardId ->
                homeViewModel.selectBoard(accountId, pickedBoardId)
                showSwitcher = false
            },
            onDismiss = { showSwitcher = false },
        )
    }

    if (showCreateBoard) {
        // The new board's last-used pointer swaps the root list in place, so
        // dismissing is all this needs to do on success.
        CreateBoardSheet(
            teamId = null,
            onCreated = { showCreateBoard = false },
            onDismiss = { showCreateBoard = false },
        )
    }

    if (showCreateTeam && homeViewModel != null) {
        CreateTeamDialog(
            onCreate = { name ->
                homeViewModel.createTeam(name)
                showCreateTeam = false
            },
            onDismiss = { showCreateTeam = false },
        )
    }

    if (showJoinTeam) {
        JoinTeamDialog(
            onJoin = { token ->
                showJoinTeam = false
                onOpenInvite(token)
            },
            onDismiss = { showJoinTeam = false },
        )
    }
}

// Zero-team "Create team" (EXP-188): a plain name dialog — the create runs in
// HomeViewModel; success flips the empty state to create-board via the synced
// (head-started) teams table.
@Composable
private fun CreateTeamDialog(
    onCreate: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Create a team") },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                singleLine = true,
                label = { Text("Team name") },
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            TextButton(onClick = { onCreate(name) }, enabled = name.isNotBlank()) {
                Text("Create")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

// Zero-team "Join team" (EXP-188): paste an invite link (or bare token); the
// extracted token routes into the existing invite/{token} accept screen.
@Composable
private fun JoinTeamDialog(
    onJoin: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var input by remember { mutableStateOf("") }
    val token = WebLinks.extractInviteToken(input)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Join a team") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    "Ask a teammate for an invite link and paste it here.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
                OutlinedTextField(
                    value = input,
                    onValueChange = { input = it },
                    singleLine = true,
                    label = { Text("Invite link or code") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { token?.let(onJoin) }, enabled = token != null) {
                Text("Join")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun IssueListContent(
    state: IssueListState,
    permissions: TeamPermissions,
    soloMemberId: String?,
    collapsed: Set<IssueStatus>,
    onToggleCollapsed: (IssueStatus, Boolean) -> Unit,
    onOpenIssue: (String) -> Unit,
    onInlineStatus: (String) -> Unit,
    onInlinePriority: (String) -> Unit,
    viewModel: IssueListViewModel,
    selectedIds: Set<String>,
    onToggleSelect: (String) -> Unit,
    onEnterSelection: (String) -> Unit,
) {
    val usersById = remember(state.users) { state.users.associateBy { it.id } }

    PullToRefreshBox(
        isRefreshing = state.isRefreshing,
        onRefresh = viewModel::refresh,
        modifier = Modifier.fillMaxSize(),
    ) {
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            // Removable pills for the active filters (the filter trigger
            // itself lives in the nav row since EXP-251). Gated so an
            // unfiltered list has no zero-height item eating a spacedBy gap.
            if (!state.filters.isEmpty) {
                item(key = "pills") {
                    ActiveFilterPills(
                        filters = state.filters,
                        labels = state.labels,
                        onToggleStatus = viewModel::toggleStatus,
                        onTogglePriority = viewModel::togglePriority,
                        onToggleLabel = viewModel::toggleLabel,
                        onClear = viewModel::clearFilters,
                    )
                }
            }

            if (state.groups.isEmpty()) {
                item(key = "empty") {
                    Box(
                        modifier = Modifier.fillMaxWidth().padding(top = 64.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            "No issues yet",
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                    }
                }
            } else {
                state.groups.forEach { group ->
                    val isCollapsed = group.status in collapsed
                    item(key = "header-${group.status.wire}") {
                        StatusHeader(
                            status = group.status,
                            count = group.issues.size,
                            collapsed = isCollapsed,
                            onToggle = { onToggleCollapsed(group.status, isCollapsed) },
                        )
                    }
                    if (!isCollapsed) {
                        items(group.issues, key = { it.issue.id }) { entry ->
                            // Long-press enters multi-select (EXP-239 — it
                            // replaced this list's per-row action sheet; Mark
                            // done / Move to backlog live in the selection bar
                            // now, and MyIssues keeps LongPressIssueRow).
                            val selectionActive = selectedIds.isNotEmpty()
                            // Inline status/priority edit is offered only out of
                            // selection mode and when the viewer may mutate the
                            // issue (same gate as entering selection).
                            val canMutate = permissions.canMutateIssue(entry.issue.creatorId)
                            val inlineEditable = canMutate && !selectionActive
                            IssueRow(
                                issue = entry.issue,
                                labels = entry.labels,
                                // Solo teams hide the assignee avatar (one member).
                                assignee = if (soloMemberId != null) null else usersById[entry.issue.assigneeId],
                                selected = if (selectionActive) entry.issue.id in selectedIds else null,
                                onClick = {
                                    if (selectionActive) {
                                        onToggleSelect(entry.issue.id)
                                    } else {
                                        onOpenIssue(entry.issue.id)
                                    }
                                },
                                onLongClick = if (canMutate) {
                                    {
                                        if (selectionActive) {
                                            onToggleSelect(entry.issue.id)
                                        } else {
                                            onEnterSelection(entry.issue.id)
                                        }
                                    }
                                } else {
                                    null
                                },
                                onStatusClick = if (inlineEditable) {
                                    { onInlineStatus(entry.issue.id) }
                                } else {
                                    null
                                },
                                onPriorityClick = if (inlineEditable) {
                                    { onInlinePriority(entry.issue.id) }
                                } else {
                                    null
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}

// The Issues tab root's inline board switcher: current board name + an
// up/down expander glyph as one tappable glass control (iOS combobox pattern).
@Composable
private fun BoardSwitcherControl(
    name: String?,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .glassButton()
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            name ?: "Issues",
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Icon(
            Icons.Filled.UnfoldMore,
            contentDescription = "Switch board",
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (enabled) TextEmphasis.Secondary else TextEmphasis.Quaternary,
            ),
        )
    }
}

// Circular glass icon button (iOS .ultraThinMaterial nav circle) — same fill +
// hairline stroke combination as Modifier.glassRow, just on a circle.
@Composable
private fun CircleIconButton(icon: ImageVector, contentDescription: String, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .size(38.dp)
            .clip(CircleShape)
            .background(GlassTokens.RowFill, CircleShape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeRow, CircleShape)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = contentDescription,
            modifier = Modifier.size(20.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

// Nav-row filter trigger: the circular glass filter button with its
// active-count badge (EXP-251 — moved up from the removed tab-preset row).
@Composable
private fun FilterButton(count: Int, onClick: () -> Unit) {
    BadgedBox(badge = {
        if (count > 0) Badge { Text(count.toString()) }
    }) {
        CircleIconButton(Icons.Filled.FilterList, "Filters", onClick = onClick)
    }
}

@Composable
private fun StatusHeader(
    status: IssueStatus,
    count: Int,
    collapsed: Boolean,
    onToggle: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onToggle)
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (collapsed) Icons.AutoMirrored.Filled.KeyboardArrowRight else Icons.Filled.KeyboardArrowDown,
            contentDescription = if (collapsed) "Expand" else "Collapse",
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.width(6.dp))
        StatusIcon(status, size = 14.dp)
        Spacer(Modifier.width(8.dp))
        Text(
            status.label,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(8.dp))
        Text(
            count.toString(),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun IssueRow(
    issue: IssueEntity,
    labels: List<LabelEntity>,
    assignee: UserEntity?,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)? = null,
    // Multi-select rendering (EXP-239): null = no selection UI; false/true
    // show the leading check indicator, true additionally tints the row.
    selected: Boolean? = null,
    // Inline property edit (EXP-247): non-null makes the status / priority
    // glyph its own tap target (long-press still forwards to [onLongClick]).
    // Null leaves the glyphs as plain decorations (SearchScreen, selection mode).
    onStatusClick: (() -> Unit)? = null,
    onPriorityClick: (() -> Unit)? = null,
) {
    val status = IssueStatus.fromWire(issue.status)
    val priority = IssuePriority.fromWire(issue.priority)
    val rowShape = RoundedCornerShape(GlassTokens.RowRadius)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .then(
                if (selected == true) {
                    Modifier
                        .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.12f), rowShape)
                        .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.45f), rowShape)
                } else {
                    Modifier
                },
            )
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
            // Content height floor, so a row is the same height regardless of
            // WHICH optional glyphs it happens to hold — the selection
            // checkmark, the assignee avatar. Without it, entering selection
            // mode changes the tallest element and every row below re-flows
            // vertically, which reads as the list jumping (EXP-251). 22dp =
            // the avatar, the tallest thing a row has ever held; a larger font
            // scale still grows rows, equally in both modes.
            .heightIn(min = RowContentMinHeight),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (selected != null) {
            Icon(
                if (selected) Icons.Filled.CheckCircle else Icons.Outlined.Circle,
                contentDescription = if (selected) "Selected" else "Not selected",
                modifier = Modifier.size(20.dp),
                tint = if (selected) {
                    MaterialTheme.colorScheme.primary
                } else {
                    MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                },
            )
            Spacer(Modifier.width(10.dp))
        }
        IconColumn(onClick = onPriorityClick, onLongClick = onLongClick) {
            PriorityIcon(priority, size = 16.dp)
        }
        Text(
            issue.identifier,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            // Min-width identifier column (fits "EXP-9999" in the monospace
            // labelMedium style) so the priority icon, identifier, status icon
            // and title line up across rows for typical digit counts — but a
            // min (not fixed) width so longer identifiers (10-char prefixes,
            // big numbers, large font scale) still render in full instead of
            // clipping to a plausible-but-wrong identifier.
            modifier = Modifier.widthIn(min = 60.dp),
        )
        IconColumn(onClick = onStatusClick, onLongClick = onLongClick) {
            StatusIcon(status, size = 16.dp)
        }
        Text(
            issue.title,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (labels.isNotEmpty()) {
            Spacer(Modifier.width(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                labels.take(3).forEach { label ->
                    LabelDot(remember(label.color) { parseColor(label.color) })
                }
            }
        }
        if (issue.dueDate != null) {
            Spacer(Modifier.width(8.dp))
            Icon(
                Icons.Filled.CalendarMonth,
                contentDescription = "Due date",
                modifier = Modifier.size(13.dp),
                tint = dueDateColor(issue.dueDate),
            )
            Spacer(Modifier.width(3.dp))
            Text(
                formatDueDate(issue.dueDate),
                style = MaterialTheme.typography.labelSmall,
                color = dueDateColor(issue.dueDate),
                // The date pill must never wrap ("Tomorrow" used to break onto
                // a second line when a long title squeezed the row, EXP-58):
                // softWrap=false lays it out at its intrinsic single-line
                // width — the weighted title, measured last, absorbs the
                // squeeze instead.
                maxLines = 1,
                softWrap = false,
            )
        }
        if (assignee != null) {
            Spacer(Modifier.width(8.dp))
            InitialsAvatar(nameOrEmail = assignee.name ?: assignee.email, size = 22.dp)
        }
        Spacer(Modifier.width(6.dp))
        Icon(
            Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

/**
 * Floor for an issue row's content height (excludes the row's own vertical
 * padding). Matches the assignee avatar, the tallest element a row can hold —
 * see the usage in [IssueRow] for why the floor has to exist at all.
 */
private val RowContentMinHeight = 22.dp

/** Which bulk-property sheet the selection bar has open. */
private enum class BulkSheet { Status, Priority, Assignee, Labels }

/** A hoisted single-issue inline edit fired from a list-row glyph tap. */
private enum class InlineKind { Status, Priority }

private data class InlineEdit(val issueId: String, val kind: InlineKind)

/**
 * A list-row status / priority glyph column (EXP-247): a fixed-width,
 * full-row-height box, and when [onClick] is non-null also its own tap target
 * that opens the inline picker and forwards long-press to the row's selection
 * gesture. Full-height (not a fixed square) so the touch target never grows
 * the row.
 *
 * The column is the SAME width whether or not it is tappable (EXP-251).
 * Previously the non-tappable branches hand-rolled their own spacing — 26dp
 * for priority, 36dp for status against 32dp here — so the identifier and
 * title sat at different x positions depending on whether the viewer could
 * edit the row, and shifted again when entering selection mode (which turns
 * inline editing off).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun IconColumn(
    onClick: (() -> Unit)?,
    onLongClick: (() -> Unit)?,
    content: @Composable () -> Unit,
) {
    Box(
        modifier = Modifier
            .width(32.dp)
            .fillMaxHeight()
            .then(
                if (onClick != null) {
                    Modifier.combinedClickable(onClick = onClick, onLongClick = onLongClick)
                } else {
                    Modifier
                },
            ),
        contentAlignment = Alignment.Center,
    ) {
        content()
    }
}

/**
 * The floating multi-select action bar (EXP-239 / EXP-247) — the mobile sibling
 * of the desktop bulk bar / web BulkActionBar: count + Status/Priority/Assignee/
 * Labels property buttons + Start coding (repo-backed boards only) + clear. The
 * shared-value glyphs reflect a selection that all share one status/priority;
 * mixed selections fall back to neutral placeholder glyphs. Opaque backing
 * beneath the glass tint so scrolling rows never bleed through.
 */
@Composable
private fun SelectionBar(
    count: Int,
    sharedStatus: IssueStatus?,
    sharedPriority: IssuePriority?,
    showAssignee: Boolean,
    showStartCoding: Boolean,
    devicesLoading: Boolean,
    onClear: () -> Unit,
    onStatus: () -> Unit,
    onPriority: () -> Unit,
    onAssignee: () -> Unit,
    onLabels: () -> Unit,
    onStartCoding: () -> Unit,
) {
    val shape = RoundedCornerShape(percent = 50)
    val neutral = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    // Suppress the 48dp minimum interactive inflation so the 34dp icon buttons
    // keep the bar at ~46dp instead of ballooning it.
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides Dp.Unspecified) {
        Row(
            modifier = Modifier
                .clip(shape)
                .background(DesignTokens.Palette.Card, shape)
                .background(GlassTokens.RowFillActive, shape)
                .border(GlassTokens.Hairline, GlassTokens.StrokeActive, shape)
                .padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            IconButton(onClick = onClear, modifier = Modifier.size(34.dp)) {
                Icon(
                    Icons.Filled.Close,
                    contentDescription = "Clear selection",
                    modifier = Modifier.size(18.dp),
                    tint = neutral,
                )
            }
            Text(
                count.toString(),
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                modifier = Modifier.padding(horizontal = 4.dp),
            )
            IconButton(onClick = onStatus, modifier = Modifier.size(34.dp)) {
                if (sharedStatus != null) {
                    StatusIcon(sharedStatus, size = 18.dp)
                } else {
                    Icon(
                        Icons.Filled.Checklist,
                        contentDescription = "Status",
                        modifier = Modifier.size(18.dp),
                        tint = neutral,
                    )
                }
            }
            IconButton(onClick = onPriority, modifier = Modifier.size(34.dp)) {
                if (sharedPriority != null) {
                    PriorityIcon(sharedPriority, size = 18.dp)
                } else {
                    Icon(
                        priorityIcon(IssuePriority.None),
                        contentDescription = "Priority",
                        modifier = Modifier.size(18.dp),
                        tint = neutral,
                    )
                }
            }
            if (showAssignee) {
                IconButton(onClick = onAssignee, modifier = Modifier.size(34.dp)) {
                    Icon(
                        Icons.Outlined.Person,
                        contentDescription = "Assignee",
                        modifier = Modifier.size(18.dp),
                        tint = neutral,
                    )
                }
            }
            IconButton(onClick = onLabels, modifier = Modifier.size(34.dp)) {
                Icon(
                    Icons.Filled.Tag,
                    contentDescription = "Labels",
                    modifier = Modifier.size(18.dp),
                    tint = neutral,
                )
            }
            if (showStartCoding) {
                Spacer(Modifier.width(4.dp))
                Row(
                    modifier = Modifier
                        .height(34.dp)
                        .clip(shape)
                        .background(MaterialTheme.colorScheme.primary)
                        .clickable(onClick = onStartCoding)
                        .padding(horizontal = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (devicesLoading) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(14.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onPrimary,
                        )
                    } else {
                        Icon(
                            Icons.Filled.PlayArrow,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                            tint = MaterialTheme.colorScheme.onPrimary,
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(
                            "Start coding",
                            style = MaterialTheme.typography.labelLarge,
                            color = MaterialTheme.colorScheme.onPrimary,
                            maxLines = 1,
                        )
                    }
                }
            }
        }
    }
}

/**
 * The selection bar's bulk-label sheet (EXP-247): tri-state rows — a check when
 * every selected issue carries the label, a dash when only some do, nothing
 * otherwise. Tapping removes it from all when all have it, else adds it to the
 * ones missing it; the sheet stays open across toggles.
 */
@Composable
private fun BulkLabelSheet(
    teamLabels: List<LabelEntity>,
    selectedEntries: List<IssueWithLabels>,
    onToggle: (labelId: String, allHave: Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    val filtered = remember(teamLabels, query) {
        val q = query.trim()
        if (q.isEmpty()) teamLabels
        else teamLabels.filter { it.name.contains(q, ignoreCase = true) }
    }
    val counts = remember(selectedEntries) {
        selectedEntries.flatMap { entry -> entry.labels.map { it.id } }
            .groupingBy { it }
            .eachCount()
    }
    val total = selectedEntries.size

    GlassSheet(title = "Labels", onDismiss = onDismiss) {
        GlassSheetSearchField(
            value = query,
            onValueChange = { query = it },
            placeholder = "Search labels",
        )
        Spacer(Modifier.height(4.dp))
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = 420.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            filtered.forEach { label ->
                val count = counts[label.id] ?: 0
                val allHave = total > 0 && count == total
                val someHave = count in 1 until total
                GlassSheetRow(
                    label = label.name,
                    leading = {
                        Box(
                            modifier = Modifier
                                .size(10.dp)
                                .background(parseColor(label.color), CircleShape),
                        )
                    },
                    trailing = {
                        when {
                            allHave -> Icon(
                                Icons.Filled.Check,
                                contentDescription = "On every selected issue",
                                modifier = Modifier.size(18.dp),
                                tint = AccentIndigo,
                            )
                            someHave -> Icon(
                                Icons.Filled.Remove,
                                contentDescription = "On some selected issues",
                                modifier = Modifier.size(18.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                            else -> {}
                        }
                    },
                    onClick = { onToggle(label.id, allHave) },
                )
            }
            if (filtered.isEmpty()) {
                Text(
                    if (query.isBlank()) "No labels yet." else "No matching labels",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 20.dp, vertical = 12.dp),
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

/** Transient outcome chip above/instead of the selection bar (EXP-239). */
@Composable
private fun NoticeChip(
    text: String,
    isError: Boolean,
    onClick: (() -> Unit)?,
) {
    val shape = RoundedCornerShape(12.dp)
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = if (isError) {
            MaterialTheme.colorScheme.error
        } else {
            MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        },
        modifier = Modifier
            .clip(shape)
            .background(DesignTokens.Palette.Card, shape)
            .background(GlassTokens.RowFill, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeRow, shape)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 14.dp, vertical = 8.dp),
    )
}
