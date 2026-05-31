package com.exponential.app.ui.issue

import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.priorityIcon
import com.exponential.app.domain.statusIcon
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.markdown.MarkdownEditor
import java.util.UUID

data class CreateIssuePayload(
    val title: String,
    val status: IssueStatus,
    val priority: IssuePriority,
    val description: String?,
    val assigneeId: String?,
    val dueDate: String?,
    val dueTime: String?,
    val endTime: String?,
    val recurrenceInterval: Int?,
    val recurrenceUnit: String?,
    val pendingImages: Map<String, Uri>,
    val keepOpen: Boolean,
)

@OptIn(ExperimentalMaterial3Api::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun CreateIssueSheet(
    isCreating: Boolean,
    error: String?,
    users: List<UserEntity>,
    isModerator: Boolean,
    initialTitle: String = "",
    initialDescription: String = "",
    initialPendingImages: Map<String, Uri> = emptyMap(),
    onDismiss: () -> Unit,
    onCreate: (CreateIssuePayload) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var title by remember { mutableStateOf(initialTitle) }
    var description by remember { mutableStateOf(initialDescription) }
    var status by remember { mutableStateOf(IssueStatus.Backlog) }
    var priority by remember { mutableStateOf(IssuePriority.None) }
    var assigneeId by remember { mutableStateOf<String?>(null) }
    var dueDate by remember { mutableStateOf<String?>(null) }
    var dueTime by remember { mutableStateOf<String?>(null) }
    var endTime by remember { mutableStateOf<String?>(null) }
    var recurrenceInterval by remember { mutableStateOf<Int?>(null) }
    var recurrenceUnit by remember { mutableStateOf<String?>(null) }
    var createMore by remember { mutableStateOf(false) }
    var statusMenuOpen by remember { mutableStateOf(false) }
    var priorityMenuOpen by remember { mutableStateOf(false) }
    var assigneeMenuOpen by remember { mutableStateOf(false) }
    var datePickerOpen by remember { mutableStateOf(false) }
    var dueTimePickerOpen by remember { mutableStateOf(false) }
    var endTimePickerOpen by remember { mutableStateOf(false) }
    var recurrenceSheetOpen by remember { mutableStateOf(false) }

    val pendingImages = remember { mutableStateMapOf<String, Uri>().apply { putAll(initialPendingImages) } }
    val assigneeUser = users.firstOrNull { it.id == assigneeId }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(modifier = Modifier.padding(16.dp).fillMaxWidth()) {
            Text("New Issue", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(12.dp))
            OutlinedTextField(
                value = title,
                onValueChange = { title = it },
                placeholder = { Text("Issue title") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            MarkdownEditor(
                markdown = description,
                editable = true,
                onChange = { description = it },
                onUploadImage = { uri ->
                    val placeholder = "draft://${UUID.randomUUID()}"
                    pendingImages[placeholder] = uri
                    placeholder
                },
                imageUploadEnabled = true,
                placeholder = "Description (markdown supported)",
                minHeight = 120.dp,
                initialPendingImages = initialPendingImages,
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
                    Text(
                        assigneeUser?.name ?: assigneeUser?.email ?: "Unassigned",
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                OutlinedButton(onClick = { datePickerOpen = true }, enabled = isModerator) {
                    Text(dueDate?.let { formatDueDate(it) } ?: "Due date")
                }
                if (dueDate != null) {
                    OutlinedButton(onClick = { dueTimePickerOpen = true }, enabled = isModerator) {
                        Icon(Icons.Filled.Schedule, null, modifier = Modifier.width(16.dp))
                        Spacer(Modifier.width(6.dp))
                        Text(dueTime ?: "Start time")
                    }
                    OutlinedButton(onClick = { endTimePickerOpen = true }, enabled = isModerator) {
                        Text(endTime ?: "End time")
                    }
                }
                OutlinedButton(onClick = { recurrenceSheetOpen = true }, enabled = isModerator) {
                    Icon(Icons.Filled.Repeat, null, modifier = Modifier.width(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(formatRecurrence(recurrenceInterval, recurrenceUnit))
                }
            }
            if (error != null) {
                Spacer(Modifier.height(8.dp))
                Text(error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }
            Spacer(Modifier.height(16.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(checked = createMore, onCheckedChange = { createMore = it })
                    Text("Create more")
                }
                Button(
                    enabled = !isCreating && title.isNotBlank(),
                    onClick = {
                        onCreate(
                            CreateIssuePayload(
                                title = title,
                                status = status,
                                priority = priority,
                                description = description,
                                assigneeId = assigneeId,
                                dueDate = dueDate,
                                dueTime = dueTime,
                                endTime = endTime,
                                recurrenceInterval = recurrenceInterval,
                                recurrenceUnit = recurrenceUnit,
                                pendingImages = pendingImages.toMap(),
                                keepOpen = createMore,
                            )
                        )
                        if (createMore) {
                            title = ""
                            description = ""
                            pendingImages.clear()
                        }
                    },
                ) {
                    Icon(Icons.Filled.Check, null)
                    Spacer(Modifier.width(6.dp))
                    Text(if (isCreating) "Creating…" else "Create")
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }

    if (statusMenuOpen && isModerator) {
        IssuePickerSheet(
            title = "Status",
            items = issueStatusOrder,
            selected = status,
            labelOf = { it.label },
            iconOf = { statusIcon(it) },
            onSelect = { status = it },
            onDismiss = { statusMenuOpen = false },
        )
    }

    if (priorityMenuOpen && isModerator) {
        IssuePickerSheet(
            title = "Priority",
            items = issuePriorityOrder,
            selected = priority,
            labelOf = { it.label },
            iconOf = { priorityIcon(it) },
            onSelect = { priority = it },
            onDismiss = { priorityMenuOpen = false },
        )
    }

    if (assigneeMenuOpen && isModerator) {
        val assigneeItems: List<UserEntity?> = listOf<UserEntity?>(null) + users
        IssuePickerSheet(
            title = "Assignee",
            items = assigneeItems,
            selected = assigneeItems.firstOrNull { it?.id == assigneeId },
            keyOf = { it?.id ?: "__unassigned__" },
            labelOf = { user -> user?.name ?: user?.email ?: "Unassigned" },
            onSelect = { assigneeId = it?.id },
            onDismiss = { assigneeMenuOpen = false },
        )
    }

    if (datePickerOpen) {
        IssueDatePickerDialog(
            initialDate = dueDate,
            onConfirm = { dueDate = it; datePickerOpen = false },
            onDismiss = { datePickerOpen = false },
        )
    }

    if (dueTimePickerOpen) {
        IssueTimePickerDialog(
            initialTime = dueTime,
            title = "Start time",
            onConfirm = { dueTime = it; dueTimePickerOpen = false },
            onClear = { dueTime = null; dueTimePickerOpen = false },
            onDismiss = { dueTimePickerOpen = false },
        )
    }

    if (endTimePickerOpen) {
        IssueTimePickerDialog(
            initialTime = endTime,
            title = "End time",
            onConfirm = { endTime = it; endTimePickerOpen = false },
            onClear = { endTime = null; endTimePickerOpen = false },
            onDismiss = { endTimePickerOpen = false },
        )
    }

    if (recurrenceSheetOpen) {
        RecurrenceSheet(
            interval = recurrenceInterval,
            unit = recurrenceUnit,
            onApply = { i, u ->
                recurrenceInterval = i
                recurrenceUnit = u
                recurrenceSheetOpen = false
            },
            onDismiss = { recurrenceSheetOpen = false },
        )
    }
}
