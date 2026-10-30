package com.exponential.app.ui.thread

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
import com.exponential.app.ui.components.SessionXref
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
import com.exponential.app.ui.components.TranscriptDivider
import com.exponential.app.ui.theme.liveArrival
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.snapshotFlow
import com.exponential.app.domain.Chats
import androidx.compose.foundation.interaction.collectIsPressedAsState
import com.exponential.app.domain.ThreadRowAction
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.session.LocalAskCardTrailing
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.QuestionOption
import com.exponential.app.domain.withFreeTextRow
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
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
    const val SEND = "Send"
    const val ADD = "Add"
    const val PRIVATE = "Private"

    fun unlink(identifier: String) = "Unlink $identifier"

    /** An agent channel's composer: `Message <name>`. */
    fun messageAgent(name: String) = "Message $name"
}

/** A tab of the strip as its saveable page key. */
/** The Stories tab's page key (`?tab=stories` on web). */
const val STORIES_TAB_KEY = "stories"

fun threadTabKey(tab: SessionTree.ThreadTab): String = when (tab) {
    SessionTree.ThreadTab.Conversation -> "conversation"
    is SessionTree.ThreadTab.IssueTab -> "issue:${tab.issueId}"
    SessionTree.ThreadTab.Add -> "add"
    is SessionTree.ThreadTab.SessionTab -> "session:${tab.sessionId}"
    SessionTree.ThreadTab.Stories -> STORIES_TAB_KEY
}

/** A `question` answer: (message id, the answer, settled(ok)); EXP-1280
 *  [AskAnswer] carries the picked labels + the surface's data model. */
typealias ThreadAnswer = (messageId: String, answer: AskAnswer, settled: (Boolean) -> Unit) -> Unit

/** A `plan` decision: (message id, the decision, settled(ok)); [PlanAnswer]
 *  = approved + fresh ("Yes, and start with a fresh context", ask-dock.json)
 *  + feedback + EXP-1280 the questions' picks and the surface's data model. */
typealias ThreadPlanDecision = (messageId: String, decision: PlanAnswer, settled: (Boolean) -> Unit) -> Unit

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

/** An agent channel's header: back, the agent's icon (its colour) + name.
 *  NEW-41: no settings gear (web/desktop/iOS reach an agent's settings
 *  through Settings → Agents only). */
