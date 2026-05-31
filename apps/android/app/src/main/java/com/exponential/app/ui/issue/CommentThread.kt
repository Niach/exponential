package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.text.BasicTextField
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowCircleUp
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.QuestionMark
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.data.api.getCommentBodyText
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.CommentKind
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.commentKindOf
import com.exponential.app.ui.markdown.MarkdownView
import kotlinx.coroutines.launch

// iOS comment palette (CommentRow.swift / CommentComposer.swift) — explicit white
// tiers because the issue screen floats on AppBackground (a Box, not a Material
// Surface), so any Text without an explicit color would inherit LocalContentColor's
// black default (the bug the user hit). Mirrors the glass theme exactly.
private val CommentAuthor = Color.White.copy(alpha = 0.9f)
private val CommentMeta = Color.White.copy(alpha = 0.5f)
private val CommentAvatarBg = Color.White.copy(alpha = 0.08f)
private val CommentAvatarText = Color.White.copy(alpha = 0.7f)
private val CommentFieldBg = Color.White.copy(alpha = 0.06f)
private val CommentFieldText = Color.White.copy(alpha = 0.9f)
private val CommentAccent = Color(red = 0.42f, green = 0.64f, blue = 1.0f)

// Mirrors apps/web/src/components/issue-timeline.tsx: renders the four
// comment kinds, the agent plan-approval CTAs on the latest plan, and the
// Retry CTA on error-shaped terminal regular comments.
@Composable
fun CommentThread(
    issueId: String,
    canApprovePlan: Boolean,
    viewModel: CommentThreadViewModel = hiltViewModel(),
) {
    LaunchedEffect(issueId) { viewModel.bind(issueId) }
    val state by viewModel.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    var draft by remember { mutableStateOf("") }
    var editingId by remember { mutableStateOf<String?>(null) }
    var sending by remember { mutableStateOf(false) }
    var pendingPlanAction by remember { mutableStateOf(false) }
    var pendingRetry by remember { mutableStateOf(false) }

    val latestPlanId = remember(state.comments) {
        state.comments.lastOrNull { commentKindOf(it.kind) == CommentKind.Plan }?.id
    }
    val retryAnchorId = remember(state.comments) {
        state.comments.lastOrNull { isErrorComment(it) }?.id
    }

    HorizontalDivider()
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
        Text(
            if (state.comments.isEmpty()) "Comments"
            else "Comments (${state.comments.size})",
            style = MaterialTheme.typography.labelMedium,
            color = CommentMeta,
        )
        Spacer(Modifier.height(8.dp))

        if (state.comments.isEmpty()) {
            Text(
                "No comments yet. Be the first to add one.",
                style = MaterialTheme.typography.bodySmall,
                color = CommentMeta,
            )
        }

        state.comments.forEach { comment ->
            // Stable identity per comment so list churn (e.g. an active Electric
            // sync) doesn't re-key rows and force their markdown to re-parse.
            key(comment.id) {
            when (commentKindOf(comment.kind)) {
                CommentKind.Regular -> RegularCommentRow(
                    comment = comment,
                    author = state.usersById[comment.authorId],
                    isAuthor = state.currentUserId != null && comment.authorId == state.currentUserId,
                    isAdmin = state.isAdmin,
                    isEditing = editingId == comment.id,
                    showRetry = comment.id == retryAnchorId,
                    retrying = pendingRetry && comment.id == retryAnchorId,
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
                    onRetry = {
                        scope.launch {
                            pendingRetry = true
                            viewModel.retry()
                            pendingRetry = false
                        }
                    },
                )
                CommentKind.Question -> QuestionCommentRow(
                    comment = comment,
                    author = state.usersById[comment.authorId],
                )
                CommentKind.Plan -> PlanCommentRow(
                    comment = comment,
                    isLatestPlan = comment.id == latestPlanId,
                    issueState = state.issue?.agentPlanState,
                    approved = state.issue?.agentPlanApprovedAt != null,
                    canApprovePlan = canApprovePlan,
                    isApproving = pendingPlanAction,
                    onApprove = {
                        scope.launch {
                            pendingPlanAction = true
                            viewModel.approvePlan()
                            pendingPlanAction = false
                        }
                    },
                    onRequestChanges = {
                        scope.launch {
                            pendingPlanAction = true
                            viewModel.requestChanges()
                            pendingPlanAction = false
                        }
                    },
                )
            }
            }
        }

        Spacer(Modifier.height(8.dp))
        // Glass composer matching iOS CommentComposer.swift: rounded translucent
        // field + an up-arrow send button (blue when there's text, dimmed when empty).
        Row(
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            BasicTextField(
                value = draft,
                onValueChange = { draft = it },
                enabled = !sending,
                textStyle = MaterialTheme.typography.bodyMedium.copy(color = CommentFieldText),
                cursorBrush = SolidColor(CommentAccent),
                maxLines = 6,
                modifier = Modifier.weight(1f),
                decorationBox = { inner ->
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(18.dp))
                            .background(CommentFieldBg)
                            .padding(horizontal = 12.dp, vertical = 10.dp),
                    ) {
                        if (draft.isEmpty()) {
                            Text(
                                "Write a comment…",
                                style = MaterialTheme.typography.bodyMedium,
                                color = CommentMeta,
                            )
                        }
                        inner()
                    }
                },
            )
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

