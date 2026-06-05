package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.DeleteOutline
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.NotificationsOff
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.PersonOff
import androidx.compose.material.icons.filled.SmartToy
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.priorityIcon
import com.exponential.app.domain.statusIcon
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.extractDescriptionMarkdown
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.PlanColors
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection

// iOS-parity issue detail: a centered "Issue" nav title, an identifier chip +
// overflow header row, a large editable title, the description editor, then the
// grouped Status/Priority/Assignee card, separate Due-date / Times / Repeat
// cards, a Labels section (colored-dot toggle chips), attachments and comments.
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun IssueDetailScreen(
    issueId: String,
    onBack: () -> Unit,
    viewModel: IssueDetailViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val isSubscribed by viewModel.isSubscribed.collectAsStateWithLifecycle()
    val isModerator = permissions.isModerator
    val issue = state.issue
    var titleField by remember { mutableStateOf("") }
    var descriptionField by remember { mutableStateOf("") }
    var statusMenuOpen by remember { mutableStateOf(false) }
    var priorityMenuOpen by remember { mutableStateOf(false) }
    var assigneeMenuOpen by remember { mutableStateOf(false) }
    var datePickerOpen by remember { mutableStateOf(false) }
    var dueTimePickerOpen by remember { mutableStateOf(false) }
    var endTimePickerOpen by remember { mutableStateOf(false) }
    var recurrenceSheetOpen by remember { mutableStateOf(false) }
    var labelsOpen by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }

    LaunchedEffect(issue?.id) {
        if (issue != null) {
            titleField = issue.title
            descriptionField = extractDescriptionMarkdown(issue.description)
        }
    }

    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Issue") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    if (issue != null) {
                        IconButton(onClick = { viewModel.toggleSubscribe() }) {
                            Icon(
                                if (isSubscribed) Icons.Filled.Notifications else Icons.Filled.NotificationsOff,
                                contentDescription = if (isSubscribed) "Unsubscribe" else "Subscribe",
                                tint = if (isSubscribed) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                    containerColor = Color.Transparent,
                ),
            )
        },
        containerColor = Color.Transparent,
    ) { padding ->
        if (issue == null) {
            Column(
                modifier = Modifier.padding(padding).fillMaxSize(),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text("Loading…", color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary))
            }
            return@Scaffold
        }

        val status = IssueStatus.fromWire(issue.status)
        val priority = IssuePriority.fromWire(issue.priority)
        val mutedTint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

        Column(
            modifier = Modifier
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 8.dp)
                .fillMaxWidth(),
        ) {
            // Header: identifier chip + plan badge + overflow
            Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    issue.identifier,
                    style = MaterialTheme.typography.labelMedium,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .glassButton()
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                )
                Spacer(Modifier.width(8.dp))
                PlanStateBadge(issue.agentPlanState, issue.status)
                Spacer(Modifier.weight(1f))
                if (isModerator) {
                    var overflowOpen by remember { mutableStateOf(false) }
                    Box {
                        IconButton(onClick = { overflowOpen = true }) {
                            Icon(Icons.Filled.MoreVert, contentDescription = "Issue actions")
                        }
                        DropdownMenu(expanded = overflowOpen, onDismissRequest = { overflowOpen = false }) {
                            DropdownMenuItem(
                                leadingIcon = { Icon(Icons.Filled.DeleteOutline, contentDescription = null) },
                                text = { Text("Delete issue") },
                                onClick = {
                                    overflowOpen = false
                                    confirmDelete = true
                                },
                            )
                        }
                    }
                }
            }

            Spacer(Modifier.height(8.dp))
            // Large title (borderless, save on focus-loss)
            BasicTextField(
                value = titleField,
                onValueChange = { titleField = it },
                readOnly = !isModerator,
                textStyle = MaterialTheme.typography.headlineSmall.copy(
                    color = MaterialTheme.colorScheme.onSurface,
                ),
                cursorBrush = SolidColor(MaterialTheme.colorScheme.onSurface),
                modifier = Modifier
                    .fillMaxWidth()
                    .onFocusChanged { focus ->
                        if (isModerator && !focus.isFocused && titleField.isNotBlank() && titleField != issue.title) {
                            viewModel.updateTitle(titleField)
                        }
                    },
                decorationBox = { inner ->
                    if (titleField.isEmpty()) {
                        Text(
                            "Title",
                            style = MaterialTheme.typography.headlineSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                    inner()
                },
            )

            Spacer(Modifier.height(16.dp))
            MarkdownEditor(
                markdown = descriptionField,
                editable = isModerator,
                onChange = {
                    descriptionField = it
                    viewModel.updateDescription(it)
                },
                onUploadImage = if (isModerator) { uri -> viewModel.uploadImage(uri) } else null,
                imageUploadEnabled = isModerator,
            )
            DisposableEffect(Unit) {
                onDispose { viewModel.flushDescription() }
            }

            Spacer(Modifier.height(20.dp))
            // Grouped Status / Priority / Assignee card
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .glassSection()
                    .padding(vertical = 4.dp)
                    .alpha(if (isModerator) 1f else 0.55f),
            ) {
                DetailRow(label = "Status", enabled = isModerator, onClick = { statusMenuOpen = true }) {
                    StatusIcon(status, size = 14.dp)
                    Spacer(Modifier.width(6.dp))
                    Text(status.label, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                }
                CardDivider()
                DetailRow(label = "Priority", enabled = isModerator, onClick = { priorityMenuOpen = true }) {
                    PriorityIcon(priority, size = 14.dp)
                    Spacer(Modifier.width(6.dp))
                    Text(priority.label, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                }
                CardDivider()
                DetailRow(label = "Assignee", enabled = isModerator, onClick = { assigneeMenuOpen = true }) {
                    val assignee = state.assignee
                    Text(
                        assignee?.name ?: assignee?.email ?: "Unassigned",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = if (assignee != null) TextEmphasis.Primary else TextEmphasis.Tertiary,
                        ),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }

            Spacer(Modifier.height(20.dp))
            // Due date card
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .glassSection()
                    .padding(vertical = 4.dp)
                    .alpha(if (isModerator) 1f else 0.55f),
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .then(if (isModerator) Modifier.clickable { datePickerOpen = true } else Modifier)
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(
                        Icons.Filled.CalendarMonth,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                        tint = if (issue.dueDate != null) dueDateColor(issue.dueDate) else mutedTint,
                    )
                    Spacer(Modifier.width(10.dp))
                    Text("Due date", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                    Spacer(Modifier.weight(1f))
                    if (issue.dueDate != null) {
                        Text(
                            formatDueDate(issue.dueDate),
                            style = MaterialTheme.typography.bodyMedium,
                            color = dueDateColor(issue.dueDate),
                        )
                        Spacer(Modifier.width(8.dp))
                        Icon(
                            Icons.Filled.Close,
                            contentDescription = "Clear due date",
                            modifier = Modifier
                                .size(18.dp)
                                .then(if (isModerator) Modifier.clickable { viewModel.updateDueDate(null) } else Modifier),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    } else {
                        Text(
                            "None",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
            }

            // Start / End time card (only when a due date is set)
            if (issue.dueDate != null) {
                Spacer(Modifier.height(20.dp))
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassSection()
                        .padding(vertical = 4.dp)
                        .alpha(if (isModerator) 1f else 0.55f),
                ) {
                    DetailRow(label = "Start time", enabled = isModerator, onClick = { dueTimePickerOpen = true }) {
                        TimeValue(issue.dueTime)
                    }
                    CardDivider()
                    DetailRow(label = "End time", enabled = isModerator, onClick = { endTimePickerOpen = true }) {
                        TimeValue(issue.endTime)
                    }
                }
            }

            Spacer(Modifier.height(20.dp))
            // Repeat card
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .glassSection()
                    .padding(vertical = 4.dp)
                    .alpha(if (isModerator) 1f else 0.55f),
            ) {
                DetailRow(label = "Repeat", enabled = isModerator, onClick = { recurrenceSheetOpen = true }) {
                    Text(
                        formatRecurrence(issue.recurrenceInterval, issue.recurrenceUnit),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = if (issue.recurrenceInterval == null) TextEmphasis.Tertiary else TextEmphasis.Primary,
                        ),
                    )
                }
            }

            Spacer(Modifier.height(20.dp))
            // Labels section (all workspace labels as colored-dot toggle chips)
            Text(
                "Labels",
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Spacer(Modifier.height(8.dp))
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                val assignedIds = remember(state.issueLabels) { state.issueLabels.map { it.id }.toSet() }
                state.workspaceLabels.forEach { label ->
                    val assigned = label.id in assignedIds
                    Row(
                        modifier = Modifier
                            .glassButton(active = assigned)
                            .then(if (isModerator) Modifier.clickable { viewModel.toggleLabel(label.id, assigned) } else Modifier)
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(modifier = Modifier.size(8.dp).background(parseColor(label.color), CircleShape))
                        Spacer(Modifier.width(5.dp))
                        Text(label.name, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurface)
                    }
                }
                if (isModerator) {
                    Row(
                        modifier = Modifier
                            .glassButton()
                            .clickable { labelsOpen = true }
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(14.dp), tint = mutedTint)
                        Spacer(Modifier.width(4.dp))
                        Text("Label", style = MaterialTheme.typography.labelSmall, color = mutedTint)
                    }
                }
            }

            Spacer(Modifier.height(20.dp))
            AttachmentList(issueId = issue.id)

            Spacer(Modifier.height(20.dp))
            // Agent plan/question lifecycle (first-class panel; the plan/question
            // text is fetched server-side, not synced).
            AgentPlanPanel(
                issueId = issue.id,
                canApprovePlan = permissions.canApprovePlan(issue.creatorId),
            )

            Spacer(Modifier.height(8.dp))
            CommentThread(issueId = issue.id)
        }
    }

    if (statusMenuOpen && issue != null && isModerator) {
        val currentStatus = IssueStatus.fromWire(issue.status)
        IssuePickerSheet(
            title = "Status",
            items = issueStatusOrder,
            selected = currentStatus,
            labelOf = { it.label },
            iconOf = { statusIcon(it) },
            onSelect = { viewModel.updateStatus(it) },
            onDismiss = { statusMenuOpen = false },
        )
    }

    if (priorityMenuOpen && issue != null && isModerator) {
        val currentPriority = IssuePriority.fromWire(issue.priority)
        IssuePickerSheet(
            title = "Priority",
            items = issuePriorityOrder,
            selected = currentPriority,
            labelOf = { it.label },
            iconOf = { priorityIcon(it) },
            onSelect = { viewModel.updatePriority(it) },
            onDismiss = { priorityMenuOpen = false },
        )
    }

    if (assigneeMenuOpen && isModerator) {
        // People first, then Agents — assigning to an agent creates a plan request.
        val people = state.users.filter { !it.isAgent }
        val agents = state.users.filter { it.isAgent }
        val assigneeItems: List<com.exponential.app.data.db.UserEntity?> =
            listOf<com.exponential.app.data.db.UserEntity?>(null) + people + agents
        IssuePickerSheet(
            title = "Assignee",
            items = assigneeItems,
            selected = assigneeItems.firstOrNull { it?.id == state.assignee?.id },
            keyOf = { it?.id ?: "__unassigned__" },
            labelOf = { user ->
                when {
                    user == null -> "Unassigned"
                    user.isAgent -> "${user.name ?: user.email} · agent"
                    else -> user.name ?: user.email
                }
            },
            iconOf = { user ->
                when {
                    user == null -> Icons.Filled.PersonOff
                    user.isAgent -> Icons.Filled.SmartToy
                    else -> Icons.Filled.Person
                }
            },
            onSelect = { viewModel.updateAssignee(it?.id) },
            onDismiss = { assigneeMenuOpen = false },
        )
    }

    if (datePickerOpen) {
        IssueDatePickerDialog(
            initialDate = issue?.dueDate,
            onConfirm = { viewModel.updateDueDate(it); datePickerOpen = false },
            onDismiss = { datePickerOpen = false },
        )
    }

    if (labelsOpen) {
        LabelPickerSheet(
            workspaceLabels = state.workspaceLabels,
            selectedLabelIds = state.issueLabels.map { it.id }.toSet(),
            onToggle = { id, assigned -> viewModel.toggleLabel(id, assigned) },
            onCreate = { name, color -> viewModel.createAndAssignLabel(name, color) },
            onDismiss = { labelsOpen = false },
        )
    }

    if (dueTimePickerOpen && issue != null) {
        IssueTimePickerDialog(
            initialTime = issue.dueTime,
            title = "Start time",
            onConfirm = { viewModel.updateDueTime(it); dueTimePickerOpen = false },
            onClear = { viewModel.updateDueTime(null); dueTimePickerOpen = false },
            onDismiss = { dueTimePickerOpen = false },
        )
    }

    if (endTimePickerOpen && issue != null) {
        IssueTimePickerDialog(
            initialTime = issue.endTime,
            title = "End time",
            onConfirm = { viewModel.updateEndTime(it); endTimePickerOpen = false },
            onClear = { viewModel.updateEndTime(null); endTimePickerOpen = false },
            onDismiss = { endTimePickerOpen = false },
        )
    }

    if (recurrenceSheetOpen && issue != null) {
        RecurrenceSheet(
            interval = issue.recurrenceInterval,
            unit = issue.recurrenceUnit,
            onApply = { i, u -> viewModel.updateRecurrence(i, u); recurrenceSheetOpen = false },
            onDismiss = { recurrenceSheetOpen = false },
        )
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete issue") },
            text = { Text("This action cannot be undone.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    viewModel.delete(onBack)
                }) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmDelete = false }) { Text("Cancel") }
            },
        )
    }
}

