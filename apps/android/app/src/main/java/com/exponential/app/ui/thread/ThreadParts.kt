package com.exponential.app.ui.thread

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
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.QuestionOption
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WorkCardInput
import com.exponential.app.domain.WorkCardSpec
import com.exponential.app.domain.WorkCards
import com.exponential.app.ui.components.ComposerSubmitButton
import com.exponential.app.ui.components.ComposerToolButton
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.components.PendingAttachmentStrip
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.LiveGreen
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.session.AnsweredRow
import com.exponential.app.ui.session.AskCard
import com.exponential.app.ui.session.LeadText
import com.exponential.app.ui.session.QuestionOptionButton
import com.exponential.app.ui.session.WorkCardView
import com.exponential.app.ui.session.threadCard
import com.exponential.app.ui.session.threadCardTextStyle
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

// EXP-1296 Threads v2 (Model v3): the THREAD screen's stateless pieces, so
// the screen and the styleguide captures draw the SAME composables — the
// header (title, or an agent's icon + name), the ONE tab strip (the phone
// pager's tabs), the conversation rows (`SessionTree.ThreadRow`), THE session
// card (`session-card.json`) and the composer box ([GlassComposer], the one
// composer shell). Copy comes from `SessionTree` (the fixtures) — never the
// word "thread".

/** The few words the fixtures leave to the client (no "thread" word). */
internal object ThreadUiCopy {
    const val LINK_ISSUE = "Link an issue"
    const val NEW_ISSUE = "New issue"
    const val RENAME = "Rename"
    const val SAVE = "Save"
    const val QUEUED = "Queued"
    const val APPROVE = "Approve"
    const val REJECT = "Reject"
    const val APPROVED = "Approved"
    const val REJECTED = "Rejected"
    const val ANSWER = "Answer"
    const val SEND = "Send"
    const val ADD = "Add"
    const val SETTINGS = "Agent settings"
    const val PRIVATE = "Private"

    fun unlink(identifier: String) = "Unlink $identifier"

    /** An agent channel's composer: `Message <name>`. */
    fun messageAgent(name: String) = "Message $name"
}

/** A tab of the strip as its saveable page key. */
fun threadTabKey(tab: SessionTree.ThreadTab): String = when (tab) {
    SessionTree.ThreadTab.Conversation -> "conversation"
    is SessionTree.ThreadTab.IssueTab -> "issue:${tab.issueId}"
    SessionTree.ThreadTab.Add -> "add"
    is SessionTree.ThreadTab.Guide -> "guide"
}

private val Muted: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
private val Secondary: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

// ── Header ──────────────────────────────────────────────────────────────────

/** The thread's header: back, then the title (a lock when private); a
 *  long-press on the title renames ([onRename]; null = no rename). */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ThreadHeader(
    title: String,
    private: Boolean,
    onBack: (() -> Unit)?,
    modifier: Modifier = Modifier,
    onRename: (() -> Unit)? = null,
) {
    Column(modifier = modifier.fillMaxWidth().testTag("thread-header")) {
        Row(
            modifier = Modifier.fillMaxWidth().height(48.dp).padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) TopBarBackButton(onClick = onBack)
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .padding(top = 2.dp, bottom = 10.dp)
                .then(
                    if (onRename != null) {
                        Modifier.combinedClickable(onClick = {}, onLongClick = onRename)
                    } else {
                        Modifier
                    },
                ),
            verticalAlignment = Alignment.CenterVertically,
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
            if (private) {
                Spacer(Modifier.width(8.dp))
                Icon(
                    ExpIcons.uiPrivate,
                    contentDescription = ThreadUiCopy.PRIVATE,
                    tint = Muted,
                    modifier = Modifier.size(16.dp).testTag("thread-private"),
                )
            }
        }
    }
}

/** An agent channel's header: back, the agent's icon (its colour) + name,
 *  and the settings affordance. */
