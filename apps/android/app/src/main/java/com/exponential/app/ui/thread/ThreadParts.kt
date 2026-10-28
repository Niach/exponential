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
fun threadTabKey(tab: SessionTree.ThreadTab): String = when (tab) {
    SessionTree.ThreadTab.Conversation -> "conversation"
    is SessionTree.ThreadTab.IssueTab -> "issue:${tab.issueId}"
    SessionTree.ThreadTab.Add -> "add"
    is SessionTree.ThreadTab.SessionTab -> "session:${tab.sessionId}"
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

/** The strip's hidden-chip zones (each with its own "+N"). */
private enum class StripZone { Issues, Sessions }

/** The open sheet: a chip's subtree, or a zone's hidden chips (THE chips
 *  the measure pass folded behind its "+N", iOS/web parity). */
private sealed interface StripSheet {
    data class Subtree(val tab: SessionTree.ThreadTab) : StripSheet
    data class Hidden(val zone: StripZone, val tabs: List<SessionTree.ThreadTab>) : StripSheet
}

/** The page key of an issue (a top-level chip or a folded sub-issue). */
private fun issueKey(issueId: String) = "issue:$issueId"
private fun sessionKey(sessionId: String) = "session:$sessionId"

/**
 * EXP-1334 (session-thread.json): the ONE strip, and NOTHING scrolls — "+"
 * (Link an issue · New issue) and Conversation pinned left, Guide pinned
 * right; between them the issue chips then the session chips, laid out by
 * [SessionTree.partitionThreadStrip] over their MEASURED widths: everything
 * full (identifier + title) → identifiers only → fold, each zone with its own
 * "+N" (a sheet of the hidden chips). Issue chips = the top-level linked
 * issues, session chips = the dispatcher's DIRECT children; a chip with
 * folded rows wears a chevron-down and a TAP opens its subtree sheet (row 1 =
 * the chip itself, then THE tree rows with their guides). The active chip
 * reads `<chip> › <open one>` while a folded issue is open. A long-press on
 * an issue chip opens its menu (Unlink).
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
    /** An issue's title (the full tier's chip label); null = identifier only. */
    issueTitle: (String) -> String? = { null },
    onIssueMenu: ((SessionTree.ThreadTab.IssueTab) -> Unit)? = null,
    onOpenSession: ((sessionId: String) -> Unit)? = null,
) {
    var sheet by remember { mutableStateOf<StripSheet?>(null) }
    val issueTabs = tabs.filterIsInstance<SessionTree.ThreadTab.IssueTab>()
    val sessionTabs = tabs.filterIsInstance<SessionTree.ThreadTab.SessionTab>()

    // A chip's tap: its page / session, or its subtree when it folds rows.
    val activate: (SessionTree.ThreadTab) -> Unit = { tab ->
        when (tab) {
            is SessionTree.ThreadTab.IssueTab ->
                if (tab.nested.isEmpty()) onSelect(issueKey(tab.issueId)) else sheet = StripSheet.Subtree(tab)
            is SessionTree.ThreadTab.SessionTab ->
                if (tab.nested.isEmpty()) onOpenSession?.invoke(tab.sessionId) else sheet = StripSheet.Subtree(tab)
            else -> onSelect(threadTabKey(tab))
        }
    }

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
        ThreadStripChips(
            issueTabs = issueTabs,
            sessionTabs = sessionTabs,
            selectedKey = selectedKey,
            issueStatus = issueStatus,
            issueTitle = issueTitle,
            onActivate = activate,
            onIssueMenu = onIssueMenu,
            onMore = { zone, hidden -> sheet = StripSheet.Hidden(zone, hidden) },
            modifier = Modifier.weight(1f),
        )
    }

    when (val open = sheet) {
        is StripSheet.Subtree -> ThreadSubtreeSheet(
            tab = open.tab,
            selectedKey = selectedKey,
            issueStatus = issueStatus,
            issueTitle = issueTitle,
            onPickIssue = { id ->
                sheet = null
                onSelect(issueKey(id))
            },
            onPickSession = { id ->
                sheet = null
                onOpenSession?.invoke(id)
            },
            onDismiss = { sheet = null },
        )
        is StripSheet.Hidden -> {
            // The "+N": ONLY the chips the strip had no room for (the measure
            // pass names them), never the zone's shown ones.
            val hidden = open.tabs
            GlassSheet(title = null, onDismiss = { sheet = null }, modifier = Modifier.testTag("thread-strip-more-sheet")) {
                hidden.forEach { tab ->
                    when (tab) {
                        is SessionTree.ThreadTab.IssueTab -> GlassSheetRow(
                            label = listOfNotNull(tab.identifier, issueTitle(tab.issueId)).joinToString("  "),
                            selected = selectedKey == issueKey(tab.issueId) ||
                                tab.nested.any { selectedKey == issueKey(it.issueId) },
                            onClick = {
                                sheet = null
                                activate(tab)
                            },
                            leading = { issueStatus(tab.issueId)?.let { StatusIcon(it, size = 16.dp) } },
                        )
                        is SessionTree.ThreadTab.SessionTab -> GlassSheetRow(
                            label = tab.name,
                            selected = selectedKey == sessionKey(tab.sessionId) ||
                                tab.nested.any { selectedKey == sessionKey(it.sessionId) },
                            onClick = {
                                sheet = null
                                activate(tab)
                            },
                            leading = { SessionTabMark(tab.markState) },
                        )
                        else -> Unit
                    }
                }
                Spacer(Modifier.height(8.dp))
            }
        }
        null -> Unit
    }
}