@Composable
private fun RegularCommentRow(
    comment: CommentEntity,
    author: UserEntity?,
    isAuthor: Boolean,
    isAdmin: Boolean,
    isEditing: Boolean,
    showRetry: Boolean,
    retrying: Boolean,
    onEdit: () -> Unit,
    onCancelEdit: () -> Unit,
    onSaveEdit: (String) -> Unit,
    onDelete: () -> Unit,
    onRetry: () -> Unit,
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
                MarkdownView(bodyText)
                if (showRetry) {
                    Spacer(Modifier.height(4.dp))
                    OutlinedButton(
                        onClick = onRetry,
                        enabled = !retrying,
                    ) {
                        Icon(Icons.Filled.Refresh, contentDescription = null, modifier = Modifier.size(14.dp))
                        Spacer(Modifier.width(6.dp))
                        Text(if (retrying) "Retrying…" else "Retry")
                    }
                }
            }
        }
    }
}

@Composable
private fun QuestionCommentRow(comment: CommentEntity, author: UserEntity?) {
    val bodyText = remember(comment.body) { getCommentBodyText(comment.body) }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .background(Color(0x14B388F5), RoundedCornerShape(8.dp))
            .border(0.5.dp, Color(0x44B388F5), RoundedCornerShape(8.dp))
            .padding(10.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                Icons.Filled.QuestionMark,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = Color(0xFFB388F5),
            )
            Spacer(Modifier.width(6.dp))
            Text(
                (author?.name ?: author?.email ?: "Agent") + " asks",
                style = MaterialTheme.typography.labelMedium,
                color = CommentAuthor,
            )
            Spacer(Modifier.width(8.dp))
            Text(
                relativeTime(comment.createdAt),
                style = MaterialTheme.typography.labelSmall,
                color = CommentMeta,
            )
        }
        Spacer(Modifier.height(4.dp))
        MarkdownView(bodyText)
    }
}

@Composable
private fun PlanCommentRow(
    comment: CommentEntity,
    isLatestPlan: Boolean,
    issueState: String?,
    approved: Boolean,
    canApprovePlan: Boolean,
    isApproving: Boolean,
    onApprove: () -> Unit,
    onRequestChanges: () -> Unit,
) {
    val bodyText = remember(comment.body) { getCommentBodyText(comment.body) }
    val awaitingApproval = isLatestPlan && issueState == "awaiting_approval"

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .background(Color(0x142563EB), RoundedCornerShape(8.dp))
            .border(0.5.dp, Color(0x442563EB), RoundedCornerShape(8.dp))
            .padding(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                "Plan",
                style = MaterialTheme.typography.labelMedium,
                color = Color(0xFF60A5FA),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                relativeTime(comment.createdAt),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (approved && isLatestPlan) {
                Spacer(Modifier.weight(1f))
                Icon(
                    Icons.Filled.Check,
                    contentDescription = null,
                    modifier = Modifier.size(12.dp),
                    tint = Color(0xFF34D399),
                )
                Spacer(Modifier.width(4.dp))
                Text(
                    "Approved",
                    style = MaterialTheme.typography.labelSmall,
                    color = Color(0xFF34D399),
                )
            }
        }
        Spacer(Modifier.height(8.dp))
        MarkdownView(bodyText)

        if (awaitingApproval && canApprovePlan) {
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    onClick = onApprove,
                    enabled = !isApproving,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = Color(0xFF22C55E).copy(alpha = 0.22f),
                        contentColor = Color(0xFF22C55E),
                    ),
                ) {
                    Icon(Icons.Filled.Check, contentDescription = null, modifier = Modifier.size(14.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(if (isApproving) "Approving…" else "Approve")
                }
                OutlinedButton(
                    onClick = onRequestChanges,
                    enabled = !isApproving,
                ) {
                    Text("Request changes")
                }
            }
        }
    }
}

// Same agent terminal-error patterns the web timeline uses. Tests-failed /
// agent-error / no-repo / no-auth show a Retry CTA; "PR opened" is
// terminal but not an error.
private val errorBodyPatterns = listOf(
    Regex("^Tests failed after retry"),
    Regex("^Agent encountered an error"),
    Regex("^No GitHub repo linked"),
    Regex("Companion is not authenticated to GitHub"),
)

private fun isErrorComment(comment: CommentEntity): Boolean {
    if (commentKindOf(comment.kind) != CommentKind.Regular) return false
    val body = getCommentBodyText(comment.body)
    return errorBodyPatterns.any { it.containsMatchIn(body) }
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
