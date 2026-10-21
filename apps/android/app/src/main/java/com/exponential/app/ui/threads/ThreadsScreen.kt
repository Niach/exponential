package com.exponential.app.ui.threads

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.TreeGuides
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.AgentWorkingMark
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TeamSectionHeader
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.components.teamBands
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * EXP-1296 Threads v2: the Threads TAB (it replaced Reviews,
 * `threads-list.json`). ONE list in the list language: the Agents band first
 * (an agent row opens its channel, the same conversation screen), then
 * [SessionTree.partitionThreads]' Needs you · Running · Recent. Phones list
 * EVERY team; with several teams the thread groups sit under one team band
 * each. A row opens its thread ([onOpenThread] = `thread/{rootId}`).
 */
@Composable
fun ThreadsScreen(
    onOpenThread: (rootSessionId: String) -> Unit,
    /** EXP-1323: a nested child row opens that session's view. */
    onOpenSession: (sessionId: String) -> Unit = onOpenThread,
    /** NEW-7: an agent with no conversation of mine yet = its empty DM. */
    onOpenAgent: (agentId: String) -> Unit = {},
    viewModel: ThreadsViewModel = hiltViewModel(),
) {
    val groups by viewModel.groups.collectAsStateWithLifecycle()
    val agents by viewModel.agents.collectAsStateWithLifecycle()
    val conversations by viewModel.conversations.collectAsStateWithLifecycle()
    val teams by viewModel.teams.collectAsStateWithLifecycle()
    val meta by viewModel.rowMeta.collectAsStateWithLifecycle()
    val words by viewModel.words.collectAsStateWithLifecycle()
    Scaffold(containerColor = Color.Transparent) { padding ->
        ThreadsContent(
            groups = groups,
            agents = agents,
            conversations = conversations,
            teams = teams,
            rowMeta = meta,
            words = words,
            onOpenThread = onOpenThread,
            onOpenSession = onOpenSession,
            onOpenAgent = onOpenAgent,
            modifier = Modifier.padding(padding),
        )
    }
}

/** The tab's stateless body. */
@Composable
fun ThreadsContent(
    groups: List<SessionTree.ThreadsListGroup>,
    agents: List<SessionTree.AgentRow>,
    /** NEW-2: the viewer's conversation session per agent id (absent = none yet). */
    conversations: Map<String, String>,
    teams: List<TeamEntity>,
    rowMeta: Map<String, ThreadRowMeta>,
    /** EXP-1309: each row's quiet word (web `threadRowWord`), by root id. */
    words: Map<String, SessionTree.ThreadRowWord>,
    onOpenThread: (rootSessionId: String) -> Unit,
    modifier: Modifier = Modifier,
    /** EXP-1323: a nested child row (depth > 0) opens that session. */
    onOpenSession: (sessionId: String) -> Unit = onOpenThread,
    /** NEW-7: an agent row with no conversation yet opens the empty DM. */
    onOpenAgent: (agentId: String) -> Unit = {},
) {
    Column(modifier = modifier.fillMaxSize()) {
        // The root screens' header (Actions parity): a plain large title.
        Text(
            SessionTree.ListCopy.TITLE,
            style = MaterialTheme.typography.headlineLarge,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
        )
        if (groups.isEmpty() && agents.isEmpty()) {
            Box(modifier = Modifier.weight(1f).fillMaxWidth().testTag(SessionTree.ListCopy.TEST_ID_LIST)) {
                EmptyState(EmptyStateCopy.threads)
            }
            return@Column
        }
        LazyColumn(
            modifier = Modifier.weight(1f).fillMaxWidth().testTag(SessionTree.ListCopy.TEST_ID_LIST),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
        ) {
            if (agents.isNotEmpty()) {
                item(key = "__agents__") {
                    SectionHeader(SessionTree.ListCopy.AGENTS, leading = { BandGlyph(ExpIcons.settingsAgents, Muted) })
                }
                items(agents, key = { "agent_${it.agentId}" }) { agent ->
                    AgentListRow(
                        agent,
                        onClick = { conversations[agent.agentId]?.let(onOpenThread) ?: onOpenAgent(agent.agentId) },
                    )
                }
                item(key = "__agents_gap__") { Spacer(Modifier.height(12.dp)) }
            }
            threadGroups(groups, teams, rowMeta, words, onOpenThread, onOpenSession)
        }
    }
}

