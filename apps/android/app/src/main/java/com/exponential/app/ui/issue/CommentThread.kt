package com.exponential.app.ui.issue

import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material3.Icon
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
import com.exponential.app.data.db.CommentKind
import com.exponential.app.data.db.commentKindOf
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import kotlinx.coroutines.launch

// iOS comment palette (CommentRow.swift / CommentComposer.swift) — explicit white
// tiers because the issue screen floats on AppBackground (a Box, not a Material
// Surface), so any Text without an explicit color would inherit LocalContentColor's
// black default. Mirrors the glass theme exactly. Internal so the extracted
// EventRow / RegularCommentRow / IssueDetailBottomBar share the exact values.
internal val CommentAuthor = Color.White.copy(alpha = 0.9f)
internal val CommentMeta = Color.White.copy(alpha = 0.5f)
internal val CommentAvatarBg = Color.White.copy(alpha = 0.08f)
internal val CommentAvatarText = Color.White.copy(alpha = 0.7f)
internal val CommentAccent = Color(red = 0.42f, green = 0.64f, blue = 1.0f)

// Timeline gutter geometry (EXP-240): the shared leading column every timeline
// row aligns to — event dot, collapsed-run dot, and comment avatar.
internal val TimelineGutterWidth = 28.dp
internal val TimelineRail = Color.White.copy(alpha = 0.08f)

// The activity timeline: the synthesized "created the issue" item, regular
// comments as glass cards, and activity events (status/assignee/label/PR
// changes) merged by time along a gutter rail; runs of >2 consecutive events
// collapse behind a "Show N activity items" expander (EXP-240). Mirrors
// apps/web/src/components/issue-timeline.tsx. Composing happens in the docked
// bottom-bar composer — the VM instance is shared with it (hoisted draft).
@Composable
fun CommentThread(
    issueId: String,
    viewModel: CommentThreadViewModel,
    // Solo teams hide the comment-edit toolbar's @ button (EXP-246) — threaded
    // explicitly from the screen's soloMemberId gate, like the assignee chip.
    mentionEnabled: Boolean = true,
) {
    LaunchedEffect(issueId) { viewModel.bind(issueId) }
    val state by viewModel.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    var editingId by remember { mutableStateOf<String?>(null) }
    // Expanded collapsed-runs, keyed by the run's first event id so sync
    // re-emits don't reset expansion; reset per issue.
    var expandedRuns by remember(issueId) { mutableStateOf(setOf<String>()) }

    val humanComments = remember(state.comments) {
        state.comments.filter { commentKindOf(it.kind) == CommentKind.Regular }
    }
    // Timeline: the created item pinned first, then regular comments + activity
    // events merged by time. The id is a secondary sort key so items sharing a
    // createdAt (e.g. a comment + the status event of one mutation) keep a
    // stable order across syncs.
    val timeline = remember(humanComments, state.events, state.issue) {
        val merged = (humanComments.map { TimelineItem.Comment(it) } +
            state.events.map { TimelineItem.Event(it) })
            .sortedWith(compareBy({ it.createdAt }, { it.id }))
        listOfNotNull(state.issue?.let { TimelineItem.Created(it) }) + merged
    }
    val rows = remember(timeline, expandedRuns) { collapseTimeline(timeline, expandedRuns) }
    // Team members for @mention autocomplete.
    val mentionMembers = remember(state.usersById) {
        state.usersById.values
            .map { MentionMember(it.name ?: it.email, it.email) }
    }

    // No divider above (iOS separates by spacing only) and no extra horizontal
    // padding: the screen already pads 20dp, so the thread aligns full-width
    // with the description/metadata above (iOS parity).
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 12.dp)
            .animateContentSize(tween(280)),
    ) {
        Text(
            "Activity",
            style = MaterialTheme.typography.labelMedium,
            color = CommentMeta,
        )
        Spacer(Modifier.height(8.dp))

        rows.forEachIndexed { index, row ->
            val lineAbove = index > 0
            val lineBelow = index < rows.lastIndex
            when (row) {
                is TimelineRow.CollapsedRun -> key(row.runKey) {
                    CollapsedRunRow(
                        count = row.count,
                        lineAbove = lineAbove,
                        lineBelow = lineBelow,
                        onExpand = { expandedRuns = expandedRuns + row.runKey },
                    )
                }
                is TimelineRow.Single -> when (val item = row.item) {
                    is TimelineItem.Created -> key(item.id) {
                        CreatedRow(
                            item = item,
                            usersById = state.usersById,
                            lineBelow = lineBelow,
                        )
                    }
                    is TimelineItem.Event -> key(item.event.id) {
                        EventRow(
                            event = item.event,
                            usersById = state.usersById,
                            labelsById = state.labelsById,
                            lineAbove = lineAbove,
                            lineBelow = lineBelow,
                        )
                    }
                    is TimelineItem.Comment -> {
                        val comment = item.comment
                        // Stable identity per comment so list churn (e.g. an active
                        // Electric sync) doesn't re-key rows and force re-parse.
                        key(comment.id) {
                            RegularCommentRow(
                                comment = comment,
                                lineAbove = lineAbove,
                                lineBelow = lineBelow,
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
                                mentionEnabled = mentionEnabled,
                            )
                        }
                    }
                }
            }
        }
    }
}

