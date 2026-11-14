package com.exponential.app.ui.chat

import com.exponential.app.domain.Dock
import androidx.compose.foundation.layout.ColumnScope
import com.exponential.app.ui.components.DockSheetScope
import com.exponential.app.ui.components.DockAutoOpen
import com.exponential.app.ui.components.DockCards
import com.exponential.app.ui.components.DockSheetBody
import com.exponential.app.ui.components.DockSheetFooter
import com.exponential.app.ui.components.picker.LocalPickerPresentation
import com.exponential.app.ui.components.picker.PickerDefaults
import com.exponential.app.ui.components.picker.PickerPresentation
import com.exponential.app.ui.theme.fullBleed
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.RowScope
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.TranscriptLoadEarlierButton
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.PillSize
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import com.exponential.app.ui.components.LOCKED_DIVIDER_PREFIX
import com.exponential.app.ui.components.TranscriptDivider
import com.exponential.app.ui.theme.liveArrival
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.snapshotFlow
import com.exponential.app.domain.Chats
import androidx.compose.foundation.interaction.collectIsPressedAsState
import com.exponential.app.domain.ChatRowAction
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.session.LocalAskCardTrailing
import com.exponential.app.ui.session.DismissedCardLine
import com.exponential.app.domain.ResultsTab
import com.exponential.app.ui.plan.LocalOpenPlanPage
import com.exponential.app.ui.session.CardDismissButton
import com.exponential.app.ui.session.LocalCardDismiss
import com.exponential.app.domain.PlanPage
import androidx.compose.ui.platform.LocalDensity
import com.exponential.app.ui.components.JumpToBottomButton
import com.exponential.app.ui.session.isNearBottom
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
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
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.WaitingCards
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.QuestionOption
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.TranscriptGrouping
import com.exponential.app.domain.WorkCardInput
import com.exponential.app.domain.WorkCardSpec
import com.exponential.app.domain.WorkCards
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.markdown.LocalIssueRefBare
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.session.LeadText
import com.exponential.app.ui.session.QuestionCard
import com.exponential.app.ui.session.WorkCardView
import com.exponential.app.ui.session.chatCard
import com.exponential.app.ui.session.chatCardTextStyle
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

// EXP-1296 (Model v3): the CHAT screen's stateless pieces, so
// the screen and the styleguide captures draw the SAME composables — the
// header title, the conversation rows (`SessionTree.SessionChatRow`), THE session
// card (`session-card.json`) and the composer box ([GlassComposer], the one
// composer shell). Copy comes from `SessionTree` (the fixtures) — never the
// word "chat".

/** The few words the fixtures leave to the client (no "chat" word). */
internal object ChatUiCopy {
    const val LINK_ISSUE = "Link an issue"
    const val NEW_ISSUE = "New issue"
    /** The contract word, lowercase (`Name · time · queued`). */
    const val QUEUED = "queued"
    /** EXP-1317 ×4: the queued bubble's ✕. */
    const val REMOVE_FROM_QUEUE = "Remove from queue"
    const val SEND = "Send"
    const val ADD = "Add"
    const val PRIVATE = "Private"

    fun unlink(identifier: String) = "Unlink $identifier"
}

/** A `question` answer: (message id, the answer, settled(ok)); EXP-1280
 *  [AskAnswer] carries the picked labels + the surface's data model. */
typealias ChatAnswer = (messageId: String, answer: AskAnswer, settled: (Boolean) -> Unit) -> Unit

/** A `plan` decision: (message id, the decision, settled(ok)); [PlanAnswer]
 *  = approved + fresh ("Yes, and start with a fresh context", ask-dock.json)
 *  + feedback + EXP-1280 the questions' picks and the surface's data model. */
typealias ChatPlanDecision = (messageId: String, decision: PlanAnswer, settled: (Boolean) -> Unit) -> Unit

private val Muted: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
private val Secondary: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

// ── Header ──────────────────────────────────────────────────────────────────

/** The header band's title (EXP-1316): ONE line, the lock after it when
 *  private. Derived, never renamed, not tappable. */