@Composable
fun AgentChannelHeader(
    name: String,
    icon: String?,
    color: String?,
    onBack: (() -> Unit)?,
    onSettings: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth().testTag("thread-header")) {
        Row(
            modifier = Modifier.fillMaxWidth().height(48.dp).padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) TopBarBackButton(onClick = onBack)
            Spacer(Modifier.weight(1f))
            if (onSettings != null) {
                IconButton(onClick = onSettings, modifier = Modifier.testTag("agent-channel-settings")) {
                    Icon(ExpIcons.navSettings, contentDescription = ThreadUiCopy.SETTINGS, tint = Secondary, modifier = Modifier.size(20.dp))
                }
            }
        }
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(top = 2.dp, bottom = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AgentGlyph(icon = icon, color = color, size = 24.dp)
            Spacer(Modifier.width(10.dp))
            Text(
                name,
                fontSize = 22.sp,
                lineHeight = 28.sp,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag("thread-title"),
            )
        }
        HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
    }
}

/** An agent's icon (a pickable icon name) in its colour. */
@Composable
fun AgentGlyph(icon: String?, color: String?, size: androidx.compose.ui.unit.Dp, modifier: Modifier = Modifier) {
    val glyph = icon?.let { ExpIcons.byName(it) }
        ?: ExpIcons.byName(SessionTree.ListCopy.AGENT_DEFAULT_ICON)
        ?: ExpIcons.settingsAgents
    val tint = runCatching { parseColor(color ?: SessionTree.ListCopy.AGENT_DEFAULT_COLOR) }
        .getOrDefault(MaterialTheme.colorScheme.onSurface)
    Icon(glyph, contentDescription = null, tint = tint, modifier = modifier.size(size))
}

// ── The tab strip = the pager's tabs ────────────────────────────────────────

/**
 * Phones: the ONE strip IS the pager's tabs — Conversation, one chip per
 * linked issue (status glyph + mono identifier), "+" (link / file an issue),
 * Guide (a green dot while it holds anything). A long-press on an issue chip
 * opens its menu (Unlink).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ThreadTabStrip(
    tabs: List<SessionTree.ThreadTab>,
    selectedKey: String,
    onSelect: (SessionTree.ThreadTab) -> Unit,
    onAdd: (() -> Unit)?,
    issueStatus: (String) -> ResolvedIssueStatus?,
    modifier: Modifier = Modifier,
    onIssueMenu: ((SessionTree.ThreadTab.IssueTab) -> Unit)? = null,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(start = 12.dp, end = 12.dp, bottom = 8.dp)
                .testTag(SessionTree.ThreadCopy.TEST_ID_TABS),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            tabs.forEach { tab ->
                val key = threadTabKey(tab)
                val selected = key == selectedKey
                val textColor = if (selected) MaterialTheme.colorScheme.onSurface else Secondary
                when (tab) {
                    SessionTree.ThreadTab.Add -> if (onAdd != null) {
                        IconButton(onClick = onAdd, modifier = Modifier.size(36.dp).testTag("thread-tab-add")) {
                            Icon(ExpIcons.uiAdd, contentDescription = ThreadUiCopy.ADD, tint = Secondary, modifier = Modifier.size(16.dp))
                        }
                    }
                    SessionTree.ThreadTab.Conversation -> ThreadTab(selected, { onSelect(tab) }, null, "thread-tab-$key") {
                        Text(SessionTree.ThreadCopy.CONVERSATION, style = MaterialTheme.typography.labelLarge, color = textColor)
                    }
                    is SessionTree.ThreadTab.IssueTab -> ThreadTab(
                        selected = selected,
                        onClick = { onSelect(tab) },
                        onLongClick = onIssueMenu?.let { menu -> { menu(tab) } },
                        testTag = "thread-tab-$key",
                    ) {
                        issueStatus(tab.issueId)?.let {
                            StatusIcon(it, size = 14.dp)
                            Spacer(Modifier.width(5.dp))
                        }
                        Text(
                            tab.identifier,
                            style = MaterialTheme.typography.labelMedium,
                            fontFamily = FontFamily.Monospace,
                            color = textColor,
                        )
                    }
                    is SessionTree.ThreadTab.Guide -> ThreadTab(selected, { onSelect(tab) }, null, "thread-tab-$key") {
                        Text(SessionTree.ThreadCopy.GUIDE, style = MaterialTheme.typography.labelLarge, color = textColor)
                        if (tab.dot) {
                            Spacer(Modifier.width(6.dp))
                            Box(Modifier.size(6.dp).background(LiveGreen, CircleShape).testTag("thread-guide-dot"))
                        }
                    }
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
    onClick: () -> Unit,
    onLongClick: (() -> Unit)?,
    testTag: String,
    content: @Composable () -> Unit,
) {
    val shape = RoundedCornerShape(10.dp)
    Row(
        modifier = Modifier
            .clip(shape)
            .then(
                if (selected) {
                    Modifier
                        .background(GlassTokens.OpaqueCardFill, shape)
                        .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
                } else {
                    Modifier
                },
            )
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .heightIn(min = 36.dp)
            .padding(horizontal = 10.dp, vertical = 8.dp)
            .testTag(testTag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        content()
    }
}

// ── THE session card ────────────────────────────────────────────────────────

/** The card's state icon CONCEPT (`session-card.json` `icons`). */
internal fun sessionCardIcon(concept: String): ImageVector = when (concept) {
    "ui-clock" -> ExpIcons.uiClock
    "coding-running" -> ExpIcons.codingRunning
    "coding-assistant" -> ExpIcons.codingAssistant
    "coding-needs-input" -> ExpIcons.codingNeedsInput
    "notification-session-blocked" -> ExpIcons.notificationSessionBlocked
    "coding-ended" -> ExpIcons.codingEnded
    else -> ExpIcons.codingAssistant
}

