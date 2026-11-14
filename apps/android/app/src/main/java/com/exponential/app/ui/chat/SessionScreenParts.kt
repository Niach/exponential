package com.exponential.app.ui.chat

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.WaitingCards
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.ExpMono
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionScreen
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.TreeGuides
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.ChatMenuItem
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.SessionRow
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// #EXP-1732 (session-screen.json): THE session screen's phone pieces, shared
// by every kind (a chat, an agent's or the Assistant's conversation, an app's
// editor): the header band's tabs (THE segmented control, labels + a muted
// count, scrolling sideways on overflow), THE `…` (#FEED-239: no gear), the
// settings item, and the Issues / Sessions tab bodies (THE issue rows, THE
// tree rows).

/** #EXP-1732: one issue of the tree (a linked one, or one nested under it). */
data class SessionScreenIssue(val issueId: String, val identifier: String, val depth: Int)

/** #EXP-1732: one session under the root, as THE tree row ([depth] for the guides). */
data class SessionScreenSession(
    val sessionId: String,
    val name: String,
    val markState: SessionTree.SessionState,
    val depth: Int,
)

/** #EXP-1732: the Issues and Sessions tabs' rows (their counts = their sizes). */
data class SessionScreenItems(val issues: List<SessionScreenIssue>, val sessions: List<SessionScreenSession>) {
    fun issue(issueId: String): SessionScreenIssue? = issues.firstOrNull { it.issueId == issueId }
    fun session(sessionId: String): SessionScreenSession? = sessions.firstOrNull { it.sessionId == sessionId }

    companion object {
        val Empty = SessionScreenItems(emptyList(), emptyList())
    }
}

/**
 * #EXP-1732: the tabs' rows out of the chat's tabs: EVERY issue of the tree
 * (nested ones included) and every session under the root (the main session
 * of a single-session chat excluded: it IS the Chat tab), in tree order.
 */
fun sessionScreenItems(tabs: List<SessionTree.ChatTab>, mainSessionId: String?): SessionScreenItems {
    val issues = tabs.filterIsInstance<SessionTree.ChatTab.IssueTab>().flatMap { tab ->
        listOf(SessionScreenIssue(tab.issueId, tab.identifier, 0)) +
            tab.nested.map { SessionScreenIssue(it.issueId, it.identifier, it.depth) }
    }.distinctBy { it.issueId }
    val sessions = tabs.filterIsInstance<SessionTree.ChatTab.SessionTab>().flatMap { tab ->
        listOf(SessionScreenSession(tab.sessionId, tab.name, tab.markState, 0)) +
            tab.nested.map { SessionScreenSession(it.sessionId, it.name, it.markState, it.depth) }
    }.filter { it.sessionId != mainSessionId }.distinctBy { it.sessionId }
    return SessionScreenItems(issues, sessions)
}

/**
 * #EXP-1732: an AGENT's tree (its chat view carries the conversation only,
 * session-chat.json): every session under [rootId] depth-first in the tree's
 * order (a session whose parent is outside the tree reads as a top row) and
 * the root's [linked] issues (id, identifier).
 */
fun sessionScreenTreeItems(
    rootId: String,
    tree: List<SessionTree.Session>,
    linked: List<Pair<String, String>>,
): SessionScreenItems {
    val now = java.time.Instant.now().toString()
    val ids = tree.map { it.id }.toSet()
    val under = tree.filter { it.id != rootId }
    val isTop: (SessionTree.Session) -> Boolean = { s -> s.parentId == rootId || s.parentId == null || s.parentId !in ids }
    val children = under.filterNot(isTop).groupBy { it.parentId }
    val out = mutableListOf<SessionScreenSession>()
    val seen = HashSet<String>()
    fun add(s: SessionTree.Session, depth: Int) {
        if (!seen.add(s.id)) return
        out.add(
            SessionScreenSession(
                sessionId = s.id,
                name = SessionTree.sessionCard(s, s.title.orEmpty(), now).name,
                markState = SessionTree.sessionTabMarkState(s),
                depth = depth,
            ),
        )
        children[s.id].orEmpty().forEach { add(it, depth + 1) }
    }
    under.filter(isTop).forEach { add(it, 0) }
    val issues = linked.distinctBy { it.first }.map { (id, identifier) -> SessionScreenIssue(id, identifier, 0) }
    return SessionScreenItems(issues, out)
}

/**
 * #EXP-1732: THE tabs under the header row = THE existing segmented control
 * ([GlassSegmentedControl]), testId `session-tabs`: each option the label,
 * then its count as a muted number when not null. Labels only, no icons;
 * TAB SIZING: every segment hugs its label (+ count), never stretched to
 * equal widths nor squeezed, and the strip scrolls sideways when it
 * overflows (never wrapping).
 */
