package com.exponential.app.ui.issue

import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.priorityIcon
import com.exponential.app.domain.statusIcon
import com.exponential.app.ui.markdown.MarkdownEditor
import java.util.UUID

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CreateIssueSheet(
    isCreating: Boolean,
    error: String?,
    onDismiss: () -> Unit,
    onCreate: (
        title: String,
        status: IssueStatus,
        priority: IssuePriority,
        description: String?,
        dueDate: String?,
        pendingImages: Map<String, Uri>,
        keepOpen: Boolean,
    ) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var title by remember { mutableStateOf("") }
    var description by remember { mutableStateOf("") }
    var status by remember { mutableStateOf(IssueStatus.Backlog) }
    var priority by remember { mutableStateOf(IssuePriority.None) }
    var dueDate by remember { mutableStateOf<String?>(null) }
    var createMore by remember { mutableStateOf(false) }
    var statusMenuOpen by remember { mutableStateOf(false) }
    var priorityMenuOpen by remember { mutableStateOf(false) }
    var datePickerOpen by remember { mutableStateOf(false) }

    // Image upload defers actual /api/issues/<id>/images calls until after the
    // issue exists. Picking an image stashes its content URI under a stable
    // placeholder URL ("draft://<uuid>") that is inserted into the markdown so
    // the editor can preview it. On submit, the parent VM strips placeholders,
    // creates the issue, uploads each image, then updates the description with
    // the real URLs.
    val pendingImages = remember { mutableStateMapOf<String, Uri>() }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
        Column(modifier = Modifier.padding(16.dp).fillMaxWidth()) {
            Text("New issue", style = MaterialTheme.typography.titleMedium)
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
            )
            Spacer(Modifier.height(12.dp))
            Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                OutlinedButton(onClick = { statusMenuOpen = true }) {
                    Icon(statusIcon(status), null, modifier = Modifier.width(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(status.label)
                }
                DropdownMenu(expanded = statusMenuOpen, onDismissRequest = { statusMenuOpen = false }) {
                    issueStatusOrder.forEach { item ->
                        DropdownMenuItem(
                            text = { Text(item.label) },
                            leadingIcon = { Icon(statusIcon(item), null) },
                            onClick = { status = item; statusMenuOpen = false },
                        )
                    }
                }
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = { priorityMenuOpen = true }) {
                    Icon(priorityIcon(priority), null, modifier = Modifier.width(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(priority.label)
                }
                DropdownMenu(expanded = priorityMenuOpen, onDismissRequest = { priorityMenuOpen = false }) {
                    issuePriorityOrder.forEach { item ->
                        DropdownMenuItem(
                            text = { Text(item.label) },
                            leadingIcon = { Icon(priorityIcon(item), null) },
                            onClick = { priority = item; priorityMenuOpen = false },
                        )
                    }
                }
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = { datePickerOpen = true }) {
                    Text(dueDate ?: "Due date")
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
                verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
            ) {
                Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                    Checkbox(checked = createMore, onCheckedChange = { createMore = it })
                    Text("Create more")
                }
                Button(
                    enabled = !isCreating && title.isNotBlank(),
                    onClick = {
                        onCreate(
                            title,
                            status,
                            priority,
                            description,
                            dueDate,
                            pendingImages.toMap(),
                            createMore,
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
                    Text(if (isCreating) "Creating…" else "Create issue")
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }

    if (datePickerOpen) {
        IssueDatePickerDialog(
            initialDate = dueDate,
            onConfirm = { dueDate = it; datePickerOpen = false },
            onDismiss = { datePickerOpen = false },
        )
    }
}