/** A tone token (`muted | foreground | amber | rose`) as a colour. */
@Composable
internal fun sessionToneColor(tone: String): Color = when (tone) {
    "foreground" -> MaterialTheme.colorScheme.onSurface
    "amber" -> NeedsInputAmber
    "rose" -> DesignTokens.Semantic.Red
    else -> Muted
}

/**
 * THE session card ×4 (`session-card.json`): the Work card's HEADER row — the
 * state icon, the name (medium, truncates), the muted state word, trailing
 * the elapsed time with a 12px spinner in front while working. No device,
 * account or branch. Tapping opens that session ([onClick]).
 */
@Composable
fun SessionCard(card: SessionTree.SessionCard, onClick: (() -> Unit)?, modifier: Modifier = Modifier) {
    val style = threadCardTextStyle()
    val muted = Muted
    val toneColor = sessionToneColor(card.tone)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .threadCard()
            .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .height(WorkCardSpec.HEADER.dp)
            .padding(horizontal = 12.dp)
            .testTag(card.testId),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            sessionCardIcon(card.icon),
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = if (card.tone == "amber" || card.tone == "rose") toneColor else muted,
        )
        LeadText(
            card.name,
            style = style.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface,
        ) {
            Text(
                card.kindWord,
                style = style,
                // session-card.json: the kindWord is ALWAYS muted; the tone
                // rides on the state icon only.
                color = muted,
                maxLines = 1,
                modifier = Modifier.testTag("session-card-state"),
            )
        }
        if (card.running) InlineSpinner(size = 12.dp)
        card.elapsed?.let {
            Text(it, style = style.copy(fontFeatureSettings = "tnum"), color = muted, maxLines = 1)
        }
    }
}

// ── Conversation rows ───────────────────────────────────────────────────────

/** What a message row shows beyond the model: its author's name, their
 *  user row (the avatar) and the `HH:mm` it was written. */
data class ThreadRowMeta(
    val authorName: String,
    val time: String,
    val user: UserEntity? = null,
)

/** The agent a channel belongs to (its rows' avatar + name). */
data class ThreadAgent(val name: String, val icon: String?, val color: String?)

/**
 * The Conversation page: the model's rows in order. A thread draws a
 * person's message as the right-aligned bubble and the agent's as prose; an
 * AGENT CHANNEL ([agent] non-null) draws every message as a comment row
 * (avatar, name, time) and skips its own root card.
 */