@Composable
fun ChatTitle(title: String, private: Boolean, modifier: Modifier = Modifier) {
    Row(modifier = modifier.testTag("chat-header"), verticalAlignment = Alignment.CenterVertically) {
        Text(
            title,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false).testTag("chat-title"),
        )
        if (private) {
            Spacer(Modifier.width(6.dp))
            Icon(
                ExpIcons.uiPrivate,
                contentDescription = ChatUiCopy.PRIVATE,
                tint = Muted,
                modifier = Modifier.size(14.dp).testTag("chat-private"),
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

// ── THE session card ────────────────────────────────────────────────────────

/** The card's state icon CONCEPT (`session-card.json` `icons`). */
internal fun sessionCardIcon(concept: String): ImageVector = when (concept) {
    "ui-clock" -> ExpIcons.uiClock
    "coding-running" -> ExpIcons.codingRunning
    "coding-assistant" -> ExpIcons.codingAssistant
    "coding-needs-input" -> ExpIcons.codingNeedsInput
    "notification-session-blocked" -> ExpIcons.notificationSessionBlocked
    "coding-ended" -> ExpIcons.codingEnded
    // EXP-1374: interrupted + background.
    "run-pause" -> ExpIcons.runPause
    "trace-background" -> ExpIcons.traceBackground
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
fun SessionCard(
    card: SessionTree.SessionCard,
    onClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    /** EXP-1323: inside another card (a started suggestion): no own shell. */
    bare: Boolean = false,
    /** #EXP-1464 (chat-row-action.json): the trailing chevron of a row
     *  that does something; [expanded] turns it down. */
    chevron: Boolean = false,
    expanded: Boolean = false,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .then(if (bare) Modifier else Modifier.chatCard())
            .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .testTag(card.testId),
    ) {
        SessionCardHeader(card, chevron, expanded)
    }
}

@Composable
private fun SessionCardHeader(card: SessionTree.SessionCard, chevron: Boolean, expanded: Boolean) {
    val style = chatCardTextStyle()
    val muted = Muted
    val toneColor = sessionToneColor(card.tone)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(WorkCardSpec.HEADER.dp)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            sessionCardIcon(card.icon),
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = toneColor,
        )
        LeadText(
            card.name,
            style = style.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface,
        ) {
            Text(
                card.kindWord,
                style = style,
                // #EXP-1466 session-card.json: the word stays muted unless
                // the tone is amber.
                color = if (card.tone == "amber") toneColor else muted,
                maxLines = 1,
                modifier = Modifier.testTag("session-card-state"),
            )
        }
        if (card.running) InlineSpinner(size = 12.dp)
        card.elapsed?.let {
            Text(it, style = style.copy(fontFeatureSettings = "tnum"), color = muted, maxLines = 1)
        }
        if (chevron) {
            Icon(
                if (expanded) ExpIcons.uiChevronDown else ExpIcons.uiChevronRight,
                contentDescription = null,
                tint = muted,
                modifier = Modifier.size(14.dp),
            )
        }
    }
}

// ── Conversation rows ───────────────────────────────────────────────────────

/** What a message row shows beyond the model: its author's name, their
 *  user row (the avatar) and the `HH:mm` it was written. */
data class ChatRowMeta(
    val authorName: String,
    val time: String,
    val user: UserEntity? = null,
)

/** The agent a channel belongs to (its rows' avatar + name). */
data class ChatAgent(val name: String, val icon: String?, val color: String?, val assistant: Boolean = false)

/**
 * #EXP-1464 (chat-row-action.json): what an EXPANDING session row needs
 * from its host: whether a flag stands behind a session's `needs_you`
 * ([blocked]: it opens instead) and THE waiting question / plan card of a
 * session ([waitingCard], null while it has not synced = the skeleton line).
 * Without them a Conversation's session rows only open.
 */
class ChatRowActions(
    val blocked: (sessionId: String) -> Boolean,
    val waitingCard: @Composable (sessionId: String) -> SessionTree.SessionChatRow?,
)

/**
 * The Conversation page: the model's rows in order. #EXP-1736
 * (transcript-grouping.json): ONE transcript for every chat, read like a
 * group chat: the viewer's bubbles right, everyone else's left under their
 * sender's avatar + name ([TranscriptBubbleRow], [MessageBubble]); a child's
 * report and another session's message are that session's bubble (named
 * `<session> ›`, opening it); every card spans the column.
 */
@Composable
fun ChatConversation(
    rows: List<SessionTree.SessionChatRow>,
    rootId: String,
    meta: (SessionTree.SessionChatRow.MessageRow) -> ChatRowMeta?,
    onOpenCard: (SessionTree.SessionCard) -> Unit,
    onAnswer: ChatAnswer,
    onDecidePlan: ChatPlanDecision,
    canUnqueue: (SessionTree.SessionChatRow.MessageRow) -> Boolean,
    onUnqueue: (messageId: String) -> Unit,
    modifier: Modifier = Modifier,
    /** The agent whose chat this is (its bubbles' mark), null = a session's. */
    agent: ChatAgent? = null,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    state: LazyListState = rememberLazyListState(),
    /** The floating band's height: the jump button sits above it. */
    jumpBottomInset: Dp = 0.dp,
    /** EXP-1323: a report line / a started suggestion opens that session. */
    onOpenSession: ((sessionId: String) -> Unit)? = null,
    /** EXP-1323: the suggested tasks cards' actions (null = read-only). */
    suggestions: ChatSuggestionActions? = null,
    /** EXP-1323: the live tail after the last row (the Tasks card + the
     *  status line), the host's; null = none. */
    tail: (@Composable () -> Unit)? = null,
    /** THE ask dock: the docked rows' ids. Session 58cb32df (FEED-140): a
     *  WAITING plan or question card stays inline at its place WITH its
     *  actions besides the dock; only the suggestion rows leave. */
    dockedIds: Set<String> = emptySet(),
    /** Session 58cb32df: a surface card's `reply` posts its text here (the
     *  person's next message); null = inert. */
    onSurfaceReply: ((String) -> Unit)? = null,
    /** S-3: the view's `origin` (who started the viewed session); #EXP-1736
     *  its prompt reads as that session's bubble. Null = a person started it. */
    origin: SessionTree.SessionXref? = null,
    /** S-3: an inter-session link PUSHES that session (null = inert). */
    onOpenXref: ((sessionId: String) -> Unit)? = null,
    /** #EXP-1464: a Needs-you session row expands into its card (null = opens). */
    rowActions: ChatRowActions? = null,
    /** EXP-1553: THE transcript's wall clock (`HH:mm`), the one formatter
     *  the bubbles' times ([meta]) and the divider pills share, so both read
     *  the same zone; a host formatting its bubbles otherwise passes its own. */
    clock: (wire: String) -> String? = ::clockTime,
    /** #EXP-1736: the conversation's OWN sender (its session, title, agent);
     *  null = the root, untitled. */
    chat: TranscriptGrouping.Chat? = null,
    /** #EXP-1736: the viewer (their own bubbles sit right). */
    viewerId: String? = null,
    /** #FEED-237: an own row's delivery mark (message-delivery.json); null =
     *  from the row (queued = the clock, else two checks). */
    deliveryState: ((SessionTree.SessionChatRow.MessageRow) -> com.exponential.app.domain.MessageDelivery.State?)? = null,
    /** A specimen showing the transcript's head: opens at the top, no
     *  follow and no jump button. */
    startAtTop: Boolean = false,
) {
    // #EXP-1464 (chat-row-action.json): the expanded session rows (by row
    // id) and the sessions the viewer answered from one (client-held).
    var expandedRows by rememberSaveable(rootId) { mutableStateOf(listOf<String>()) }
    var answeredSessions by rememberSaveable(rootId) { mutableStateOf(listOf<String>()) }
    val openRowSession: ((String) -> Unit)? = onOpenXref ?: onOpenSession
    // EXP-1334: no root card any more (the conversation IS the dispatcher).
    val shown = if (dockedIds.isEmpty()) {
        rows
    } else {
        rows.filter {
            it.id !in dockedIds || it is SessionTree.SessionChatRow.Plan || it is SessionTree.SessionChatRow.Question
        }
    }
    // chats.json: a row ARRIVES when its id was not in the transcript the
    // screen first painted (history, a reopen or a scroll never animate).
    val arrivals = rememberLiveArrivals(rootId, shown)
    // chats.json FOLLOW + JUMP (#EXP-1383): the chat opens at the newest row
    // and follows new rows while the reader sits within `followSlack` of the
    // bottom; a drag further up stops following and shows THE jump-to-bottom
    // button. Only user drags flip follow-mode.
    var follow by rememberSaveable(rootId) { mutableStateOf(!startAtTop) }
    val followSlackPx = with(LocalDensity.current) { Chats.Transcript.FOLLOW_SLACK.dp.toPx() }
    val jumpScope = rememberCoroutineScope()
    // chats.json WINDOW (#EXP-1380, review of #1218: `windowCases`): the
    // newest `transcript.window` rows, the start reaching up to the oldest
    // WAITING plan / question card. The FIRST paint pins the start
    // ([pinnedStart]): arrivals append below it whether the reader follows
    // or not, so the window only grows; Load earlier = ONE press, one
    // `windowStep` up ([loadedSteps]). A reopen starts again at the newest.
    var pinnedStart by rememberSaveable(rootId) { mutableStateOf<Int?>(null) }
    var loadedSteps by rememberSaveable(rootId) { mutableStateOf(0) }
    val waitingIndexes = remember(shown) { transcriptWaitingIndexes(shown) }
    val windowStart = Chats.transcriptWindowStart(shown.size, waitingIndexes, pinnedStart, loadedSteps)
    // #EXP-1736 (transcript-grouping.json): who sent each row, the groups.
    // ONE origin rule (data first): the view's origin takes the first
    // message while the window holds the conversation's start.
    val fromStart = windowStart == 0
    val layouts = remember(shown, chat, origin, fromStart, viewerId, rootId, meta) {
        chatTranscriptLayout(shown, chat ?: TranscriptGrouping.Chat(rootId, "", null, null), origin, fromStart, viewerId, meta)
    }
    val painted = remember(shown, windowStart) { if (windowStart == 0) shown else shown.subList(windowStart, shown.size) }
    val header = if (windowStart > 0) 1 else 0
    val itemCount = header + painted.size + (if (tail != null) 1 else 0)
    // Load earlier keeps the reader's place: the row that was first on screen
    // stays at its offset once the rows above it land.
    var anchor by remember(rootId) { mutableStateOf<Pair<String, Int>?>(null) }
    // The last item count a scroll was placed for (not state: no recompose).
    val placedCount = remember(rootId) { intArrayOf(-1) }
    // No flash of the window's top: the landing (open, a follower's arrival)
    // and the Load earlier anchor are REQUESTED in the composition's own
    // frame (`requestScrollToItem`), so the first measure of the new rows is
    // already at its place — never a frame of the window start first.
    SideEffect {
        if (painted.isEmpty()) return@SideEffect
        if (pinnedStart == null) pinnedStart = windowStart
        val held = anchor
        if (held != null) {
            val index = painted.indexOfFirst { it.id == held.first }
            if (index >= 0) state.requestScrollToItem(index + header, -held.second)
            anchor = null
        } else if (follow && itemCount != placedCount[0]) {
            state.requestScrollToItem(itemCount - 1, PIN_END_OFFSET)
        }
        placedCount[0] = itemCount
    }
    // waiting-cards.json DEEP LINKS: THE pending jump into a card (a waiting
    // chip, a Needs you row): Load earlier ONE step at a time until it is
    // painted, its layout, the centre, the ring; the list stops following.
    val deepLink = com.exponential.app.ui.session.rememberDeepLink(
        listState = state,
        request = com.exponential.app.ui.session.LocalCardScrollRequest.current,
        indexOf = { id -> painted.indexOfFirst { it.id == id }.takeIf { it >= 0 }?.let { it + header } },
        more = { windowStart > 0 },
        loadedMark = { painted.size },
        page = { loadedSteps += 1 },
        onTake = { follow = false },
    )
    val linkPending = rememberUpdatedState(deepLink.pending)
    LaunchedEffect(state, followSlackPx) {
        snapshotFlow { state.isScrollInProgress to state.isNearBottom(followSlackPx) }
            .distinctUntilChanged()
            // A deep link's own scrolls never re-arm follow.
            .collect { (dragging, nearBottom) -> if (dragging && !linkPending.value) follow = nearBottom }
    }
    // The floating band's height lands after the first layout: a follower
    // re-pins to the true end when it does (the session view's rule).
    // EXP-1316: re-pin whenever a follower could still scroll forward (a
    // row grew after its first layout, the band changed height).
    // #EXP-1380: always via the LAST item, never a long scrollBy from the
    // top — that composed every row on the way down (the OOM on long chats).
    LaunchedEffect(state) {
        snapshotFlow { follow && !state.isScrollInProgress && state.canScrollForward }
            .collect { behind -> if (behind) state.pinToEnd() }
    }
    LaunchedEffect(painted.size, follow, jumpBottomInset) {
        if (follow && painted.isNotEmpty()) state.pinToEnd()
    }
    // EXP-760 ×4 (web `chat-markdown.tsx`): every conversation text chips
    // BARE identifiers too (`FB-8`; agents omit the `#`) when the screen
    // provides [LocalIssueRefs]. Display-only: the composer sits outside.
    CompositionLocalProvider(
        LocalIssueRefBare provides true,
        com.exponential.app.ui.session.LocalCardHighlight provides deepLink.ringing,
    ) {
        Box(modifier = modifier) {
        LazyColumn(
            state = state,
            modifier = Modifier.fillMaxSize().testTag(SessionTree.ChatCopy.TEST_ID_CONVERSATION),
            contentPadding = contentPadding,
            // #EXP-1736: each row carries its own gap (`layout.gap*`).
        ) {
            if (header > 0) {
                // THE Load earlier ×4 (the run view's, muted small text).
                item(key = "load-earlier") {
                    TranscriptLoadEarlierButton(
                        onClick = {
                            val first = state.layoutInfo.visibleItemsInfo.firstOrNull { it.key != "load-earlier" }
                            anchor = (first?.key as? String)?.let { it to first.offset }
                            loadedSteps += 1
                        },
                    )
                }
            }
            items(painted, key = { it.id }) { row ->
                val layout = layouts[row.id]
                val sender = layout?.sender
                TranscriptBubbleRow(
                    layout = layout,
                    modifier = Modifier.liveArrival(arrivals(row.id)),
                    avatar = {
                        if (sender != null) {
                            SenderAvatar(
                                sender = sender,
                                user = (row as? SessionTree.SessionChatRow.MessageRow)
                                    ?.takeIf { sender.kind == TranscriptGrouping.SenderKind.Member }
                                    ?.let { meta(it)?.user },
                                agent = agent,
                            )
                        }
                    },
                ) {
                when (row) {
                    // #EXP-1736 (transcript-grouping.json): ONE bubble for every
                    // message: the viewer's right, anyone else's left under
                    // its sender (a cross-session sender's opens that session).
                    is SessionTree.SessionChatRow.MessageRow -> {
                        val rowMeta = meta(row)
                        when (layout?.side) {
                            TranscriptGrouping.Side.Full, null -> ChatEventLine(row.body)
                            TranscriptGrouping.Side.Own -> MessageBubble(
                                text = row.body,
                                own = true,
                                time = rowMeta?.time,
                                queued = row.queued,
                                onUnqueue = if (row.queued && canUnqueue(row)) ({ onUnqueue(row.id) }) else null,
                                // #FEED-237: the viewer's bubble carries its
                                // delivery mark, never a `queued` caption.
                                status = deliveryState?.invoke(row) ?: if (row.queued) {
                                    com.exponential.app.domain.MessageDelivery.State.Sending
                                } else {
                                    com.exponential.app.domain.MessageDelivery.State.Read
                                },
                            )
                            TranscriptGrouping.Side.Other -> {
                                val link = sender?.link
                                val open = onOpenXref
                                if (layout.isRelay()) {
                                    // #EXP-1772: a relay: its name above the
                                    // bubble, only the name opens the session.
                                    RelayBubble(
                                        author = layout.authorLine(),
                                        time = rowMeta?.time,
                                        onOpen = if (link != null && open != null) ({ open(link) }) else null,
                                        queued = row.queued,
                                    ) { MessageFold(row.body) { MarkdownView(markdown = row.body, softBreaksAsNewlines = true) } }
                                } else {
                                    MessageBubble(
                                        text = row.body,
                                        own = false,
                                        time = rowMeta?.time,
                                        author = layout.authorLine(),
                                        queued = row.queued,
                                    )
                                }
                            }
                        }
                    }
                    is SessionTree.SessionChatRow.Divider -> TranscriptDivider(row.kind, clock(row.at))
                    is SessionTree.SessionChatRow.Locked -> TranscriptDivider(LOCKED_DIVIDER_PREFIX + row.reason, null)
                    is SessionTree.SessionChatRow.Surface -> AgentColumn {
                        SurfaceCard(
                            row = row,
                            animate = arrivals(row.id),
                            onReply = { text -> onSurfaceReply?.invoke(text) },
                        )
                    }
                    is SessionTree.SessionChatRow.SessionRow -> {
                        val card = row.card
                        val action = ChatRowAction.chatRowAction(
                            type = "session",
                            sessionId = card.sessionId,
                            rootId = rootId,
                            state = card.state,
                            blocked = rowActions?.blocked?.invoke(card.sessionId) ?: false,
                            // No waiting card to draw = it opens.
                            inline = rowActions == null || ChatRowAction.waitingCardInline(rows, card.sessionId),
                            answered = card.sessionId in answeredSessions,
                        )
                        val shownCard = action.kindWord?.let { word ->
                            card.copy(kindWord = word, tone = action.tone ?: card.tone)
                        } ?: card
                        when (action.action) {
                            ChatRowAction.Kind.Expand -> {
                                val expanded = row.id in expandedRows
                                val toggle = {
                                    expandedRows = if (expanded) expandedRows - row.id else expandedRows + row.id
                                }
                                if (expanded && rowActions != null) {
                                    ExpandedSessionRow(
                                        card = shownCard,
                                        waiting = rowActions.waitingCard(card.sessionId),
                                        onCollapse = toggle,
                                        onOpenChat = { onOpenCard(card) },
                                        onAnswer = onAnswer,
                                        onDecidePlan = onDecidePlan,
                                        onAnswered = {
                                            answeredSessions = answeredSessions - card.sessionId + card.sessionId
                                            expandedRows = expandedRows - row.id
                                        },
                                    )
                                } else {
                                    SessionCard(shownCard, onClick = toggle, chevron = true)
                                }
                            }
                            ChatRowAction.Kind.Open ->
                                SessionCard(shownCard, onClick = { onOpenCard(card) }, chevron = true)
                            ChatRowAction.Kind.None -> SessionCard(shownCard, onClick = null)
                        }
                    }
                    is SessionTree.SessionChatRow.Question -> ChatAskCard(row, onAnswer, onDecidePlan)
                    is SessionTree.SessionChatRow.Plan -> ChatAskCard(row, onAnswer, onDecidePlan)
                    is SessionTree.SessionChatRow.Subagent -> WorkCardView(
                        card = WorkCards.of(WorkCardInput.Subagent(title = row.title, state = row.status)),
                    )
                    is SessionTree.SessionChatRow.Picture -> ChatPictureRow(row)
                    is SessionTree.SessionChatRow.Event -> {
                        val action = ChatRowAction.chatRowAction(
                            type = "event",
                            sessionId = row.sessionId,
                            rootId = rootId,
                            state = null,
                            blocked = false,
                            inline = false,
                            answered = false,
                        )
                        val open = openRowSession
                        val target = action.sessionId
                        ChatEventLine(
                            row.text,
                            onClick = if (action.action != ChatRowAction.Kind.None && open != null && target != null) {
                                { open(target) }
                            } else {
                                null
                            },
                        )
                    }
                    // #EXP-1736: a child's report = its bubble (the summary),
                    // named `<child> ›`; #EXP-1772: a relay, only the name
                    // above it opens the child.
                    is SessionTree.SessionChatRow.Report -> {
                        val action = ChatRowAction.chatRowAction(
                            type = "report",
                            sessionId = row.sessionId,
                            rootId = rootId,
                            state = null,
                            blocked = false,
                            inline = false,
                            answered = false,
                        )
                        val open = onOpenXref?.takeIf { action.action != ChatRowAction.Kind.None }
                        RelayBubble(
                            author = layout.authorLine(),
                            time = null,
                            onOpen = open?.let { { it(row.sessionId) } },
                        ) {
                            val text = row.summary.ifEmpty { row.text }
                            MessageFold(text) { MarkdownView(markdown = text, softBreaksAsNewlines = true) }
                        }
                    }
                    is SessionTree.SessionChatRow.Suggestions -> ChatSuggestionsCard(
                        row = row,
                        actions = suggestions,
                        onOpenSession = onOpenSession,
                    )
                }
                }
            }
            if (tail != null) {
                item(key = "__live_tail__") {
                    Box(Modifier.padding(top = TranscriptGrouping.Layout.GAP_BETWEEN.dp)) { tail() }
                }
            }
        }
        JumpToBottomButton(
            visible = !follow && !startAtTop && shown.isNotEmpty(),
            // #EXP-1383 (owner 2026-10-24: "exactly as in sessions"): the run
            // view's tap — animate to the newest row (the pinned window still
            // ends on it), finish on the true end and resume following; the
            // start stays pinned (chats.json WINDOW: never re-slides).
            onClick = {
                jumpScope.launch {
                    val last = state.layoutInfo.totalItemsCount - 1
                    if (last >= 0) {
                        state.animateScrollToItem(last)
                        state.scrollBy(100_000f)
                    }
                    follow = true
                }
            },
            // The run view's placement: centred 12dp above the composer's
            // rung ([jumpBottomInset] = the floating band, whose top 8dp is
            // the bar's own padding), so band + 4dp.
            modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = jumpBottomInset + 4.dp),
        )
        }
    }
}

