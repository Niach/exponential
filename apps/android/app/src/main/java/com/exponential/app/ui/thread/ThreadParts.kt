package com.exponential.app.ui.thread

import android.content.Intent
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
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.platform.LocalDensity
import com.exponential.app.ui.components.JumpToBottomButton
import com.exponential.app.ui.session.isNearBottom
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
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
import androidx.compose.runtime.CompositionLocalProvider
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.unit.Dp
import com.exponential.app.ui.components.FloatingComposer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.net.toUri
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.QuestionOption
import com.exponential.app.domain.withFreeTextRow
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WorkCardInput
import com.exponential.app.domain.WorkCardSpec
import com.exponential.app.domain.WorkCards
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.ComposerSubmitButton
import com.exponential.app.ui.components.ComposerToolButton
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.components.PendingAttachmentStrip
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.LiveGreen
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.markdown.LocalIssueRefBare
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.session.LeadText
import com.exponential.app.ui.session.QuestionCard
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
    /** The contract word, lowercase (`Name · time · queued`). */
    const val QUEUED = "queued"
    /** EXP-1317 ×4: the queued bubble's ✕. */
    const val REMOVE_FROM_QUEUE = "Remove from queue"
    const val APPROVE = "Approve"
    const val REJECT = "Reject"
    const val APPROVED = "Approved"
    const val REJECTED = "Rejected"
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
    is SessionTree.ThreadTab.SessionTab -> "session:${tab.sessionId}"
    is SessionTree.ThreadTab.Guide -> "guide"
}

/** A `question` answer: (message id, answer, settled(ok)). */
typealias ThreadAnswer = (messageId: String, answer: String, settled: (Boolean) -> Unit) -> Unit

/** A `plan` decision: (message id, approved, feedback, settled(ok)). */
typealias ThreadPlanDecision = (messageId: String, approved: Boolean, feedback: String?, settled: (Boolean) -> Unit) -> Unit

private val Muted: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
private val Secondary: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

// ── Header ──────────────────────────────────────────────────────────────────

/** The header band's title (EXP-1316): ONE line, the lock after it when
 *  private. Derived, never renamed, not tappable. */
@Composable
fun ThreadTitle(title: String, private: Boolean, modifier: Modifier = Modifier) {
    Row(modifier = modifier.testTag("thread-header"), verticalAlignment = Alignment.CenterVertically) {
        Text(
            title,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false).testTag("thread-title"),
        )
        if (private) {
            Spacer(Modifier.width(6.dp))
            Icon(
                ExpIcons.uiPrivate,
                contentDescription = ThreadUiCopy.PRIVATE,
                tint = Muted,
                modifier = Modifier.size(14.dp).testTag("thread-private"),
            )
        }
    }
}

/**
 * EXP-1307: the thread header's `…` on the Guide (session-thread.json
 * `guideMenuEntries` ×4): Open on GitHub, then Close PR (destructive) while
 * the PR is open and the viewer is a member. THE app menu (the issue `…`'s
 * [GlassDropdownMenu]); the host hosts Close's confirm. Empty = no trigger.
 */
