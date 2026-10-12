package com.exponential.app.ui.thread

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.ThreadView
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.IssueChipSize
import com.exponential.app.ui.components.SessionRowBadgeSize
import com.exponential.app.ui.components.SessionRowMarkSize
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.LiveGreen
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.theme.glassCard

// EXP-1284: the THREAD's phone pieces (draft 3), stateless so the Thread
// screen and the styleguide's capture screens draw the SAME composables:
// the header, the child strip as pager tabs (Conversation · one tab per issue
// · Guide, the "+" trailing), the conversation (Run N blocks, the agent's
// link rows, the parallel card), the reply box and the list rows. The words
// are the ×4 copy (`thread-view.json` + the brief).

/** The ×4 copy of the thread surfaces. */
object ThreadCopy {
    const val CONVERSATION = "Conversation"
    const val GUIDE = "Guide"
    const val THREAD_WORD = "thread"
    const val LINK_ISSUE = "Link an issue"
    const val NEW_ISSUE = "New issue in this thread"
    const val START_RUN = "Start a session here"
    const val REPLY_PLACEHOLDER = "Reply in this thread"
    const val THREADS = "Threads"
    const val SHOW_WORK = "Show work"
    const val HIDE_WORK = "Hide work"
    const val OPEN_THREAD = "Open thread"
    const val RUNNING = "Running"
    const val RECENT = "Recent"

    fun unlink(identifier: String) = "Unlink $identifier"
}

/** A child of the strip as the saveable page key: `conversation`,
 *  `issue:<id>`, `guide` (the web's `?child=`). */
fun threadChildKey(child: ThreadView.Child): String = when (child) {
    ThreadView.Child.Conversation -> "conversation"
    is ThreadView.Child.IssueChild -> "issue:${child.issueId}"
    ThreadView.Child.Add -> "add"
    is ThreadView.Child.Guide -> "guide"
}

/** What a run block needs beyond the model: its agent (the mark), whether
 *  it is mid-turn (the spinner) and its name (the folded line). */
data class ThreadRunMeta(
    val agent: String?,
    val busy: Boolean,
    val name: String,
)

/** The block header's mark state: the working spinner while busy, amber
 *  while it waits on a person, the bare mark otherwise. */
internal fun threadMarkState(state: ThreadView.RunState, busy: Boolean): CodingSessionDisplayState? = when (state) {
    ThreadView.RunState.NeedsInput -> CodingSessionDisplayState.NeedsInput
    ThreadView.RunState.Live -> if (busy) CodingSessionDisplayState.Working else null
    ThreadView.RunState.Ended -> null
}

private val Muted: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
private val Secondary: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

// ── Header ──────────────────────────────────────────────────────────────────