@Composable
fun SessionScreenTabs(
    tabs: List<SessionScreen.TabEntry>,
    active: SessionScreen.Tab,
    onSelect: (SessionScreen.Tab) -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .padding(start = 12.dp, end = 12.dp, bottom = 8.dp)
            .testTag(SessionScreen.TestIds.TABS),
    ) {
        Box(Modifier.horizontalScroll(rememberScrollState())) {
            GlassSegmentedControl(
                options = tabs.map { it.tab },
                selected = active,
                label = { tab -> tabs.firstOrNull { it.tab == tab }?.label ?: tab.label },
                onSelect = onSelect,
                testTag = { tab -> SessionScreen.TestIds.tab(tab) },
                trailing = { tab -> segmentCount(tabs.firstOrNull { it.tab == tab }?.count) },
                description = { tab ->
                    val entry = tabs.firstOrNull { it.tab == tab }
                    entry?.count?.let { "${entry.label}, $it" }
                },
                hug = true,
                // #EXP-1834: an empty counted tab is disabled, never opened.
                disabled = { tab -> tabs.firstOrNull { it.tab == tab }?.disabled == true },
            )
        }
    }
}

/** A segment's muted count after its label (null = none). */
private fun segmentCount(count: Int?): (@Composable () -> Unit)? {
    if (count == null) return null
    return { SessionTabCount(count) }
}

@Composable
private fun SessionTabCount(count: Int) {
    Spacer(Modifier.width(6.dp))
    Text(
        "$count",
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        maxLines = 1,
        softWrap = false,
    )
}

/**
 * #FEED-239: THE `…` of every kind ([SessionScreen.menu]) as the header
 * menu's groups (split at each `separator`): labels `copy.menu` by kind,
 * icons `menuIcons`, Delete destructive. [onPick] runs the entry (Compact /
 * New conversation SEND [SessionScreen.MenuEntry.send]).
 */
fun sessionScreenMenuGroups(
    kind: SessionScreen.Kind,
    entries: List<SessionScreen.MenuEntry>,
    onPick: (SessionScreen.MenuEntry) -> Unit,
): List<List<ChatMenuItem>> {
    val groups = mutableListOf(mutableListOf<ChatMenuItem>())
    for (entry in entries) {
        if (entry == SessionScreen.MenuEntry.Separator) {
            groups += mutableListOf<ChatMenuItem>()
            continue
        }
        groups.last() += ChatMenuItem(
            label = SessionScreen.menuLabel(kind, entry),
            icon = entry.icon?.let(ExpIcons::byConcept) ?: ExpIcons.uiMore,
            onClick = { onPick(entry) },
            destructive = entry == SessionScreen.MenuEntry.Delete,
            testTag = "${SessionScreen.TestIds.MENU}-${entry.wire}",
        )
    }
    return groups.filter { it.isNotEmpty() }
}

/** [sessionScreenMenuGroups] flattened for one list (a row's sheet): a
 *  hairline above each group's first row. */
fun sessionScreenMenuItems(
    kind: SessionScreen.Kind,
    entries: List<SessionScreen.MenuEntry>,
    onPick: (SessionScreen.MenuEntry) -> Unit,
): List<ChatMenuItem> = sessionScreenMenuGroups(kind, entries, onPick).flatMapIndexed { index, group ->
    group.mapIndexed { i, item -> if (index > 0 && i == 0) item.copy(dividerBefore = true) else item }
}

/**
 * #FEED-239: an agent's (the Assistant's) SETTINGS opened as the screen's
 * item (`open: settings`; the `…`'s settings entry): THE agent settings page
 * as the pushed page (its own `‹` header), never a navigation to Settings.
 * [onClose] = its back (system Back too); [onDeleted] = the agent is gone.
 */
@Composable
fun AgentSettingsItem(agentId: String, onClose: () -> Unit, onDeleted: () -> Unit) {
    androidx.activity.compose.BackHandler(onBack = onClose)
    com.exponential.app.ui.apps.ArgsStore("agentId", agentId) {
        com.exponential.app.ui.settings.AgentSettingsScreen(onBack = onClose, onCreated = {}, onDeleted = onDeleted)
    }
}

/**
 * #EXP-1785 (waiting-cards.json, phones = `button`): THE strip chip box,
 * icon-only (`icons.waiting` in the question tint) + the count muted, at the
 * START of the header's trailing slot; accessible name `copy.waitingCount`.
 */
@Composable
fun SessionWaitingButton(count: Int, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val name = WaitingCards.waitingCountLabel(count)
    val shape = RoundedCornerShape(10.dp)
    Row(
        modifier = modifier
            .clip(shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
            .clickable(onClick = onClick)
            .height(32.dp)
            .padding(horizontal = 10.dp)
            .testTag(WaitingCards.TestIds.BUTTON)
            .semantics(mergeDescendants = true) { contentDescription = name },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            ExpIcons.uiHelp,
            contentDescription = null,
            tint = DesignTokens.Semantic.Yellow,
            modifier = Modifier.size(14.dp),
        )
        Spacer(Modifier.width(6.dp))
        Text(
            "$count",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            softWrap = false,
        )
    }
}

