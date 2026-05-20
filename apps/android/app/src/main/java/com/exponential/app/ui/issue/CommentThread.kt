package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.data.api.getCommentBodyText
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.UserEntity
import kotlinx.coroutines.launch

@Composable
fun CommentThread(
    issueId: String,
    viewModel: CommentThreadViewModel = hiltViewModel(),
) {
    LaunchedEffect(issueId) { viewModel.bind(issueId) }
    val state by viewModel.state.collectAsState()
    val scope = rememberCoroutineScope()
    var draft by remember { mutableStateOf("") }
    var editingId by remember { mutableStateOf<String?>(null) }
    var sending by remember { mutableStateOf(false) }

    HorizontalDivider()
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
        Text(
            if (state.comments.isEmpty()) "Comments"
            else "Comments (${state.comments.size})",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(8.dp))

        if (state.comments.isEmpty()) {
            Text(
                "No comments yet. Be the first to add one.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        state.comments.forEach { comment ->
            CommentRow(
                comment = comment,
                author = state.usersById[comment.authorId],
                isAuthor = state.currentUserId != null && comment.authorId == state.currentUserId,
                isAdmin = state.isAdmin,
                isEditing = editingId == comment.id,
                onEdit = { editingId = comment.id },
                onCancelEdit = { editingId = null },
                onSaveEdit = { text ->
                    scope.launch {
                        viewModel.updateComment(comment.id, text)
                        editingId = null
                    }
                },
                onDelete = {
                    scope.launch { viewModel.deleteComment(comment.id) }
                },
            )
        }

        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.Bottom) {
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                placeholder = { Text("Write a comment…") },
                modifier = Modifier.weight(1f),
                maxLines = 4,
                enabled = !sending,
            )
            Spacer(Modifier.width(8.dp))
            IconButton(
                onClick = {
                    val trimmed = draft.trim()
                    if (trimmed.isEmpty()) return@IconButton
                    sending = true
                    scope.launch {
                        viewModel.createComment(trimmed)
                        draft = ""
                        sending = false
                    }
                },
                enabled = !sending && draft.isNotBlank(),
            ) {
                Icon(Icons.AutoMirrored.Filled.Send, contentDescription = "Send")
            }
        }
    }
}

@Composable
private fun CommentRow(
    comment: CommentEntity,
    author: UserEntity?,
    isAuthor: Boolean,
    isAdmin: Boolean,
    isEditing: Boolean,
    onEdit: () -> Unit,
    onCancelEdit: () -> Unit,
    onSaveEdit: (String) -> Unit,
    onDelete: () -> Unit,
) {
    val canModify = isAuthor || isAdmin
    val bodyText = remember(comment.body) { getCommentBodyText(comment.body) }
    var draft by remember(comment.id, isEditing) { mutableStateOf(bodyText) }
    var menuOpen by remember { mutableStateOf(false) }

    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Box(
            modifier = Modifier
                .size(28.dp)
                .padding(top = 2.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                initials(author?.name ?: author?.email ?: "?"),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(Modifier.width(8.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    author?.name ?: author?.email ?: "Someone",
                    style = MaterialTheme.typography.labelMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    relativeTime(comment.createdAt) +
                        if (comment.editedAt != null) " · edited" else "",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (canModify && !isEditing) {
                    Spacer(Modifier.weight(1f))
                    Box {
                        IconButton(onClick = { menuOpen = true }) {
                            Icon(Icons.Filled.MoreVert, contentDescription = "Comment actions")
                        }
                        DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                            DropdownMenuItem(
                                text = { Text("Edit") },
                                onClick = { menuOpen = false; onEdit() },
                            )
                            DropdownMenuItem(
                                text = { Text("Delete") },
                                onClick = { menuOpen = false; onDelete() },
                            )
                        }
                    }
                }
            }
            if (isEditing) {
                OutlinedTextField(
                    value = draft,
                    onValueChange = { draft = it },
                    modifier = Modifier.fillMaxWidth(),
                    maxLines = 4,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    TextButton(
                        onClick = {
                            val trimmed = draft.trim()
                            if (trimmed.isEmpty() || trimmed == bodyText) onCancelEdit()
                            else onSaveEdit(trimmed)
                        },
                    ) { Text("Save") }
                    TextButton(onClick = onCancelEdit) { Text("Cancel") }
                }
            } else {
                Text(bodyText, style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

private fun initials(name: String): String =
    name.split(" ", limit = 2).mapNotNull { it.firstOrNull()?.toString() }.joinToString("").uppercase()

private fun relativeTime(iso: String): String {
    return try {
        val instant = java.time.Instant.parse(iso)
        val now = java.time.Instant.now()
        val seconds = java.time.Duration.between(instant, now).seconds
        when {
            seconds < 60 -> "just now"
            seconds < 3600 -> "${seconds / 60}m ago"
            seconds < 86400 -> "${seconds / 3600}h ago"
            else -> "${seconds / 86400}d ago"
        }
    } catch (_: Throwable) {
        ""
    }
}