@Composable
fun AgentChannelHeader(
    name: String,
    icon: String?,
    color: String?,
    onBack: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth().testTag("thread-header")) {
        Row(
            modifier = Modifier.fillMaxWidth().height(48.dp).padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) TopBarBackButton(onClick = onBack)
            Spacer(Modifier.weight(1f))
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

/** The page key of an issue / a session (the strip's open item). */
internal fun issueKey(issueId: String) = "issue:$issueId"
internal fun sessionKey(sessionId: String) = "session:$sessionId"

/** #EXP-1433: one issue of the strip (a top-level chip or a nested one). */
data class StripIssue(val issueId: String, val identifier: String)

/** #EXP-1433: one session under the root, as THE tree row ([depth] for the guides). */
data class StripSession(
    val sessionId: String,
    val name: String,
    val markState: SessionTree.SessionState,
    val depth: Int,
)

/**
 * #EXP-1433 (session-thread.json STRIP): what sits between Conversation and
 * Stories. [issues] = EVERY issue of the tree (nested ones included),
 * [sessions] = every session under the root (the main session excluded);
 * [mode] = [SessionTree.threadStripMode] over their counts.
 */
data class ThreadStripItems(val issues: List<StripIssue>, val sessions: List<StripSession>) {
    val mode: SessionTree.ThreadStripMode = SessionTree.threadStripMode(issues.size, sessions.size)

    /** The chips mode's ONE issue (it keeps its chip and its page). */
    val single: StripIssue? get() = if (mode.mode == SessionTree.StripMode.Chips) issues.firstOrNull() else null

    /** The group item a page [key] names (null = none of the group). */
    fun tabOf(key: String): SessionTree.ThreadTab? {
        issues.firstOrNull { issueKey(it.issueId) == key }?.let {
            return SessionTree.ThreadTab.IssueTab(it.issueId, it.identifier)
        }
        sessions.firstOrNull { sessionKey(it.sessionId) == key }?.let {
            return SessionTree.ThreadTab.SessionTab(it.sessionId, it.name, it.markState)
        }
        return null
    }

    /** The group tab's open label: the identifier, or the session's name. */
    fun label(key: String): String? =
        issues.firstOrNull { issueKey(it.issueId) == key }?.identifier
            ?: sessions.firstOrNull { sessionKey(it.sessionId) == key }?.name
}

/** #EXP-1433: the strip's items out of the thread's tabs (the chips with
 *  their folded rows, flattened in strip order). */
fun threadStripItems(tabs: List<SessionTree.ThreadTab>, mainSessionId: String?): ThreadStripItems {
    val issues = tabs.filterIsInstance<SessionTree.ThreadTab.IssueTab>().flatMap { tab ->
        listOf(StripIssue(tab.issueId, tab.identifier)) + tab.nested.map { StripIssue(it.issueId, it.identifier) }
    }.distinctBy { it.issueId }
    val sessions = tabs.filterIsInstance<SessionTree.ThreadTab.SessionTab>().flatMap { tab ->
        listOf(StripSession(tab.sessionId, tab.name, tab.markState, 0)) +
            tab.nested.map { StripSession(it.sessionId, it.name, it.markState, it.depth) }
    }.filter { it.sessionId != mainSessionId }.distinctBy { it.sessionId }
    return ThreadStripItems(issues, sessions)
}

/**
 * #EXP-1433 (session-thread.json STRIP, amends EXP-1334): the ONE strip, and
 * NOTHING scrolls — "+" (Link an issue · New issue) and Conversation pinned
 * left, Stories pinned right; between them [SessionTree.threadStripMode]: at
 * most ONE issue and no session = that issue's chip (the pager: its
 * identifier; a long-press opens its menu), anything more = ONE group tab
 * (the `session-tree` glyph + the muted count; active = the glyph + the open
 * item's label) opening THE bottom sheet over every issue and session. A
 * pick opens that issue / session IN PLACE ([onSelect] / [onOpenSession]).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ThreadTabStrip(
    tabs: List<SessionTree.ThreadTab>,
    selectedKey: String,
    onSelect: (key: String) -> Unit,
    onAdd: (() -> Unit)?,
    issueStatus: (String) -> ResolvedIssueStatus?,
    modifier: Modifier = Modifier,
    /** An issue's title (the group sheet's rows, the chip's accessible name). */
    issueTitle: (String) -> String? = { null },
    onIssueMenu: ((SessionTree.ThreadTab.IssueTab) -> Unit)? = null,
    onOpenSession: ((sessionId: String) -> Unit)? = null,
    /** stories.json STORIES TAB: the tab's dot (`storyTab().unseen`). */
    storiesDot: Boolean = false,
    /** NEW-46: the single-session thread's main session (never a group row). */
    mainSessionId: String? = null,
    /** The styleguide's static paint: the group sheet held open. */
    groupSheetOpen: Boolean = false,
) {
    var sheet by remember { mutableStateOf(groupSheetOpen) }
    val items = remember(tabs, mainSessionId) { threadStripItems(tabs, mainSessionId) }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(start = 12.dp, end = 12.dp, bottom = 8.dp)
            .testTag(SessionTree.ThreadCopy.TEST_ID_TABS),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(StripGap),
    ) {
        if (SessionTree.ThreadTab.Add in tabs && onAdd != null) {
            IconButton(onClick = onAdd, modifier = Modifier.size(ThreadTabHeight).testTag("thread-tab-add")) {
                Icon(ExpIcons.uiAdd, contentDescription = ThreadUiCopy.ADD, tint = Secondary, modifier = Modifier.size(16.dp))
            }
        }
        // Phones: Conversation is an ICON-ONLY tab (session-thread.json:
        // action-chat; the label stays the accessible name).
        val conversationSelected = selectedKey == "conversation"
        ThreadTab(conversationSelected, { onSelect("conversation") }, null, "thread-tab-conversation") {
            Icon(
                ExpIcons.actionChat,
                contentDescription = SessionTree.ThreadCopy.CONVERSATION,
                tint = if (conversationSelected) MaterialTheme.colorScheme.onSurface else Secondary,
                modifier = Modifier.size(16.dp),
            )
        }
        Row(
            modifier = Modifier.weight(1f).clipToBounds().testTag("thread-strip-zone"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val single = items.single
            when {
                single != null -> {
                    val tab = tabs.filterIsInstance<SessionTree.ThreadTab.IssueTab>().firstOrNull { it.issueId == single.issueId }
                        ?: SessionTree.ThreadTab.IssueTab(single.issueId, single.identifier)
                    IssueStripChip(
                        tab = tab,
                        selected = selectedKey == issueKey(single.issueId),
                        title = issueTitle(single.issueId),
                        status = issueStatus(single.issueId),
                        onClick = { onSelect(issueKey(single.issueId)) },
                        onLongClick = onIssueMenu?.let { menu -> { menu(tab) } },
                    )
                }
                items.mode.mode == SessionTree.StripMode.Group -> ThreadStripGroupTab(
                    count = items.mode.count,
                    openLabel = items.label(selectedKey),
                    onClick = { sheet = true },
                )
            }
        }
        // stories.json STORIES TAB: pinned right, after the last chip;
        // phones ICON-ONLY (`thread-stories`), the dot on the icon's corner
        // while a story waits or is unseen.
        if (SessionTree.ThreadTab.Stories in tabs) {
            val storiesSelected = selectedKey == STORIES_TAB_KEY
            ThreadTab(storiesSelected, { onSelect(STORIES_TAB_KEY) }, null, Stories.Tab.TestIds.TAB) {
                Box {
                    Icon(
                        ExpIcons.threadStories,
                        contentDescription = Stories.Tab.Copy.LABEL,
                        tint = if (storiesSelected) MaterialTheme.colorScheme.onSurface else Secondary,
                        modifier = Modifier.size(16.dp),
                    )
                    if (storiesDot) {
                        Box(
                            Modifier
                                .align(Alignment.TopEnd)
                                .offset(x = 3.dp, y = (-3).dp)
                                .size(6.dp)
                                .background(LiveGreen, CircleShape)
                                .testTag("thread-stories-dot"),
                        )
                    }
                }
            }
        }
    }

    if (sheet && items.mode.mode == SessionTree.StripMode.Group) {
        ThreadStripGroupSheet(
            items = items,
            selectedKey = selectedKey,
            issueStatus = issueStatus,
            issueTitle = issueTitle,
            onPickIssue = { id ->
                sheet = false
                onSelect(issueKey(id))
            },
            onPickSession = { id ->
                sheet = false
                onOpenSession?.invoke(id)
            },
            onDismiss = { sheet = false },
        )
    }
}