/** chats.json WINDOW: the indexes of the WAITING plan / question cards
 *  (a plan without a decision, an unanswered question) — the window's start
 *  never cuts one off. */
internal fun transcriptWaitingIndexes(rows: List<SessionTree.SessionChatRow>): List<Int> =
    rows.indices.filter { i ->
        when (val row = rows[i]) {
            is SessionTree.SessionChatRow.Plan -> row.decision == null
            is SessionTree.SessionChatRow.Question -> !row.answered
            else -> false
        }
    }

/** #EXP-1736 (transcript-grouping.json): the conversation's rows as the
 *  transcript's items, by row id: messages and reports carry a sender,
 *  subagent lines are the agent's trace, every card is a full row. */
internal fun chatTranscriptLayout(
    rows: List<SessionTree.SessionChatRow>,
    chat: TranscriptGrouping.Chat,
    origin: SessionTree.SessionXref?,
    fromStart: Boolean,
    viewerId: String?,
    meta: (SessionTree.SessionChatRow.MessageRow) -> ChatRowMeta?,
): Map<String, TranscriptGrouping.RowLayout> {
    val sessions = LinkedHashMap<String, TranscriptGrouping.SessionRef>()
    fun addXref(xref: SessionTree.SessionXref?) {
        if (xref != null) sessions[xref.sessionId] = TranscriptGrouping.SessionRef(xref.sessionId, xref.name, xref.agentId)
    }
    addXref(origin)
    val members = LinkedHashMap<String, String>()
    val items = rows.map { row ->
        when (row) {
            is SessionTree.SessionChatRow.MessageRow -> {
                addXref(row.xref)
                val authorId = row.authorId
                if (row.author == "user" && authorId != null && authorId !in members) {
                    meta(row)?.authorName?.takeIf { it.isNotEmpty() }?.let { members[authorId] = it }
                }
                TranscriptGrouping.Item(
                    id = row.id,
                    kind = TranscriptGrouping.ItemKind.Message,
                    authorKind = row.author,
                    authorId = row.authorId,
                    authorSessionId = row.authorSessionId,
                )
            }
            is SessionTree.SessionChatRow.Report -> {
                addXref(row.xref)
                TranscriptGrouping.Item(
                    id = row.id,
                    kind = TranscriptGrouping.ItemKind.Message,
                    authorKind = "agent",
                    authorSessionId = row.sessionId,
                )
            }
            is SessionTree.SessionChatRow.Subagent -> TranscriptGrouping.Item(row.id, TranscriptGrouping.ItemKind.Trace)
            else -> TranscriptGrouping.Item(row.id, TranscriptGrouping.ItemKind.Full)
        }
    }
    val layouts = TranscriptGrouping.transcriptLayout(
        TranscriptGrouping.Input(
            viewerId = viewerId,
            chat = chat.copy(originSessionId = origin?.sessionId, fromStart = fromStart),
            members = members.map { (id, name) -> TranscriptGrouping.Named(id, name) },
            agents = chat.agentId?.let { listOf(TranscriptGrouping.Named(it, chat.title)) }.orEmpty(),
            sessions = sessions.values.toList(),
            items = items,
        ),
    )
    return layouts.associateBy { it.id }
}

