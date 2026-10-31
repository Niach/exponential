package com.exponential.app.ui.session

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.getCommentBodyText
import com.exponential.app.data.db.AppEntity
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.ExpToolDisplay
import com.exponential.app.navigation.EntityTarget
import com.exponential.app.navigation.LocalEntityNavigator
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.commentAuthorName
import com.exponential.app.ui.components.commentCaption
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.issue.longRelativeTime
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.GlassTokens
import kotlinx.coroutines.flow.flowOf

// EXP-1241 (EXP-1259 D14): an `exponential_comments_create` call renders the
// REAL comment, in the issue timeline's comment look: "Commented on <issue
// chip>" over a card holding the author row (the agent mark, the name, "via
// MCP · time") and the markdown body clamped to three lines behind "Show
// more". Tapping the card opens the issue scrolled to that comment. Until the
// row syncs (or for a comment this client cannot see) it is the result's own
// preview: the ref's title as the body, the run's owner (whose key posted it),
// "via MCP" and the settle's time. Web `CommentCallCard` + `CommentCallRow`.

object CommentCallCopy {
    const val DONE = "Commented on"
    const val RUNNING = "Commenting on"
    const val SHOW_MORE = "Show more"
    const val SHOW_LESS = "Show less"
    const val TEST_ID = "comment-call-card"

    /** The contract `expToolDisplay` row of `exponential_comments_create`. */
    const val TOOL_ROW = "comments_create"

    /** The body clamp: three lines (web `max-h-[4.5em]`). */
    const val CLAMP_LINES = 3

    /** The clamped body's fade: opaque to [CLAMP_FADE_FROM], clear from
     *  [CLAMP_FADE_TO] (web `mask-image` fades 55% → 100%; the bottom band
     *  here is fully clear so the line the clamp crosses never shows as a cut
     *  half-line, whatever the markdown's block spacing). */
    const val CLAMP_FADE_FROM = 0.4f
    const val CLAMP_FADE_TO = 0.85f
}

/** What the transcript knows about the run a comment call sits in: its agent
 *  (the author row's mark) and its owner (the author until the row syncs). */
data class RunCards(val agent: String?, val ownerId: String?)

val LocalRunCards = staticCompositionLocalOf { RunCards(agent = null, ownerId = null) }

/** Whether [item] renders as the comment card: a SETTLED, successful
 *  `exponential_comments_create` call (running or failed keeps its row). */
fun isCommentCallCard(item: AgentFeedItem.Tool): Boolean =
    item.settled && !item.failed && ExpToolDisplay.row(item.name) == CommentCallCopy.TOOL_ROW

/** The comment and issue a settled call's preview names. */
data class CommentCallTarget(
    val commentId: String?,
    val commentText: String?,
    val issueKey: String?,
    /** The result's issue title (the chip's title until the issue resolves). */
    val issueTitle: String? = null,
)

fun commentCallTarget(item: AgentFeedItem.Tool): CommentCallTarget {
    val refs = item.preview?.refs.orEmpty()
    val comment = refs.firstOrNull { it.kind == "comment" }
    val issue = refs.firstOrNull { it.kind == "issue" }
    return CommentCallTarget(
        commentId = comment?.id ?: item.preview?.id,
        commentText = comment?.title ?: item.preview?.title,
        // The result's issue, else the call's subject (its `issueId` input).
        issueKey = issue?.identifier ?: issue?.id ?: item.detail?.trim()?.takeIf { it.isNotEmpty() },
        issueTitle = issue?.title?.takeIf { it.isNotBlank() },
    )
}

/** The data half: resolves the synced comment, its author and its issue. */
@Composable
fun CommentCallRow(item: AgentFeedItem.Tool) {
    val resolver = LocalEntityRefResolver.current
    val runCards = LocalRunCards.current
    val target = remember(item) { commentCallTarget(item) }
    val comment by remember(resolver, target.commentId) {
        val id = target.commentId
        if (resolver == null || id == null) {
            flowOf(null)
        } else {
            resolver.db.scopedQuery<CommentEntity?>(null) { it.commentDao().observeById(id) }
        }
    }.collectAsState(null)
    val authorId = comment?.authorId ?: runCards.ownerId.takeIf { comment == null }
    val author by remember(resolver, authorId) {
        if (resolver == null || authorId == null) {
            flowOf(null)
        } else {
            resolver.db.scopedQuery<UserEntity?>(null) { it.userDao().observeById(authorId) }
        }
    }.collectAsState(null)
    // #VAPP-186: the app whose MCP tool wrote it ("via <app>").
    val viaAppId = comment?.viaAppId
    val viaApp by remember(resolver, viaAppId) {
        if (resolver == null || viaAppId == null) {
            flowOf(null)
        } else {
            resolver.db.scopedQuery<AppEntity?>(null) { it.appDao().observeById(viaAppId) }
        }
    }.collectAsState(null)
    val issueRefs = LocalIssueRefs.current
    val issue = remember(issueRefs, comment, target.issueKey) {
        val row = comment
        when {
            issueRefs == null -> null
            row != null -> issueRefs.candidates.firstOrNull { it.issueId == row.issueId }
            target.issueKey != null -> issueRefs.resolve(target.issueKey)
                ?: issueRefs.candidates.firstOrNull { it.issueId == target.issueKey }
            else -> null
        }
    }
    val row = comment
    val text = if (row != null) getCommentBodyText(row.body) else target.commentText
    val authorName = when {
        row != null -> commentAuthorName(row, author, reporterName = null)
        else -> author?.let { userDisplayName(it, it.id) }
    }
    val time = when {
        row != null -> relativeTime(row.createdAt)
        item.settledAt != null -> longRelativeTime(System.currentTimeMillis() - item.settledAt)
        else -> null
    }
    val caption = listOfNotNull(
        if (row != null) commentCaption(row, viaApp?.name) else VIA_MCP,
        time?.takeIf { it.isNotEmpty() },
    ).joinToString(" · ")
    val navigator = LocalEntityNavigator.current
    val issueId = row?.issueId ?: issue?.issueId
    val commentId = target.commentId
    CommentCallCard(
        running = !item.settled,
        issue = when {
            issue != null -> {
                {
                    IssueChip(
                        identifier = issue.identifier,
                        title = issue.title.takeIf { it.isNotBlank() },
                        status = issue.resolvedStatus,
                        onClick = if (issueRefs?.canOpen == true) {
                            { issueRefs.onOpen(issue) }
                        } else {
                            null
                        },
                    )
                }
            }
            target.issueKey != null -> {
                { IssueChip(identifier = target.issueKey, title = target.issueTitle, status = null) }
            }
            else -> null
        },
        author = authorName,
        mark = { AgentRunMark(agent = runCards.agent, state = null, size = 14.dp, badgeSize = 0.dp) },
        caption = caption.takeIf { it.isNotEmpty() },
        body = text?.takeIf { item.settled && it.isNotBlank() },
        onOpen = if (issueId != null && commentId != null) {
            { navigator.open(EntityTarget.Issue(issueId, commentId = commentId)) }
        } else {
            null
        },
    )
}