/** The strip's one gap. */
private val StripGap = 4.dp

/** #EXP-1433: THE group tab — the strip chip box, the `session-tree` glyph
 *  and the count (muted) at rest; ACTIVE with the open item's label
 *  (truncated at `stripGroupMaxWidth`) while one of its items is open. */
@Composable
private fun ThreadStripGroupTab(count: Int, openLabel: String?, onClick: () -> Unit) {
    val active = openLabel != null
    val name = SessionTree.ThreadCopy.STRIP_GROUP
    ThreadTab(
        selected = active,
        onClick = onClick,
        onLongClick = null,
        testTag = SessionTree.ThreadCopy.TEST_ID_GROUP,
        chip = true,
        modifier = Modifier.semantics {
            contentDescription = if (active) "$name: $openLabel" else "$name ($count)"
        },
    ) {
        Icon(
            ExpIcons.sessionTree,
            contentDescription = null,
            tint = if (active) MaterialTheme.colorScheme.onSurface else Secondary,
            modifier = Modifier.size(14.dp),
        )
        Spacer(Modifier.width(6.dp))
        if (openLabel != null) {
            Text(
                openLabel,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.widthIn(max = SessionTree.ThreadCopy.STRIP_GROUP_MAX_WIDTH.dp)
                    .testTag("thread-strip-group-label"),
            )
        } else {
            Text(
                "$count",
                style = MaterialTheme.typography.labelMedium,
                color = Muted,
                maxLines = 1,
                softWrap = false,
                modifier = Modifier.testTag("thread-strip-group-count"),
            )
        }
    }
}

/** An issue chip = the strip's issue chip box (×4): the status glyph · the
 *  mono identifier (phones: the identifier alone; [title] is its accessible
 *  name's tail). */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun IssueStripChip(
    tab: SessionTree.ThreadTab.IssueTab,
    selected: Boolean,
    title: String?,
    status: ResolvedIssueStatus?,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)?,
) {
    ThreadTab(
        selected = selected,
        onClick = onClick,
        onLongClick = onLongClick,
        testTag = "thread-tab-issue:${tab.issueId}",
        chip = true,
        modifier = Modifier.semantics {
            contentDescription = listOfNotNull(tab.identifier, title).joinToString(" ")
        },
    ) {
        // An unsynced issue (another team's) is the one chip without a glyph.
        status?.let {
            StatusIcon(it, size = 13.dp)
            Spacer(Modifier.width(5.dp))
        }
        Text(
            tab.identifier,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = if (selected) MaterialTheme.colorScheme.onSurface else MdStyle.ChipToken,
            maxLines = 1,
            softWrap = false,
        )
    }
}

/**
 * #EXP-1433: the group tab's bottom sheet — the issue rows first (status
 * glyph, mono identifier, title: the issue search list's row), a separator,
 * then the session rows (THE tree rows over [TreeGuides.compute]: the run
 * mark, the name and its state word). The open row wears the check.
 */