/** A requested end landing: past the last item's size, so the measure
 *  settles it flush with the viewport's end (the list clamps the overshoot). */
private const val PIN_END_OFFSET = 100_000

/** Scroll to the very end through the LAST item (only the rows on screen
 *  compose; a long scrollBy from far up composes every row it passes). */
private suspend fun LazyListState.pinToEnd() {
    val last = layoutInfo.totalItemsCount - 1
    if (last < 0) return
    scrollToItem(last)
    scrollBy(100_000f)
}

/** EXP-1317 ×4: a person's own queued row's ✕ (`sessions.unqueue`). */
@Composable
internal fun UnqueueButton(onUnqueue: () -> Unit) {
    Box(
        modifier = Modifier
            .padding(start = 2.dp)
            .size(24.dp)
            .clip(CircleShape)
            .clickable(role = Role.Button, onClickLabel = ChatUiCopy.REMOVE_FROM_QUEUE, onClick = onUnqueue)
            .testTag("chat-message-unqueue"),
        contentAlignment = Alignment.Center,
    ) {
        Icon(ExpIcons.uiClose, contentDescription = ChatUiCopy.REMOVE_FROM_QUEUE, tint = Muted, modifier = Modifier.size(14.dp))
    }
}

/**
 * EXP-1316 ONE card ×4, EXP-1280 the RICH card (plan-card.json): a
 * `question` or `plan` row is [PlanAskCard] over the row's data, answered
 * through `sessions.answer` (the picks + the surface's data model). An
 * answered row renders closed; the card locks while the answer is in flight
 * and stays locked until the synced row flips.
 */