private const val VIA_MCP = "via MCP"

/** The layout half (the styleguide draws it with fixed content). */
@Composable
fun CommentCallCard(
    /** The issue chip the comment sits on (null = not resolvable here). */
    issue: (@Composable () -> Unit)?,
    /** The comment's author name (the key owner the agent posted as). */
    author: String?,
    /** The muted caption after the name: "via MCP · 2 minutes ago". */
    caption: String?,
    /** The markdown body; null while the call runs. */
    body: String?,
    modifier: Modifier = Modifier,
    /** The author row's leading mark (the agent's run mark). */
    mark: (@Composable () -> Unit)? = null,
    running: Boolean = false,
    /** Open the issue scrolled to the comment; null = inert. */
    onOpen: (() -> Unit)? = null,
) {
    val style = threadCardTextStyle()
    val muted = threadCardMuted()
    Column(
        modifier = modifier
            .fillMaxWidth()
            .threadCard()
            .padding(12.dp)
            .testTag(CommentCallCopy.TEST_ID),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            ExponentialMark(size = 14.dp)
            Text(
                if (running) CommentCallCopy.RUNNING else CommentCallCopy.DONE,
                style = style.copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
            )
            if (running) InlineSpinner(size = 12.dp)
            if (issue != null) Box(Modifier.weight(1f, fill = false)) { issue() }
        }
        if (body != null) {
            var expanded by remember { mutableStateOf(false) }
            var overflows by remember(body) { mutableStateOf(false) }
            val shape = RoundedCornerShape(8.dp)
            val lineHeight = MaterialTheme.typography.bodyMedium.lineHeight
            val clampPx = with(LocalDensity.current) { (lineHeight * CommentCallCopy.CLAMP_LINES).roundToPx() }
            Column(
                modifier = Modifier
                    .padding(top = 10.dp)
                    .fillMaxWidth()
                    .clip(shape)
                    .background(GlassTokens.RowFill, shape)
                    .border(GlassTokens.Hairline, GlassTokens.StrokeRow, shape)
                    .then(
                        if (onOpen != null) Modifier.clickable(role = Role.Button, onClick = onOpen) else Modifier,
                    )
                    .padding(start = 12.dp, end = 12.dp, top = 10.dp, bottom = 12.dp),
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    mark?.invoke()
                    if (author != null) {
                        Text(
                            author,
                            style = MaterialTheme.typography.bodySmall.copy(fontWeight = FontWeight.Medium),
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                    }
                    if (caption != null) {
                        Text(caption, style = style, color = muted, maxLines = 1)
                    }
                }
                Box(
                    modifier = Modifier
                        .padding(top = 4.dp)
                        .fillMaxWidth()
                        .then(
                            if (expanded) {
                                Modifier
                            } else {
                                Modifier
                                    .clipToBounds()
                                    .then(if (overflows) Modifier.clampFade() else Modifier)
                                    .layout { measurable, constraints ->
                                        val placeable = measurable.measure(
                                            constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity),
                                        )
                                        val over = placeable.height > clampPx + 1
                                        if (over != overflows) overflows = over
                                        layout(placeable.width, minOf(placeable.height, clampPx)) {
                                            placeable.placeRelative(0, 0)
                                        }
                                    }
                            },
                        ),
                ) {
                    MarkdownView(body)
                }
                if (overflows || expanded) {
                    Text(
                        if (expanded) CommentCallCopy.SHOW_LESS else CommentCallCopy.SHOW_MORE,
                        style = style,
                        color = muted,
                        modifier = Modifier
                            .padding(top = 4.dp)
                            .clickable(role = Role.Button) { expanded = !expanded },
                    )
                }
            }
        }
    }
}

/** The clamped body's bottom fade (web `mask-image: linear-gradient(...)`):
 *  a clamp through a heading or a paragraph gap reads as "there is more", never a cut line. */
private fun Modifier.clampFade(): Modifier =
    graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen).drawWithContent {
        drawContent()
        drawRect(
            Brush.verticalGradient(
                CommentCallCopy.CLAMP_FADE_FROM to Color.Black,
                CommentCallCopy.CLAMP_FADE_TO to Color.Transparent,
            ),
            blendMode = BlendMode.DstIn,
        )
    }