@Composable
fun ThreadGuideMenuButton(
    prUrl: String?,
    entries: List<SessionTree.GuideMenuEntry>,
    onClosePr: () -> Unit,
) {
    if (entries.isEmpty()) return
    val context = LocalContext.current
    var open by remember { mutableStateOf(false) }
    // The Box anchors the dropdown to the button.
    Box {
        CircleIconButton(
            ExpIcons.uiMore,
            "More",
            onClick = { open = true },
            modifier = Modifier.padding(end = 8.dp).testTag(SessionTree.ThreadCopy.TEST_ID_GUIDE_MENU),
            borderless = true,
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            for (entry in entries) when (entry) {
                SessionTree.GuideMenuEntry.OpenGithub -> GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiGithub, contentDescription = null) },
                    text = { Text(DomainContract.diffUiOpenOnGithub) },
                    enabled = !prUrl.isNullOrBlank(),
                    onClick = {
                        open = false
                        // A device with no browser throws rather than
                        // resolving the intent — a dead tap beats a crash.
                        runCatching {
                            val intent = Intent(Intent.ACTION_VIEW, prUrl.orEmpty().toUri())
                            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                            context.startActivity(intent)
                        }
                    },
                    modifier = Modifier.testTag("thread-guide-menu-open-github"),
                )
                SessionTree.GuideMenuEntry.ClosePr -> GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.prClosed, contentDescription = null) },
                    text = { Text(DomainContract.diffUiClosePr) },
                    destructive = true,
                    onClick = {
                        open = false
                        onClosePr()
                    },
                    modifier = Modifier.testTag("thread-guide-menu-close-pr"),
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
 * linked issue (status glyph + mono identifier in a bordered chip), "+"
 * (link / file an issue, right after the issues), the session chips (the
 * viewed level's sessions, their mark + name; a tap pushes that session's
 * view), Guide (a green dot while it holds anything). Every chip is one
 * height. A long-press on an issue chip opens its menu (Unlink).
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
    onOpenSession: ((SessionTree.ThreadTab.SessionTab) -> Unit)? = null,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(start = 12.dp, end = 12.dp, bottom = 8.dp)
                .testTag(SessionTree.ThreadCopy.TEST_ID_TABS),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            tabs.forEach { tab ->
                val key = threadTabKey(tab)
                val selected = key == selectedKey
                val textColor = if (selected) MaterialTheme.colorScheme.onSurface else Secondary
                when (tab) {
                    SessionTree.ThreadTab.Add -> if (onAdd != null) {
                        IconButton(onClick = onAdd, modifier = Modifier.size(ThreadTabHeight).testTag("thread-tab-add")) {
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
                        chip = true,
                    ) {
                        issueStatus(tab.issueId)?.let {
                            StatusIcon(it, size = 14.dp)
                            Spacer(Modifier.width(6.dp))
                        }
                        Text(
                            tab.identifier,
                            style = MaterialTheme.typography.labelMedium,
                            fontFamily = FontFamily.Monospace,
                            color = if (selected) MaterialTheme.colorScheme.onSurface else MdStyle.ChipToken,
                            maxLines = 1,
                        )
                    }
                    is SessionTree.ThreadTab.SessionTab -> ThreadTab(
                        selected = false,
                        onClick = { onOpenSession?.invoke(tab) },
                        onLongClick = null,
                        testTag = "thread-tab-session",
                        chip = true,
                    ) {
                        SessionTabMark(tab.markState)
                        Spacer(Modifier.width(6.dp))
                        Text(
                            tab.name,
                            style = MaterialTheme.typography.labelMedium,
                            color = Secondary,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.widthIn(max = 160.dp),
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
    }
}

/** EXP-1316: every chip of the strip is ONE height. */
private val ThreadTabHeight = 36.dp

/** A session chip's lead: the thread row mark of that session. */
@Composable
private fun SessionTabMark(state: SessionTree.SessionState) {
    val (icon, tint) = when (state) {
        SessionTree.SessionState.NeedsYou -> ExpIcons.codingNeedsInput to NeedsInputAmber
        SessionTree.SessionState.Working -> ExpIcons.codingRunning to MaterialTheme.colorScheme.onSurface
        SessionTree.SessionState.Ended -> ExpIcons.codingEnded to Muted
        else -> ExpIcons.codingAssistant to Muted
    }
    Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(14.dp))
}

/** One strip tab. [chip] = an entity chip (issue / session): a bordered
 *  capsule even when not selected; the others wear the frame only while
 *  selected. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ThreadTab(
    selected: Boolean,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)?,
    testTag: String,
    chip: Boolean = false,
    content: @Composable () -> Unit,
) {
    val shape = RoundedCornerShape(10.dp)
    Row(
        modifier = Modifier
            .clip(shape)
            .then(
                when {
                    selected -> Modifier
                        .background(GlassTokens.OpaqueCardFill, shape)
                        .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
                    chip -> Modifier.border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
                    else -> Modifier
                },
            )
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .height(ThreadTabHeight)
            .padding(horizontal = 10.dp)
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
    onAnswer: ThreadAnswer,
    onDecidePlan: ThreadPlanDecision,
    canUnqueue: (SessionTree.ThreadRow.MessageRow) -> Boolean,
    onUnqueue: (messageId: String) -> Unit,
    modifier: Modifier = Modifier,
    agent: ThreadAgent? = null,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    state: LazyListState = rememberLazyListState(),
    /** The floating band's height: the jump button sits above it. */
    jumpBottomInset: Dp = 0.dp,
) {
    val shown = remember(rows, agent, rootId) {
        if (agent != null) rows.filterNot { it is SessionTree.ThreadRow.SessionRow && it.id == rootId } else rows
    }
    // EXP-1316 (1n): the session feed's scroll — the thread opens at the
    // newest row and follows new rows while the reader sits at the bottom
    // (~96dp slack); a drag up stops following and shows THE jump-to-bottom
    // button. Only user drags flip follow-mode.
    var follow by rememberSaveable(rootId) { mutableStateOf(true) }
    val followSlackPx = with(LocalDensity.current) { 96.dp.toPx() }
    val jumpScope = rememberCoroutineScope()
    LaunchedEffect(state, followSlackPx) {
        snapshotFlow { state.isScrollInProgress to state.isNearBottom(followSlackPx) }
            .distinctUntilChanged()
            .collect { (dragging, nearBottom) -> if (dragging) follow = nearBottom }
    }
    // The floating band's height lands after the first layout: a follower
    // re-pins to the true end when it does (the session view's rule).
    // EXP-1316: re-pin whenever a follower could still scroll forward (a
    // row grew after its first layout, the band changed height).
    LaunchedEffect(state) {
        snapshotFlow { follow && !state.isScrollInProgress && state.canScrollForward }
            .collect { behind -> if (behind) state.scrollBy(1_000_000f) }
    }
    LaunchedEffect(shown.size, follow, jumpBottomInset) {
        if (follow && shown.isNotEmpty()) {
            state.scrollToItem(shown.lastIndex)
            state.scrollBy(1_000_000f)
        }
    }
    // EXP-760 ×4 (web `thread-markdown.tsx`): every conversation text chips
    // BARE identifiers too (`FB-8`; agents omit the `#`) when the screen
    // provides [LocalIssueRefs]. Display-only: the composer sits outside.
    CompositionLocalProvider(LocalIssueRefBare provides true) {
        Box(modifier = modifier) {
        LazyColumn(
            state = state,
            modifier = Modifier.fillMaxSize().testTag(SessionTree.ThreadCopy.TEST_ID_CONVERSATION),
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
                    is SessionTree.ThreadRow.Question -> ThreadAskCard(row, onAnswer, onDecidePlan)
                    is SessionTree.ThreadRow.Plan -> ThreadAskCard(row, onAnswer, onDecidePlan)
                    is SessionTree.ThreadRow.Subagent -> WorkCardView(
                        card = WorkCards.of(WorkCardInput.Subagent(title = row.title, state = row.status)),
                    )
                    is SessionTree.ThreadRow.Picture -> ThreadPictureRow(row)
                    is SessionTree.ThreadRow.Event -> ThreadEventLine(row.text)
                }
            }
        }
        JumpToBottomButton(
            visible = !follow && shown.isNotEmpty(),
            onClick = {
                jumpScope.launch {
                    state.animateScrollToItem(shown.lastIndex)
                    state.scrollBy(1_000_000f)
                    follow = true
                }
            },
            modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = jumpBottomInset + 8.dp),
        )
        }
    }
}