/**
 * The thread's header: back, the header caption at the right of the bar, the
 * title (+ the muted `thread` word) under it. A long-press on the title
 * renames ([onRename]; null = no rename).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ThreadHeader(
    title: String,
    caption: String,
    onBack: (() -> Unit)?,
    modifier: Modifier = Modifier,
    onRename: (() -> Unit)? = null,
) {
    Column(modifier = modifier.fillMaxWidth().testTag("thread-header")) {
        Row(
            modifier = Modifier.fillMaxWidth().height(48.dp).padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) TopBarBackButton(onClick = onBack) else Spacer(Modifier.width(8.dp))
            Spacer(Modifier.weight(1f))
            if (caption.isNotEmpty()) {
                Text(
                    caption,
                    style = MaterialTheme.typography.labelMedium,
                    color = Muted,
                    maxLines = 1,
                    modifier = Modifier.padding(end = 12.dp).testTag("thread-header-caption"),
                )
            }
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .padding(top = 4.dp, bottom = 10.dp)
                .then(
                    if (onRename != null) {
                        Modifier.combinedClickable(onClick = {}, onLongClick = onRename)
                    } else {
                        Modifier
                    },
                ),
            verticalAlignment = Alignment.Bottom,
        ) {
            Text(
                title,
                fontSize = 22.sp,
                lineHeight = 28.sp,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false).testTag("thread-title"),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                ThreadCopy.THREAD_WORD,
                style = MaterialTheme.typography.labelMedium,
                color = Muted,
                modifier = Modifier.padding(bottom = 4.dp),
            )
        }
    }
}

// ── The child strip = the pager tabs ────────────────────────────────────────

/**
 * Phones: the strip IS the pager's tabs (replacing Issue · Run · Guide):
 * Conversation, one tab per issue (status glyph + mono identifier), the
 * Guide (green dot when it has entries), and the "+" trailing outside the
 * scroll. A long-press on an issue tab opens its menu. A child the viewer
 * has not seen yet ([fresh]) fades in with a ring.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ThreadChildTabs(
    strip: List<ThreadView.Child>,
    selectedKey: String,
    onSelect: (ThreadView.Child) -> Unit,
    onAdd: (() -> Unit)?,
    issueStatus: (String) -> ResolvedIssueStatus?,
    modifier: Modifier = Modifier,
    onIssueMenu: ((ThreadView.Child.IssueChild) -> Unit)? = null,
    fresh: (String) -> Boolean = { false },
) {
    Column(modifier = modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 10.dp, end = 0.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val scroll = rememberScrollState()
            Row(
                modifier = Modifier
                    .weight(1f)
                    // A strip wider than the phone fades out at its end
                    // instead of cutting a tab in half.
                    .graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen)
                    .drawWithContent {
                        drawContent()
                        if (scroll.canScrollForward) {
                            drawRect(
                                Brush.horizontalGradient(
                                    0f to Color.Black,
                                    0.85f to Color.Black,
                                    1f to Color.Transparent,
                                ),
                                blendMode = BlendMode.DstIn,
                            )
                        }
                    }
                    .horizontalScroll(scroll)
                    .testTag("thread-tabs"),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                strip.forEach { child ->
                    if (child == ThreadView.Child.Add) return@forEach
                    val key = threadChildKey(child)
                    val selected = key == selectedKey
                    val issue = child as? ThreadView.Child.IssueChild
                    ThreadTab(
                        selected = selected,
                        ring = issue != null && fresh(issue.issueId),
                        onClick = { onSelect(child) },
                        onLongClick = issue?.let { tab -> onIssueMenu?.let { menu -> { menu(tab) } } },
                        testTag = "thread-tab-$key",
                    ) {
                        when (child) {
                            ThreadView.Child.Conversation -> Text(
                                ThreadCopy.CONVERSATION,
                                style = MaterialTheme.typography.labelLarge,
                                color = if (selected) MaterialTheme.colorScheme.onSurface else Secondary,
                            )
                            is ThreadView.Child.IssueChild -> {
                                issueStatus(child.issueId)?.let { StatusIcon(it, size = 14.dp) }
                                Spacer(Modifier.width(5.dp))
                                Text(
                                    child.identifier,
                                    style = MaterialTheme.typography.labelMedium,
                                    fontFamily = FontFamily.Monospace,
                                    color = if (selected) MaterialTheme.colorScheme.onSurface else Secondary,
                                )
                            }
                            is ThreadView.Child.Guide -> {
                                Text(
                                    ThreadCopy.GUIDE,
                                    style = MaterialTheme.typography.labelLarge,
                                    color = if (selected) MaterialTheme.colorScheme.onSurface else Secondary,
                                )
                                if (child.dot) {
                                    Spacer(Modifier.width(6.dp))
                                    Box(Modifier.size(6.dp).background(LiveGreen, CircleShape).testTag("thread-guide-dot"))
                                }
                            }
                            ThreadView.Child.Add -> Unit
                        }
                    }
                }
            }
            if (onAdd != null) {
                IconButton(onClick = onAdd, modifier = Modifier.size(40.dp).testTag("thread-add")) {
                    Icon(ExpIcons.uiAdd, contentDescription = "Add to this thread", tint = Secondary, modifier = Modifier.size(18.dp))
                }
            }
        }
        HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ThreadTab(
    selected: Boolean,
    ring: Boolean,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)?,
    testTag: String,
    content: @Composable () -> Unit,
) {
    val shape = RoundedCornerShape(10.dp)
    // A fresh child (the agent just linked it) fades in with a ring once.
    val appear = remember { Animatable(if (ring) 0f else 1f) }
    val ringAlpha = remember { Animatable(if (ring) 1f else 0f) }
    val enter = Motion.slow<Float>()
    LaunchedEffect(ring) {
        if (ring) {
            appear.animateTo(1f, enter)
            kotlinx.coroutines.delay(2_000)
            ringAlpha.animateTo(0f, enter)
        }
    }
    Row(
        modifier = Modifier
            .alpha(appear.value)
            .clip(shape)
            .then(if (selected) Modifier.background(GlassTokens.OpaqueCardFill, shape) else Modifier)
            .then(
                when {
                    ringAlpha.value > 0f -> Modifier.border(1.dp, LiveGreen.copy(alpha = ringAlpha.value * 0.7f), shape)
                    selected -> Modifier.border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
                    else -> Modifier
                },
            )
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .heightIn(min = 36.dp)
            .padding(horizontal = 9.dp, vertical = 8.dp)
            .testTag(testTag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        content()
    }
}

// ── The conversation ────────────────────────────────────────────────────────

/**
 * The Conversation child: the model's blocks in order — `Run N` blocks
 * (the header row, then the run's own content via [runBody]; an ended run
 * folds to its name + Show work), the issues its agent linked as rows under
 * it, and the runs one parent started as ONE card. [runLead] draws above a
 * block's header (the owner's opening message).
 */