@Composable
fun ThreadConversation(
    rows: List<SessionTree.ThreadRow>,
    rootId: String,
    meta: (SessionTree.ThreadRow.MessageRow) -> ThreadRowMeta?,
    onOpenCard: (SessionTree.SessionCard) -> Unit,
    onAnswer: (messageId: String, answer: String) -> Unit,
    onDecidePlan: (messageId: String, approved: Boolean) -> Unit,
    canUnqueue: (SessionTree.ThreadRow.MessageRow) -> Boolean,
    onUnqueue: (messageId: String) -> Unit,
    modifier: Modifier = Modifier,
    agent: ThreadAgent? = null,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    state: LazyListState = rememberLazyListState(),
) {
    val shown = remember(rows, agent, rootId) {
        if (agent != null) rows.filterNot { it is SessionTree.ThreadRow.SessionRow && it.id == rootId } else rows
    }
    // The newest row stays in view as the conversation grows.
    LaunchedEffect(shown.size) {
        if (shown.isNotEmpty()) state.animateScrollToItem(shown.lastIndex)
    }
    LazyColumn(
        state = state,
        modifier = modifier.testTag(SessionTree.ThreadCopy.TEST_ID_CONVERSATION),
        contentPadding = contentPadding,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        items(shown, key = { it.id }) { row ->
            when (row) {
                is SessionTree.ThreadRow.MessageRow -> {
                    val rowMeta = meta(row)
                    if (agent != null) {
                        ThreadCommentRow(
                            row = row,
                            meta = rowMeta,
                            agent = agent,
                            onUnqueue = if (row.queued && canUnqueue(row)) ({ onUnqueue(row.id) }) else null,
                        )
                    } else {
                        when (row.author) {
                            "user" -> ThreadMessageBubble(
                                text = row.body,
                                caption = rowMeta?.let { listOf(it.authorName, it.time).filter(String::isNotEmpty).joinToString(" · ") },
                                queued = row.queued,
                                onUnqueue = if (row.queued && canUnqueue(row)) ({ onUnqueue(row.id) }) else null,
                            )
                            "agent" -> MarkdownView(
                                markdown = row.body,
                                softBreaksAsNewlines = true,
                                modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp).testTag("thread-agent-message"),
                            )
                            else -> ThreadEventLine(row.body)
                        }
                    }
                }
                is SessionTree.ThreadRow.SessionRow -> SessionCard(row.card, onClick = { onOpenCard(row.card) })
                is SessionTree.ThreadRow.Question -> ThreadQuestionCard(row, onAnswer = { onAnswer(row.id, it) })
                is SessionTree.ThreadRow.Plan -> ThreadPlanCard(row, onDecide = { onDecidePlan(row.id, it) })
                is SessionTree.ThreadRow.Subagent -> WorkCardView(
                    card = WorkCards.of(WorkCardInput.Subagent(title = row.title, state = row.status)),
                )
                is SessionTree.ThreadRow.Picture -> ThreadPictureRow(row)
                is SessionTree.ThreadRow.Event -> ThreadEventLine(row.text)
            }
        }
    }
}

/** A person's message: the right-aligned bubble, `name · HH:mm` under it;
 *  while the device has not taken it: `Queued` + the revoke ✕ (mine only). */
@Composable
fun ThreadMessageBubble(
    text: String,
    caption: String?,
    queued: Boolean,
    onUnqueue: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth(), horizontalAlignment = Alignment.End) {
        val shape = RoundedCornerShape(16.dp)
        Text(
            text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier
                .padding(start = 48.dp)
                .widthIn(max = 520.dp)
                .clip(shape)
                .background(GlassTokens.OpaqueCardFill, shape)
                .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
                .padding(horizontal = 14.dp, vertical = 10.dp)
                .testTag("thread-message-bubble"),
        )
        Row(
            modifier = Modifier.padding(top = 4.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (queued) {
                Text(
                    ThreadUiCopy.QUEUED,
                    style = MaterialTheme.typography.labelSmall,
                    color = Muted,
                    modifier = Modifier.testTag("thread-message-queued"),
                )
                if (onUnqueue != null) {
                    Icon(
                        ExpIcons.uiClose,
                        contentDescription = null,
                        tint = Muted,
                        modifier = Modifier
                            .padding(start = 4.dp)
                            .size(14.dp)
                            .clip(CircleShape)
                            .clickable(role = Role.Button, onClick = onUnqueue)
                            .testTag("thread-message-unqueue"),
                    )
                }
                if (!caption.isNullOrEmpty()) Text(" · ", style = MaterialTheme.typography.labelSmall, color = Muted)
            }
            if (!caption.isNullOrEmpty()) {
                Text(caption, style = MaterialTheme.typography.labelSmall, color = Muted, maxLines = 1)
            }
        }
    }
}