/** A person's message: the right-aligned bubble, `name · HH:mm` under it;
 *  while the device has not taken it: `· queued` + the revoke ✕ (mine only)
 *  at the END of that caption line. */
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
            // EXP-1317 ×4: ONE caption line `Name · time · queued`, the
            // `Remove from queue` X at its END.
            if (!caption.isNullOrEmpty()) {
                Text(caption, style = MaterialTheme.typography.labelSmall, color = Muted, maxLines = 1)
            }
            if (queued) {
                if (!caption.isNullOrEmpty()) Text(" · ", style = MaterialTheme.typography.labelSmall, color = Muted)
                Text(
                    ThreadUiCopy.QUEUED,
                    style = MaterialTheme.typography.labelSmall,
                    color = Muted,
                    modifier = Modifier.testTag("thread-message-queued"),
                )
                if (onUnqueue != null) {
                    UnqueueButton(onUnqueue)
                }
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
                        UnqueueButton(onUnqueue)
                    }
                }
            }
            Spacer(Modifier.height(2.dp))
            MarkdownView(markdown = row.body, softBreaksAsNewlines = true)
        }
    }
}

/** EXP-1317 ×4: a person's own queued row's ✕ (`sessions.unqueue`). */
@Composable
private fun UnqueueButton(onUnqueue: () -> Unit) {
    Box(
        modifier = Modifier
            .padding(start = 2.dp)
            .size(24.dp)
            .clip(CircleShape)
            .clickable(role = Role.Button, onClickLabel = ThreadUiCopy.REMOVE_FROM_QUEUE, onClick = onUnqueue)
            .testTag("thread-message-unqueue"),
        contentAlignment = Alignment.Center,
    ) {
        Icon(ExpIcons.uiClose, contentDescription = ThreadUiCopy.REMOVE_FROM_QUEUE, tint = Muted, modifier = Modifier.size(14.dp))
    }
}

