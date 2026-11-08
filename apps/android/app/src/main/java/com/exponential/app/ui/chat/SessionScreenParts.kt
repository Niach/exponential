package com.exponential.app.ui.chat

import androidx.compose.foundation.ExperimentalFoundationApi
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
// count, scrolling sideways on overflow), THE gear, and the Issues / Sessions
// tab bodies (THE issue rows, THE tree rows).

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

/** #EXP-1732: THE gear (agents, the Assistant and apps): the header's ghost
 *  icon button, concept `nav-settings`, named `Settings`. */
@Composable
fun SessionGearButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    ChatHeaderButton(
        ExpIcons.navSettings,
        SessionScreen.Copy.SETTINGS,
        onClick = onClick,
        modifier = modifier.testTag(SessionScreen.TestIds.GEAR),
    )
}

/**
 * #EXP-1732 the Issues tab: led by the `+` (THE Menu: Link an issue, New
 * issue), then THE issue rows (status glyph, mono identifier, title; nested
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
    onAdd: (() -> Unit)?,
    contentPadding: PaddingValues,
    modifier: Modifier = Modifier,
    onMenu: ((SessionScreenIssue) -> Unit)? = null,
) {
    val guides = remember(issues) { TreeGuides.compute(issues.map { it.depth }) }
    LazyColumn(
        modifier = modifier.fillMaxSize().testTag(SessionScreen.TestIds.ISSUES),
        contentPadding = contentPadding,
    ) {
        if (onAdd != null) {
            item(key = "add") {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    ChatHeaderButton(
                        ExpIcons.uiAdd,
                        SessionScreen.Copy.LINK_ISSUE,
                        onClick = onAdd,
                        modifier = Modifier.testTag("${SessionScreen.TestIds.ISSUES}-add"),
                    )
                }
            }
        }
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
                        fontFamily = FontFamily.Monospace,
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
