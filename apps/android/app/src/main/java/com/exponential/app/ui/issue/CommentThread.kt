package com.exponential.app.ui.issue

import android.net.Uri
import androidx.compose.foundation.background
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowCircleUp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.data.api.getCommentBodyText
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.CommentKind
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.commentKindOf
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MentionMember
import kotlinx.coroutines.launch

// iOS comment palette (CommentRow.swift / CommentComposer.swift) — explicit white
// tiers because the issue screen floats on AppBackground (a Box, not a Material
// Surface), so any Text without an explicit color would inherit LocalContentColor's
// black default. Mirrors the glass theme exactly.
private val CommentAuthor = Color.White.copy(alpha = 0.9f)
private val CommentMeta = Color.White.copy(alpha = 0.5f)
private val CommentAvatarBg = Color.White.copy(alpha = 0.08f)
private val CommentAvatarText = Color.White.copy(alpha = 0.7f)
private val CommentFieldBg = Color.White.copy(alpha = 0.06f)
private val CommentAccent = Color(red = 0.42f, green = 0.64f, blue = 1.0f)

// The human conversation thread: regular comments + non-agent events
// (status/assignee/label changes), plus a collapsible "Agent activity" feed for
// agent lifecycle events. Plan/question comments and the plan approval / retry
// affordances live in the AgentPlanPanel (a sibling above this view), so this
// view stays a plain human thread. Mirrors apps/web/src/components/issue-timeline.tsx.
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
    // Timeline: regular comments + non-agent events merged by time. Agent
    // lifecycle events go to the AgentActivityFeed below.
    val timeline = remember(humanComments, state.events) {
        (humanComments.map { TimelineItem.Comment(it) } +
            state.events.filter { it.type !in agentEventTypes }.map { TimelineItem.Event(it) })
            .sortedBy { it.createdAt }
    }
    // Workspace members for @mention autocomplete (agents excluded — you mention people).
    val mentionMembers = remember(state.usersById) {
        state.usersById.values
            .filter { !it.isAgent }
            .map { MentionMember(it.name ?: it.email, it.email) }
    }

    HorizontalDivider()
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
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
                    EventRow(item.event, state.usersById[item.event.actorUserId])
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
                                    viewModel.updateComment(comment.id, text)
                                    editingId = null
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

        AgentActivityFeed(events = state.events, usersById = state.usersById)

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
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
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
                    Icon(
                        Icons.Filled.ArrowCircleUp,
                        contentDescription = "Send",
                        modifier = Modifier.size(30.dp),
                        tint = if (draft.isBlank()) Color.White.copy(alpha = 0.3f) else CommentAccent,
                    )
                }
            }
        }
    }
}

@Composable
private fun RegularCommentRow(
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

// Relative timestamp ("3h ago"). Internal so the AgentActivityFeed can reuse it.
internal fun relativeTime(iso: String): String {
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

// One timeline entry — either a comment or a synced activity event, merged by time.
private sealed interface TimelineItem {
    val createdAt: String

    data class Comment(val comment: CommentEntity) : TimelineItem {
        override val createdAt get() = comment.createdAt
    }

    data class Event(val event: IssueEventEntity) : TimelineItem {
        override val createdAt get() = event.createdAt
    }
}

// Compact Linear-style activity line for non-agent events (status/assignee/label).
@Composable
private fun EventRow(event: IssueEventEntity, actor: UserEntity?) {
    val who = actor?.name ?: actor?.email ?: "Someone"
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(CommentMeta))
        Text(
            "$who ${agentEventVerb(event.type)} · ${relativeTime(event.createdAt)}",
            style = MaterialTheme.typography.labelSmall,
            color = CommentMeta,
        )
    }
}
