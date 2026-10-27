package com.exponential.app.ui.issue

import com.exponential.app.domain.EmptyStateCopy
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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.IssueGraph
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.TeamPermissions
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.priorityIcon
import com.exponential.app.ui.components.IssueRowContent
import com.exponential.app.ui.components.BlocksBadge
import com.exponential.app.ui.components.picker.AssigneePicker
import com.exponential.app.ui.components.picker.LabelPicker
import com.exponential.app.ui.components.picker.PickerChecked
import com.exponential.app.ui.components.picker.PriorityPicker
import com.exponential.app.ui.components.picker.StatusPicker
import com.exponential.app.ui.components.issuePriorityPickerOptions
import com.exponential.app.ui.components.pickedPriority
import com.exponential.app.ui.components.toPickerLabel
import com.exponential.app.ui.components.toPickerMember
import com.exponential.app.ui.components.toPickerAgent
import com.exponential.app.ui.components.actorRow
import com.exponential.app.ui.components.AgentAvatar
import com.exponential.app.domain.AgentSettings
import com.exponential.app.ui.components.picker.assigneeFromPickerValue
import com.exponential.app.ui.components.picker.assigneePickerValue
import com.exponential.app.ui.components.toPickerRow
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.BottomNavDefaults
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.SelectionBarShell
import com.exponential.app.ui.components.SelectionBarSlot
import com.exponential.app.ui.components.SelectionGlyph
import com.exponential.app.ui.components.selectedRowHighlight
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.IssueGraphSheet
import com.exponential.app.ui.components.LabelDot
import com.exponential.app.ui.components.LocalBottomBarSuppression
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.IssueGroupBand
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.gettingstarted.GettingStartedSection
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.domain.TeamLiveRuns
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.home.HomeViewModel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.onboarding.CreateBoardSheet
import com.exponential.app.ui.onboarding.TeamSetupSheet
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassCard
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.domain.Prompts

/**
 * How the issue list is mounted:
 * - [Root] — the Board tab's home. No back button; the page header is the
 *   board's name (#EXP-1384: the board is switched by the Board tab's menu).
 *   The board swaps in place.
 * - [Pushed] — a pushed `board/{boardId}` destination (share target,
 *   deep link, search): back button, fixed board.
 */
enum class IssueListMode { Root, Pushed }

// Thread 58cb32df: the board chips' keys (chats.json `shell.boardChips`).
private const val BOARD_CHIP_ALL = "all"
private const val BOARD_CHIP_MINE = "mine"
private const val BOARD_CHIP_DRAFTS = "drafts"

/** The board chips (chats.json `shell.boardChips`): All issues · My issues N
 *  · Drafts N (only while there are drafts), select Pills, a count after My
 *  issues and Drafts. */