@Composable
fun ChatAskCard(row: SessionTree.SessionChatRow, onAnswer: ChatAnswer, onDecidePlan: ChatPlanDecision) {
    // FEED-140: a plan the agent replaced with a newer one paints CLOSED and
    // collapsed to `copy.superseded` (never in the dock).
    if (row is SessionTree.SessionChatRow.Plan && row.decision == AskDock.DECISION_SUPERSEDED) {
        SupersededPlanLine()
        return
    }
    // #EXP-1561 (waiting-cards.json): a DISMISSED card = ONE quiet line.
    if (row is SessionTree.SessionChatRow.Plan && row.decision == AskDock.DECISION_DISMISSED) {
        DismissedCardLine(plan = true, title = AskDock.askDockTitle(row.plan))
        return
    }
    if (row is SessionTree.SessionChatRow.Question && row.dismissed == true) {
        DismissedCardLine(plan = false, title = AskDock.askDockTitle(row.question))
        return
    }
    if (row is SessionTree.SessionChatRow.Plan) {
        ChatPlanRow(row)
        return
    }
    Column(Modifier.testTag("chat-question-card")) {
        PlanAskCard(row, onAnswer, onDecidePlan)
    }
}

/**
 * #EXP-1662 (plan-page.json): a plan in the Conversation = ONE row in THE
 * session card's box: the `coding-plan` glyph (amber while it waits, emerald
 * check approved, muted otherwise), `planRowLabel` (one line), the status word,
 * the chevron; a press opens the chat's plan tab on THAT plan. THE × while it
 * waits, gone once the press is in flight.
 */
@Composable
private fun ChatPlanRow(row: SessionTree.SessionChatRow.Plan) {
    val openPlan = LocalOpenPlanPage.current
    val dismiss = LocalCardDismiss.current
    var dismissing by remember(row.id) { mutableStateOf(false) }
    val waiting = row.decision == null
    val approved = row.decision == "approved"
    val style = chatCardTextStyle()
    val tone = if (waiting) DesignTokens.Semantic.Yellow else Muted
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .chatCard()
            .heightIn(min = PlanPage.Layout.ROW_HEIGHT.dp)
            .testTag("chat-plan-card"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .weight(1f)
                .heightIn(min = PlanPage.Layout.ROW_HEIGHT.dp)
                .clickable(role = Role.Button) { openPlan.open(row.sessionId, row.id) }
                .padding(horizontal = 12.dp)
                .testTag("plan-row"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.weight(1f),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Icon(
                    if (approved) ExpIcons.uiCheck else ExpIcons.codingPlan,
                    contentDescription = null,
                    tint = if (approved) DesignTokens.Semantic.Green else tone,
                    modifier = Modifier.size(16.dp),
                )
                Text(
                    PlanPage.planRowLabel(row.plan),
                    style = style.copy(fontWeight = FontWeight.Medium),
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                Text(
                    row.decision?.let { AskDock.planDecisionLabel(it, row.fresh) } ?: PlanPage.Copy.NEEDS_YOU,
                    style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Normal),
                    color = tone,
                    maxLines = 1,
                    softWrap = false,
                )
            }
            Icon(ExpIcons.uiChevronRight, contentDescription = null, tint = Muted, modifier = Modifier.size(16.dp))
        }
        if (waiting && dismiss != null && !dismissing) {
            Box(Modifier.padding(end = 8.dp)) {
                CardDismissButton(
                    onDismiss = {
                        dismissing = true
                        dismiss(row.id) { ok -> if (!ok) dismissing = false }
                    },
                    corner = false,
                )
            }
        }
    }
}