/** The strip's one gap (between chips and between the zones). */
private val StripGap = 4.dp

/**
 * The strip's middle: every chip measured at its natural width (issue chips at
 * both tiers), then [SessionTree.partitionThreadStrip] decides which are
 * drawn and whether a zone folds behind its "+N" (iOS `ThreadStripTabs`
 * parity). Identifiers and "+N" never truncate; when even the contract's
 * survivors overflow, a zone folds WHOLE into its "+N" (the sessions first
 * while an issue is the shown page, else the issues), and the shown issue's
 * open `A › B` label is the only text that may ellipsize. Nothing ever slides
 * under the pinned Guide.
 */
@Composable
private fun ThreadStripChips(
    issueTabs: List<SessionTree.ThreadTab.IssueTab>,
    sessionTabs: List<SessionTree.ThreadTab.SessionTab>,
    selectedKey: String,
    issueStatus: (String) -> ResolvedIssueStatus?,
    issueTitle: (String) -> String?,
    onActivate: (SessionTree.ThreadTab) -> Unit,
    onIssueMenu: ((SessionTree.ThreadTab.IssueTab) -> Unit)?,
    onMore: (zone: StripZone, hidden: List<SessionTree.ThreadTab>) -> Unit,
    modifier: Modifier = Modifier,
) {
    // The open folded issue names its chip: `<chip> › <open one>`.
    fun openNested(tab: SessionTree.ThreadTab.IssueTab) =
        tab.nested.firstOrNull { selectedKey == issueKey(it.issueId) }?.identifier
    val activeIssue = issueTabs.indexOfFirst { tab ->
        selectedKey == issueKey(tab.issueId) || openNested(tab) != null
    }.takeIf { it >= 0 }
    // EXP-1343: a session chip is a page too: active while it or a folded
    // session is shown, reading `<chip> › <open one>`.
    fun openNestedSession(tab: SessionTree.ThreadTab.SessionTab) =
        tab.nested.firstOrNull { selectedKey == sessionKey(it.sessionId) }?.name
    fun sessionActive(tab: SessionTree.ThreadTab.SessionTab) =
        selectedKey == sessionKey(tab.sessionId) || openNestedSession(tab) != null
    val activeSession = sessionTabs.indexOfFirst(::sessionActive).takeIf { it >= 0 }
    androidx.compose.ui.layout.SubcomposeLayout(modifier.clipToBounds().testTag("thread-strip-chips")) { constraints ->
        val loose = androidx.compose.ui.unit.Constraints()
        val gap = StripGap.roundToPx()
        val room = constraints.maxWidth
        @Composable
        fun IssueChipFor(tab: SessionTree.ThreadTab.IssueTab, full: Boolean) = IssueStripChip(
            tab = tab,
            selected = selectedKey == issueKey(tab.issueId) || openNested(tab) != null,
            title = if (full) issueTitle(tab.issueId) else null,
            openNested = openNested(tab),
            status = issueStatus(tab.issueId),
            onClick = { onActivate(tab) },
            onLongClick = onIssueMenu?.let { menu -> { menu(tab) } },
        )
        val full = issueTabs.mapIndexed { index, tab ->
            subcompose("issue-full-$index") { IssueChipFor(tab, full = true) }.first().measure(loose)
        }
        val short = issueTabs.mapIndexed { index, tab ->
            subcompose("issue-short-$index") { IssueChipFor(tab, full = false) }.first().measure(loose)
        }
        val sessions = sessionTabs.mapIndexed { index, tab ->
            subcompose("session-$index") {
                SessionStripChip(
                    tab,
                    selected = sessionActive(tab),
                    openNested = openNestedSession(tab),
                    onClick = { onActivate(tab) },
                )
            }.first().measure(loose)
        }
        // The "+N" probe at the widest count a zone can fold (10+ chips
        // would otherwise clip the real chip).
        val overflowW = subcompose("overflow-probe") { MoreChip(maxOf(issueTabs.size, sessionTabs.size), onClick = {}) }
            .first().measure(loose).width
        val layout = SessionTree.partitionThreadStrip(
            issues = full.indices.map { SessionTree.StripIssueWidth(full[it].width.toFloat(), short[it].width.toFloat()) },
            sessions = sessions.map { it.width.toFloat() },
            available = room.toFloat(),
            gap = gap.toFloat(),
            overflowW = overflowW.toFloat(),
            activeIssue = activeIssue,
            activeSession = activeSession,
        )
        val isFull = layout.tier == SessionTree.StripTier.Full
        val issueWidths = (if (isFull) full else short).map { it.width }
        var shownIssues = layout.issues.sorted()
        var shownSessions = layout.sessions.sorted()
        fun zoneWidth(widths: List<Int>, hidden: Boolean): Int {
            val all = widths + if (hidden) listOf(overflowW) else emptyList()
            return if (all.isEmpty()) 0 else all.sum() + gap * (all.size - 1)
        }
        fun sessionsWidth() = if (sessionTabs.isEmpty()) {
            0
        } else {
            zoneWidth(shownSessions.map { sessions[it].width }, shownSessions.size < sessionTabs.size)
        }
        fun issuesWidth() = if (issueTabs.isEmpty()) {
            0
        } else {
            zoneWidth(shownIssues.map { issueWidths[it] }, shownIssues.size < issueTabs.size)
        }
        val sep = if (issueTabs.isNotEmpty() && sessionTabs.isNotEmpty()) gap else 0
        fun fits() = room == androidx.compose.ui.unit.Constraints.Infinity ||
            issuesWidth() + sep + sessionsWidth() <= room
        // Whatever the contract keeps, nothing may clip: an over-full zone
        // folds WHOLE into its "+N" (a zone always keeps its "+N" or a chip).
        val order = if (activeIssue != null) listOf(StripZone.Sessions, StripZone.Issues) else listOf(StripZone.Issues, StripZone.Sessions)
        for (zone in order) {
            if (fits()) break
            if (zone == StripZone.Sessions) shownSessions = emptyList() else shownIssues = listOfNotNull(activeIssue)
        }

        val placed = mutableListOf<androidx.compose.ui.layout.Placeable>()
        val sessionW = sessionsWidth()
        val issueRoom = if (room == androidx.compose.ui.unit.Constraints.Infinity) {
            Int.MAX_VALUE
        } else {
            (room - if (sessionTabs.isEmpty()) 0 else sessionW + sep).coerceAtLeast(0)
        }
        // Over-full (the shown issue plus its "+N"), the open `A › B` label
        // truncates: the shown chip is re-measured into what is left.
        val excess = issuesWidth() - issueRoom
        shownIssues.forEach { index ->
            val natural = if (isFull) full[index] else short[index]
            placed.add(
                if (excess > 0 && index == activeIssue) {
                    subcompose("issue-fit-$index") { IssueChipFor(issueTabs[index], full = isFull) }.first()
                        .measure(androidx.compose.ui.unit.Constraints(maxWidth = (natural.width - excess).coerceAtLeast(0)))
                } else {
                    natural
                },
            )
        }
        val hiddenIssues = issueTabs.filterIndexed { index, _ -> index !in shownIssues }
        if (hiddenIssues.isNotEmpty()) {
            placed.add(
                subcompose("issue-more") { MoreChip(hiddenIssues.size, onClick = { onMore(StripZone.Issues, hiddenIssues) }) }
                    .first().measure(loose),
            )
        }
        shownSessions.forEach { placed.add(sessions[it]) }
        val hiddenSessions = sessionTabs.filterIndexed { index, _ -> index !in shownSessions }
        if (hiddenSessions.isNotEmpty()) {
            placed.add(
                subcompose("session-more") { MoreChip(hiddenSessions.size, onClick = { onMore(StripZone.Sessions, hiddenSessions) }) }
                    .first().measure(loose),
            )
        }
        val height = (placed.maxOfOrNull { it.height } ?: 0).coerceAtLeast(constraints.minHeight)
        val width = room.takeIf { it != androidx.compose.ui.unit.Constraints.Infinity }
            ?: placed.sumOf { it.width + gap }
        layout(width, height) {
            var x = 0
            placed.forEachIndexed { index, placeable ->
                if (index > 0) x += gap
                placeable.placeRelative(x, (height - placeable.height) / 2)
                x += placeable.width
            }
        }
    }
}