@Composable
fun ThreadConversation(
    view: ThreadView.View,
    runMeta: (String) -> ThreadRunMeta?,
    issueStatus: (String) -> ResolvedIssueStatus?,
    onOpenIssue: (String) -> Unit,
    onOpenRun: (String) -> Unit,
    parallelCaption: (ThreadView.ParallelRow) -> String,
    modifier: Modifier = Modifier,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    issueTitle: (String) -> Pair<String, String>? = { null },
    runLead: @Composable (ThreadView.RunBlock) -> Unit = {},
    runBody: @Composable (ThreadView.RunBlock, expanded: Boolean) -> Unit = { _, _ -> },
    footer: (LazyListScope.() -> Unit)? = null,
) {
    var expanded by rememberSaveable { mutableStateOf(setOf<String>()) }
    LazyColumn(
        modifier = modifier.testTag("thread-conversation"),
        contentPadding = contentPadding,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        view.conversation.forEach { block ->
            when (block) {
                is ThreadView.RunBlock -> item(key = "run-${block.runId}") {
                    val meta = runMeta(block.runId)
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        runLead(block)
                        ThreadBlockHeader(
                            label = block.label,
                            caption = block.caption,
                            state = block.state,
                            agent = meta?.agent,
                            busy = meta?.busy == true,
                            onClick = { onOpenRun(block.runId) },
                        )
                        // The issues its agent linked sit right under the run.
                        block.links.forEach { link ->
                            ThreadLinkRow(link, issueStatus(link.issueId), onOpen = { onOpenIssue(link.issueId) })
                        }
                        val open = block.runId in expanded
                        if (block.folded) {
                            FoldedRunLine(
                                name = meta?.name.orEmpty(),
                                open = open,
                                onToggle = {
                                    expanded = if (open) expanded - block.runId else expanded + block.runId
                                },
                            )
                            if (open) runBody(block, true)
                        } else {
                            runBody(block, false)
                        }
                    }
                }
                is ThreadView.ParallelBlock -> item(key = "parallel-${block.runs.first().runId}") {
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        val first = runMeta(block.runs.first().runId)
                        ThreadBlockHeader(
                            label = block.label,
                            caption = block.caption,
                            state = block.state,
                            agent = first?.agent,
                            busy = block.runs.any { runMeta(it.runId)?.busy == true },
                            onClick = null,
                        )
                        ThreadParallelCard(
                            block = block,
                            runMeta = runMeta,
                            issueChip = { issueId -> issueTitle(issueId)?.first },
                            onOpenRun = onOpenRun,
                            caption = parallelCaption,
                        )
                        block.links.forEach { link ->
                            ThreadLinkRow(link, issueStatus(link.issueId), onOpen = { onOpenIssue(link.issueId) })
                        }
                    }
                }
            }
        }
        footer?.invoke(this)
    }
}