/** One row of a team band: the group it sits in + the row. */
private data class GroupedRow(val key: SessionTree.ThreadsGroupKey, val row: SessionTree.ThreadsListRow)

private fun LazyListScope.threadGroups(
    groups: List<SessionTree.ThreadsListGroup>,
    teams: List<TeamEntity>,
    rowMeta: Map<String, ThreadRowMeta>,
    words: Map<String, SessionTree.ThreadRowWord>,
    onOpenThread: (String) -> Unit,
    onOpenSession: (String) -> Unit,
) {
    val flat = groups.flatMap { group -> group.rows.map { GroupedRow(group.key, it) } }
    teamBands(flat, teams) { it.row.teamId }.forEach { band ->
        val bandId = band.team?.id.orEmpty()
        band.team?.let { team ->
            item(key = "__team_${team.id}__") { TeamSectionHeader(team) }
        }
        band.items.groupBy { it.key }.forEach { (key, rows) ->
            item(key = "__group_${bandId}_${key.wire}__") {
                SectionHeader(SessionTree.ListCopy.GROUPS.getValue(key), leading = { GroupGlyph(key) })
            }
            // EXP-1323: a live tree's children hang off their root with THE
            // tree connector (SessionRow depth + TreeGuides, ×4).
            val guides = TreeGuides.compute(rows.map { it.row.depth })
            itemsIndexed(rows, key = { _, it -> "row_${it.row.sessionId}" }) { index, entry ->
                TreeGuidesRow(depth = entry.row.depth, guide = guides.getOrNull(index)) {
                    ThreadListRow(
                        row = entry.row,
                        meta = rowMeta[entry.row.sessionId],
                        onClick = {
                            if (entry.row.depth == 0) onOpenThread(entry.row.sessionId) else onOpenSession(entry.row.sessionId)
                        },
                        threadsTab = true,
                        word = words[entry.row.sessionId],
                    )
                }
            }
            item(key = "__group_gap_${bandId}_${key.wire}__") { Spacer(Modifier.height(12.dp)) }
        }
    }
}

private val Muted: Color @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

@Composable
private fun BandGlyph(icon: androidx.compose.ui.graphics.vector.ImageVector, tint: Color) {
    Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(14.dp))
}

/** EXP-1318 ×4: a group band's glyph — Needs you amber, Running the working
 *  mark, Recent the check (web `GROUP_GLYPH`). */
@Composable
private fun GroupGlyph(key: SessionTree.ThreadsGroupKey) {
    when (key) {
        SessionTree.ThreadsGroupKey.NeedsYou -> BandGlyph(ExpIcons.codingNeedsInput, NeedsInputAmber)
        SessionTree.ThreadsGroupKey.Running -> AgentWorkingMark(agent = null, size = 14.dp)
        SessionTree.ThreadsGroupKey.Recent -> BandGlyph(ExpIcons.uiCheck, Muted)
    }
}

/** An agent: its icon in its color (the working mark while it works), its
 *  name, `needs input` while it waits on a person (EXP-1309, web
 *  `ThreadsListItem`). */
@Composable
private fun AgentListRow(agent: SessionTree.AgentRow, onClick: () -> Unit) {
    val tint = remember(agent.color) { parseColor(agent.color) }
    val icon = ExpIcons.byName(agent.icon)
        ?: ExpIcons.byName(SessionTree.ListCopy.AGENT_DEFAULT_ICON)
        ?: ExpIcons.navThreads
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(onClick = onClick)
            .testTag(SessionTree.ListCopy.TEST_ID_AGENT_ROW)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (agent.busy) {
            AgentWorkingMark(agent = null, size = 16.dp)
        } else {
            Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(16.dp))
        }
        Spacer(Modifier.width(10.dp))
        Text(
            agent.name,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (agent.needsYou) QuietWord(SessionTree.ThreadRowWord.NeedsInput)
    }
}

/** A thread: its state mark, the title (it names the issue, EXP-1316: no
 *  identifier slot; EXP-1339: no lock), then a spinner while busy or an
 *  amber dot when it needs you. EXP-1309/1318: on the Threads tab
 *  ([threadsTab], web `ThreadsListView`) the row ends on ONE quiet [word]
 *  (`merged` = no text), and its state moves into the lead (web `rowLead`): the
 *  badged mark when it needs you (EXP-1344: beats busy), the working mark
 *  while busy, the PR glyph
 *  `ready to merge` / `merged` name, else the mark (dimmed with no word). */