@Composable
private fun ThreadStripGroupSheet(
    items: ThreadStripItems,
    selectedKey: String,
    issueStatus: (String) -> ResolvedIssueStatus?,
    issueTitle: (String) -> String?,
    onPickIssue: (String) -> Unit,
    onPickSession: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val guides = remember(items.sessions) {
        com.exponential.app.domain.TreeGuides.compute(items.sessions.map { it.depth })
    }
    GlassSheet(
        title = SessionTree.ThreadCopy.STRIP_GROUP,
        onDismiss = onDismiss,
        modifier = Modifier.testTag("${SessionTree.ThreadCopy.TEST_ID_GROUP}-menu"),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .testTag("thread-strip-group-rows"),
        ) {
            items.issues.forEach { issue ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 44.dp)
                        .clickable { onPickIssue(issue.issueId) }
                        .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
                        .testTag("thread-strip-group-issue"),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    issueStatus(issue.issueId)?.let { StatusIcon(it, size = 16.dp) }
                    Text(
                        issue.identifier,
                        style = MaterialTheme.typography.bodyMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                    )
                    Text(
                        issueTitle(issue.issueId).orEmpty(),
                        style = MaterialTheme.typography.bodyMedium,
                        color = Secondary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    if (selectedKey == issueKey(issue.issueId)) {
                        Icon(ExpIcons.uiCheck, contentDescription = null, tint = Secondary, modifier = Modifier.size(16.dp))
                    }
                }
            }
            if (items.issues.isNotEmpty() && items.sessions.isNotEmpty()) {
                HorizontalDivider(
                    thickness = GlassTokens.Hairline,
                    color = GlassTokens.StrokeRow,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                )
            }
            items.sessions.forEachIndexed { index, session ->
                com.exponential.app.ui.components.TreeGuidesRow(depth = session.depth, guide = guides.getOrNull(index)) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 44.dp)
                            .clickable { onPickSession(session.sessionId) }
                            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
                            .testTag("thread-strip-group-session"),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        SessionTabMark(session.markState)
                        Text(
                            session.name,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        Text(
                            SessionTree.CardCopy.STATES.getValue(session.markState),
                            style = MaterialTheme.typography.bodyMedium,
                            color = sessionToneColor(SessionTree.CardCopy.TONES.getValue(session.markState)),
                            maxLines = 1,
                        )
                        Spacer(Modifier.weight(1f))
                        if (selectedKey == sessionKey(session.sessionId)) {
                            Icon(ExpIcons.uiCheck, contentDescription = null, tint = Secondary, modifier = Modifier.size(16.dp))
                        }
                    }
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

/** EXP-1316: every chip of the strip is ONE height — entity-chip `tab` size
 *  (web `STRIP_TAB_CLASS`: `h-8 rounded-md` = 32dp tall, the 10dp row radius). */
private val ThreadTabHeight = 32.dp

/** A session chip's lead: the thread row mark of that session. */
@Composable
private fun SessionTabMark(state: SessionTree.SessionState, size: Dp = 14.dp) {
    // NEW-41 (`sessionTabMarkState` ×4): a session chip's lead = that
    // session's thread row mark (web `AgentRunMark`, iOS
    // `ThreadSessionChipMark`): the brand mark, the amber badge while it
    // needs you, the working mark while it works, dimmed once ended. The
    // chip names no CLI, so the default brand mark leads (iOS `agent: nil`).
    AgentRunMark(agent = DEFAULT_AGENT, state = state, size = size, badgeSize = 6.dp)
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
    /** The tab's own semantics (an accessible name over its content). */
    modifier: Modifier = Modifier,
    content: @Composable RowScope.() -> Unit,
) {
    val shape = RoundedCornerShape(10.dp)
    Row(
        modifier = modifier
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
    /** #EXP-1464 (thread-row-action.json): the trailing chevron of a row
     *  that does something; [expanded] turns it down. */
    chevron: Boolean = false,
    expanded: Boolean = false,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .then(if (bare) Modifier else Modifier.threadCard())
            .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .testTag(card.testId),
    ) {
        SessionCardHeader(card, chevron, expanded)
    }
}

@Composable
private fun SessionCardHeader(card: SessionTree.SessionCard, chevron: Boolean, expanded: Boolean) {
    val style = threadCardTextStyle()
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
data class ThreadRowMeta(
    val authorName: String,
    val time: String,
    val user: UserEntity? = null,
)

/** The agent a channel belongs to (its rows' avatar + name). */
data class ThreadAgent(val name: String, val icon: String?, val color: String?, val assistant: Boolean = false)

/**
 * #EXP-1464 (thread-row-action.json): what an EXPANDING session row needs
 * from its host: whether a flag stands behind a session's `needs_you`
 * ([blocked]: it opens instead) and THE waiting question / plan card of a
 * session ([waitingCard], null while it has not synced = the skeleton line).
 * Without them a Conversation's session rows only open.
 */
class ThreadRowActions(
    val blocked: (sessionId: String) -> Boolean,
    val waitingCard: @Composable (sessionId: String) -> SessionTree.ThreadRow?,
)

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
    /** EXP-1323: a report line / a started suggestion opens that session. */
    onOpenSession: ((sessionId: String) -> Unit)? = null,
    /** EXP-1323: the suggested tasks cards' actions (null = read-only). */
    suggestions: ThreadSuggestionActions? = null,
    /** EXP-1323: the live tail after the last row (the Tasks card + the
     *  status line), the host's; null = none. */
    tail: (@Composable () -> Unit)? = null,
    /** THE ask dock: the docked rows' ids. Thread 58cb32df (FEED-140): a
     *  WAITING plan or question card stays inline at its place WITH its
     *  actions besides the dock; only the suggestion rows leave. */
    dockedIds: Set<String> = emptySet(),
    /** Thread 58cb32df: a surface card's `reply` posts its text here (the
     *  person's next message); null = inert. */
    onSurfaceReply: ((String) -> Unit)? = null,
    /** S-3: the view's `origin` (`Started by` / `Reported to`), the
     *  Conversation's first line; null = a person started it. */
    origin: SessionTree.SessionXref? = null,
    /** S-3: an inter-session link PUSHES that session (null = inert). */
    onOpenXref: ((sessionId: String) -> Unit)? = null,
    /** #EXP-1464: a Needs-you session row expands into its card (null = opens). */
    rowActions: ThreadRowActions? = null,
) {
    // #EXP-1464 (thread-row-action.json): the expanded session rows (by row
    // id) and the sessions the viewer answered from one (client-held).
    var expandedRows by rememberSaveable(rootId) { mutableStateOf(listOf<String>()) }
    var answeredSessions by rememberSaveable(rootId) { mutableStateOf(listOf<String>()) }
    val openRowSession: ((String) -> Unit)? = onOpenXref ?: onOpenSession
    // EXP-1334: no root card any more (the conversation IS the dispatcher).
    val shown = if (dockedIds.isEmpty()) {
        rows
    } else {
        rows.filter {
            it.id !in dockedIds || it is SessionTree.ThreadRow.Plan || it is SessionTree.ThreadRow.Question
        }
    }
    // chats.json: a row ARRIVES when its id was not in the transcript the
    // screen first painted (history, a reopen or a scroll never animate).
    val arrivals = rememberLiveArrivals(rootId, shown)
    // chats.json FOLLOW + JUMP (#EXP-1383): the chat opens at the newest row
    // and follows new rows while the reader sits within `followSlack` of the
    // bottom; a drag further up stops following and shows THE jump-to-bottom
    // button. Only user drags flip follow-mode.
    var follow by rememberSaveable(rootId) { mutableStateOf(true) }
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
    LaunchedEffect(state, followSlackPx) {
        snapshotFlow { state.isScrollInProgress to state.isNearBottom(followSlackPx) }
            .distinctUntilChanged()
            .collect { (dragging, nearBottom) -> if (dragging) follow = nearBottom }
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
            if (origin != null) {
                item(key = "__origin__") { SessionXref(origin, onOpenXref) }
            }
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
                Box(Modifier.liveArrival(arrivals(row.id))) {
                when (row) {
                    // Thread 58cb32df (chats.json TRANSCRIPT): phones paint the
                    // conversation as CHAT BUBBLES, a thread and an agent's
                    // chat alike: the person's on the right, the agent's in a
                    // card-fill bubble on the left, each with its time.
                    is SessionTree.ThreadRow.MessageRow -> {
                        val rowMeta = meta(row)
                        // S-3: a row from ANOTHER session = its link header
                        // right above the bubble, on the bubble's side.
                        Column(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalAlignment = if (row.author == "user") Alignment.End else Alignment.Start,
                            verticalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            row.xref?.let { SessionXref(it, onOpenXref) }
                            when (row.author) {
                                "user", "agent" -> ChatBubble(
                                    text = row.body,
                                    fromPerson = row.author == "user",
                                    time = rowMeta?.time,
                                    queued = row.queued,
                                    onUnqueue = if (row.queued && canUnqueue(row)) ({ onUnqueue(row.id) }) else null,
                                )
                                else -> ThreadEventLine(row.body)
                            }
                        }
                    }
                    is SessionTree.ThreadRow.Divider -> TranscriptDivider(row.kind, clockTime(row.at))
                    is SessionTree.ThreadRow.Surface -> AgentColumn {
                        SurfaceCard(
                            row = row,
                            animate = arrivals(row.id),
                            onReply = { text -> onSurfaceReply?.invoke(text) },
                        )
                    }
                    is SessionTree.ThreadRow.SessionRow -> {
                        val card = row.card
                        val action = ThreadRowAction.threadRowAction(
                            type = "session",
                            sessionId = card.sessionId,
                            rootId = rootId,
                            state = card.state,
                            blocked = rowActions?.blocked?.invoke(card.sessionId) ?: false,
                            // No waiting card to draw = it opens.
                            inline = rowActions == null || ThreadRowAction.waitingCardInline(rows, card.sessionId),
                            answered = card.sessionId in answeredSessions,
                        )
                        val shownCard = action.kindWord?.let { word ->
                            card.copy(kindWord = word, tone = action.tone ?: card.tone)
                        } ?: card
                        when (action.action) {
                            ThreadRowAction.Kind.Expand -> {
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
                            ThreadRowAction.Kind.Open ->
                                SessionCard(shownCard, onClick = { onOpenCard(card) }, chevron = true)
                            ThreadRowAction.Kind.None -> SessionCard(shownCard, onClick = null)
                        }
                    }
                    is SessionTree.ThreadRow.Question -> ThreadAskCard(row, onAnswer, onDecidePlan)
                    is SessionTree.ThreadRow.Plan -> ThreadAskCard(row, onAnswer, onDecidePlan)
                    is SessionTree.ThreadRow.Subagent -> WorkCardView(
                        card = WorkCards.of(WorkCardInput.Subagent(title = row.title, state = row.status)),
                    )
                    is SessionTree.ThreadRow.Picture -> ThreadPictureRow(row)
                    is SessionTree.ThreadRow.Event -> {
                        val action = ThreadRowAction.threadRowAction(
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
                        ThreadEventLine(
                            row.text,
                            onClick = if (action.action != ThreadRowAction.Kind.None && open != null && target != null) {
                                { open(target) }
                            } else {
                                null
                            },
                        )
                    }
                    // S-3: a report renders AS its `Report from <chip> ›` line.
                    is SessionTree.ThreadRow.Report -> {
                        val action = ThreadRowAction.threadRowAction(
                            type = "report",
                            sessionId = row.sessionId,
                            rootId = rootId,
                            state = null,
                            blocked = false,
                            inline = false,
                            answered = false,
                        )
                        SessionXref(row.xref, onOpenXref?.takeIf { action.action != ThreadRowAction.Kind.None })
                    }
                    is SessionTree.ThreadRow.Suggestions -> ThreadSuggestionsCard(
                        row = row,
                        actions = suggestions,
                        onOpenSession = onOpenSession,
                    )
                }
                }
            }
            if (tail != null) {
                item(key = "__live_tail__") { tail() }
            }
        }
        JumpToBottomButton(
            visible = !follow && shown.isNotEmpty(),
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
internal fun transcriptWaitingIndexes(rows: List<SessionTree.ThreadRow>): List<Int> =
    rows.indices.filter { i ->
        when (val row = rows[i]) {
            is SessionTree.ThreadRow.Plan -> row.decision == null
            is SessionTree.ThreadRow.Question -> !row.answered
            else -> false
        }
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

/**
 * EXP-1316 ONE card ×4, EXP-1280 the RICH card (plan-card.json): a
 * `question` or `plan` row is [PlanAskCard] over the row's data, answered
 * through `sessions.answer` (the picks + the surface's data model). An
 * answered row renders closed; the card locks while the answer is in flight
 * and stays locked until the synced row flips.
 */
@Composable
fun ThreadAskCard(row: SessionTree.ThreadRow, onAnswer: ThreadAnswer, onDecidePlan: ThreadPlanDecision) {
    // FEED-140: a plan the agent replaced with a newer one paints CLOSED and
    // collapsed to `copy.superseded` (never in the dock).
    if (row is SessionTree.ThreadRow.Plan && row.decision == AskDock.DECISION_SUPERSEDED) {
        SupersededPlanLine()
        return
    }
    Column(Modifier.testTag(if (row is SessionTree.ThreadRow.Plan) "thread-plan-card" else "thread-question-card")) {
        PlanAskCard(row, onAnswer, onDecidePlan)
    }
}

/** FEED-140: a superseded plan card, closed and collapsed to one line. */
@Composable
fun SupersededPlanLine(modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(GlassTokens.CardRadius)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(GlassTokens.CardFill, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .testTag("thread-plan-superseded"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(ExpIcons.codingPlan, contentDescription = null, tint = Muted, modifier = Modifier.size(14.dp))
        Text(
            AskDock.Copy.SUPERSEDED,
            style = MaterialTheme.typography.bodySmall,
            color = Muted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * Thread 58cb32df (chats.json TRANSCRIPT): ONE chat bubble. The person's on
 * the right (max 80% of the column, the glass fill under the strong
 * hairline), the agent's on the left on THE card surface (the opaque card
 * fill + card hairline, N-12 ONE gray; max 94%, its
 * top-leading corner 6dp); each carries its time bottom-right. A person's
 * queued message reads `queued` with the revoke ✕ (mine only).
 */
@Composable
fun ChatBubble(
    text: String,
    fromPerson: Boolean,
    time: String?,
    modifier: Modifier = Modifier,
    queued: Boolean = false,
    onUnqueue: (() -> Unit)? = null,
) {
    BoxWithConstraints(modifier = modifier.fillMaxWidth()) {
        val max = maxWidth * (if (fromPerson) 0.8f else 0.94f)
        val shape = if (fromPerson) {
            RoundedCornerShape(18.dp)
        } else {
            RoundedCornerShape(topStart = 6.dp, topEnd = 18.dp, bottomEnd = 18.dp, bottomStart = 18.dp)
        }
        Column(
            modifier = Modifier
                .align(if (fromPerson) Alignment.CenterEnd else Alignment.CenterStart)
                .widthIn(max = max)
                .clip(shape)
                .background(if (fromPerson) GlassTokens.RowFillActive else GlassTokens.OpaqueCardFill, shape)
                .border(GlassTokens.Hairline, if (fromPerson) GlassTokens.StrokeStrong else GlassTokens.StrokeCard, shape)
                .padding(start = 14.dp, end = 14.dp, top = 10.dp, bottom = 8.dp)
                .testTag(if (fromPerson) "thread-message-bubble" else "thread-agent-message"),
        ) {
            MarkdownView(markdown = text, softBreaksAsNewlines = true)
            val caption = listOfNotNull(
                time?.takeIf { it.isNotEmpty() },
                ThreadUiCopy.QUEUED.takeIf { queued },
            ).joinToString(" · ")
            if (caption.isNotEmpty() || onUnqueue != null) {
                Row(
                    modifier = Modifier.align(Alignment.End).padding(top = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        caption,
                        style = MaterialTheme.typography.labelSmall,
                        color = Muted,
                        maxLines = 1,
                        modifier = Modifier.testTag(if (queued) "thread-message-queued" else "chat-bubble-time"),
                    )
                    if (onUnqueue != null) UnqueueButton(onUnqueue)
                }
            }
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
 * Thread 58cb32df: which rows ARRIVED while the chat was open — every id not
 * in the first non-empty transcript painted for [key] (a reopen = a new
 * composition = nothing animates).
 */
@Composable
fun rememberLiveArrivals(key: String, rows: List<SessionTree.ThreadRow>): (String) -> Boolean {
    val initial = remember(key) { mutableStateOf<Set<String>?>(null) }
    if (initial.value == null && rows.isNotEmpty()) initial.value = rows.mapTo(HashSet()) { it.id }
    val seen = initial.value
    return { id -> seen != null && id !in seen }
}

/**
 * N-12: THE Dock in a thread (dock.json; web `ThreadDock`): the host's
 * [sources] (the root's Queue, queued messages, banners) plus the
 * suggested-tasks rows with an open item (only with [suggestions]; null =
 * read-only, the rows stay in the Conversation) as DockCards above the
 * composer; a suggestions card opens [ThreadSuggestionsSheet], the rest
 * through [sessionSheet]. S-3 STORIES: a waiting plan or question is a story
 * now, never a DockCard (it stays inline in the Conversation, answerable),
 * so nothing opens itself. [initialOpenId] / [initialExpanded] seed the
 * styleguide's states.
 */
@Composable
fun ThreadDock(
    rows: List<SessionTree.ThreadRow>,
    modifier: Modifier = Modifier,
    sources: List<Dock.Source> = emptyList(),
    sessionSheet: (@Composable ColumnScope.(Dock.Row, DockSheetScope, () -> Unit) -> Unit)? = null,
    action: (@Composable (Dock.Row) -> Unit)? = null,
    initialOpenId: String? = null,
    initialExpanded: Boolean = false,
    suggestions: ThreadSuggestionActions? = null,
    onOpenSession: ((String) -> Unit)? = null,
) {
    val suggested = remember(rows, suggestions != null) {
        if (suggestions != null) threadSuggestionSources(rows) else emptyList()
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
        val threadRow = byId[row.id]
        if (threadRow is SessionTree.ThreadRow.Suggestions) {
            ThreadSuggestionsSheet(threadRow, suggestions, onOpenSession)
        } else {
            sessionSheet?.invoke(this, row, scope, close)
        }
    }
}

/** dock.json `suggestionSources` over a thread's rows. */
fun threadSuggestionSources(rows: List<SessionTree.ThreadRow>): List<Dock.Source.Suggestions> =
    Dock.suggestionSources(
        rows.filterIsInstance<SessionTree.ThreadRow.Suggestions>().map { row ->
            Dock.SuggestionRow(row.id, row.header, row.items.map { it.state.wire to it.title })
        },
    )

/** N-12: the Conversation rows THE Dock holds (they leave the transcript):
 *  the suggestion rows with an open item (web `threadDockedIds`). S-3: the
 *  waiting asks stay inline (stories.json). */
fun threadDockedIds(rows: List<SessionTree.ThreadRow>): Set<String> =
    threadSuggestionSources(rows).mapTo(mutableSetOf()) { it.id }

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

/** An `event` card (and a system message): ONE quiet line. #EXP-1464: a
 *  line about another session ([onClick] non-null) becomes a button with the
 *  trailing chevron and the pressed wash; otherwise it stays plain. */
@Composable
fun ThreadEventLine(text: String, onClick: (() -> Unit)? = null) {
    if (onClick == null) {
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            color = Muted,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp).testTag(ThreadRowAction.TestIds.EVENT),
        )
        return
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(6.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 4.dp, vertical = 2.dp)
            .testTag(ThreadRowAction.TestIds.EVENT),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            color = Muted,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        Icon(ExpIcons.uiChevronRight, contentDescription = null, tint = Muted, modifier = Modifier.size(12.dp))
    }
}

/**
 * #EXP-1464 (thread-row-action.json `expand`): a Needs-you session row,
 * expanded in place = THE session card (chevron down; a tap collapses it)
 * over THE question / plan card of that session ([waiting]; null = the
 * skeleton line while it syncs), its label line carrying `Open chat`.
 * A sent answer marks the session answered ([onAnswered]) and collapses.
 */
@Composable
private fun ExpandedSessionRow(
    card: SessionTree.SessionCard,
    waiting: SessionTree.ThreadRow?,
    onCollapse: () -> Unit,
    onOpenChat: () -> Unit,
    onAnswer: ThreadAnswer,
    onDecidePlan: ThreadPlanDecision,
    onAnswered: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().testTag(ThreadRowAction.TestIds.EXPANDED),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        SessionCard(card, onClick = onCollapse, chevron = true, expanded = true)
        if (waiting == null) {
            SkeletonRows(rows = 1, rowHeight = 20.dp, dot = false)
        } else {
            CompositionLocalProvider(LocalAskCardTrailing provides { OpenChatLink(onOpenChat) }) {
                ThreadAskCard(
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
        ThreadRowAction.Copy.OPEN_CHAT,
        style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, fontWeight = FontWeight.Normal),
        color = if (pressed) MaterialTheme.colorScheme.onSurface else Muted,
        maxLines = 1,
        modifier = Modifier
            .clip(RoundedCornerShape(6.dp))
            .clickable(interactionSource = interaction, indication = null, role = Role.Button, onClick = onClick)
            .padding(horizontal = 4.dp, vertical = 2.dp)
            .testTag(ThreadRowAction.TestIds.OPEN_CHAT),
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
data class ThreadSuggestionActions(
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
 * DockCard and this list is its sheet ([ThreadSuggestionsSheet]); in the
 * Conversation the card stays for the rows with no open item. Open = title,
 * text, the ⋯ menu and Start, one row on every width; started = the child's session card
 * (its caption under the name) opening it; here = `Sent here`; filed =
 * `Filed EXP-9`.
 */
@Composable
fun ThreadSuggestionsCard(
    row: SessionTree.ThreadRow.Suggestions,
    actions: ThreadSuggestionActions?,
    onOpenSession: ((String) -> Unit)?,
) {
    var optionsFor by remember(row.id) { mutableStateOf<SessionTree.SuggestionRow?>(null) }
    Column(modifier = Modifier.fillMaxWidth().threadCard().testTag("thread-suggestions")) {
        Row(
            modifier = Modifier.fillMaxWidth().heightIn(min = WorkCardSpec.HEADER.dp).padding(horizontal = 12.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(ExpIcons.uiChecklist, contentDescription = null, tint = Muted, modifier = Modifier.size(14.dp))
            Text(
                row.header,
                style = threadCardTextStyle().copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).testTag("thread-suggestions-header"),
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
                    modifier = Modifier.testTag("thread-suggestions-start-all"),
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
    actions: ThreadSuggestionActions?,
    onOpenSession: ((String) -> Unit)?,
    onOptions: () -> Unit,
) {
    val style = threadCardTextStyle()
    val tag = Modifier.testTag("thread-suggestion-${item.state.wire}")
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
                            modifier = Modifier.testTag("thread-suggestion-options"),
                        )
                        GlassPill(
                            label = SessionTree.SuggestionsCopy.START,
                            onClick = { actions.onStart(rowId, item.id, null, null) },
                            size = PillSize.Sm,
                            primary = true,
                            loading = busy,
                            enabled = !busy,
                            modifier = Modifier.testTag("thread-suggestion-start"),
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
fun ColumnScope.ThreadSuggestionsSheet(
    row: SessionTree.ThreadRow.Suggestions,
    actions: ThreadSuggestionActions?,
    onOpenSession: ((String) -> Unit)?,
) {
    var optionsFor by remember(row.id) { mutableStateOf<SessionTree.SuggestionRow?>(null) }
    // N-6: the Dock sheet is a sheet host: the ⋯ menu discloses inline.
    val inline = LocalPickerPresentation.current == PickerPresentation.Inline
    DockSheetBody {
        Column(modifier = Modifier.fillMaxWidth().fullBleed(12.dp).testTag("thread-suggestions-sheet")) {
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
                    modifier = Modifier.testTag("thread-suggestions-start-all"),
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
    actions: ThreadSuggestionActions,
    onPick: (SuggestionPick) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = item.title, onDismiss = onDismiss, modifier = Modifier.testTag("thread-suggestion-menu")) {
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
    actions: ThreadSuggestionActions,
    onPick: (SuggestionPick) -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(max = PickerDefaults.InlineMaxHeight)
            .verticalScroll(rememberScrollState())
            .padding(bottom = 6.dp)
            .testTag("thread-suggestion-menu"),
    ) {
        SuggestionOptionsRows(actions = actions, onPick = onPick)
    }
}

/** THE ⋯ rows and their Start on… / Start with… pages, either presentation. */
@Composable
private fun SuggestionOptionsRows(
    actions: ThreadSuggestionActions,
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