/** An agent channel's message: a comment row (avatar = the agent's icon or
 *  the person's avatar, the name, the time, the body). */
@Composable
fun ThreadCommentRow(
    row: SessionTree.ThreadRow.MessageRow,
    meta: ThreadRowMeta?,
    agent: ThreadAgent,
    onUnqueue: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    val fromAgent = row.author != "user"
    Row(modifier = modifier.fillMaxWidth().testTag("thread-comment-row"), verticalAlignment = Alignment.Top) {
        if (fromAgent) {
            Box(
                modifier = Modifier
                    .size(28.dp)
                    .clip(CircleShape)
                    .background(GlassTokens.OpaqueCardFill, CircleShape)
                    .border(GlassTokens.Hairline, GlassTokens.StrokeCard, CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                AgentGlyph(icon = agent.icon, color = agent.color, size = 16.dp)
            }
        } else {
            UserAvatar(user = meta?.user, nameOrEmail = meta?.authorName, size = 28.dp, userId = row.authorId)
        }
        Spacer(Modifier.width(10.dp))
        Column(
            modifier = Modifier
                .weight(1f)
                .glassCard()
                .padding(horizontal = 12.dp, vertical = 10.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    if (fromAgent) agent.name else meta?.authorName.orEmpty(),
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                meta?.time?.takeIf { it.isNotEmpty() }?.let {
                    Spacer(Modifier.width(8.dp))
                    Text(it, style = MaterialTheme.typography.labelSmall, color = Muted)
                }
                if (row.queued) {
                    Spacer(Modifier.width(8.dp))
                    Text(ThreadUiCopy.QUEUED, style = MaterialTheme.typography.labelSmall, color = Muted, modifier = Modifier.testTag("thread-message-queued"))
                    if (onUnqueue != null) {
                        Icon(
                            ExpIcons.uiClose,
                            contentDescription = null,
                            tint = Muted,
                            modifier = Modifier
                                .padding(start = 4.dp)
                                .size(14.dp)
                                .clip(CircleShape)
                                .clickable(role = Role.Button, onClick = onUnqueue)
                                .testTag("thread-message-unqueue"),
                        )
                    }
                }
            }
            Spacer(Modifier.height(2.dp))
            MarkdownView(markdown = row.body, softBreaksAsNewlines = true)
        }
    }
}

/** A `question` card, answered IN place — the session view's ask card
 *  ([AskCard] + [QuestionOptionButton] + [AnsweredRow]) over plain data: the
 *  question, one numbered row per option; answered = the answer row. No
 *  options = a field + Answer. */
@Composable
fun ThreadQuestionCard(row: SessionTree.ThreadRow.Question, onAnswer: (String) -> Unit) {
    AskCard(plan = false, heading = null, meta = null, modifier = Modifier.testTag("thread-question-card")) {
        MarkdownView(row.question, softBreaksAsNewlines = true)
        if (row.answered) {
            Box(Modifier.padding(top = 4.dp)) { AnsweredRow(row.answer) }
        } else if (row.options.isNotEmpty()) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                row.options.forEachIndexed { index, option ->
                    Box(Modifier.testTag("thread-question-option")) {
                        QuestionOptionButton(
                            option = QuestionOption(label = option, key = option),
                            ordinal = index + 1,
                            primary = false,
                            selected = false,
                            checked = null,
                            enabled = true,
                            dimmed = false,
                            onClick = { onAnswer(option) },
                        )
                    }
                }
            }
        } else {
            var draft by remember(row.id) { mutableStateOf("") }
            GlassTextField(
                value = draft,
                onValueChange = { draft = it },
                modifier = Modifier.fillMaxWidth().testTag("thread-question-field"),
                minLines = 1,
                maxLines = 4,
            )
            GlassPill(
                ThreadUiCopy.ANSWER,
                size = PillSize.Sm,
                primary = true,
                enabled = draft.isNotBlank(),
                onClick = { onAnswer(draft.trim()) },
                modifier = Modifier.align(Alignment.End),
            )
        }
    }
}