@Composable
internal fun ThreadListRow(
    row: SessionTree.ThreadsListRow,
    meta: ThreadRowMeta?,
    onClick: () -> Unit,
    threadsTab: Boolean = false,
    word: SessionTree.ThreadRowWord? = null,
) {
    val state = SessionTree.threadRowMarkState(row.needsYou, row.busy)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(onClick = onClick)
            .testTag(SessionTree.ListCopy.TEST_ID_ROW)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (threadsTab) {
            ThreadsTabLead(row, meta?.agent, word)
        } else {
            AgentRunMark(agent = meta?.agent, state = state, size = 14.dp, badgeSize = 6.dp)
        }
        Column(modifier = Modifier.weight(1f)) {
            // EXP-1323: unread news = a bold title ×4.
            Text(
                row.title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (row.unread) FontWeight.SemiBold else null,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag(if (row.unread) "threads-row-unread" else "threads-row-title"),
            )
            // EXP-1323: the live caption (what it does right now), muted.
            row.caption?.let { caption ->
                Text(
                    caption,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.testTag("threads-row-caption"),
                )
            }
        }
        // EXP-1339: no lock on a list row; `merged` has no text (its lead
        // alone turns pr-merged violet).
        if (threadsTab && word?.text != null) QuietWord(word)
        // EXP-1318 ×4: the ROOT's device, last (after the word),
        // its icon as the Devices screen draws it; unknown device = none.
        meta?.device?.let { device ->
            Icon(
                deviceIcon(device.icon, device.isServer),
                contentDescription = device.name,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(14.dp).testTag("threads-row-device"),
            )
        }
        when {
            threadsTab -> Unit
            row.needsYou -> NeedsYouDot()
            row.busy -> InlineSpinner()
        }
    }
}

/** Web `rowLead`: a Threads-tab row's state, in its lead. */
@Composable
private fun ThreadsTabLead(row: SessionTree.ThreadsListRow, agent: String?, word: SessionTree.ThreadRowWord?) {
    // EXP-1344 ×4: threadRowMarkState FIRST, at every depth: the amber badge
    // BEATS busy (a parked plan/question keeps the device busy).
    val mark = SessionTree.threadRowMarkState(row.needsYou, row.busy)
    when {
        mark == SessionTree.SessionState.NeedsYou ->
            AgentRunMark(agent = agent, state = SessionTree.SessionState.NeedsYou, size = 14.dp, badgeSize = 6.dp)
        mark == SessionTree.SessionState.Working -> AgentWorkingMark(agent = agent, size = 14.dp)
        word == SessionTree.ThreadRowWord.ReadyToMerge ->
            Icon(ExpIcons.prOpen, contentDescription = null, tint = DesignTokens.Semantic.Green, modifier = Modifier.size(14.dp))
        word == SessionTree.ThreadRowWord.Merged ->
            Icon(ExpIcons.prMerged, contentDescription = null, tint = DesignTokens.Avatar.Violet, modifier = Modifier.size(14.dp))
        else -> AgentRunMark(
            agent = agent,
            state = if (word == null) SessionTree.SessionState.Ended else SessionTree.SessionState.Waiting,
            size = 14.dp,
            badgeSize = 6.dp,
        )
    }
}

/** EXP-1309: a row's ONE quiet trailing word (`stack` wears its glyph). */
@Composable
private fun QuietWord(word: SessionTree.ThreadRowWord) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier.testTag("threads-row-word"),
    ) {
        if (word == SessionTree.ThreadRowWord.Stack) {
            Icon(ExpIcons.prStack, contentDescription = null, tint = muted, modifier = Modifier.size(12.dp))
        }
        Text(word.text.orEmpty(), style = MaterialTheme.typography.labelSmall, color = muted, maxLines = 1)
    }
}

/** The amber dot a row carries while it waits on a person. */
@Composable
private fun NeedsYouDot() {
    Box(
        modifier = Modifier
            .size(8.dp)
            .background(DesignTokens.Semantic.Yellow, CircleShape)
            .testTag("threads-row-needs-you"),
    )
}