// The synthesized first item: "«creator» created the issue" (widget-filed
// issues carry no user creator → "Feedback widget").
@Composable
private fun CreatedRow(
    item: TimelineItem.Created,
    usersById: Map<String, com.exponential.app.data.db.UserEntity>,
    lineBelow: Boolean,
) {
    val issue = item.issue
    val who = if (issue.source == DomainContract.issueSourceWidget) {
        "Feedback widget"
    } else {
        userDisplayName(issue.creatorId?.let { usersById[it] }, issue.creatorId)
    }
    val time = relativeTime(issue.createdAt)
    Row(
        modifier = Modifier.fillMaxWidth().height(IntrinsicSize.Min),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        TimelineGutter(lineAbove = false, lineBelow = lineBelow)
        Text(
            "$who created the issue" + if (time.isNotEmpty()) " · $time" else "",
            style = MaterialTheme.typography.labelSmall,
            color = CommentMeta,
            modifier = Modifier.padding(vertical = 8.dp),
        )
    }
}

// A folded run of consecutive events: a gutter ellipsis (iOS parity — not the
// event dot) + "Show N activity items". Expansion animates via the thread
// column's animateContentSize.
@Composable
private fun CollapsedRunRow(
    count: Int,
    lineAbove: Boolean,
    lineBelow: Boolean,
    onExpand: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().height(IntrinsicSize.Min),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        TimelineGutter(
            lineAbove = lineAbove,
            lineBelow = lineBelow,
            marker = {
                Icon(
                    Icons.Filled.MoreHoriz,
                    contentDescription = null,
                    modifier = Modifier.align(Alignment.Center).size(14.dp),
                    tint = CommentMeta,
                )
            },
        )
        Text(
            "Show $count activity items",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier
                .padding(vertical = 4.dp)
                .glassButton()
                .clickable(onClick = onExpand)
                .padding(horizontal = 10.dp, vertical = 5.dp),
        )
    }
}

// The shared leading gutter: a 6dp dot (or a custom [marker]) on a 1dp
// vertical rail. [lineAbove] / [lineBelow] draw the connecting segments toward
// the neighboring rows. Parent rows use height(IntrinsicSize.Min) so
// fillMaxHeight resolves.
@Composable
internal fun TimelineGutter(
    lineAbove: Boolean,
    lineBelow: Boolean,
    modifier: Modifier = Modifier,
    marker: (@Composable BoxScope.() -> Unit)? = null,
) {
    Box(modifier = modifier.width(TimelineGutterWidth).fillMaxHeight()) {
        Column(
            modifier = Modifier
                .fillMaxHeight()
                .width(1.dp)
                .align(Alignment.Center),
        ) {
            Box(
                Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .background(if (lineAbove) TimelineRail else Color.Transparent),
            )
            Spacer(Modifier.height(12.dp))
            Box(
                Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .background(if (lineBelow) TimelineRail else Color.Transparent),
            )
        }
        if (marker != null) {
            marker()
        } else {
            Box(
                modifier = Modifier
                    .align(Alignment.Center)
                    .size(6.dp)
                    .clip(CircleShape)
                    .background(CommentMeta),
            )
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