/** FEED-140 (EXP-1553, web's PlanCard parity): a superseded plan = THE
 *  plan card CLOSED — the `Plan ready` header (plan glyph) on THE card
 *  surface, its body collapsed to the muted `× Replaced by a newer plan`
 *  line, the whole card at 80 % opacity. */
@Composable
fun SupersededPlanLine(modifier: Modifier = Modifier) {
    com.exponential.app.ui.session.AskCard(
        plan = true,
        heading = AskDock.Copy.PLAN_READY,
        meta = null,
        modifier = modifier
            .graphicsLayer { alpha = 0.8f }
            .testTag("chat-plan-superseded"),
    ) {
        Row(
            verticalAlignment = Alignment.Top,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Icon(
                ExpIcons.uiClose,
                contentDescription = null,
                tint = Muted,
                modifier = Modifier.padding(top = 2.dp).size(14.dp),
            )
            Text(
                AskDock.Copy.SUPERSEDED,
                style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, fontWeight = FontWeight.Normal),
                color = Muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** The agent's column: cards (a surface preview) sit left, as wide as the
 *  agent's bubble may grow (94%). */
@Composable
fun AgentColumn(content: @Composable () -> Unit) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        Box(Modifier.align(Alignment.CenterStart).widthIn(max = maxWidth * 0.94f)) { content() }
    }
}

/**
 * Session 58cb32df: which rows ARRIVED while the chat was open — every id not
 * in the first non-empty transcript painted for [key] (a reopen = a new
 * composition = nothing animates).
 */
@Composable
fun rememberLiveArrivals(key: String, rows: List<SessionTree.SessionChatRow>): (String) -> Boolean {
    val initial = remember(key) { mutableStateOf<Set<String>?>(null) }
    if (initial.value == null && rows.isNotEmpty()) initial.value = rows.mapTo(HashSet()) { it.id }
    val seen = initial.value
    return { id -> seen != null && id !in seen }
}

/**
 * N-12: THE Dock in a chat (dock.json; web `ChatDock`): the host's
 * [sources] (the root's Queue, queued messages, banners) plus the
 * suggested-tasks rows with an open item (only with [suggestions]; null =
 * read-only) as DockCards above the root's session view's composer
 * (#EXP-1732: the Chat tab's Dock, [com.exponential.app.ui.session.SessionPage]'s
 * `dock`) — the rest as before: DockCards above the
 * composer; a suggestions card opens [ChatSuggestionsSheet], the rest
 * through [sessionSheet]. S-3: a waiting plan or question is
 * never a DockCard (it stays inline in the Conversation, answerable),
 * so nothing opens itself. [initialOpenId] / [initialExpanded] seed the
 * styleguide's states.
 */
@Composable
fun ChatDock(
    rows: List<SessionTree.SessionChatRow>,
    modifier: Modifier = Modifier,
    sources: List<Dock.Source> = emptyList(),
    sessionSheet: (@Composable ColumnScope.(Dock.Row, DockSheetScope, () -> Unit) -> Unit)? = null,
    action: (@Composable (Dock.Row) -> Unit)? = null,
    initialOpenId: String? = null,
    initialExpanded: Boolean = false,
    suggestions: ChatSuggestionActions? = null,
    onOpenSession: ((String) -> Unit)? = null,
) {
    val suggested = remember(rows, suggestions != null) {
        if (suggestions != null) chatSuggestionSources(rows) else emptyList()
    }
    val docked = Dock.dockRows(suggested + sources)
    var openId by remember { mutableStateOf(initialOpenId) }
    val byId = remember(rows) { rows.associateBy { it.id } }
    DockCards(
        rows = docked,
        openId = openId,
        onOpenChange = { openId = it },
        modifier = modifier,
        action = action,
        initialExpanded = initialExpanded,
    ) { row, scope, close ->
        val chatRow = byId[row.id]
        if (chatRow is SessionTree.SessionChatRow.Suggestions) {
            ChatSuggestionsSheet(chatRow, suggestions, onOpenSession)
        } else {
            sessionSheet?.invoke(this, row, scope, close)
        }
    }
}

/** dock.json `suggestionSources` over a chat's rows. */
fun chatSuggestionSources(rows: List<SessionTree.SessionChatRow>): List<Dock.Source.Suggestions> =
    Dock.suggestionSources(
        rows.filterIsInstance<SessionTree.SessionChatRow.Suggestions>().map { row ->
            Dock.SuggestionRow(row.id, row.header, row.items.map { it.state.wire to it.title })
        },
    )

/** A `picture` card: the attachment through the markdown image path (auth,
 *  probed size), the caption muted under it. */
@Composable
fun ChatPictureRow(row: SessionTree.SessionChatRow.Picture) {
    Column(modifier = Modifier.fillMaxWidth().testTag("chat-picture"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        MarkdownView(markdown = "![${row.caption.orEmpty()}](/api/attachments/${row.attachmentId})")
        row.caption?.takeIf { it.isNotBlank() }?.let {
            Text(it, style = MaterialTheme.typography.labelSmall, color = Muted)
        }
    }
}

/** An `event` card (and a system message): #EXP-1772 THE inline marker
 *  ([FeedMarkerRow]: the centred muted label). #EXP-1464: a line about
 *  another session ([onClick] non-null) is a button with the trailing
 *  chevron that opens it; otherwise it stays plain. */
@Composable
fun ChatEventLine(text: String, onClick: (() -> Unit)? = null) {
    FeedMarkerRow(text, onClick = onClick, testTag = ChatRowAction.TestIds.EVENT)
}

/**
 * #EXP-1464 (chat-row-action.json `expand`): a Needs-you session row,
 * expanded in place = THE session card (chevron down; a tap collapses it)
 * over THE question card of that session, its label line carrying `Open chat`,
 * or #EXP-1662 THE plan row ([waiting]; null = the skeleton line while it
 * syncs). A sent answer or a dismissal marks the session answered
 * ([onAnswered]) and collapses.
 */
@Composable
private fun ExpandedSessionRow(
    card: SessionTree.SessionCard,
    waiting: SessionTree.SessionChatRow?,
    onCollapse: () -> Unit,
    onOpenChat: () -> Unit,
    onAnswer: ChatAnswer,
    onDecidePlan: ChatPlanDecision,
    onAnswered: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().testTag(ChatRowAction.TestIds.EXPANDED),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        SessionCard(card, onClick = onCollapse, chevron = true, expanded = true)
        if (waiting == null) {
            SkeletonRows(rows = 1, rowHeight = 20.dp, dot = false)
        } else {
            val dismiss = LocalCardDismiss.current
            CompositionLocalProvider(
                LocalAskCardTrailing provides { OpenChatLink(onOpenChat) },
                LocalCardDismiss provides dismiss?.let { host ->
                    { id: String, settled: (Boolean) -> Unit ->
                        host(id) { ok ->
                            settled(ok)
                            if (ok) onAnswered()
                        }
                    }
                },
            ) {
                ChatAskCard(
                    waiting,
                    onAnswer = { id, answer, settled ->
                        onAnswer(id, answer) { ok ->
                            settled(ok)
                            if (ok) onAnswered()
                        }
                    },
                    onDecidePlan = { id, decision, settled ->
                        onDecidePlan(id, decision) { ok ->
                            settled(ok)
                            if (ok) onAnswered()
                        }
                    },
                )
            }
        }
    }
}

/** #EXP-1464 `copy.openChat`: the expanded card's trailing text link,
 *  muted xs text; pressed = foreground. */
@Composable
private fun OpenChatLink(onClick: () -> Unit) {
    val interaction = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    Text(
        ChatRowAction.Copy.OPEN_CHAT,
        style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, fontWeight = FontWeight.Normal),
        color = if (pressed) MaterialTheme.colorScheme.onSurface else Muted,
        maxLines = 1,
        modifier = Modifier
            .clip(RoundedCornerShape(6.dp))
            .clickable(interactionSource = interaction, indication = null, role = Role.Button, onClick = onClick)
            .padding(horizontal = 4.dp, vertical = 2.dp)
            .testTag(ChatRowAction.TestIds.OPEN_CHAT),
    )
}