@Composable
private fun BoardChips(
    selected: String,
    mineCount: Int,
    draftsCount: Int,
    onSelect: (String) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        com.exponential.app.domain.Chats.Shell.BOARD_CHIPS.forEach { chip ->
            if (chip == BOARD_CHIP_DRAFTS && draftsCount == 0) return@forEach
            val label = when (chip) {
                BOARD_CHIP_MINE -> com.exponential.app.domain.Chats.Copy.BOARD_CHIP_MINE + (if (mineCount > 0) " $mineCount" else "")
                BOARD_CHIP_DRAFTS -> com.exponential.app.domain.Chats.Copy.BOARD_CHIP_DRAFTS + " $draftsCount"
                else -> com.exponential.app.domain.Chats.Copy.BOARD_CHIP_ALL
            }
            GlassPill(
                label,
                onClick = { onSelect(chip) },
                mode = PillMode.Select,
                selected = chip == selected,
                modifier = Modifier.testTag("${com.exponential.app.domain.Chats.TestIds.BOARD_CHIP}-$chip"),
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueListScreen(
    boardId: String?,
    mode: IssueListMode,
    onOpenIssue: (String) -> Unit,
    onBack: () -> Unit = {},
    onOpenSettings: () -> Unit = {},
    // EXP-825: the selection bar's Start coding navigates to the Agent page
    // composer with the checked issues chipped.
    onOpenAgent: (AgentComposerSeed) -> Unit = {},
    // EXP-686: search left the bottom bar and rides the board header
    // instead.
    onOpenSearch: () -> Unit = {},
    // EXP-1270: Getting started's "Start a run" opens the Agent home; the
    // empty board's own "New issue" pill (the compose FAB is hidden behind
    // the empty state).
    onOpenAgentHome: () -> Unit = {},
    onNewIssue: () -> Unit = {},
    // EXP-1075: my LIVE runs grouped by team (AppViewModel.liveRunsByTeam) and
    // the team the app is on — the board switcher SHEET marks the teams with
    // live runs (EXP-1210: the pill itself wears no dot). Root mount only; a
    // pushed board has no switcher.
    liveRunsByTeam: Map<String, TeamLiveRuns> = emptyMap(),
    selectedTeamId: String? = null,
    /** Thread 58cb32df: a Drafts chip row reopens the New issue page. */
    onOpenDraft: (boardId: String, draftId: String) -> Unit = { _, _ -> },
    viewModel: IssueListViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    // FEED-79: a failed row or bulk write (status, priority, delete...).
    FailedActionToast(state.error, onShown = viewModel::consumeError)
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val syncBanner by viewModel.syncBanner.collectAsStateWithLifecycle()
    val syncingVisible by viewModel.syncingVisible.collectAsStateWithLifecycle()
    var showCreateBoard by remember { mutableStateOf(false) }
    // Which team the create-board sheet targets — the switcher names one, every
    // other entry point means "the selected team" (null).
    var createBoardTeamId by remember { mutableStateOf<String?>(null) }
    // EXP-698: ONE create-or-join sheet, replacing the two stock Material
    // dialogs — the same form the onboarding wizard's team step renders.
    var showTeamSetup by remember { mutableStateOf(false) }
    // Collapse state keys on the GROUP KEY (status row id / `builtin:<key>`).
    var collapsed by remember { mutableStateOf(emptySet<String>()) }

    // Multi-select mode (EXP-239): long-press a row to enter, tap toggles,
    // the floating selection bar acts on the whole selection. Mode is active
    // exactly while the selection is non-empty.
    var selectedIds by remember { mutableStateOf(emptySet<String>()) }
    val toaster = LocalToaster.current
    // Which bulk-property sheet the selection bar has open (null = none).
    var bulkSheet by remember { mutableStateOf<BulkSheet?>(null) }
    // Inline single-issue status/priority edit fired from a list-row icon tap.
    var inlineEdit by remember { mutableStateOf<InlineEdit?>(null) }
    val selectionActive = selectedIds.isNotEmpty()
    // Delete confirmation for the selection bar (EXP-698 r5).
    var confirmBulkDelete by remember { mutableStateOf(false) }
    // EXP-980: the row whose blocks badge opened the mini-graph (null = none).
    var graphIssueId by remember { mutableStateOf<String?>(null) }
    // EXP-698 r5 (Mechanism A): the selection bar takes the tab bar's slot, so
    // the tab bar + its FAB slide out while a selection is up. Covers both
    // mounts — the Issues root and a pushed board — and restores on dispose,
    // so a back-out mid-selection never strands a hidden bar.
    val barSuppression = LocalBottomBarSuppression.current
    DisposableEffect(selectionActive) {
        barSuppression?.suppressed = selectionActive
        onDispose { barSuppression?.suppressed = false }
    }
    val haptics = LocalHapticFeedback.current
    val soloMemberIdRaw by viewModel.soloMemberId.collectAsStateWithLifecycle()
    // #EXP-1383: the team's agents; a solo team hides its assignee control
    // only while it has none (an agent is someone to assign to).
    val teamAgents by viewModel.teamAgents.collectAsStateWithLifecycle()
    val soloMemberId = soloMemberIdRaw.takeIf { teamAgents.isEmpty() }
    val steerEnabled by viewModel.steerEnabled.collectAsStateWithLifecycle()
    val steerDevices by viewModel.devices.collectAsStateWithLifecycle()

    // Selected rows resolved back to their entries — drives the selection
    // bar's shared status/priority glyphs and the bulk property sheets. A
    // single distinct value means every selected issue shares it.
    val selectedEntries = remember(state.groups, selectedIds) {
        state.groups.flatMap { it.issues }.filter { it.issue.id in selectedIds }
    }
    // Each row carries its OWN resolved status (EXP-980: a nested sub-issue
    // sits in its ROOT's group, so the group's row no longer speaks for it).
    val statusByIssueId = remember(state.groups) {
        state.groups
            .flatMap { group -> group.issues.map { it.issue.id to (it.status ?: group.status) } }
            .toMap()
    }
    val sharedStatus = remember(statusByIssueId, selectedIds) {
        selectedIds.mapNotNull { statusByIssueId[it] }.distinctBy { it.id }.singleOrNull()
    }
    val sharedPriority = remember(selectedEntries) {
        selectedEntries.map { IssuePriority.fromWire(it.issue.priority) }.distinct().singleOrNull()
    }
    val sharedAssignee = remember(selectedEntries) {
        selectedEntries.map { assigneePickerValue(it.issue.assigneeId, it.issue.assigneeAgentId) }.distinct().singleOrNull()
    }

    // Selection ids are board-scoped — a board swap drops them.
    LaunchedEffect(state.board?.id) { selectedIds = emptySet() }
    // Back gesture leaves selection mode before it pops the screen.
    BackHandler(enabled = selectionActive) { selectedIds = emptySet() }

    // Root mode resolves the board outside the nav args (last-used → first),
    // so the ViewModel is re-pointed whenever the resolution changes.
    LaunchedEffect(boardId) { viewModel.setBoard(boardId.orEmpty()) }

    // The switcher tree + bootstrap live in the (old home) switcher ViewModel;
    // only the Root mount needs them. The mode of a mounted screen never
    // changes, so the conditional composable calls are stable.
    val homeViewModel: HomeViewModel? = if (mode == IssueListMode.Root) hiltViewModel() else null
    // EXP-1270: Getting started is a ROOT affordance (a pushed board is
    // somewhere you navigated to, not the app's front door): the block under
    // an EMPTY board, its targets bound by [GettingStartedSection].
    val gettingStarted: (@Composable (Modifier) -> Unit)? = if (mode == IssueListMode.Root) {
        { modifier -> GettingStartedSection(onOpenAgent = onOpenAgentHome, modifier = modifier, contentPadding = PaddingValues(horizontal = ListGutter)) }
    } else {
        null
    }
    val homeState = homeViewModel?.state?.collectAsStateWithLifecycle()?.value
    if (homeViewModel != null) {
        // FEED-79: the bootstrap runs on launch, so a failed team load is
        // quiet (never a toast); the root keeps drawing its empty / syncing
        // state.
        LaunchedEffect(Unit) { homeViewModel.bootstrap() }
    }

    // Thread 58cb32df: the board chips' state + counts (Root mount only).
    var boardChip by rememberSaveable { mutableStateOf(BOARD_CHIP_ALL) }
    val myIssuesViewModel: com.exponential.app.ui.myissues.MyIssuesViewModel? =
        if (mode == IssueListMode.Root) hiltViewModel() else null
    val draftsViewModel: com.exponential.app.ui.drafts.DraftsViewModel? =
        if (mode == IssueListMode.Root) hiltViewModel() else null
    val myIssuesCount = myIssuesViewModel?.state?.collectAsStateWithLifecycle()?.value
        ?.groups?.sumOf { it.issues.size } ?: 0
    val drafts = draftsViewModel?.drafts?.collectAsStateWithLifecycle()?.value.orEmpty()
    // The Drafts chip leaves with the last draft: its pick falls back.
    if (boardChip == BOARD_CHIP_DRAFTS && drafts.isEmpty()) boardChip = BOARD_CHIP_ALL

    Scaffold(containerColor = Color.Transparent) { padding ->
        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
        Column(modifier = Modifier.fillMaxSize()) {
            // #EXP-1384 (chats.json `shell.header`): THE page header — the
            // board's name large, search + inbox circles (the shell's). The
            // board is switched only by the Board tab's menu now; a pushed
            // board wears the back control before its title.
            PageHeader(
                title = state.board?.name ?: com.exponential.app.domain.Chats.Copy.TABS.getValue("board"),
                onBack = if (mode == IssueListMode.Pushed) onBack else null,
            )

            // Thread 58cb32df (chats.json `shell.boardChips`): All issues ·
            // My issues N · Drafts N (Drafts only while there are drafts) as
            // select Pills; My issues = the viewer's assigned issues of the
            // active team grouped by status, Drafts = the team's drafts.
            val chipsShown = mode == IssueListMode.Root && !boardId.isNullOrBlank()
            if (chipsShown) {
                BoardChips(
                    selected = boardChip,
                    mineCount = myIssuesCount,
                    draftsCount = drafts.size,
                    onSelect = { boardChip = it },
                )
            }

            // Sync activity, pinned in the same strip (EXP-264): shown while
            // the core shapes are behind, so a list that hasn't caught up
            // reads as "working on it" instead of as the truth.
            if (syncingVisible) {
                GlobalSyncChip(
                    visible = true,
                    modifier = Modifier
                        .align(Alignment.CenterHorizontally)
                        .padding(vertical = 4.dp),
                )
            }

            SyncBannerRow(syncBanner, Modifier.padding(horizontal = 16.dp, vertical = 4.dp))

            if (mode == IssueListMode.Root && boardId.isNullOrBlank()) {
                // No board on this account yet (companion app: boards are
                // created on web/desktop). When the ACTIVE account already has a
                // board, the current one is still resolving — show skeleton
                // rows (EXP-1275), not the empty copy. An active account with
                // only boardless teams (a fresh account) falls through to the
                // empty state, never a perpetual placeholder.
                if (homeState?.activeAccountHasBoard == true) {
                    SkeletonRows(modifier = Modifier.padding(horizontal = ListGutter).padding(top = 4.dp))
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
                        // team): ONE pill onto the create-or-join sheet, which
                        // owns both paths and its own error lines. Byte-equal
                        // copy to iOS `IssuesHomeView.emptyStateHint`.
                        EmptyState(
                            EmptyStateCopy.noTeam,
                            action = {
                                GlassPill(
                                    "Create or join a team",
                                    icon = ExpIcons.uiAdd,
                                    size = PillSize.Md,
                                    mode = PillMode.Action,
                                    onClick = { showTeamSetup = true },
                                )
                            },
                        )
                    } else if (stillSyncing) {
                        // EXP-1275: still syncing = skeleton rows, never a
                        // "Syncing…" line.
                        SkeletonRows(modifier = Modifier.padding(horizontal = ListGutter).padding(top = 4.dp))
                    } else {
                        // EXP-698 r5: the boardless team is the other place
                        // the getting-started checklist belongs — same empty
                        // state as the IDE and web, cards under it, the whole
                        // thing scrollable because the cards outgrow a phone.
                        Column(
                            modifier = Modifier
                                .fillMaxSize()
                                .verticalScroll(rememberScrollState())
                                .padding(horizontal = ListGutter)
                                .padding(bottom = BottomBarInset),
                        ) {
                            EmptyState(
                                EmptyStateCopy.boards,
                                action = {
                                    GlassPill(
                                        EmptyStateCopy.boards.action!!,
                                        icon = ExpIcons.uiAdd,
                                        size = PillSize.Md,
                                        primary = true,
                                        onClick = {
                                            createBoardTeamId = selectedTeamId
                                            showCreateBoard = true
                                        },
                                    )
                                },
                            )
                        }
                    }
                }
            } else if (boardChip == BOARD_CHIP_MINE && myIssuesViewModel != null) {
                com.exponential.app.ui.myissues.MyIssuesListContent(
                    onOpenIssue = onOpenIssue,
                    viewModel = myIssuesViewModel,
                    modifier = Modifier.weight(1f),
                )
            } else if (boardChip == BOARD_CHIP_DRAFTS && draftsViewModel != null) {
                com.exponential.app.ui.drafts.DraftsListContent(
                    onOpenDraft = onOpenDraft,
                    viewModel = draftsViewModel,
                    modifier = Modifier.weight(1f),
                )
            } else {
                IssueListContent(
                    state = state,
                    permissions = permissions,
                    soloMemberId = soloMemberId,
                    agents = teamAgents,
                    collapsed = collapsed,
                    onToggleCollapsed = { groupKey, isCollapsed ->
                        collapsed = if (isCollapsed) collapsed - groupKey else collapsed + groupKey
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
                        // Resolve relay + device presence while the user is
                        // still picking, so Start coding is ready when tapped.
                        if (state.board?.repositoryId != null) viewModel.ensureSteerLoaded()
                    },
                    onOpenGraph = { id -> graphIssueId = id },
                    gettingStarted = gettingStarted,
                    onNewIssue = onNewIssue,
                )
            }
        }

        // Floating selection bar, above the app's bottom bar zone (EXP-405 —
        // back to the bottom overlay so entering multi-select never reflows
        // the list; mirrors GatedServersBanner). Outcomes are toasts (EXP-1031).
        if (selectionActive) {
            // #EXP-1388: THE slot the Chats selection bar stands in too.
            SelectionBarSlot {
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
                        // Same gate as the single-issue delete: any member
                        // moderates (permissions are membership-only).
                        showDelete = permissions.isModerator,
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
                                online.isEmpty() -> toaster.error(
                                    "No desktop online. Open the Exponential desktop app to run here.",
                                )
                                // EXP-825: the composer IS the launcher; the
                                // selection clears at once (web parity).
                                else -> {
                                    onOpenAgent(AgentComposerSeed(issueIds = selectedIds.toList()))
                                    selectedIds = emptySet()
                                }
                            }
                        },
                        onDelete = { confirmBulkDelete = true },
                    )
                }
            }
        }

        }
    }

    if (confirmBulkDelete) {
        val count = selectedIds.size
        PromptAlert(
            prompt = Prompts.DeleteIssues.prompt(count),
            onDismiss = { confirmBulkDelete = false },
            handlers = mapOf(
                "delete" to {
                    confirmBulkDelete = false
                    viewModel.bulkDelete(selectedIds)
                    selectedIds = emptySet()
                },
            ),
        )
    }

    // Bulk property sheets (EXP-247) — status/priority/assignee apply then
    // clear the selection; the label sheet stays open across tri-state toggles.
    when (bulkSheet) {
        // Duplicate is set through the mark-duplicate flow, never picked.
        BulkSheet.Status -> BulkStatusPicker(
            statuses = state.teamStatuses.filter { it.category != IssueStatusCategory.Duplicate },
            selected = sharedStatus,
            onSelect = {
                viewModel.bulkUpdateStatus(selectedIds, it)
                selectedIds = emptySet()
            },
            onDismiss = { bulkSheet = null },
        )
        BulkSheet.Priority -> PriorityPicker(
            options = issuePriorityPickerOptions(),
            value = setOfNotNull(sharedPriority?.wire),
            onChange = { picked ->
                pickedPriority(picked)?.let { viewModel.bulkUpdatePriority(selectedIds, it) }
                selectedIds = emptySet()
            },
            open = true,
            onOpenChange = { open -> if (!open) bulkSheet = null },
        )
        BulkSheet.Assignee -> AssigneePicker(
            members = state.teamUsers.map { it.toPickerMember() },
            agents = teamAgents.map { it.toPickerAgent() },
            value = sharedAssignee.orEmpty(),
            onChange = { picked ->
                viewModel.bulkUpdateAssignee(selectedIds, assigneeFromPickerValue(picked.firstOrNull()))
                selectedIds = emptySet()
            },
            open = true,
            onOpenChange = { open -> if (!open) bulkSheet = null },
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
            InlineKind.Status -> BulkStatusPicker(
                statuses = state.teamStatuses.filter { it.category != IssueStatusCategory.Duplicate },
                selected = statusByIssueId[edit.issueId],
                onSelect = { viewModel.updateStatus(edit.issueId, it) },
                onDismiss = { inlineEdit = null },
            )
            InlineKind.Priority -> PriorityPicker(
                options = issuePriorityPickerOptions(),
                value = setOfNotNull(editIssue?.priority?.let { IssuePriority.fromWire(it).wire }),
                onChange = { picked ->
                    pickedPriority(picked)?.let { viewModel.updatePriority(edit.issueId, it) }
                },
                open = true,
                onOpenChange = { open -> if (!open) inlineEdit = null },
            )
        }
    }

    if (showCreateBoard) {
        // The new board's last-used pointer swaps the root list in place, so
        // dismissing is all this needs to do on success. The empty state names
        // the selected team; a null team falls back to the default team.
        CreateBoardSheet(
            teamId = createBoardTeamId,
            onCreated = { showCreateBoard = false; createBoardTeamId = null },
            onDismiss = { showCreateBoard = false; createBoardTeamId = null },
        )
    }

    // EXP-980: the mini-graph a row's blocks pill opens — the transitive chain
    // around that issue, the same view the blocked-start dialog draws.
    graphIssueId?.let { subjectId ->
        val graphIssues by viewModel.allIssues.collectAsStateWithLifecycle()
        val graphRelations by viewModel.relations.collectAsStateWithLifecycle()
        IssueGraphSheet(
            subjectIds = listOf(subjectId),
            issues = graphIssues,
            relations = graphRelations,
            onOpenIssue = onOpenIssue,
            onDismiss = { graphIssueId = null },
        )
    }

    if (showTeamSetup) {
        // Both paths already point the team selection at the new team, so the
        // empty state behind the sheet flips on its own — closing is all this
        // has to do.
        TeamSetupSheet(
            onDismiss = { showTeamSetup = false },
            onCreated = { showTeamSetup = false },
            onJoined = { showTeamSetup = false },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)
@Composable
private fun IssueListContent(
    state: IssueListState,
    permissions: TeamPermissions,
    soloMemberId: String?,
    /** #EXP-1383: the team's agents (agent assignees on the rows). */
    agents: List<com.exponential.app.data.db.AgentDefinitionEntity>,
    collapsed: Set<String>,
    onToggleCollapsed: (groupKey: String, collapsed: Boolean) -> Unit,
    onOpenIssue: (String) -> Unit,
    onInlineStatus: (String) -> Unit,
    onInlinePriority: (String) -> Unit,
    viewModel: IssueListViewModel,
    selectedIds: Set<String>,
    onToggleSelect: (String) -> Unit,
    onEnterSelection: (String) -> Unit,
    // EXP-980: a row's blocks pill opens the mini-graph for that issue.
    onOpenGraph: (String) -> Unit,
    // EXP-698 r5: the empty board's own affordances — null on a pushed board,
    // whose empty state is just an empty state.
    gettingStarted: (@Composable (Modifier) -> Unit)?,
    onNewIssue: () -> Unit,
) {
    val usersById = remember(state.users) { state.users.associateBy { it.id } }
    val agentsById = remember(agents) { agents.associateBy { it.id } }

    // EXP-1115: nothing to show until the issues shape has snapshotted at
    // least once — a full resync (or a fresh install) would otherwise flash
    // "No issues yet" plus the getting-started checklist for as long as the
    // snapshot takes. EXP-1275: skeleton rows in the issue row's geometry.
    if (state.groups.isEmpty() && !state.issuesSynced) {
        SkeletonRows(modifier = Modifier.padding(horizontal = ListGutter).padding(top = 4.dp))
        return
    }

    LazyColumn(
        // EXP-642: the capture suites scroll to a named row
        // (`performScrollToNode`) — a lazy list's off-screen rows are not
        // composed, so the list itself needs a handle.
        modifier = Modifier.fillMaxSize().testTag("issue-list"),
        // EXP-614: no horizontal contentPadding — the gutter rides the
        // individual items instead, so the sticky status header's tinted
        // band spans the full width (web/iOS parity) while the rows keep
        // their 16dp inset.
        contentPadding = PaddingValues(top = 4.dp, bottom = BottomBarInset),
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        if (state.groups.isEmpty()) {
            // EXP-698 r5: the shared empty-board shape on every client —
            // a circled glyph, a title, one line of guidance and the one
            // primary action (the compose FAB is behind this state).
            item(key = "empty") {
                EmptyState(
                    EmptyStateCopy.issues,
                    modifier = Modifier.padding(horizontal = ListGutter),
                    action = {
                        GlassPill(
                            EmptyStateCopy.issues.action!!,
                            icon = ExpIcons.uiAdd,
                            size = PillSize.Md,
                            primary = true,
                            onClick = onNewIssue,
                        )
                    },
                )
            }
            if (gettingStarted != null) {
                item(key = "getting-started") {
                    gettingStarted(Modifier.padding(top = 20.dp))
                }
            }
        } else {
            state.groups.forEach { group ->
                val isCollapsed = group.status.id in collapsed
                // EXP-620: a plain scrolling item, not a stickyHeader.
                // Pinning (EXP-578) is what forced a background onto the
                // header in the first place — an opaque fill to hide the
                // rows passing underneath, since Compose has no cheap
                // backdrop blur — and EXP-593 then washed the status colour
                // over it. Scrolling with its rows, the header needs
                // neither. P35: it is THE filled group band ×3 phones.
                item(key = "header-${group.status.id}") {
                    // EXP-523: headers ride the same reflow as their rows.
                    Box(
                        Modifier
                            .animateItem()
                            .fillMaxWidth(),
                    ) {
                        // EXP-1248: the ONE issue group band (the only
                        // list band that keeps its count).
                        IssueGroupBand(
                            glyph = { StatusIcon(group.status, size = 14.dp) },
                            name = group.status.name,
                            count = group.issues.size,
                            collapsed = isCollapsed,
                            onToggle = { onToggleCollapsed(group.status.id, isCollapsed) },
                            // The list carries no horizontal contentPadding
                            // (EXP-614): the band carries the gutter.
                            gutter = ListGutter,
                        )
                    }
                }
                if (!isCollapsed) {
                    // EXP-980/965: sub-issues hang off their parent with
                    // the shared elbow connector — the SAME rule and
                    // painter the run lists draw.
                    val guides = TreeGuides.compute(group.issues.map { it.depth })
                    itemsIndexed(group.issues, key = { _, it -> it.issue.id }) { index, entry ->
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
                        TreeGuidesRow(
                            depth = entry.depth,
                            guide = guides.getOrNull(index),
                            // The list spaces its rows; the branch bridges
                            // that gap instead of breaking at every row.
                            gap = RowGap,
                            // EXP-523: a status change moves an issue to
                            // another group. Without this the row
                            // teleports; with it it slides to where it
                            // landed.
                            modifier = Modifier
                                .animateItem()
                                .padding(horizontal = ListGutter),
                        ) {
                            IssueRow(
                                issue = entry.issue,
                                labels = entry.labels,
                                // The row's OWN resolved status: nesting
                                // can park it in another group's band.
                                resolvedStatus = entry.status ?: group.status,
                                // Solo teams hide the assignee avatar (one member).
                                assignee = if (soloMemberId != null) null else usersById[entry.issue.assigneeId],
                                // #EXP-1383: an agent assignee wears THE AGENT MARK.
                                assigneeAgent = entry.issue.assigneeAgentId?.let { agentsById[it] }?.actorRow(),
                                selected = if (selectionActive) entry.issue.id in selectedIds else null,
                                // EXP-980: what blocks this row, and what
                                // it blocks — the pill opens the graph.
                                blocks = entry.blocks,
                                onBlocksClick = { onOpenGraph(entry.issue.id) },
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

@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun IssueRow(
    issue: IssueEntity,
    labels: List<LabelEntity>,
    assignee: UserEntity?,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)? = null,
    /** #EXP-1383: an agent assignee (exclusive with [assignee]): the mark. */
    assigneeAgent: AgentSettings.AgentRowLike? = null,
    // Multi-select rendering (EXP-239): null = no selection UI; false/true
    // show the leading check indicator, true additionally tints the row.
    selected: Boolean? = null,
    // Inline property edit (EXP-247): non-null makes the status / priority
    // glyph its own tap target (long-press still forwards to [onLongClick]).
    // Null leaves the glyphs as plain decorations (SearchScreen, selection mode).
    onStatusClick: (() -> Unit)? = null,
    onPriorityClick: (() -> Unit)? = null,
    // EXP-314: the per-board list passes the issue's RESOLVED team status row
    // (custom name/color/clock glyph). Cross-team callers (search, My Issues)
    // leave it null and get the anchor-enum glyph, which is correct for the
    // builtins and never wrong-team.
    resolvedStatus: ResolvedIssueStatus? = null,
    // EXP-980: the blocks badge after the title — what blocks this issue and
    // what it blocks. Null = nothing to say, no pill. [onBlocksClick] opens
    // the mini-graph; it is the PILL's tap, never the row's.
    blocks: IssueGraph.Counts? = null,
    onBlocksClick: (() -> Unit)? = null,
    // EXP-523: lets a LazyColumn caller pass `Modifier.animateItem()` so rows
    // slide when the list reorders. Every call site names its arguments, so
    // this sits at the end without breaking any of them.
    modifier: Modifier = Modifier,
) {
    val status = IssueStatus.fromWire(issue.status)
    val priority = IssuePriority.fromWire(issue.priority)
    Row(
        modifier = modifier
            .fillMaxWidth()
            // EXP-627/EXP-642: iOS parity (`issue-row-<identifier>`) — the
            // store compositor measures its board pop-out rect off one named
            // row, and the capture suites address rows by identifier.
            .testTag("issue-row-${issue.identifier}")
            .glassRow()
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            // #EXP-1388: THE selected-row paint (the Chats rows' too).
            .selectedRowHighlight(selected == true)
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
        // Wave D (M31): THE issue row's slots — the leading selection check +
        // priority column, the mono identifier column, the status column, the
        // title, its badges and the chevron.
        IssueRowContent(
            title = issue.title,
            leading = {
                // #EXP-1388: the shared selection glyph cell (the Chats rows' too).
                if (selected != null) SelectionGlyph(selected)
                IconColumn(onClick = onPriorityClick, onLongClick = onLongClick) {
                    PriorityIcon(priority, size = 16.dp)
                }
            },
            leadingGap = 0.dp,
            identifier = issue.identifier,
            // Min-width identifier column (fits "EXP-9999" in the monospace
            // labelMedium style) so the priority icon, identifier, status icon
            // and title line up across rows for typical digit counts — but a
            // min (not fixed) width so longer identifiers (10-char prefixes,
            // big numbers, large font scale) still render in full instead of
            // clipping to a plausible-but-wrong identifier.
            identifierMinWidth = 60.dp,
            identifierEllipsis = true,
            identifierGap = 0.dp,
            status = {
                IconColumn(onClick = onStatusClick, onLongClick = onLongClick) {
                    if (resolvedStatus != null) {
                        StatusIcon(resolvedStatus, size = 16.dp)
                    } else {
                        StatusIcon(status, size = 16.dp)
                    }
                }
            },
            statusGap = 0.dp,
            titleAccessory = {
                if (blocks != null) {
                    Spacer(Modifier.width(6.dp))
                    BlocksBadge(blocks, onClick = onBlocksClick)
                }
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
                        ExpIcons.uiDueDate,
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
                if (assigneeAgent != null) {
                    Spacer(Modifier.width(8.dp))
                    AgentAvatar(icon = assigneeAgent.icon, color = assigneeAgent.color, size = 22.dp)
                } else if (assignee != null) {
                    Spacer(Modifier.width(8.dp))
                    UserAvatar(
                        user = assignee,
                        nameOrEmail = assignee.name ?: assignee.email,
                        size = 22.dp,
                    )
                }
            },
            trailingGap = 6.dp,
            trailing = {
                Icon(
                    ExpIcons.uiChevronRight,
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            },
        )
    }
}

// Horizontal gutter every list item carries; the sticky status header band is
// deliberately NOT inset by it (EXP-614).
private val ListGutter = com.exponential.app.ui.components.SelectionRowGutter

/** The list's own row spacing — what a nesting connector has to bridge. */
private val RowGap = 3.dp

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
 * A list-row status / priority glyph column (EXP-247): a 32dp-wide box the
 * height of its own glyph, and when [onClick] is non-null also its own tap
 * target that opens the inline picker and forwards long-press to the row's
 * selection gesture.
 *
 * The [fillMaxHeight] is inert — every caller is a LazyColumn item, so the
 * Row's incoming max height is Infinity and Compose skips the fill. It stays
 * only as a no-op guard for a future bounded-height caller; the tap target is
 * really 32dp x glyph height. Widen it with `heightIn(min = ...)`, never by
 * relying on the fill.
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
    sharedStatus: ResolvedIssueStatus?,
    sharedPriority: IssuePriority?,
    showAssignee: Boolean,
    showStartCoding: Boolean,
    showDelete: Boolean,
    devicesLoading: Boolean,
    onClear: () -> Unit,
    onStatus: () -> Unit,
    onPriority: () -> Unit,
    onAssignee: () -> Unit,
    onLabels: () -> Unit,
    onStartCoding: () -> Unit,
    onDelete: () -> Unit,
) {
    // EXP-1176: the bar's one text button is the row-radius rectangle like the
    // web bulk bar's Button; only the pill keeps the capsule.
    val shape = GlassTokens.ButtonShape
    val neutral = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    // #EXP-1388: the chrome (card, close, count) is the shared shell the
    // Chats selection bar wears too.
    SelectionBarShell(count = count.toString(), onClear = onClear) {
            IconButton(onClick = onStatus, modifier = Modifier.size(32.dp)) {
                if (sharedStatus != null) {
                    StatusIcon(sharedStatus, size = 18.dp)
                } else {
                    Icon(
                        ExpIcons.uiChecklist,
                        contentDescription = "Status",
                        modifier = Modifier.size(18.dp),
                        tint = neutral,
                    )
                }
            }
            IconButton(onClick = onPriority, modifier = Modifier.size(32.dp)) {
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
                IconButton(onClick = onAssignee, modifier = Modifier.size(32.dp)) {
                    Icon(
                        ExpIcons.uiAssignee,
                        contentDescription = "Assignee",
                        modifier = Modifier.size(18.dp),
                        tint = neutral,
                    )
                }
            }
            IconButton(onClick = onLabels, modifier = Modifier.size(32.dp)) {
                Icon(
                    ExpIcons.settingsLabels,
                    contentDescription = "Labels",
                    modifier = Modifier.size(18.dp),
                    tint = neutral,
                )
            }
            if (showStartCoding) {
                // The ONE primary action starts the selection on the composer
                // at once (one issue, or a batch of several): a menu with a
                // single entry was a tap for nothing (web parity).
                Row(
                    modifier = Modifier
                        .testTag("bulk-start-coding")
                        .height(32.dp)
                        .clip(shape)
                        .background(MaterialTheme.colorScheme.primary)
                        .clickable(onClick = onStartCoding)
                        .padding(horizontal = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    // EXP-698: the LABEL always renders — only the GLYPH slot
                    // swaps for the spinner while the devices load (the
                    // [GlassPill] `loading` contract). Dropping the text left
                    // the bar's one primary action as a blank white capsule
                    // with a dot in it.
                    if (devicesLoading) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(14.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onPrimary,
                        )
                    } else {
                        Icon(
                            ExpIcons.actionRun,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                            tint = MaterialTheme.colorScheme.onPrimary,
                        )
                    }
                    Spacer(Modifier.width(6.dp))
                    Text(
                        "Start coding",
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onPrimary,
                        maxLines = 1,
                    )
                }
            }
            // Destructive, so it sits LAST and alone on the far side of the
            // one primary action (EXP-698 r5, ×4). Members only — the same
            // gate the single-issue delete takes.
            if (showDelete) {
                IconButton(onClick = onDelete, modifier = Modifier.size(32.dp)) {
                    Icon(
                        ExpIcons.uiDelete,
                        contentDescription = "Delete issues",
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.error,
                    )
                }
            }
    }
}

/**
 * The selection bar's bulk-label sheet (EXP-247): tri-state rows — the FULL
 * highlight when every selected issue carries the label, the wash alone when
 * only some do, nothing otherwise. Tapping removes it from all when all have
 * it, else adds it to the ones missing it; the sheet stays open across
 * toggles.
 *
 * EXP-1021: the shared [LabelPicker]. The dash-and-check column is gone — a
 * partial row now says so in the same highlight language every other picker
 * speaks ([PickerChecked.Some]), which is what let this sheet stop being a
 * fourth variant of the label list.
 */
@Composable
private fun BulkLabelSheet(
    teamLabels: List<LabelEntity>,
    selectedEntries: List<IssueWithLabels>,
    onToggle: (labelId: String, allHave: Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    val counts = remember(selectedEntries) {
        selectedEntries.flatMap { entry -> entry.labels.map { it.id } }
            .groupingBy { it }
            .eachCount()
    }
    val total = selectedEntries.size
    // `value` is the labels EVERY selected issue carries, so the picker's own
    // toggle maths stays right: adding a partial row adds it to the ones
    // missing it, removing a full one strips it from all.
    val allHave = remember(teamLabels, counts, total) {
        teamLabels.filter { total > 0 && counts[it.id] == total }.map { it.id }.toSet()
    }

    LabelPicker(
        labels = teamLabels.map { label ->
            val count = counts[label.id] ?: 0
            label.toPickerLabel().copy(
                checked = when {
                    total > 0 && count == total -> PickerChecked.All
                    count > 0 -> PickerChecked.Some
                    else -> PickerChecked.None
                },
            )
        },
        value = allHave,
        onChange = { next ->
            // The picker reports the whole new set; this surface writes one
            // toggle at a time, so the difference IS the tapped label.
            (next - allHave).forEach { onToggle(it, false) }
            (allHave - next).forEach { onToggle(it, true) }
        },
        query = query,
        onQueryChange = { query = it },
        emptyText = if (query.isBlank()) "No labels yet" else "No matching labels",
        open = true,
        onOpenChange = { open -> if (!open) onDismiss() },
    )
}

/**
 * The board list's status picker (EXP-1021): the shared [StatusPicker] over
 * RESOLVED team rows, so the bulk bar and the long-press inline edit pick the
 * same way — both hand back the row itself, which is what the mutations take.
 */
@Composable
private fun BulkStatusPicker(
    statuses: List<ResolvedIssueStatus>,
    selected: ResolvedIssueStatus?,
    onSelect: (ResolvedIssueStatus) -> Unit,
    onDismiss: () -> Unit,
) {
    StatusPicker(
        statuses = statuses.map { it.toPickerRow() },
        value = setOfNotNull(selected?.id),
        onChange = { picked ->
            picked.firstOrNull()?.let { id -> statuses.firstOrNull { it.id == id } }?.let(onSelect)
        },
        open = true,
        onOpenChange = { open -> if (!open) onDismiss() },
    )
}
