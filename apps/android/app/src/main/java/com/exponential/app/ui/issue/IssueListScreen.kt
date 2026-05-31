package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.FilterList
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
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
import androidx.compose.ui.platform.LocalContext
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
import com.exponential.app.ui.components.InitialsAvatar
import com.exponential.app.ui.components.LabelDot
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.share.SharePrefill
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueListScreen(
    projectId: String,
    onOpenIssue: (String) -> Unit,
    onBack: () -> Unit,
    sharePrefill: SharePrefill? = null,
    onSharePrefillConsumed: () -> Unit = {},
    viewModel: IssueListViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    var showCreate by remember { mutableStateOf(false) }
    var showFilters by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    var collapsed by remember { mutableStateOf(emptySet<IssueStatus>()) }

    // Content shared into the app routes here with a prefill; capture it once
    // (it gets cleared right after) and open the create sheet pre-populated.
    var capturedPrefill by remember { mutableStateOf<SharePrefill?>(null) }
    LaunchedEffect(sharePrefill) {
        if (sharePrefill != null && capturedPrefill == null) {
            capturedPrefill = sharePrefill
            showCreate = true
            onSharePrefillConsumed()
        }
    }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            // Pinned nav row: circular back + (filter) + create — iOS nav buttons.
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircleIconButton(Icons.AutoMirrored.Filled.ArrowBack, "Back", onClick = onBack)
                Spacer(Modifier.weight(1f))
                BadgedBox(badge = {
                    if (state.filters.count > 0) Badge { Text(state.filters.count.toString()) }
                }) {
                    CircleIconButton(Icons.Filled.FilterList, "Filters", onClick = { showFilters = true })
                }
                if (permissions.canCreate) {
                    Spacer(Modifier.width(8.dp))
                    CircleIconButton(Icons.Filled.Add, "Create issue", onClick = { showCreate = true })
                }
            }

            val filteredGroups = remember(state.groups, query) {
                if (query.isBlank()) {
                    state.groups
                } else {
                    state.groups
                        .map { group ->
                            group.copy(
                                issues = group.issues.filter {
                                    it.issue.title.contains(query, ignoreCase = true)
                                },
                            )
                        }
                        .filter { it.issues.isNotEmpty() }
                }
            }
            val usersById = remember(state.users) { state.users.associateBy { it.id } }

            PullToRefreshBox(
                isRefreshing = state.isRefreshing,
                onRefresh = viewModel::refresh,
                modifier = Modifier.fillMaxSize(),
            ) {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
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
                    item(key = "search") {
                        SearchField(query = query, onQueryChange = { query = it })
                        Spacer(Modifier.height(4.dp))
                    }
                    state.project?.githubRepo?.takeIf { it.isNotBlank() }?.let { repo ->
                        item(key = "repo") {
                            GithubRepoBanner(repo)
                            Spacer(Modifier.height(4.dp))
                        }
                    }
                    item(key = "pills") {
                        FilterPills(
                            active = state.tab,
                            hasFilters = !state.filters.isEmpty,
                            onSelect = viewModel::setTab,
                            onClear = viewModel::clearFilters,
                        )
                    }

                    if (filteredGroups.isEmpty()) {
                        item(key = "empty") {
                            Box(
                                modifier = Modifier.fillMaxWidth().padding(top = 64.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    if (query.isBlank()) "No issues yet" else "No issues match",
                                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                )
                            }
                        }
                    } else {
                        filteredGroups.forEach { group ->
                            val isCollapsed = group.status in collapsed
                            item(key = "header-${group.status.wire}") {
                                StatusHeader(
                                    status = group.status,
                                    count = group.issues.size,
                                    collapsed = isCollapsed,
                                    onToggle = {
                                        collapsed = if (isCollapsed) collapsed - group.status else collapsed + group.status
                                    },
                                )
                            }
                            if (!isCollapsed) {
                                items(group.issues, key = { it.issue.id }) { entry ->
                                    SwipeableIssueRow(
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
    }

    if (showCreate) {
        CreateIssueSheet(
            isCreating = state.isCreating,
            error = state.error,
            users = state.users,
            isModerator = permissions.isModerator,
            initialTitle = capturedPrefill?.title ?: "",
            initialDescription = capturedPrefill?.description ?: "",
            initialPendingImages = capturedPrefill?.pendingImages ?: emptyMap(),
            onDismiss = {
                showCreate = false
                capturedPrefill = null
            },
            onCreate = { payload ->
                viewModel.createIssue(
                    title = payload.title,
                    status = payload.status,
                    priority = payload.priority,
                    description = payload.description,
                    dueDate = payload.dueDate,
                    assigneeId = payload.assigneeId,
                    dueTime = payload.dueTime,
                    endTime = payload.endTime,
                    recurrenceInterval = payload.recurrenceInterval,
                    recurrenceUnit = payload.recurrenceUnit,
                    pendingImages = payload.pendingImages,
                )
                if (!payload.keepOpen) {
                    showCreate = false
                    capturedPrefill = null
                }
            },
        )
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
}

// Circular glass icon button (iOS .ultraThinMaterial nav circle).
@Composable
private fun CircleIconButton(icon: ImageVector, contentDescription: String, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .size(38.dp)
            .clip(CircleShape)
            .background(GlassTokens.RowFill, CircleShape)
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

// Always-visible rounded glass search field (iOS .searchable).
@Composable
private fun SearchField(query: String, onQueryChange: (String) -> Unit) {
    TextField(
        value = query,
        onValueChange = onQueryChange,
        modifier = Modifier.fillMaxWidth(),
        placeholder = {
            Text("Search issues", color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary))
        },
        leadingIcon = {
            Icon(
                Icons.Filled.Search,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        },
        trailingIcon = {
            if (query.isNotEmpty()) {
                IconButton(onClick = { onQueryChange("") }) {
                    Icon(Icons.Filled.Close, contentDescription = "Clear search")
                }
            }
        },
        singleLine = true,
        shape = RoundedCornerShape(12.dp),
        colors = TextFieldDefaults.colors(
            focusedContainerColor = GlassTokens.RowFill,
            unfocusedContainerColor = GlassTokens.RowFill,
            disabledContainerColor = GlassTokens.RowFill,
            focusedIndicatorColor = Color.Transparent,
            unfocusedIndicatorColor = Color.Transparent,
            disabledIndicatorColor = Color.Transparent,
        ),
    )
}

// Inline glass filter pills (iOS filter bar): the three tab presets as glass
// capsules, plus a "Clear" pill when any advanced filter is active.
@Composable
private fun FilterPills(
    active: FilterTab,
    hasFilters: Boolean,
    onSelect: (FilterTab) -> Unit,
    onClear: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
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

@Composable
internal fun IssueRow(
    issue: IssueEntity,
    labels: List<LabelEntity>,
    assignee: UserEntity?,
    onClick: () -> Unit,
) {
    val status = IssueStatus.fromWire(issue.status)
    val priority = IssuePriority.fromWire(issue.priority)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable(onClick = onClick)
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

// Surfaces the project's linked GitHub repo as a tappable banner (parity with
// iOS). The OAuth device flow that wires the repo lives on the web app; mobile
// can read but not change the link.
@Composable
private fun GithubRepoBanner(repo: String) {
    val context = LocalContext.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable {
                runCatching {
                    val uri = android.net.Uri.parse("https://github.com/$repo")
                    val intent = android.content.Intent(android.content.Intent.ACTION_VIEW, uri)
                    intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                    context.startActivity(intent)
                }
            }
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            Icons.Filled.Code,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(8.dp))
        Text(
            repo,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Icon(
            Icons.AutoMirrored.Filled.OpenInNew,
            contentDescription = "Open on GitHub",
            modifier = Modifier.size(13.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}
