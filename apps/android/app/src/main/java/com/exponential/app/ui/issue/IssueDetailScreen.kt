package com.exponential.app.ui.issue

import com.exponential.app.ui.parseColor

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Archive
import androidx.compose.material.icons.filled.DeleteOutline
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Unarchive
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.ui.graphics.Color
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.priorityIcon
import com.exponential.app.domain.statusIcon
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.extractDescriptionMarkdown

@OptIn(ExperimentalMaterial3Api::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun IssueDetailScreen(
    issueId: String,
    onBack: () -> Unit,
    viewModel: IssueDetailViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsState()
    val permissions by viewModel.permissions.collectAsState()
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

    LaunchedEffect(issue?.id) {
        if (issue != null) {
            titleField = issue.title
            descriptionField = extractDescriptionMarkdown(issue.description)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(issue?.identifier ?: "Issue", maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    if (isModerator) {
                        var overflowOpen by remember { mutableStateOf(false) }
                        Box {
                            IconButton(onClick = { overflowOpen = true }) {
                                Icon(Icons.Filled.MoreVert, contentDescription = "Issue actions")
                            }
                            DropdownMenu(expanded = overflowOpen, onDismissRequest = { overflowOpen = false }) {
                                DropdownMenuItem(
                                    leadingIcon = {
                                        Icon(
                                            if (issue?.archivedAt == null) Icons.Filled.Archive else Icons.Filled.Unarchive,
                                            contentDescription = null,
                                        )
                                    },
                                    text = { Text(if (issue?.archivedAt == null) "Archive" else "Unarchive") },
                                    onClick = {
                                        overflowOpen = false
                                        viewModel.toggleArchive()
                                    },
                                )
                                DropdownMenuItem(
                                    leadingIcon = { Icon(Icons.Filled.DeleteOutline, contentDescription = null) },
                                    text = { Text("Delete issue") },
                                    onClick = {
                                        overflowOpen = false
                                        viewModel.delete(onBack)
                                    },
                                )
                            }
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                ),
            )
        },
        containerColor = MaterialTheme.colorScheme.background,
    ) { padding ->
        if (issue == null) {
            Column(
                modifier = Modifier.padding(padding).fillMaxSize(),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text("Loading…", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            return@Scaffold
        }

        val status = IssueStatus.fromWire(issue.status)
        val priority = IssuePriority.fromWire(issue.priority)

        Column(
            modifier = Modifier
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 12.dp)
                .fillMaxWidth(),
        ) {
            PlanStateBadge(issue.agentPlanState, issue.status)
            OutlinedTextField(
                value = titleField,
                onValueChange = { titleField = it },
                modifier = Modifier
                    .fillMaxWidth()
                    .onFocusChanged { focus ->
                        if (!focus.isFocused && titleField.isNotBlank() && titleField != issue.title) {
                            viewModel.updateTitle(titleField)
                        }
                    },
                textStyle = MaterialTheme.typography.titleLarge,
                singleLine = true,
            )
            Spacer(Modifier.height(12.dp))
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedButton(onClick = { statusMenuOpen = true }, enabled = isModerator) {
                    Icon(statusIcon(status), null, modifier = Modifier.width(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(status.label)
                }
                OutlinedButton(onClick = { priorityMenuOpen = true }, enabled = isModerator) {
                    Icon(priorityIcon(priority), null, modifier = Modifier.width(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(priority.label)
                }
                OutlinedButton(onClick = { assigneeMenuOpen = true }, enabled = isModerator) {
                    Icon(Icons.Filled.Person, null, modifier = Modifier.width(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(state.assignee?.name ?: state.assignee?.email ?: "Unassigned", maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                OutlinedButton(onClick = { datePickerOpen = true }, enabled = isModerator) {
                    Text(issue.dueDate ?: "Due date")
                }
                if (issue.dueDate != null) {
                    OutlinedButton(onClick = { dueTimePickerOpen = true }, enabled = isModerator) {
                        Icon(Icons.Filled.Schedule, null, modifier = Modifier.width(16.dp))
                        Spacer(Modifier.width(6.dp))
                        Text(issue.dueTime ?: "Start time")
                    }
                    OutlinedButton(onClick = { endTimePickerOpen = true }, enabled = isModerator) {
                        Text(issue.endTime ?: "End time")
                    }
                }
                OutlinedButton(onClick = { recurrenceSheetOpen = true }, enabled = isModerator) {
                    Icon(Icons.Filled.Repeat, null, modifier = Modifier.width(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(formatRecurrence(issue.recurrenceInterval, issue.recurrenceUnit))
                }
                OutlinedButton(onClick = { labelsOpen = true }) {
                    Icon(Icons.Filled.Add, null, modifier = Modifier.size(14.dp))
                    Spacer(Modifier.width(6.dp))
                    Text("Labels")
                }
            }

            if (state.issueLabels.isNotEmpty()) {
                Spacer(Modifier.height(8.dp))
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    state.issueLabels.forEach { label ->
                        Row(
                            modifier = Modifier
                                .background(
                                    parseColor(label.color).copy(alpha = 0.18f),
                                    androidx.compose.foundation.shape.RoundedCornerShape(6.dp),
                                )
                                .padding(horizontal = 8.dp, vertical = 4.dp)
                                .clickable { viewModel.toggleLabel(label.id, true) },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box(modifier = Modifier.size(8.dp).background(parseColor(label.color), CircleShape))
                            Spacer(Modifier.width(4.dp))
                            Text(label.name, style = MaterialTheme.typography.labelSmall)
                        }
                    }
                }
            }

            Spacer(Modifier.height(20.dp))
            Text(
                "Description",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            MarkdownEditor(
                markdown = descriptionField,
                editable = true,
                onChange = {
                    descriptionField = it
                    viewModel.updateDescription(it)
                },
                onUploadImage = { uri -> viewModel.uploadImage(uri) },
                imageUploadEnabled = true,
            )

            // Persist any pending (debounced) description edit when leaving.
            DisposableEffect(Unit) {
                onDispose { viewModel.flushDescription() }
            }

            Spacer(Modifier.height(20.dp))
            AttachmentList(issueId = issue.id)

            Spacer(Modifier.height(20.dp))
            CommentThread(
                issueId = issue.id,
                canApprovePlan = permissions.canApprovePlan(issue.creatorId),
            )
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
        val assigneeItems: List<com.exponential.app.data.db.UserEntity?> =
            listOf<com.exponential.app.data.db.UserEntity?>(null) + state.users
        IssuePickerSheet(
            title = "Assignee",
            items = assigneeItems,
            selected = assigneeItems.firstOrNull { it?.id == state.assignee?.id },
            keyOf = { it?.id ?: "__unassigned__" },
            labelOf = { user -> user?.name ?: user?.email ?: "Unassigned" },
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
}

// Compact pill rendered above the title when the agent has a plan in
// flight. Hidden when there's no plan state to surface. Mirrors the state
// derivation in apps/web/src/components/issue-timeline.tsx (~lines 421-446).
@Composable
private fun PlanStateBadge(state: String?, issueStatus: String) {
    val (text, color) = when (state) {
        "drafting" -> "Drafting" to Color(0xFFEAB308)
        "awaiting_answer" -> "Awaiting answer" to Color(0xFFB388F5)
        "awaiting_approval" -> "Awaiting approval" to Color(0xFF60A5FA)
        "approved" -> {
            // Hide once the issue itself is closed.
            if (issueStatus == "done" || issueStatus == "cancelled") return
            "Approved" to Color(0xFF34D399)
        }
        else -> return
    }
    Box(
        modifier = Modifier
            .padding(bottom = 8.dp)
            .background(color.copy(alpha = 0.18f), RoundedCornerShape(6.dp))
            .padding(horizontal = 8.dp, vertical = 4.dp),
    ) {
        Text(text, style = MaterialTheme.typography.labelSmall, color = color)
    }
}

