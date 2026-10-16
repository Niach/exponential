package com.exponential.app.ui.thread

import android.content.Intent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.RowScope
import com.exponential.app.ui.components.GlassPill
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.net.toUri
import com.exponential.app.data.db.UserEntity
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

/** The strip's hidden-chip zones (each with its own "+N"). */
private enum class StripZone { Issues, Sessions }

/** The open sheet: a chip's subtree, or a zone's hidden chips. */
private sealed interface StripSheet {
    data class Subtree(val tab: SessionTree.ThreadTab) : StripSheet
    data class Hidden(val zone: StripZone) : StripSheet
}

/** The page key of an issue (a top-level chip or a folded sub-issue). */
private fun issueKey(issueId: String) = "issue:$issueId"

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
    val guide = tabs.filterIsInstance<SessionTree.ThreadTab.Guide>().firstOrNull()

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
        val conversationSelected = selectedKey == "conversation"
        ThreadTab(conversationSelected, { onSelect("conversation") }, null, "thread-tab-conversation") {
            Text(
                SessionTree.ThreadCopy.CONVERSATION,
                style = MaterialTheme.typography.labelLarge,
                color = if (conversationSelected) MaterialTheme.colorScheme.onSurface else Secondary,
                maxLines = 1,
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
            onMore = { zone -> sheet = StripSheet.Hidden(zone) },
            modifier = Modifier.weight(1f),
        )
        if (guide != null) {
            val guideSelected = selectedKey == "guide"
            ThreadTab(guideSelected, { onSelect("guide") }, null, "thread-tab-guide") {
                Text(
                    SessionTree.ThreadCopy.GUIDE,
                    style = MaterialTheme.typography.labelLarge,
                    color = if (guideSelected) MaterialTheme.colorScheme.onSurface else Secondary,
                    maxLines = 1,
                )
                if (guide.dot) {
                    Spacer(Modifier.width(6.dp))
                    Box(Modifier.size(6.dp).background(LiveGreen, CircleShape).testTag("thread-guide-dot"))
                }
            }
        }
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
            // The "+N": the chips the strip had no room for, THE same chips.
            val hidden = remember(open.zone, issueTabs, sessionTabs) {
                if (open.zone == StripZone.Issues) issueTabs else sessionTabs
            }
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

/** A squeezed chip never goes below this (its lead + an ellipsis). */
private val MinChipWidth = 40.dp

/** The strip's one gap (between chips and between the zones). */
private val StripGap = 4.dp

/**
 * The strip's middle: every chip measured (issue chips at both tiers), then
 * [SessionTree.partitionThreadStrip] decides which are drawn and whether a
 * zone folds behind its "+N".
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
    onMore: (StripZone) -> Unit,
    modifier: Modifier = Modifier,
) {
    // The open folded issue names its chip: `<chip> › <open one>`.
    fun openNested(tab: SessionTree.ThreadTab.IssueTab) =
        tab.nested.firstOrNull { selectedKey == issueKey(it.issueId) }?.identifier
    val activeIssue = issueTabs.indexOfFirst { tab ->
        selectedKey == issueKey(tab.issueId) || openNested(tab) != null
    }.takeIf { it >= 0 }
    androidx.compose.ui.layout.SubcomposeLayout(modifier.clipToBounds().testTag("thread-strip-chips")) { constraints ->
        val loose = androidx.compose.ui.unit.Constraints()
        val gap = StripGap.roundToPx()
        fun issueChip(index: Int, tab: SessionTree.ThreadTab.IssueTab, full: Boolean) =
            subcompose("issue-${if (full) "full" else "short"}-$index") {
                IssueStripChip(
                    tab = tab,
                    selected = selectedKey == issueKey(tab.issueId) || openNested(tab) != null,
                    title = if (full) issueTitle(tab.issueId) else null,
                    openNested = openNested(tab),
                    status = issueStatus(tab.issueId),
                    onClick = { onActivate(tab) },
                    onLongClick = onIssueMenu?.let { menu -> { menu(tab) } },
                )
            }.first().measure(loose)
        val full = issueTabs.mapIndexed { index, tab -> issueChip(index, tab, full = true) }
        val short = issueTabs.mapIndexed { index, tab -> issueChip(index, tab, full = false) }
        val sessions = sessionTabs.mapIndexed { index, tab ->
            subcompose("session-$index") { SessionStripChip(tab, onClick = { onActivate(tab) }) }.first().measure(loose)
        }
        val overflowW = subcompose("overflow-probe") { MoreChip(99, onClick = {}) }.first().measure(loose).width
        val layout = SessionTree.partitionThreadStrip(
            issues = full.indices.map { SessionTree.StripIssueWidth(full[it].width.toFloat(), short[it].width.toFloat()) },
            sessions = sessions.map { it.width.toFloat() },
            available = constraints.maxWidth.toFloat(),
            gap = gap.toFloat(),
            overflowW = overflowW.toFloat(),
            activeIssue = activeIssue,
            activeSession = null,
        )
        val issuePlaceables = if (layout.tier == SessionTree.StripTier.Full) full else short
        // What gets drawn, in order; a chip entry can be re-measured narrower.
        class Entry(val key: String, var placeable: androidx.compose.ui.layout.Placeable, val content: (@Composable () -> Unit)?)
        val entries = mutableListOf<Entry>()
        layout.issues.sorted().forEach { index ->
            val tab = issueTabs[index]
            val isFull = layout.tier == SessionTree.StripTier.Full
            entries.add(
                Entry("issue-$index", issuePlaceables[index]) {
                    IssueStripChip(
                        tab = tab,
                        selected = selectedKey == issueKey(tab.issueId) || openNested(tab) != null,
                        title = if (isFull) issueTitle(tab.issueId) else null,
                        openNested = openNested(tab),
                        status = issueStatus(tab.issueId),
                        onClick = { onActivate(tab) },
                        onLongClick = onIssueMenu?.let { menu -> { menu(tab) } },
                    )
                },
            )
        }
        val hiddenIssues = issueTabs.size - layout.issues.size
        if (hiddenIssues > 0) {
            entries.add(
                Entry(
                    "issue-more",
                    subcompose("issue-more") { MoreChip(hiddenIssues, onClick = { onMore(StripZone.Issues) }) }
                        .first().measure(loose),
                    null,
                ),
            )
        }
        layout.sessions.sorted().forEach { index ->
            val tab = sessionTabs[index]
            entries.add(Entry("session-$index", sessions[index]) { SessionStripChip(tab, onClick = { onActivate(tab) }) })
        }
        val hiddenSessions = sessionTabs.size - layout.sessions.size
        if (hiddenSessions > 0) {
            entries.add(
                Entry(
                    "session-more",
                    subcompose("session-more") { MoreChip(hiddenSessions, onClick = { onMore(StripZone.Sessions) }) }
                        .first().measure(loose),
                    null,
                ),
            )
        }
        // The one chip a zone always keeps may leave the strip too wide: the
        // widest chips give up the excess (their labels ellipsize), so nothing
        // ever slides under the pinned Guide.
        val room = constraints.maxWidth
        if (room != androidx.compose.ui.unit.Constraints.Infinity) {
            var excess = entries.sumOf { it.placeable.width } + gap * (entries.size - 1).coerceAtLeast(0) - room
            val floor = MinChipWidth.roundToPx()
            for (entry in entries.filter { it.content != null }.sortedByDescending { it.placeable.width }) {
                if (excess <= 0) break
                val content = entry.content ?: continue
                val before = entry.placeable.width
                val target = (before - excess).coerceAtLeast(minOf(floor, before))
                if (target >= before) continue
                entry.placeable = subcompose("fit-${entry.key}") { content() }.first()
                    .measure(androidx.compose.ui.unit.Constraints(maxWidth = target))
                excess -= before - entry.placeable.width
            }
        }
        val placed = entries.map { it.placeable }
        val height = (placed.maxOfOrNull { it.height } ?: 0).coerceAtLeast(constraints.minHeight)
        val width = constraints.maxWidth.takeIf { it != androidx.compose.ui.unit.Constraints.Infinity }
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

/** An issue chip: status glyph, mono identifier, the title at the full tier,
 *  `› <open one>` while a folded sub-issue is open, a chevron-down when it
 *  folds rows. */
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
        status?.let {
            StatusIcon(it, size = 14.dp)
            Spacer(Modifier.width(6.dp))
        }
        val tokenColor = if (selected) MaterialTheme.colorScheme.onSurface else MdStyle.ChipToken
        Text(
            tab.identifier,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = tokenColor,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (openNested != null) {
            Text(
                " › $openNested",
                style = MaterialTheme.typography.labelMedium,
                fontFamily = FontFamily.Monospace,
                color = tokenColor,
                maxLines = 1,
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

/** A session chip: its mark + name, a chevron-down when it folds rows. */
@Composable
private fun SessionStripChip(tab: SessionTree.ThreadTab.SessionTab, onClick: () -> Unit) {
    ThreadTab(
        selected = false,
        onClick = onClick,
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
            modifier = Modifier.widthIn(max = 140.dp),
        )
        if (tab.nested.isNotEmpty()) FoldChevron()
    }
}

@Composable
private fun FoldChevron() {
    Spacer(Modifier.width(4.dp))
    Icon(
        ExpIcons.uiChevronDown,
        contentDescription = null,
        tint = Muted,
        modifier = Modifier.size(12.dp).testTag("thread-tab-fold"),
    )
}

/** A zone's "+N": the hidden chips' sheet. */
@Composable
private fun MoreChip(count: Int, onClick: () -> Unit) {
    ThreadTab(
        selected = false,
        onClick = onClick,
        onLongClick = null,
        testTag = "thread-strip-more",
        chip = true,
        horizontalPadding = 7.dp,
    ) {
        Text("+$count", style = MaterialTheme.typography.labelMedium, color = Secondary, maxLines = 1)
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
                val selected = issue && selectedKey == issueKey(row.id)
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
                        val mark = row.mark ?: SessionTree.SessionState.Waiting
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
    horizontalPadding: Dp = 10.dp,
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
            .padding(horizontal = horizontalPadding)
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
        // EXP-1323: the live caption = the muted second line, at the name's x
        // (12dp padding + the 14dp icon + its 8dp gap).
        card.caption?.let { caption ->
            Text(
                caption,
                style = threadCardTextStyle(),
                color = Muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier
                    // Drawn 6dp up into the header row (web `-mt-1.5`): the
                    // 2dp of layout below + those 6 = the 8dp bottom inset.
                    .offset(y = (-6).dp)
                    .padding(start = 34.dp, end = 12.dp, bottom = 2.dp)
                    .testTag("session-card-caption"),
            )
        }
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
    /** EXP-1323: a report line / a started suggestion opens that session. */
    onOpenSession: ((sessionId: String) -> Unit)? = null,
    /** EXP-1323: the suggested tasks cards' actions (null = read-only). */
    suggestions: ThreadSuggestionActions? = null,
    /** EXP-1323: the live tail after the last row (the Tasks card + the
     *  status line), the host's; null = none. */
    tail: (@Composable () -> Unit)? = null,
) {
    // EXP-1334: no root card any more (the conversation IS the dispatcher).
    val shown = rows
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
                    is SessionTree.ThreadRow.Report -> ThreadReportLine(
                        text = row.text,
                        onOpen = onOpenSession?.let { open -> { open(row.sessionId) } },
                    )
                    is SessionTree.ThreadRow.Suggestions -> ThreadSuggestionsCard(
                        row = row,
                        actions = suggestions,
                        onOpenSession = onOpenSession,
                    )
                }
            }
            if (tail != null) {
                item(key = "__live_tail__") { tail() }
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
            // Keyed `index:label` like web — two identical labels stay two rows;
            // the answer still names the option by its label (below).
            options = row.options.mapIndexed { index, option ->
                QuestionOption(label = option.label, key = "$index:${option.label}", description = option.description)
            },
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
                    val answer = text?.takeIf { it.isNotBlank() }
                        ?: item.options.firstOrNull { it.key == key }?.label
                        ?: key.orEmpty()
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

// ── EXP-1323: reports + suggested tasks ─────────────────────────────────────

/** EXP-1323: a child's report = ONE event line `Report from <session> ›`
 *  opening that session; its summary is never repeated (the parent's next
 *  message relays it). */
@Composable
fun ThreadReportLine(text: String, onOpen: (() -> Unit)?) {
    Row(
        modifier = Modifier
            .then(if (onOpen != null) Modifier.clickable(role = Role.Button, onClick = onOpen) else Modifier)
            .padding(horizontal = 4.dp, vertical = 2.dp)
            .testTag("thread-report"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(ExpIcons.notificationReporterReply, contentDescription = null, tint = Muted, modifier = Modifier.size(14.dp))
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            color = Muted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        if (onOpen != null) {
            Icon(ExpIcons.uiChevronRight, contentDescription = null, tint = Muted, modifier = Modifier.size(14.dp))
        }
    }
}

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
 * then hairline-divided items. Open = title, text, × · and (phones STACK the
 * buttons) Options · Do it here · Start; started = the child's session card
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
            Column(
                modifier = tag.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
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
                        Box(
                            modifier = Modifier
                                .size(24.dp)
                                .clip(CircleShape)
                                .clickable(
                                    enabled = !busy,
                                    role = Role.Button,
                                    onClickLabel = SessionTree.SuggestionsCopy.DISMISS,
                                    onClick = { actions.onDismiss(rowId, item.id) },
                                )
                                .testTag("thread-suggestion-dismiss"),
                            contentAlignment = Alignment.Center,
                        ) {
                            Icon(
                                ExpIcons.uiClose,
                                contentDescription = SessionTree.SuggestionsCopy.DISMISS,
                                tint = Muted,
                                modifier = Modifier.size(12.dp),
                            )
                        }
                    }
                }
                if (actions != null) {
                    // Phones STACK the buttons under the text (web `flex-col`).
                    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        GlassPill(
                            label = SessionTree.SuggestionsCopy.OPTIONS,
                            onClick = onOptions,
                            size = PillSize.Md,
                            enabled = !busy,
                            trailing = {
                                Icon(ExpIcons.uiChevronDown, contentDescription = null, modifier = Modifier.size(12.dp))
                            },
                            modifier = Modifier.fillMaxWidth().testTag("thread-suggestion-options"),
                        )
                        GlassPill(
                            label = SessionTree.SuggestionsCopy.HERE,
                            onClick = { actions.onHere(rowId, item.id) },
                            size = PillSize.Md,
                            enabled = !busy,
                            modifier = Modifier.fillMaxWidth().testTag("thread-suggestion-here"),
                        )
                        GlassPill(
                            label = SessionTree.SuggestionsCopy.START,
                            onClick = { actions.onStart(rowId, item.id, null, null) },
                            size = PillSize.Md,
                            primary = true,
                            loading = busy,
                            enabled = !busy,
                            modifier = Modifier.fillMaxWidth().testTag("thread-suggestion-start"),
                        )
                    }
                }
            }
        }
    }
}

/** What an Options pick does to (rowId, itemId). */
private typealias SuggestionPick = (rowId: String, itemId: String) -> Unit

/**
 * EXP-1323: an open item's Options = THE menu as its phone sheet (the "+"'s
 * idiom): Start on… › (the runnable devices) · Start with… › (the models,
 * never a Default row) · File as issue · Dismiss. A submenu row pushes its
 * page inside the sheet; back returns to the rows.
 */
@Composable
private fun SuggestionOptionsSheet(
    item: SessionTree.SuggestionRow,
    actions: ThreadSuggestionActions,
    onPick: (SuggestionPick) -> Unit,
    onDismiss: () -> Unit,
) {
    var page by remember { mutableStateOf<String?>(null) }
    val iconTint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    GlassSheet(title = item.title, onDismiss = onDismiss, modifier = Modifier.testTag("thread-suggestion-menu")) {
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
        Spacer(Modifier.height(8.dp))
    }
}

private const val PAGE_DEVICE = "device"
private const val PAGE_MODEL = "model"
