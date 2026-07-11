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
import androidx.compose.material.icons.filled.RocketLaunch
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
import com.exponential.app.domain.WorkspacePermissions
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.InitialsAvatar
import com.exponential.app.ui.components.LabelDot
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.home.HomeViewModel
import com.exponential.app.ui.home.ProjectSwitcherSheet
import com.exponential.app.ui.onboarding.CreateProjectSheet
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow

/**
 * How the issue list is mounted:
 * - [Root] — the Issues tab's home. No back button; the pinned header is the
 *   inline project switcher (current project name + expander glyph → the
 *   switcher sheet) plus the settings gear. The project swaps in place.
 * - [Pushed] — a pushed `project/{projectId}` destination (share target,
 *   deep link, search): back button, fixed project.
 */
enum class IssueListMode { Root, Pushed }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueListScreen(
    projectId: String?,
    mode: IssueListMode,
    onOpenIssue: (String) -> Unit,
    onBack: () -> Unit = {},
    onOpenSettings: () -> Unit = {},
    // EXP-56: opens the current project's workspace releases (Root mode only).
    onOpenReleases: (workspaceId: String) -> Unit = {},
    viewModel: IssueListViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val syncBanner by viewModel.syncBanner.collectAsStateWithLifecycle()
    var showFilters by remember { mutableStateOf(false) }
    var showSwitcher by remember { mutableStateOf(false) }
    var showCreateProject by remember { mutableStateOf(false) }
    var collapsed by remember { mutableStateOf(emptySet<IssueStatus>()) }

    // Root mode resolves the project outside the nav args (last-used → first),
    // so the ViewModel is re-pointed whenever the resolution changes.
    LaunchedEffect(projectId) { viewModel.setProject(projectId.orEmpty()) }

    // The switcher tree + bootstrap live in the (old home) switcher ViewModel;
    // only the Root mount needs them. The mode of a mounted screen never
    // changes, so the conditional composable calls are stable.
    val homeViewModel: HomeViewModel? = if (mode == IssueListMode.Root) hiltViewModel() else null
    val homeState = homeViewModel?.state?.collectAsStateWithLifecycle()?.value
    val homeError = homeViewModel?.error?.collectAsStateWithLifecycle()?.value
    if (homeViewModel != null) {
        LaunchedEffect(Unit) { homeViewModel.bootstrap() }
    }

    // "Any signed-in account has a project" — gates the cross-account switcher
    // (so a projectless active account with a project-bearing sibling can still
    // switch to it). The spinner instead gates on the ACTIVE account
    // (activeAccountHasProject), because only the active account's project can
    // resolve into the root list.
    val hasAnyProject = homeState?.projectTree
        ?.any { group -> group.workspaceBlocks.any { it.projects.isNotEmpty() } } == true

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            // Pinned nav row. Pushed: circular back button. Root: the inline
            // project switcher control + the settings gear. The filter button
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
                        ProjectSwitcherControl(
                            name = state.project?.name,
                            enabled = hasAnyProject,
                            onClick = { showSwitcher = true },
                        )
                        Spacer(Modifier.weight(1f))
                        // Releases (EXP-56): the bottom bar is full, so the
                        // workspace's releases live behind a top-bar action.
                        state.project?.workspaceId?.let { workspaceId ->
                            CircleIconButton(
                                Icons.Filled.RocketLaunch,
                                "Releases",
                                onClick = { onOpenReleases(workspaceId) },
                            )
                            Spacer(Modifier.width(8.dp))
                        }
                        CircleIconButton(Icons.Filled.Settings, "Settings", onClick = onOpenSettings)
                    }
                }
            }

            SyncBannerRow(syncBanner, Modifier.padding(horizontal = 16.dp, vertical = 4.dp))

            if (mode == IssueListMode.Root && projectId.isNullOrBlank()) {
                // No project on this account yet (companion app: projects are
                // created on web/desktop). When the ACTIVE account already has a
                // project, the current one is still resolving — show a spinner,
                // not the empty copy. An active account with only projectless
                // workspaces (a fresh account) falls through to the empty state,
                // never a perpetual spinner.
                if (homeState?.activeAccountHasProject == true) {
                    LoadingState()
                } else {
                    // Still syncing until the workspace bootstrap finishes AND
                    // the projects shape has reached up-to-date at least once —
                    // otherwise a companion account that DOES have projects would
                    // briefly flash "Create your first project" before its
                    // projects snapshot lands. Only a settled, genuinely empty
                    // account shows the create-project copy.
                    val stillSyncing = homeState == null ||
                        homeState.isSyncing ||
                        !homeState.activeAccountProjectsSynced
                    val syncingOrError = stillSyncing || homeError != null
                    EmptyState(
                        message = when {
                            homeError != null -> homeError
                            stillSyncing -> "Syncing…"
                            else -> "No projects yet. Create your first project to get started."
                        },
                        icon = Icons.Filled.UnfoldMore,
                        action = if (syncingOrError) null else {
                            {
                                Button(onClick = { showCreateProject = true }) {
                                    Text("Create project")
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
        ProjectSwitcherSheet(
            groups = homeState?.projectTree ?: emptyList(),
            onSelect = { accountId, pickedProjectId ->
                homeViewModel.selectProject(accountId, pickedProjectId)
                showSwitcher = false
            },
            onDismiss = { showSwitcher = false },
            onCreateProject = {
                showSwitcher = false
                showCreateProject = true
            },
        )
    }

    if (showCreateProject) {
        // The new project's last-used pointer swaps the root list in place, so
        // dismissing is all this needs to do on success.
        CreateProjectSheet(
            workspaceId = null,
            onCreated = { showCreateProject = false },
            onDismiss = { showCreateProject = false },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun IssueListContent(
    state: IssueListState,
    permissions: WorkspacePermissions,
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
            // Large project title (scrolls with content, iOS .navigationTitle).
            item(key = "title") {
                Text(
                    state.project?.name ?: "Project",
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

// The Issues tab root's inline project switcher: current project name + an
// up/down expander glyph as one tappable glass control (iOS combobox pattern).
@Composable
private fun ProjectSwitcherControl(
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
            contentDescription = "Switch project",
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