/** An issue chip = the strip's issue chip box (×4, iOS `issueChip`): the
 *  status glyph · the mono identifier [· the title at the full tier], then
 *  `› <open one>` while a folded sub-issue is the shown page (the ONLY label
 *  that may ellipsize) and a chevron-down when it folds rows. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun IssueStripChip(
    tab: SessionTree.ThreadTab.IssueTab,
    selected: Boolean,
    title: String?,
    openNested: String?,
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
    ) {
        // An unsynced issue (another team's) is the one chip without a glyph.
        status?.let {
            StatusIcon(it, size = 13.dp)
            Spacer(Modifier.width(5.dp))
        }
        val tokenColor = if (selected) MaterialTheme.colorScheme.onSurface else MdStyle.ChipToken
        Text(
            tab.identifier,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = tokenColor,
            maxLines = 1,
            softWrap = false,
        )
        if (openNested != null) {
            Text(
                " › $openNested",
                style = MaterialTheme.typography.labelMedium,
                fontFamily = FontFamily.Monospace,
                color = tokenColor,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
        }
        if (!title.isNullOrBlank()) {
            Spacer(Modifier.width(6.dp))
            Text(
                title,
                style = MaterialTheme.typography.labelMedium,
                color = Secondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.widthIn(max = 140.dp),
            )
        }
        if (tab.nested.isNotEmpty()) FoldChevron()
    }
}

/** A session chip: its run mark, its NAME (truncating at the chip's cap) and
 *  a chevron-down when it folds deeper sessions. Its accessible name is
 *  `name, state word` (iOS parity: the mark's word is read, not drawn). */