// ── EXP-1323: reports + suggested tasks ─────────────────────────────────────

/** A Start on… row: a runnable device. */
data class SuggestionDevice(val id: String, val name: String, val icon: ImageVector)

/**
 * EXP-1323: what a suggested tasks card can do (`sessions.startSuggestion` /
 * `startAllSuggestions` / `dismissSuggestion` / `fileSuggestion`). [models] =
 * contract `codingModel` (value → label), never a Default row; [pending] =
 * in-flight [suggestionKey]s.
 */
data class ChatSuggestionActions(
    val onStart: (rowId: String, itemId: String, deviceId: String?, model: String?) -> Unit,
    val onStartAll: (rowId: String) -> Unit,
    val onHere: (rowId: String, itemId: String) -> Unit,
    val onDismiss: (rowId: String, itemId: String) -> Unit,
    val onFile: (rowId: String, itemId: String) -> Unit,
    val devices: List<SuggestionDevice> = emptyList(),
    val models: List<Pair<String, String>> = emptyList(),
    val pending: Set<String> = emptySet(),
)

/** The pending key of one item (or, without an item, the card's Start all). */
fun suggestionKey(rowId: String, itemId: String? = null): String =
    if (itemId != null) "$rowId:$itemId" else "$rowId:*"

/**
 * EXP-1323: THE suggested tasks card ×4 = the Work card's shell: a header
 * (`3 suggested tasks`, `· 2 started`, Start all while more than one is open),
 * then hairline-divided items. N-12: while any item is open the row is a
 * DockCard and this list is its sheet ([ChatSuggestionsSheet]); in the
 * Conversation the card stays for the rows with no open item. Open = title,
 * text, the ⋯ menu and Start, one row on every width; started = the child's session card
 * (its caption under the name) opening it; here = `Sent here`; filed =
 * `Filed EXP-9`.
 */
@Composable
fun ChatSuggestionsCard(
    row: SessionTree.SessionChatRow.Suggestions,
    actions: ChatSuggestionActions?,
    onOpenSession: ((String) -> Unit)?,
) {
    var optionsFor by remember(row.id) { mutableStateOf<SessionTree.SuggestionRow?>(null) }
    Column(modifier = Modifier.fillMaxWidth().chatCard().testTag("chat-suggestions")) {
        Row(
            modifier = Modifier.fillMaxWidth().heightIn(min = WorkCardSpec.HEADER.dp).padding(horizontal = 12.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(ExpIcons.uiChecklist, contentDescription = null, tint = Muted, modifier = Modifier.size(14.dp))
            Text(
                row.header,
                style = chatCardTextStyle().copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).testTag("chat-suggestions-header"),
            )
            if (row.startAll && actions != null) {
                val key = suggestionKey(row.id)
                GlassPill(
                    label = SessionTree.SuggestionsCopy.START_ALL,
                    onClick = { actions.onStartAll(row.id) },
                    size = PillSize.Sm,
                    primary = true,
                    loading = key in actions.pending,
                    enabled = key !in actions.pending,
                    modifier = Modifier.testTag("chat-suggestions-start-all"),
                )
            }
        }
        row.items.forEach { item ->
            HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
            SuggestionItemRow(
                rowId = row.id,
                item = item,
                actions = actions,
                onOpenSession = onOpenSession,
                onOptions = { optionsFor = item },
            )
        }
    }
    val target = optionsFor
    if (target != null && actions != null) {
        SuggestionOptionsSheet(
            item = target,
            actions = actions,
            onPick = { pick ->
                optionsFor = null
                pick(row.id, target.id)
            },
            onDismiss = { optionsFor = null },
        )
    }
}

