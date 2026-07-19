package com.exponential.app.ui.issue

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.FilterList
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.UnfoldMore
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.FilterTab
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.TeamPermissions
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
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
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow

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
    viewModel: IssueListViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val syncBanner by viewModel.syncBanner.collectAsStateWithLifecycle()
    var showFilters by remember { mutableStateOf(false) }
    var showSwitcher by remember { mutableStateOf(false) }
    var showCreateBoard by remember { mutableStateOf(false) }
    var collapsed by remember { mutableStateOf(emptySet<IssueStatus>()) }

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
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            // Pinned nav row. Pushed: circular back button. Root: the inline
            // board switcher control + the settings gear. The filter button
            // moved inline with the tab-preset chips (iOS placement); the
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
                    }
                    IssueListMode.Root -> {
                        BoardSwitcherControl(
                            name = state.board?.name,
                            enabled = hasAnyBoard,
                            onClick = { showSwitcher = true },
                        )
                        Spacer(Modifier.weight(1f))
                        CircleIconButton(Icons.Filled.Settings, "Settings", onClick = onOpenSettings)
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
                    // Still syncing until the team bootstrap finishes AND
                    // the boards shape has reached up-to-date at least once —
                    // otherwise a companion account that DOES have boards would
                    // briefly flash "Create your first board" before its
                    // boards snapshot lands. Only a settled, genuinely empty
                    // account shows the create-board copy.
                    val stillSyncing = homeState == null ||
                        homeState.isSyncing ||
                        !homeState.activeAccountBoardsSynced
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
            } else {
                IssueListContent(
                    state = state,
                    permissions = permissions,
                    collapsed = collapsed,
                    onToggleCollapsed = { status, isCollapsed ->
                        collapsed = if (isCollapsed) collapsed - status else collapsed + status
                    },
                    onOpenFilters = { showFilters = true },
                    onOpenIssue = onOpenIssue,
                    viewModel = viewModel,
                )
            }
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
            onCreateBoard = {
                showSwitcher = false
                showCreateBoard = true
            },
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
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun IssueListContent(
    state: IssueListState,
    permissions: TeamPermissions,
    collapsed: Set<IssueStatus>,
    onToggleCollapsed: (IssueStatus, Boolean) -> Unit,
    onOpenFilters: () -> Unit,
    onOpenIssue: (String) -> Unit,
    viewModel: IssueListViewModel,
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
            // Large board title (scrolls with content, iOS .navigationTitle).
            item(key = "title") {
                Text(
                    state.board?.name ?: "Board",
                    style = MaterialTheme.typography.headlineLarge,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(top = 4.dp, bottom = 12.dp),
                )
            }
            item(key = "pills") {
                FilterPills(
                    active = state.tab,
                    hasFilters = !state.filters.isEmpty,
                    filterCount = state.filters.count,
                    onSelect = viewModel::setTab,
                    onClear = viewModel::clearFilters,
                    onOpenFilters = onOpenFilters,
                )
                ActiveFilterPills(
                    filters = state.filters,
                    labels = state.labels,
                    onToggleStatus = viewModel::toggleStatus,
                    onTogglePriority = viewModel::togglePriority,
                    onToggleLabel = viewModel::toggleLabel,
                    onClear = viewModel::clearFilters,
                )
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
                            LongPressIssueRow(
                                issue = entry.issue,
                                labels = entry.labels,
                                assignee = usersById[entry.issue.assigneeId],
                                canMutate = permissions.canMutateIssue(entry.issue.creatorId),
                                onMarkDone = {
                                    viewModel.updateIssueStatus(entry.issue.id, IssueStatus.Done)
                                },
                                onMoveToBacklog = {
                                    viewModel.updateIssueStatus(entry.issue.id, IssueStatus.Backlog)
                                },
                                onClick = { onOpenIssue(entry.issue.id) },
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

// Inline glass filter pills (iOS filter bar): the circular filter button with
// its active-count badge leading, then the three tab presets as glass
// capsules, plus a "Clear" pill when any advanced filter is active.
@Composable
private fun FilterPills(
    active: FilterTab,
    hasFilters: Boolean,
    filterCount: Int,
    onSelect: (FilterTab) -> Unit,
    onClear: () -> Unit,
    onOpenFilters: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BadgedBox(badge = {
            if (filterCount > 0) Badge { Text(filterCount.toString()) }
        }) {
            CircleIconButton(Icons.Filled.FilterList, "Filters", onClick = onOpenFilters)
        }
        FilterTab.entries.forEach { tab ->
            val selected = active == tab
            Text(
                tab.label,
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (selected) TextEmphasis.Primary else TextEmphasis.Secondary,
                ),
                modifier = Modifier
                    .glassButton(active = selected)
                    .clickable { onSelect(tab) }
                    .padding(horizontal = 14.dp, vertical = 8.dp),
            )
        }
        if (hasFilters) {
            Row(
                modifier = Modifier
                    .glassButton()
                    .clickable { onClear() }
                    .padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(
                    "Clear",
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
                Icon(
                    Icons.Filled.Close,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
        }
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
) {
    val status = IssueStatus.fromWire(issue.status)
    val priority = IssuePriority.fromWire(issue.priority)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PriorityIcon(priority, size = 16.dp)
        Spacer(Modifier.width(10.dp))
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
        Spacer(Modifier.width(10.dp))
        StatusIcon(status, size = 16.dp)
        Spacer(Modifier.width(10.dp))
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