private const val PLAN_APPROVE_KEY = "approve"
private const val PLAN_REJECT_KEY = "reject"

/**
 * EXP-1316 ONE card ×4: a `question` or `plan` row is THE session view's card
 * ([QuestionCard]) over the row's data, answered through `sessions.answer`:
 * a question's options plus the free-text row (`custom`), a plan's Approve /
 * Reject with the reject's feedback field. An answered row renders closed
 * (the answer / decision shown); the card locks while the answer is in
 * flight and stays locked until the synced row flips.
 */
@Composable
fun ThreadAskCard(row: SessionTree.ThreadRow, onAnswer: ThreadAnswer, onDecidePlan: ThreadPlanDecision) {
    var state by remember(row.id) { mutableStateOf<AnswerState?>(null) }
    val settle: (Boolean) -> Unit = { ok -> state = if (ok) AnswerState.Acked else null }
    val item = when (row) {
        is SessionTree.ThreadRow.Question -> AgentFeedItem.Question(
            id = row.id.hashCode().toLong(),
            text = row.question,
            options = row.options.map { QuestionOption(label = it, key = it) },
            multiSelect = false,
            resolved = row.answered,
            answer = row.answer,
            wireId = row.id,
        ).let { if (row.custom) it.withFreeTextRow() else it }
        is SessionTree.ThreadRow.Plan -> AgentFeedItem.Question(
            id = row.id.hashCode().toLong(),
            text = row.plan,
            options = listOf(
                QuestionOption(ThreadUiCopy.APPROVE, PLAN_APPROVE_KEY),
                QuestionOption(ThreadUiCopy.REJECT, PLAN_REJECT_KEY),
            ),
            multiSelect = false,
            planMode = true,
            resolved = row.decision != null,
            answer = row.decision?.let { if (it == "approved") ThreadUiCopy.APPROVED else ThreadUiCopy.REJECTED },
            wireId = row.id,
        )
        else -> return
    }
    Box(Modifier.testTag(if (row is SessionTree.ThreadRow.Plan) "thread-plan-card" else "thread-question-card")) {
        QuestionCard(
            item = item,
            active = !item.resolved,
            answerEnabled = true,
            state = state.takeUnless { item.resolved },
            stepLabel = null,
            onAnswer = { keys, text ->
                state = AnswerState.Sending
                val key = keys.firstOrNull()
                if (row is SessionTree.ThreadRow.Plan) {
                    onDecidePlan(row.id, key == PLAN_APPROVE_KEY, null, settle)
                } else {
                    val answer = text?.takeIf { it.isNotBlank() } ?: key.orEmpty()
                    onAnswer(row.id, answer, settle)
                }
            },
            onPlanFollowUp = { _, feedback ->
                state = AnswerState.Sending
                onDecidePlan(row.id, false, feedback, settle)
            },
        )
    }
}

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
 * EXP-1316 (1e): the thread's reply box = THE [FloatingComposer]: folded, the
 * floating bar's capsule with the placeholder; tapped, THE composer card
 * ([GlassComposer]: the pending files, the field, the "+" and the send
 * glyph), folding again on blur / idle with nothing drafted.
 */
@Composable
fun ThreadReplyBox(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    pendingImages: List<PendingAttachment>,
    imageError: String?,
    sending: Boolean,
    working: Boolean,
    expanded: Boolean,
    onExpandedChange: (Boolean) -> Unit,
    onAdd: (() -> Unit)?,
    onRemoveImage: (Int) -> Unit,
    onSend: () -> Unit,
    modifier: Modifier = Modifier,
) {
    FloatingComposer(
        expanded = expanded,
        onExpandedChange = onExpandedChange,
        placeholder = placeholder,
        hasDraft = value.isNotBlank() || pendingImages.isNotEmpty(),
        working = working,
        capsuleTestTag = "thread-composer-capsule",
        modifier = modifier.testTag(SessionTree.ThreadCopy.TEST_ID_COMPOSER),
    ) {
        val canSend = (value.isNotBlank() || pendingImages.isNotEmpty()) && !sending
        val focusRequester = remember { FocusRequester() }
        LaunchedEffect(Unit) { runCatching { focusRequester.requestFocus() } }
        GlassComposer(
            opaque = true,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
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
                modifier = Modifier
                    .fillMaxWidth()
                    .focusRequester(focusRequester)
                    .then(fieldModifier)
                    .testTag("thread-composer-field"),
                placeholder = placeholder,
                minLines = 2,
                maxLines = 8,
                bordered = false,
                containerColor = Color.Transparent,
            )
        }
    }
}