@Composable
private fun SuggestionItemRow(
    rowId: String,
    item: SessionTree.SuggestionRow,
    actions: ChatSuggestionActions?,
    onOpenSession: ((String) -> Unit)?,
    onOptions: () -> Unit,
) {
    val style = chatCardTextStyle()
    val tag = Modifier.testTag("chat-suggestion-${item.state.wire}")
    when (item.state) {
        SessionTree.SuggestionState.Started -> {
            val card = item.card
            if (card != null) {
                SessionCard(
                    card = card,
                    onClick = onOpenSession?.let { open -> { open(card.sessionId) } },
                    bare = true,
                    modifier = tag,
                )
            } else {
                // Started; the child's row has not synced yet.
                Row(
                    modifier = tag.fillMaxWidth().heightIn(min = WorkCardSpec.HEADER.dp).padding(horizontal = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    InlineSpinner(size = 12.dp)
                    Text(item.title, style = style, color = Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        SessionTree.SuggestionState.Here, SessionTree.SuggestionState.Filed -> Row(
            modifier = tag.fillMaxWidth().heightIn(min = WorkCardSpec.HEADER.dp).padding(horizontal = 12.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(item.title, style = style, color = Muted, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            Text(
                if (item.state == SessionTree.SuggestionState.Here) {
                    SessionTree.SuggestionsCopy.SENT_HERE
                } else {
                    SessionTree.SuggestionsCopy.FILED.replace("{issue}", item.identifier.orEmpty())
                },
                style = style,
                color = Muted,
                maxLines = 1,
            )
        }
        SessionTree.SuggestionState.Open -> {
            val busy = actions != null && suggestionKey(rowId, item.id) in actions.pending
            // N-12 ×4: ONE row on every width: the words, the ⋯ menu, Start.
            Row(
                modifier = tag.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        item.title,
                        style = style.copy(fontWeight = FontWeight.Medium),
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    if (item.text.isNotEmpty()) {
                        Spacer(Modifier.height(2.dp))
                        Text(item.text, style = style, color = Muted)
                    }
                }
                if (actions != null) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        CircleIconButton(
                            ExpIcons.uiMore,
                            SessionTree.SuggestionsCopy.OPTIONS,
                            onClick = onOptions,
                            enabled = !busy,
                            borderless = true,
                            size = 24.dp,
                            glyphSize = 14.dp,
                            modifier = Modifier.testTag("chat-suggestion-options"),
                        )
                        GlassPill(
                            label = SessionTree.SuggestionsCopy.START,
                            onClick = { actions.onStart(rowId, item.id, null, null) },
                            size = PillSize.Sm,
                            primary = true,
                            loading = busy,
                            enabled = !busy,
                            modifier = Modifier.testTag("chat-suggestion-start"),
                        )
                    }
                }
            }
        }
    }
}

/**
 * N-12: THE suggestions sheet (a DockCard opened, dock.json `suggestions`):
 * the items as the card lists them; the pinned footer = Start all while more
 * than one is open. Draw inside [DockCards]' sheet slot.
 */
@Composable
fun ColumnScope.ChatSuggestionsSheet(
    row: SessionTree.SessionChatRow.Suggestions,
    actions: ChatSuggestionActions?,
    onOpenSession: ((String) -> Unit)?,
) {
    var optionsFor by remember(row.id) { mutableStateOf<SessionTree.SuggestionRow?>(null) }
    // N-6: the Dock sheet is a sheet host: the ⋯ menu discloses inline.
    val inline = LocalPickerPresentation.current == PickerPresentation.Inline
    DockSheetBody {
        Column(modifier = Modifier.fillMaxWidth().fullBleed(12.dp).testTag("chat-suggestions-sheet")) {
            row.items.forEachIndexed { index, item ->
                if (index > 0) HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
                SuggestionItemRow(
                    rowId = row.id,
                    item = item,
                    actions = actions,
                    onOpenSession = onOpenSession,
                    onOptions = { optionsFor = if (optionsFor?.id == item.id) null else item },
                )
                if (inline && actions != null && optionsFor?.id == item.id) {
                    SuggestionOptionsInline(
                        actions = actions,
                        onPick = { pick ->
                            optionsFor = null
                            pick(row.id, item.id)
                        },
                    )
                }
            }
        }
    }
    if (row.startAll && actions != null) {
        DockSheetFooter {
            val key = suggestionKey(row.id)
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                GlassPill(
                    label = SessionTree.SuggestionsCopy.START_ALL,
                    onClick = { actions.onStartAll(row.id) },
                    size = PillSize.Md,
                    primary = true,
                    loading = key in actions.pending,
                    enabled = key !in actions.pending,
                    modifier = Modifier.testTag("chat-suggestions-start-all"),
                )
            }
        }
    }
    val target = optionsFor
    if (!inline && target != null && actions != null) {
        SuggestionOptionsSheet(
            item = target,
            actions = actions,
            onPick = { pick ->
                optionsFor = null
                pick(row.id, target.id)
            },
            onDismiss = { optionsFor = null },
        )
    }
}

/** What an Options pick does to (rowId, itemId). */
private typealias SuggestionPick = (rowId: String, itemId: String) -> Unit

/**
 * EXP-1323: an open item's ⋯ = THE menu as its phone sheet (the "+"'s
 * idiom): Do it here · Start on… › (the runnable devices) · Start with… ›
 * (the models, never a Default row) · File as issue · Dismiss (N-6: in the
 * Dock sheet the same rows open inline, [SuggestionOptionsInline]). A submenu row pushes its
 * page inside the sheet; back returns to the rows.
 */
@Composable
private fun SuggestionOptionsSheet(
    item: SessionTree.SuggestionRow,
    actions: ChatSuggestionActions,
    onPick: (SuggestionPick) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = item.title, onDismiss = onDismiss, modifier = Modifier.testTag("chat-suggestion-menu")) {
        SuggestionOptionsRows(actions = actions, onPick = onPick)
        Spacer(Modifier.height(8.dp))
    }
}

/**
 * N-6: the same ⋯ rows INLINE (the Dock sheet is a sheet host, so no second
 * sheet): disclosed right under the item's row, at the sheet's width; the ⋯
 * again folds them.
 */
@Composable
private fun SuggestionOptionsInline(
    actions: ChatSuggestionActions,
    onPick: (SuggestionPick) -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(max = PickerDefaults.InlineMaxHeight)
            .verticalScroll(rememberScrollState())
            .padding(bottom = 6.dp)
            .testTag("chat-suggestion-menu"),
    ) {
        SuggestionOptionsRows(actions = actions, onPick = onPick)
    }
}

/** THE ⋯ rows and their Start on… / Start with… pages, either presentation. */
@Composable
private fun SuggestionOptionsRows(
    actions: ChatSuggestionActions,
    onPick: (SuggestionPick) -> Unit,
) {
    var page by remember { mutableStateOf<String?>(null) }
    val iconTint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    when (page) {
        PAGE_DEVICE -> {
            BackHandler { page = null }
            GlassSheetRow(
                label = SessionTree.SuggestionsCopy.START_ON,
                onClick = { page = null },
                leading = { Icon(ExpIcons.uiBack, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
            )
            actions.devices.forEach { device ->
                GlassSheetRow(
                    label = device.name,
                    onClick = { onPick { rowId, itemId -> actions.onStart(rowId, itemId, device.id, null) } },
                    leading = { Icon(device.icon, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
                )
            }
        }
        PAGE_MODEL -> {
            BackHandler { page = null }
            GlassSheetRow(
                label = SessionTree.SuggestionsCopy.START_WITH,
                onClick = { page = null },
                leading = { Icon(ExpIcons.uiBack, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
            )
            actions.models.forEach { (value, label) ->
                GlassSheetRow(
                    label = label,
                    onClick = { onPick { rowId, itemId -> actions.onStart(rowId, itemId, null, value) } },
                )
            }
        }
        else -> {
            val chevron: @Composable RowScope.() -> Unit = {
                Icon(ExpIcons.uiChevronRight, contentDescription = null, tint = iconTint, modifier = Modifier.size(16.dp))
            }
            GlassSheetRow(
                label = SessionTree.SuggestionsCopy.HERE,
                onClick = { onPick(actions.onHere) },
            )
            GlassSheetRow(
                label = SessionTree.SuggestionsCopy.START_ON,
                onClick = { page = PAGE_DEVICE },
                enabled = actions.devices.isNotEmpty(),
                trailing = chevron,
            )
            GlassSheetRow(
                label = SessionTree.SuggestionsCopy.START_WITH,
                onClick = { page = PAGE_MODEL },
                enabled = actions.models.isNotEmpty(),
                trailing = chevron,
            )
            HorizontalDivider(
                thickness = GlassTokens.Hairline,
                color = GlassTokens.StrokeRow,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
            )
            GlassSheetRow(
                label = SessionTree.SuggestionsCopy.FILE,
                onClick = { onPick(actions.onFile) },
            )
            GlassSheetRow(
                label = SessionTree.SuggestionsCopy.DISMISS,
                onClick = { onPick(actions.onDismiss) },
            )
        }
    }
}

private const val PAGE_DEVICE = "device"
private const val PAGE_MODEL = "model"