/** A block's header: the agent mark (spinner while busy), the label, the
 *  caption parts joined ` · `, a hairline, the trailing state word. */
@Composable
fun ThreadBlockHeader(
    label: String,
    caption: List<String>,
    state: ThreadView.RunState,
    agent: String?,
    busy: Boolean,
    onClick: (() -> Unit)?,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(vertical = 2.dp)
            .testTag("thread-block-header"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AgentRunMark(
            agent = agent,
            state = threadMarkState(state, busy),
            size = SessionRowMarkSize,
            badgeSize = SessionRowBadgeSize,
        )
        Spacer(Modifier.width(8.dp))
        Text(label, style = MaterialTheme.typography.bodySmall, color = Secondary, maxLines = 1)
        // The caption takes what it needs (ellipsized when it cannot have
        // it) and the hairline runs on from its end to the state word.
        var captionWidth by remember { mutableFloatStateOf(0f) }
        val hairline = GlassTokens.StrokeRow
        Text(
            if (caption.isEmpty()) "" else " · ${caption.joinToString(" · ")}",
            style = MaterialTheme.typography.bodySmall,
            color = Muted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            onTextLayout = { captionWidth = if (it.lineCount > 0) it.getLineRight(0) else 0f },
            modifier = Modifier
                .weight(1f)
                .drawBehind {
                    val start = captionWidth + 10.dp.toPx()
                    val end = size.width - 10.dp.toPx()
                    if (end - start > 8.dp.toPx()) {
                        drawLine(
                            color = hairline,
                            start = Offset(start, size.height / 2),
                            end = Offset(end, size.height / 2),
                            strokeWidth = GlassTokens.Hairline.toPx(),
                        )
                    }
                },
        )
        Text(
            state.word,
            style = MaterialTheme.typography.labelSmall,
            color = if (state == ThreadView.RunState.NeedsInput) NeedsInputAmber else Muted,
            modifier = Modifier.testTag("thread-block-state"),
        )
    }
}

@Composable
private fun FoldedRunLine(name: String, open: Boolean, onToggle: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = 22.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            name,
            style = MaterialTheme.typography.bodySmall,
            color = Secondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        Text(" · ", style = MaterialTheme.typography.bodySmall, color = Muted)
        Text(
            if (open) ThreadCopy.HIDE_WORK else ThreadCopy.SHOW_WORK,
            style = MaterialTheme.typography.bodySmall,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.clickable(onClick = onToggle).testTag("thread-show-work"),
        )
    }
}

/** An agent link row: `<mark> Linked | Filed and linked  [issue chip]`;
 *  the chip opens that issue's child. */
@Composable
fun ThreadLinkRow(link: ThreadView.LinkRow, status: ResolvedIssueStatus?, onOpen: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().testTag("thread-link-row"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ExponentialMark(size = 14.dp)
        Spacer(Modifier.width(8.dp))
        Text(link.label, style = MaterialTheme.typography.bodySmall, color = Secondary)
        Spacer(Modifier.width(8.dp))
        IssueChip(
            identifier = link.identifier,
            title = link.title,
            status = status,
            size = IssueChipSize.Md,
            onClick = onOpen,
        )
    }
}

/** The parallel card: one row per run (mark/spinner, name, issue chip,
 *  `device · <age>`); a row opens that run. */