@Composable
private fun SessionStripChip(
    tab: SessionTree.ThreadTab.SessionTab,
    selected: Boolean,
    openNested: String?,
    onClick: () -> Unit,
) {
    val stateWord = SessionTree.CardCopy.STATES.getValue(tab.markState)
    ThreadTab(
        selected = selected,
        onClick = onClick,
        onLongClick = null,
        testTag = "thread-tab-session",
        chip = true,
        modifier = Modifier.semantics { contentDescription = "${tab.name}, $stateWord" },
    ) {
        SessionTabMark(tab.markState, size = 13.dp)
        Spacer(Modifier.width(5.dp))
        Text(
            tab.name,
            style = MaterialTheme.typography.labelMedium,
            color = if (selected) MaterialTheme.colorScheme.onSurface else Secondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.widthIn(max = 120.dp),
        )
        if (openNested != null) {
            Text(
                " › $openNested",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.widthIn(max = 120.dp),
            )
        }
        if (tab.nested.isNotEmpty()) FoldChevron()
    }
}

@Composable
private fun FoldChevron() {
    Spacer(Modifier.width(5.dp))
    Icon(
        ExpIcons.uiChevronDown,
        contentDescription = null,
        tint = Muted,
        modifier = Modifier.size(10.dp).testTag("thread-tab-fold"),
    )
}

/** A zone's "+N": the hidden chips' sheet. Never truncates; reads
 *  `N more` (iOS parity). */
@Composable
private fun MoreChip(count: Int, onClick: () -> Unit) {
    ThreadTab(
        selected = false,
        onClick = onClick,
        onLongClick = null,
        testTag = "thread-strip-more",
        chip = true,
        modifier = Modifier.semantics { contentDescription = "$count more" },
    ) {
        Text("+$count", style = MaterialTheme.typography.labelMedium, color = Secondary, maxLines = 1, softWrap = false)
    }
}