/**
 * #EXP-1785 (waiting-cards.json `button`): THE Menu (the phones' bottom
 * sheet) titled `copy.waiting`: THE MenuRow per waiting card, Conversation
 * order: the kind icon tinted, the label, ask-dock `askDockLabel(kind)` muted.
 * A pick closes the sheet ([onPick] runs after [onDismiss]).
 */
@Composable
fun SessionWaitingSheet(
    chips: List<WaitingCards.Chip>,
    onPick: (WaitingCards.Chip) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(
        title = WaitingCards.Copy.WAITING,
        onDismiss = onDismiss,
        modifier = Modifier.testTag(WaitingCards.TestIds.MENU),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
        ) {
            chips.forEach { chip ->
                Box(Modifier.testTag("${WaitingCards.TestIds.MENU}-row")) {
                    GlassSheetRow(
                        label = chip.label,
                        onClick = {
                            onDismiss()
                            onPick(chip)
                        },
                        labelColor = MaterialTheme.colorScheme.onSurface,
                        leading = {
                            Icon(
                                if (chip.kind == AskDock.Kind.Plan) ExpIcons.codingPlan else ExpIcons.uiHelp,
                                contentDescription = null,
                                tint = if (chip.kind == AskDock.Kind.Plan) MaterialTheme.colorScheme.primary else DesignTokens.Semantic.Yellow,
                                modifier = Modifier.size(16.dp),
                            )
                        },
                        trailing = {
                            Text(
                                AskDock.label(chip.kind),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                maxLines = 1,
                                modifier = Modifier.padding(start = 8.dp),
                            )
                        },
                    )
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

/**
 * #EXP-1732 the Issues tab (#EXP-1834: no `+`; linking is the `…`'s entry):
 * THE issue rows (status glyph, mono identifier, title; nested
 * ones indented under their parent on the tree guides). A tap opens that
 * issue in place ([onOpen]); a long-press its menu ([onMenu], Unlink).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun SessionIssuesBody(
    issues: List<SessionScreenIssue>,
    issueStatus: (String) -> ResolvedIssueStatus?,
    issueTitle: (String) -> String?,
    onOpen: (issueId: String) -> Unit,
    contentPadding: PaddingValues,
    modifier: Modifier = Modifier,
    onMenu: ((SessionScreenIssue) -> Unit)? = null,
) {
    val guides = remember(issues) { TreeGuides.compute(issues.map { it.depth }) }
    LazyColumn(
        modifier = modifier.fillMaxSize().testTag(SessionScreen.TestIds.ISSUES),
        contentPadding = contentPadding,
    ) {
        itemsIndexed(issues, key = { _, issue -> issue.issueId }) { index, issue ->
            TreeGuidesRow(depth = issue.depth, guide = guides.getOrNull(index)) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 44.dp)
                        .combinedClickable(
                            onClick = { onOpen(issue.issueId) },
                            onLongClick = onMenu?.let { menu -> { menu(issue) } },
                        )
                        .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
                        .testTag("${SessionScreen.TestIds.ISSUES}-row"),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    issueStatus(issue.issueId)?.let { StatusIcon(it, size = 16.dp) }
                    Text(
                        issue.identifier,
                        style = MaterialTheme.typography.bodyMedium,
                        fontFamily = ExpMono,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                    )
                    Text(
                        issueTitle(issue.issueId).orEmpty(),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

/**
 * #EXP-1732 the Sessions tab: THE tree rows ([SessionRow] small over
 * [TreeGuides.compute], the run mark by `markState`) of every session under
 * the root, subagents included as child rows. A tap opens that session in
 * place ([onOpen]).
 */
@Composable
fun SessionSessionsBody(
    sessions: List<SessionScreenSession>,
    onOpen: (sessionId: String) -> Unit,
    contentPadding: PaddingValues,
    modifier: Modifier = Modifier,
) {
    val guides = remember(sessions) { TreeGuides.compute(sessions.map { it.depth }) }
    LazyColumn(
        modifier = modifier.fillMaxSize().testTag(SessionScreen.TestIds.SESSIONS),
        contentPadding = contentPadding,
    ) {
        itemsIndexed(sessions, key = { _, session -> session.sessionId }) { index, session ->
            SessionRow(
                title = session.name,
                depth = session.depth,
                guide = guides.getOrNull(index),
                onClick = { onOpen(session.sessionId) },
                testTag = "${SessionScreen.TestIds.SESSIONS}-row",
            ) {
                AgentRunMark(agent = DEFAULT_AGENT, state = session.markState, size = 14.dp, badgeSize = 6.dp)
            }
        }
    }
}