@Composable
private fun ThreadParallelCard(
    block: ThreadView.ParallelBlock,
    runMeta: (String) -> ThreadRunMeta?,
    issueChip: (String) -> String?,
    onOpenRun: (String) -> Unit,
    caption: (ThreadView.ParallelRow) -> String,
) {
    Column(modifier = Modifier.fillMaxWidth().glassCard().testTag("thread-parallel-card")) {
        block.runs.forEachIndexed { index, row ->
            if (index > 0) HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
            val meta = runMeta(row.runId)
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { onOpenRun(row.runId) }
                    .heightIn(min = 44.dp)
                    .padding(horizontal = 12.dp, vertical = 10.dp)
                    .testTag("thread-parallel-row"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                AgentRunMark(
                    agent = meta?.agent,
                    state = threadMarkState(row.state, meta?.busy == true),
                    size = SessionRowMarkSize,
                    badgeSize = SessionRowBadgeSize,
                    ended = row.state == ThreadView.RunState.Ended,
                )
                Spacer(Modifier.width(10.dp))
                Row(modifier = Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        row.name,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    row.issueId?.let(issueChip)?.let { identifier ->
                        Spacer(Modifier.width(8.dp))
                        IssueChip(identifier = identifier, title = null, status = null, size = IssueChipSize.Sm)
                    }
                }
                Spacer(Modifier.width(8.dp))
                Text(caption(row), style = MaterialTheme.typography.labelSmall, color = Muted, maxLines = 1)
            }
        }
    }
}

/** The owner's opening message — the transcript's right-aligned bubble. */
@Composable
fun ThreadMessageBubble(text: String) {
    Box(modifier = Modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
        Text(
            text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier
                .fillMaxWidth(0.85f)
                .wrapLeft()
                .clip(RoundedCornerShape(16.dp))
                .background(GlassTokens.OpaqueCardFill)
                .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, RoundedCornerShape(16.dp))
                .padding(horizontal = 14.dp, vertical = 10.dp)
                .testTag("thread-message-bubble"),
        )
    }
}

/** A bubble hugs its text up to its max width. */
private fun Modifier.wrapLeft(): Modifier = this.then(Modifier.widthIn(max = 520.dp))

// ── The reply box ───────────────────────────────────────────────────────────

/** `Reply in this thread`: the floating capsule over the conversation. */
@Composable
fun ThreadReplyBar(
    value: String,
    onValueChange: (String) -> Unit,
    onSend: () -> Unit,
    modifier: Modifier = Modifier,
    sending: Boolean = false,
) {
    val shape = RoundedCornerShape(22.dp)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .clip(shape)
            .background(GlassTokens.OpaqueCardFill)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
            .heightIn(min = 52.dp)
            .padding(start = 16.dp, end = 4.dp)
            .testTag("thread-reply"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        androidx.compose.foundation.text.BasicTextField(
            value = value,
            onValueChange = onValueChange,
            textStyle = MaterialTheme.typography.bodyMedium.copy(color = MaterialTheme.colorScheme.onSurface),
            cursorBrush = androidx.compose.ui.graphics.SolidColor(MaterialTheme.colorScheme.onSurface),
            maxLines = 5,
            modifier = Modifier.weight(1f).padding(vertical = 14.dp).testTag("thread-reply-field"),
            decorationBox = { inner ->
                Box {
                    if (value.isEmpty()) {
                        Text(ThreadCopy.REPLY_PLACEHOLDER, style = MaterialTheme.typography.bodyMedium, color = Muted)
                    }
                    inner()
                }
            },
        )
        if (value.isNotBlank()) {
            IconButton(onClick = onSend, enabled = !sending, modifier = Modifier.testTag("thread-reply-send")) {
                Icon(ExpIcons.uiSend, contentDescription = "Send", modifier = Modifier.size(18.dp))
            }
        }
    }
}

// ── Lists ───────────────────────────────────────────────────────────────────

/** A thread in a list (Agent page Running / Recent): its title over its
 *  `listCaption`, a live run's mark (spinner while busy, amber when it
 *  waits), else the conversation glyph. A row opens the thread. */