// One row of a grouped glass card: fixed-width label on the left, value pushed
// to the trailing edge (iOS detailRow). Tappable when [enabled].
@Composable
private fun DetailRow(
    label: String,
    enabled: Boolean,
    onClick: () -> Unit,
    value: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.width(84.dp),
        )
        Spacer(Modifier.weight(1f))
        value()
    }
}

@Composable
private fun TimeValue(time: String?) {
    Text(
        time ?: "—",
        style = MaterialTheme.typography.bodyMedium,
        fontFamily = FontFamily.Monospace,
        color = MaterialTheme.colorScheme.onSurface.copy(
            alpha = if (time != null) TextEmphasis.Primary else TextEmphasis.Tertiary,
        ),
    )
}

// Hairline divider between grouped-card rows (iOS Divider white@6%).
@Composable
private fun CardDivider() {
    HorizontalDivider(thickness = 0.5.dp, color = Color.White.copy(alpha = 0.06f))
}

// Compact pill surfaced when the agent has a plan in flight (mirrors the web
// issue-timeline state derivation). Hidden when there's no plan state.
@Composable
private fun PlanStateBadge(state: String?, issueStatus: String) {
    val (text, color) = when (state) {
        "drafting" -> "Drafting" to PlanColors.Drafting
        "awaiting_answer" -> "Awaiting answer" to PlanColors.AwaitingAnswer
        "awaiting_approval" -> "Awaiting approval" to PlanColors.AwaitingApproval
        "approved" -> {
            if (issueStatus == "done" || issueStatus == "cancelled") return
            "Approved" to PlanColors.Approved
        }
        else -> return
    }
    Box(
        modifier = Modifier
            .clip(androidx.compose.foundation.shape.RoundedCornerShape(6.dp))
            .background(color.copy(alpha = 0.18f))
            .padding(horizontal = 8.dp, vertical = 4.dp),
    ) {
        Text(text, style = MaterialTheme.typography.labelSmall, color = color)
    }
}
