package com.exponential.app.ui.issue

import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.getCommentBodyText
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.hasDraftImages

// One human comment in the thread: avatar, author + time header, markdown body,
// and (for the author/admins) an edit/delete overflow with an inline editor.
// Extracted from CommentThread.kt (pure move — no behavior change).
@Composable
internal fun RegularCommentRow(
    comment: CommentEntity,
    author: UserEntity?,
    isAuthor: Boolean,
    isAdmin: Boolean,
    isEditing: Boolean,
    onEdit: () -> Unit,
    onCancelEdit: () -> Unit,
    onSaveEdit: (String) -> Unit,
    onDelete: () -> Unit,
    onUploadImage: suspend (Uri) -> String?,
    mentionMembers: List<MentionMember>,
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
                .size(30.dp)
                .clip(RoundedCornerShape(percent = 50))
                .background(CommentAvatarBg),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                initials(author?.name ?: author?.email ?: "?"),
                style = MaterialTheme.typography.labelSmall,
                color = CommentAvatarText,
            )
        }
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    author?.name ?: author?.email ?: "Someone",
                    style = MaterialTheme.typography.labelMedium,
                    color = CommentAuthor,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    relativeTime(comment.createdAt) +
                        if (comment.editedAt != null) " · edited" else "",
                    style = MaterialTheme.typography.labelSmall,
                    color = CommentMeta,
                )
                if (canModify && !isEditing) {
                    Spacer(Modifier.weight(1f))
                    Box {
                        IconButton(onClick = { menuOpen = true }) {
                            Icon(
                                Icons.Filled.MoreVert,
                                contentDescription = "Comment actions",
                                tint = CommentMeta,
                            )
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
                MarkdownEditor(
                    markdown = draft,
                    editable = true,
                    onChange = { draft = it },
                    onUploadImage = onUploadImage,
                    placeholder = "Edit comment…",
                    minHeight = 40.dp,
                    mentionMembers = mentionMembers,
                    modifier = Modifier.fillMaxWidth(),
                )
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    TextButton(
                        onClick = {
                            val trimmed = draft.trim()
                            if (trimmed.isEmpty() || trimmed == bodyText) onCancelEdit()
                            else onSaveEdit(trimmed)
                        },
                        // Same gate as the composer's Send: saving while an image
                        // is still a draft:// placeholder would silently strip it.
                        enabled = !hasDraftImages(draft),
                    ) { Text("Save") }
                    TextButton(onClick = onCancelEdit) { Text("Cancel") }
                }
            } else {
                MarkdownView(bodyText)
            }
        }
    }
}

private fun initials(name: String): String =
    name.split(" ", limit = 2).mapNotNull { it.firstOrNull()?.toString() }.joinToString("").uppercase()
