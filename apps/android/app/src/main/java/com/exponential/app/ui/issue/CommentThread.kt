package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowCircleUp
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.CommentKind
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.commentKindOf
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.hasDraftImages
import kotlinx.coroutines.launch

// iOS comment palette (CommentRow.swift / CommentComposer.swift) — explicit white
// tiers because the issue screen floats on AppBackground (a Box, not a Material
// Surface), so any Text without an explicit color would inherit LocalContentColor's
// black default. Mirrors the glass theme exactly. Internal so the extracted
// EventRow / RegularCommentRow (same package) share the exact values.
internal val CommentAuthor = Color.White.copy(alpha = 0.9f)
internal val CommentMeta = Color.White.copy(alpha = 0.5f)
internal val CommentAvatarBg = Color.White.copy(alpha = 0.08f)
internal val CommentAvatarText = Color.White.copy(alpha = 0.7f)
internal val CommentFieldBg = Color.White.copy(alpha = 0.06f)
internal val CommentAccent = Color(red = 0.42f, green = 0.64f, blue = 1.0f)

// The human conversation thread: regular comments + activity events
// (status/assignee/label/PR changes) merged by time. Mirrors
// apps/web/src/components/issue-timeline.tsx.
@Composable
fun CommentThread(
    issueId: String,
    viewModel: CommentThreadViewModel = hiltViewModel(),
) {
    LaunchedEffect(issueId) { viewModel.bind(issueId) }
    val state by viewModel.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    var draft by remember { mutableStateOf("") }
    var editingId by remember { mutableStateOf<String?>(null) }
    var sending by remember { mutableStateOf(false) }

    val humanComments = remember(state.comments) {
        state.comments.filter { commentKindOf(it.kind) == CommentKind.Regular }
    }
    // Timeline: regular comments + activity events merged by time. The id is a
    // secondary sort key so items sharing a createdAt (e.g. a comment + the
    // status event of one mutation) keep a stable order across syncs.
    val timeline = remember(humanComments, state.events) {
        (humanComments.map { TimelineItem.Comment(it) } +
            state.events.map { TimelineItem.Event(it) })
            .sortedWith(compareBy({ it.createdAt }, { it.id }))
    }
    // Workspace members for @mention autocomplete (agents excluded — you mention people).
    val mentionMembers = remember(state.usersById) {
        state.usersById.values
            .filter { !it.isAgent }
            .map { MentionMember(it.name ?: it.email, it.email) }
    }

    HorizontalDivider()
    // No extra horizontal padding: the screen already pads 20dp, so the thread
    // aligns full-width with the description/metadata above (iOS parity).
    Column(modifier = Modifier.fillMaxWidth().padding(vertical = 12.dp)) {
        Text(
            if (humanComments.isEmpty()) "Comments"
            else "Comments (${humanComments.size})",
            style = MaterialTheme.typography.labelMedium,
            color = CommentMeta,
        )
        Spacer(Modifier.height(8.dp))

        if (timeline.isEmpty()) {
            Text(
                "No comments yet. Be the first to add one.",
                style = MaterialTheme.typography.bodySmall,
                color = CommentMeta,
            )
        }

        timeline.forEach { item ->
            when (item) {
                is TimelineItem.Event -> key(item.event.id) {
                    EventRow(
                        event = item.event,
                        usersById = state.usersById,
                        labelsById = state.labelsById,
                    )
                }
                is TimelineItem.Comment -> {
                    val comment = item.comment
                    // Stable identity per comment so list churn (e.g. an active
                    // Electric sync) doesn't re-key rows and force re-parse.
                    key(comment.id) {
                        RegularCommentRow(
                            comment = comment,
                            author = state.usersById[comment.authorId],
                            isAuthor = state.currentUserId != null && comment.authorId == state.currentUserId,
                            isAdmin = state.isAdmin,
                            isEditing = editingId == comment.id,
                            onEdit = { editingId = comment.id },
                            onCancelEdit = { editingId = null },
                            onSaveEdit = { text ->
                                scope.launch {
                                    // Keep the editor open on failure so the
                                    // typed edit isn't silently discarded.
                                    if (viewModel.updateComment(comment.id, text)) {
                                        editingId = null
                                    }
                                }
                            },
                            onDelete = {
                                scope.launch { viewModel.deleteComment(comment.id) }
                            },
                            onUploadImage = { uri -> viewModel.uploadImage(uri) },
                            mentionMembers = mentionMembers,
                        )
                    }
                }
            }
        }

        Spacer(Modifier.height(8.dp))
        // Rich markdown composer — reuses the block MarkdownEditor (same editor as
        // issue descriptions: bold/italic/strikethrough/lists/task-lists/headings/
        // links + image upload) so comments reach parity with the web composer.
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(18.dp))
                .background(CommentFieldBg)
                .padding(horizontal = 12.dp, vertical = 4.dp),
        ) {
            MarkdownEditor(
                markdown = draft,
                editable = true,
                onChange = { draft = it },
                onUploadImage = { uri -> viewModel.uploadImage(uri) },
                placeholder = "Write a comment…",
                minHeight = 40.dp,
                mentionMembers = mentionMembers,
            )
            // Send stays blocked while any embedded image is still a draft://
            // placeholder (uploading, or failed — its tile shows Retry/remove);
            // posting now would silently strip the image from the comment.
            val hasPendingImages = remember(draft) { hasDraftImages(draft) }
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.End,
            ) {
                if (hasPendingImages) {
                    Text(
                        "Waiting for images…",
                        style = MaterialTheme.typography.labelSmall,
                        color = CommentMeta,
                    )
                    Spacer(Modifier.width(8.dp))
                }
                IconButton(
                    onClick = {
                        val trimmed = draft.trim()
                        if (trimmed.isEmpty()) return@IconButton
                        sending = true
                        scope.launch {
                            // Clear the composer only when the comment actually
                            // posted; a declined/failed send keeps the draft.
                            if (viewModel.createComment(trimmed)) draft = ""
                            sending = false
                        }
                    },
                    enabled = !sending && draft.isNotBlank() && !hasPendingImages,
                ) {
                    Icon(
                        Icons.Filled.ArrowCircleUp,
                        contentDescription = "Send",
                        modifier = Modifier.size(30.dp),
                        tint = if (draft.isBlank() || hasPendingImages) {
                            Color.White.copy(alpha = 0.3f)
                        } else CommentAccent,
                    )
                }
            }
        }
    }
}

// Relative timestamp ("3h ago"). Internal so the extracted EventRow /
// RegularCommentRow can reuse it. Parses via WireTimestamps — Instant.parse
// alone rejected Electric's Postgres text encoding, blanking every synced
// row's time (EXP-169).
internal fun relativeTime(wire: String): String {
    val thenMs = com.exponential.app.domain.WireTimestamps.parseEpochMs(wire) ?: return ""
    val seconds = ((System.currentTimeMillis() - thenMs) / 1000).coerceAtLeast(0)
    return when {
        seconds < 60 -> "just now"
        seconds < 3600 -> "${seconds / 60}m ago"
        seconds < 86400 -> "${seconds / 3600}h ago"
        else -> "${seconds / 86400}d ago"
    }
}

// One timeline entry — either a comment or a synced activity event, merged by time.
private sealed interface TimelineItem {
    val createdAt: String
    val id: String

    data class Comment(val comment: CommentEntity) : TimelineItem {
        override val createdAt get() = comment.createdAt
        override val id get() = comment.id
    }

    data class Event(val event: IssueEventEntity) : TimelineItem {
        override val createdAt get() = event.createdAt
        override val id get() = event.id
    }
}