/**
 * A folded chip's subtree as the phone's bottom sheet: row 1 = the chip
 * itself, then the folded rows as THE tree rows ([TreeGuidesRow] over
 * [TreeGuides.compute]): issues = status glyph + mono identifier + title,
 * sessions = their mark, name and state word. The open one wears the check.
 */
@Composable
private fun ThreadSubtreeSheet(
    tab: SessionTree.ThreadTab,
    selectedKey: String,
    issueStatus: (String) -> ResolvedIssueStatus?,
    issueTitle: (String) -> String?,
    onPickIssue: (String) -> Unit,
    onPickSession: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    data class SubtreeRow(val id: String, val depth: Int, val label: String, val mark: SessionTree.SessionState?)
    val rows = when (tab) {
        is SessionTree.ThreadTab.IssueTab ->
            listOf(SubtreeRow(tab.issueId, 0, tab.identifier, null)) +
                tab.nested.map { SubtreeRow(it.issueId, it.depth, it.identifier, null) }
        is SessionTree.ThreadTab.SessionTab ->
            listOf(SubtreeRow(tab.sessionId, 0, tab.name, tab.markState)) +
                tab.nested.map { SubtreeRow(it.sessionId, it.depth, it.name, it.markState) }
        else -> return
    }
    val title = rows.first().label
    val guides = remember(rows) { com.exponential.app.domain.TreeGuides.compute(rows.map { it.depth }) }
    GlassSheet(title = title, onDismiss = onDismiss, modifier = Modifier.testTag("thread-subtree-sheet")) {
        rows.forEachIndexed { index, row ->
            com.exponential.app.ui.components.TreeGuidesRow(depth = row.depth, guide = guides.getOrNull(index)) {
                val issue = row.mark == null
                val selected = selectedKey == if (issue) issueKey(row.id) else sessionKey(row.id)
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { if (issue) onPickIssue(row.id) else onPickSession(row.id) }
                        .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
                        .testTag("thread-subtree-row"),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    if (issue) {
                        issueStatus(row.id)?.let { StatusIcon(it, size = 16.dp) }
                        Text(
                            row.label,
                            style = MaterialTheme.typography.bodyMedium,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                        )
                        issueTitle(row.id)?.let {
                            Text(
                                it,
                                style = MaterialTheme.typography.bodyMedium,
                                color = Secondary,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.weight(1f, fill = false),
                            )
                        }
                    } else {
                        val mark = row.mark ?: SessionTree.SessionState.Idle
                        SessionTabMark(mark)
                        Text(
                            row.label,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        Text(
                            SessionTree.CardCopy.STATES.getValue(mark),
                            style = MaterialTheme.typography.bodyMedium,
                            color = sessionToneColor(SessionTree.CardCopy.TONES.getValue(mark)),
                            maxLines = 1,
                        )
                    }
                    Spacer(Modifier.weight(1f))
                    if (selected) {
                        Icon(ExpIcons.uiCheck, contentDescription = null, tint = Secondary, modifier = Modifier.size(16.dp))
                    }
                }
            }
        }
        Spacer(Modifier.height(8.dp))
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
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .then(if (bare) Modifier else Modifier.threadCard())
            .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .testTag(card.testId),
    ) {
        SessionCardHeader(card)
    }
}

@Composable
private fun SessionCardHeader(card: SessionTree.SessionCard) {
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
) {
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
                    is SessionTree.ThreadRow.SessionRow -> SessionCard(row.card, onClick = { onOpenCard(row.card) })
                    is SessionTree.ThreadRow.Question -> ThreadAskCard(row, onAnswer, onDecidePlan)
                    is SessionTree.ThreadRow.Plan -> ThreadAskCard(row, onAnswer, onDecidePlan)
                    is SessionTree.ThreadRow.Subagent -> WorkCardView(
                        card = WorkCards.of(WorkCardInput.Subagent(title = row.title, state = row.status)),
                    )
                    is SessionTree.ThreadRow.Picture -> ThreadPictureRow(row)
                    is SessionTree.ThreadRow.Event -> ThreadEventLine(row.text)
                    // S-3: a report renders AS its `Report from <chip> ›` line.
                    is SessionTree.ThreadRow.Report -> SessionXref(row.xref, onOpenXref)
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
 * hairline), the agent's on the left in a card-fill bubble (max 94%, its
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
                .background(if (fromPerson) GlassTokens.OpaqueCardFill else DesignTokens.Palette.Card, shape)
                .then(if (fromPerson) Modifier.border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape) else Modifier)
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