@Composable
fun ThreadListRow(
    title: String,
    caption: String,
    live: Boolean,
    busy: Boolean,
    needsInput: Boolean,
    agent: String?,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    /** The live session's own mark state (review green, done blue) when the
     *  caller resolved it; null falls back to busy / needs input. */
    markState: CodingSessionDisplayState? = null,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(56.dp)
            .flatRow()
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp)
            .testTag("thread-row"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(18.dp), contentAlignment = Alignment.Center) {
            if (live) {
                AgentRunMark(
                    agent = agent,
                    state = markState ?: when {
                        needsInput -> CodingSessionDisplayState.NeedsInput
                        busy -> CodingSessionDisplayState.Working
                        else -> null
                    },
                    size = 16.dp,
                    badgeSize = SessionRowBadgeSize,
                )
            } else {
                Icon(ExpIcons.actionChat, contentDescription = null, tint = Secondary, modifier = Modifier.size(16.dp))
            }
        }
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                title,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                caption,
                style = MaterialTheme.typography.bodySmall,
                color = if (needsInput) NeedsInputAmber else Muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag("thread-row-caption"),
            )
        }
    }
}

/** The issue page's `Threads` row: one chip per thread holding the issue
 *  (a live mark while it runs); a chip opens the thread. */
@Composable
fun ThreadsRow(
    threads: List<ThreadsRowChip>,
    onOpen: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (threads.isEmpty()) return
    Row(
        modifier = modifier.fillMaxWidth().testTag("issue-threads-row"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(ThreadCopy.THREADS, style = MaterialTheme.typography.bodySmall, color = Muted)
        Spacer(Modifier.width(10.dp))
        Row(
            modifier = Modifier.weight(1f).horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            threads.forEach { chip ->
                val shape = RoundedCornerShape(8.dp)
                Row(
                    modifier = Modifier
                        .clip(shape)
                        .background(GlassTokens.OpaqueCardFill, shape)
                        .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
                        .clickable { onOpen(chip.threadId) }
                        .padding(horizontal = 10.dp, vertical = 6.dp)
                        .testTag("issue-thread-chip"),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (chip.live) {
                        AgentRunMark(
                            agent = chip.agent,
                            state = if (chip.busy) CodingSessionDisplayState.Working else null,
                            size = 12.dp,
                            badgeSize = SessionRowBadgeSize,
                        )
                    } else {
                        Icon(ExpIcons.actionChat, contentDescription = null, tint = Secondary, modifier = Modifier.size(12.dp))
                    }
                    Spacer(Modifier.width(6.dp))
                    Text(
                        chip.title,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.widthIn(max = 220.dp),
                    )
                }
            }
        }
    }
}

data class ThreadsRowChip(
    val threadId: String,
    val title: String,
    val live: Boolean,
    val busy: Boolean,
    val agent: String?,
)

/** A muted uppercase band label over a list (Running / Recent). */
@Composable
fun ThreadListBand(label: String, modifier: Modifier = Modifier) {
    Text(
        label.uppercase(),
        style = MaterialTheme.typography.labelSmall,
        fontWeight = FontWeight.Medium,
        letterSpacing = 0.6.sp,
        color = Muted,
        modifier = modifier.padding(start = 12.dp, top = 12.dp, bottom = 4.dp),
    )
}

/** Thread rows as a list section — the Agent page and the Recent sheet. */
fun LazyListScope.threadRows(
    keyPrefix: String,
    rows: List<ThreadListRowModel>,
    onOpen: (String) -> Unit,
) {
    items(rows, key = { "$keyPrefix-${it.threadId}" }) { row ->
        ThreadListRow(
            title = row.title,
            caption = row.caption,
            live = row.live,
            busy = row.busy,
            needsInput = row.needsInput,
            agent = row.agent,
            markState = row.markState,
            onClick = { onOpen(row.threadId) },
        )
    }
}

/** One list row's inputs, off a bundle or a styleguide fake. */
data class ThreadListRowModel(
    val threadId: String,
    val title: String,
    val caption: String,
    val live: Boolean,
    val busy: Boolean,
    val needsInput: Boolean,
    val agent: String?,
    val markState: CodingSessionDisplayState? = null,
)