/** A `plan` card — the session view's "Plan ready" ask card over plain
 *  data: the plan (markdown), Approve (the primary row) / Reject; decided =
 *  the answer row with the decision word. */
@Composable
fun ThreadPlanCard(row: SessionTree.ThreadRow.Plan, onDecide: (approved: Boolean) -> Unit) {
    AskCard(plan = true, heading = PLAN_READY_HEADING, meta = null, modifier = Modifier.testTag("thread-plan-card")) {
        MarkdownView(markdown = row.plan, softBreaksAsNewlines = true)
        when (row.decision) {
            null -> Column(
                modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Box(Modifier.testTag("thread-plan-approve")) {
                    QuestionOptionButton(
                        option = QuestionOption(label = ThreadUiCopy.APPROVE, key = "approve"),
                        ordinal = 1,
                        primary = true,
                        selected = false,
                        checked = null,
                        enabled = true,
                        dimmed = false,
                        onClick = { onDecide(true) },
                    )
                }
                Box(Modifier.testTag("thread-plan-reject")) {
                    QuestionOptionButton(
                        option = QuestionOption(label = ThreadUiCopy.REJECT, key = "reject"),
                        ordinal = 2,
                        primary = false,
                        selected = false,
                        checked = null,
                        enabled = true,
                        dimmed = false,
                        onClick = { onDecide(false) },
                    )
                }
            }
            else -> Box(Modifier.padding(top = 4.dp).testTag("thread-plan-decision")) {
                AnsweredRow(if (row.decision == "approved") ThreadUiCopy.APPROVED else ThreadUiCopy.REJECTED)
            }
        }
    }
}

/** The session view's plan heading (its ask card says the same). */
private const val PLAN_READY_HEADING = "Plan ready"

/** A `picture` card: the attachment through the markdown image path (auth,
 *  probed size), the caption muted under it. */
@Composable
fun ThreadPictureRow(row: SessionTree.ThreadRow.Picture) {
    Column(modifier = Modifier.fillMaxWidth().testTag("thread-picture"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        MarkdownView(markdown = "![${row.caption.orEmpty()}](/api/attachments/${row.attachmentId})")
        row.caption?.takeIf { it.isNotBlank() }?.let {
            Text(it, style = MaterialTheme.typography.labelSmall, color = Muted)
        }
    }
}

/** An `event` card (and a system message): ONE quiet line. */
@Composable
fun ThreadEventLine(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = Muted,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp).testTag("thread-event"),
    )
}

// ── The composer ────────────────────────────────────────────────────────────

/**
 * THE session composer box ([GlassComposer], the one composer shell the
 * Agent page and the session view wear): the pending files, the field, the
 * "+" (attach) and the send glyph.
 */
@Composable
fun ThreadComposerBox(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    pendingImages: List<PendingAttachment>,
    imageError: String?,
    sending: Boolean,
    onAdd: (() -> Unit)?,
    onRemoveImage: (Int) -> Unit,
    onSend: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val canSend = (value.isNotBlank() || pendingImages.isNotEmpty()) && !sending
    GlassComposer(
        opaque = true,
        modifier = modifier.testTag(SessionTree.ThreadCopy.TEST_ID_COMPOSER),
        strip = {
            PendingAttachmentStrip(items = pendingImages, enabled = !sending, onRemove = onRemoveImage)
            FieldError(imageError, modifier = Modifier.padding(bottom = 4.dp))
        },
        tools = {
            if (onAdd != null) {
                ComposerToolButton(
                    ExpIcons.uiAdd,
                    contentDescription = ThreadUiCopy.ADD,
                    onClick = onAdd,
                    enabled = !sending,
                    modifier = Modifier.testTag("thread-composer-add"),
                )
            }
        },
        submit = {
            ComposerSubmitButton(
                ExpIcons.uiSubmit,
                contentDescription = ThreadUiCopy.SEND,
                onClick = onSend,
                enabled = canSend,
                sending = sending,
                modifier = Modifier.testTag("thread-composer-send"),
            )
        },
    ) {
        GlassTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier.fillMaxWidth().testTag("thread-composer-field"),
            placeholder = placeholder,
            minLines = 2,
            maxLines = 8,
            bordered = false,
            containerColor = Color.Transparent,
        )
    }
}
