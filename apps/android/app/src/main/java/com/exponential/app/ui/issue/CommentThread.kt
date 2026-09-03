package com.exponential.app.ui.issue

import androidx.compose.animation.animateContentSize
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
import androidx.compose.material3.HorizontalDivider
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.CommentKind
import com.exponential.app.data.db.commentKindOf
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.fullBleed
import kotlinx.coroutines.launch

// iOS comment palette (CommentRow.swift / CommentComposer.swift) — explicit white
// tiers because the issue screen floats on AppBackground (a Box, not a Material
// Surface), so any Text without an explicit color would inherit LocalContentColor's
// black default. Mirrors the glass theme exactly. Internal so the extracted
// EventRow / RegularCommentRow / IssueDetailBottomBar share the exact values.
// (EXP-698 r4 retired the comment-only avatar chip: a comment draws the SAME
// `UserAvatar` as every other surface, picture and hashed hue included.)
internal val CommentAuthor = Color.White.copy(alpha = 0.9f)
internal val CommentMeta = Color.White.copy(alpha = 0.5f)
internal val CommentAccent = Color(red = 0.42f, green = 0.64f, blue = 1.0f)

// Timeline gutter geometry (EXP-240): the shared leading column every timeline
// row aligns to — event dot, collapsed-run dot, and comment avatar.
internal val TimelineGutterWidth = 28.dp
// EXP-698 r5: the rail is the CARD hairline, unified across all four clients
// — it used to be the card FILL, a paler line than any stroke beside it.
internal val TimelineRail = GlassTokens.StrokeCard

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
    // Horizontal padding of the hosting column, escaped by the top rule so the
    // line runs edge to edge (EXP-327). Compose has no negative padding.
    hostPadding: Dp = 20.dp,
) {
    LaunchedEffect(issueId) { viewModel.bind(issueId) }
    val state by viewModel.state.collectAsStateWithLifecycle()
    // EXP-554: the thread's comment-linked attachments, grouped per comment,
    // plus the add queue of whichever comment is being edited.
    val attachmentsByComment by viewModel.commentAttachments.collectAsStateWithLifecycle()
    val editAttachments by viewModel.editAttachments.collectAsStateWithLifecycle()
    val context = LocalContext.current
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
        // EXP-530: `created` events are suppressed entirely (eventRowVisible)
        // — the synthesized Created header already shows creation, and the
        // rows exist only as the automation-trigger substrate.
        val merged = (humanComments.map { TimelineItem.Comment(it) } +
            state.events.filter { eventRowVisible(it.type) }.map { TimelineItem.Event(it) })
            .sortedWith(compareBy({ it.createdAt }, { it.id }))
        listOfNotNull(state.issue?.let { TimelineItem.Created(it) }) + merged
    }
    val rows = remember(timeline, expandedRuns) { collapseTimeline(timeline, expandedRuns) }
    // Team members for @mention autocomplete.
    val mentionMembers = remember(state.usersById) {
        state.usersById.values
            .map { MentionMember(it.name ?: it.email, it.email) }
    }

    // No extra horizontal padding: the screen already pads 20dp, so the thread
    // aligns full-width with the description/metadata above (iOS parity). The
    // rule above the header is the one thing that escapes that padding — it
    // runs edge to edge so activity reads as its own region (EXP-327).
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 12.dp)
            // EXP-523: the shared `slow` token (280ms, unchanged) — and
            // now `snap()` when the OS has animations off.
            .animateContentSize(Motion.slow()),
    ) {
        HorizontalDivider(
            modifier = Modifier.fullBleed(hostPadding),
            thickness = GlassTokens.Hairline,
            color = GlassTokens.StrokeSection,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            "Activity",
            style = MaterialTheme.typography.labelMedium,
            color = CommentMeta,
            // Stable hook for the store-screenshot test (mirrors the iOS
            // `comment-thread-header` accessibility id) — the copy has already
            // drifted once and silently killed the screengrab run.
            modifier = Modifier.testTag("comment-thread-header"),
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
                            statuses = state.statuses,
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
                                isEditing = editingId == comment.id,
                                onEdit = {
                                    viewModel.clearEditAttachments()
                                    editingId = comment.id
                                },
                                onCancelEdit = {
                                    viewModel.clearEditAttachments()
                                    editingId = null
                                },
                                onSaveEdit = { text, keptIds ->
                                    scope.launch {
                                        // Keep the editor open on failure so the
                                        // typed edit isn't silently discarded.
                                        if (viewModel.updateComment(comment.id, text, keptIds)) {
                                            editingId = null
                                        }
                                    }
                                },
                                onDelete = {
                                    scope.launch { viewModel.deleteComment(comment.id) }
                                },
                                attachments = attachmentsByComment[comment.id].orEmpty(),
                                onOpenAttachment = { attachment ->
                                    scope.launch {
                                        // No in-app viewer: hand the bytes to
                                        // whatever app renders the type.
                                        val local = viewModel.downloadToCache(attachment)
                                            ?: return@launch
                                        openFile(context, local, attachment.contentType)
                                    }
                                },
                                editAttachments = if (editingId == comment.id) {
                                    editAttachments
                                } else {
                                    emptyList()
                                },
                                onAddEditAttachment = { uri, keptCount ->
                                    viewModel.addEditAttachment(uri, keptCount)
                                },
                                onRemoveEditAttachment = viewModel::removeEditAttachment,
                                mentionMembers = mentionMembers,
                            )
                        }
                    }
                }
            }
        }
    }
}

// The synthesized first item: "«creator» created the issue" (widget/agent
// issues carry no user creator → "Feedback widget" / "Agent").
@Composable
private fun CreatedRow(
    item: TimelineItem.Created,
    usersById: Map<String, com.exponential.app.data.db.UserEntity>,
    lineBelow: Boolean,
) {
    val issue = item.issue
    val who = when (issue.source) {
        DomainContract.issueSourceWidget -> "Feedback widget"
        DomainContract.issueSourceAgent -> "Agent"
        else -> userDisplayName(issue.creatorId?.let { usersById[it] }, issue.creatorId)
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
                    ExpIcons.uiMore,
                    contentDescription = null,
                    modifier = Modifier.align(Alignment.Center).size(14.dp),
                    tint = CommentMeta,
                )
            },
        )
        GlassPill(
            "Show $count activity items",
            size = PillSize.Sm,
            onClick = onExpand,
            modifier = Modifier.padding(vertical = 4.dp),
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
