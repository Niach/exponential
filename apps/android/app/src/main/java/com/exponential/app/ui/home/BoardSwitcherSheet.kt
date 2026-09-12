package com.exponential.app.ui.home

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.ServerBoardGroup
import com.exponential.app.data.db.TeamBlock
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.PinnedRow
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.ui.components.BoardRow
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.LiveDot
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.session.SessionRowTitle
import com.exponential.app.ui.session.sessionRowIdentifier
import com.exponential.app.ui.session.sessionRowTitle
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * The inline board switcher: a bottom sheet presenting every signed-in
 * account's teams and boards (server → team → board). This is
 * the old Boards home screen's tree, relocated — picking a board swaps the
 * Issues tab's list in place instead of pushing a new destination.
 * Mirrors iOS BoardSwitcherSheet. EXP-698 r5 gave every team block its own
 * "Create board" row (the target team is the block's, so nothing is ambiguous
 * any more) and put a "New team" row at the very bottom.
 */
@Composable
fun BoardSwitcherSheet(
    groups: List<ServerBoardGroup>,
    /** The board the Issues tab is showing — its row takes the active paint. */
    currentBoardId: String?,
    onSelect: (accountId: String, boardId: String) -> Unit,
    onCreateBoard: (teamId: String) -> Unit,
    onCreateTeam: () -> Unit,
    onDismiss: () -> Unit,
    /** EXP-778: the active team's resolved pins, in sort_order; empty hides
     *  the section. The sheet is the mobile sidebar, so "Pinned" sits at its
     *  top like the web/desktop rail group. */
    pinned: List<PinnedRow> = emptyList(),
    onOpenPin: (PinnedRow) -> Unit = {},
) {
    GlassSheet(title = "Switch board", onDismiss = onDismiss) {
        if (groups.isEmpty() && pinned.isEmpty()) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 40.dp, vertical = 32.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Icon(
                    ExpIcons.navBoards,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier.size(22.dp),
                )
                Text(
                    "Create your first board on the web or desktop app.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    textAlign = TextAlign.Center,
                )
            }
        } else {
            LazyColumn(
                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(18.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                if (pinned.isNotEmpty()) {
                    item(key = "pinned") {
                        PinnedSection(rows = pinned, onOpen = onOpenPin)
                    }
                }
                items(groups, key = { it.accountId }) { group ->
                    ServerSection(
                        group = group,
                        showServerHeader = groups.size > 1,
                        currentBoardId = currentBoardId,
                        onSelect = onSelect,
                        onCreateBoard = onCreateBoard,
                    )
                }
                // The last row in the sheet, under every server's teams: a
                // team is the one thing the tree above can't offer to make.
                item(key = "new-team") {
                    MutedActionRow(
                        label = "New team",
                        testTag = "board-switcher-new-team",
                        onClick = onCreateTeam,
                    )
                }
            }
        }
    }
}

@Composable
private fun ServerSection(
    group: ServerBoardGroup,
    showServerHeader: Boolean,
    currentBoardId: String?,
    onSelect: (accountId: String, boardId: String) -> Unit,
    onCreateBoard: (teamId: String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        // The server/email header only disambiguates when several accounts are
        // signed in — with a single account it's noise (iOS parity).
        if (showServerHeader) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        group.hostname,
                        style = MaterialTheme.typography.titleSmall,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    if (!group.userEmail.isNullOrBlank()) {
                        Text(
                            group.userEmail,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
            }
        }
        group.teamBlocks.forEach { block ->
            TeamBlockView(
                accountId = group.accountId,
                block = block,
                currentBoardId = currentBoardId,
                onSelect = onSelect,
                onCreateBoard = onCreateBoard,
            )
        }
    }
}

@Composable
private fun TeamBlockView(
    accountId: String,
    block: TeamBlock,
    currentBoardId: String?,
    onSelect: (accountId: String, boardId: String) -> Unit,
    onCreateBoard: (teamId: String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TeamAvatar(block.team, size = 18.dp)
            Spacer(Modifier.width(8.dp))
            // EXP-698 r5: no board COUNT — the rows below are the count,
            // and the number read as an unread badge.
            Text(
                block.team.name,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
        }
        block.boards.forEach { board ->
            key(board.id) {
                BoardRow(
                    board = board,
                    active = board.id == currentBoardId,
                    onClick = { onSelect(accountId, board.id) },
                )
            }
        }
        MutedActionRow(
            label = "Create board",
            testTag = "board-switcher-create-board",
            onClick = { onCreateBoard(block.team.id) },
        )
    }
}

/**
 * The switcher's two make-something rows. Deliberately NOT carded: a board row
 * is a thing you can switch to, and drawing "Create board" the same way would
 * have made the list of boards look one longer than it is.
 */
@Composable
private fun MutedActionRow(label: String, testTag: String, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            // The capture suites address these two by tag — their labels also
            // read as ordinary content elsewhere on the sheet.
            .testTag(testTag)
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            ExpIcons.uiAdd,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.width(10.dp))
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

/**
 * EXP-778: the "Pinned" group — the caller's own pins in the active team, each
 * row drawn like the list it came from: an issue as identifier + title, a
 * coding session with the Agent page's state dot + identity line, an action
 * as its glyph + name. Tapping opens the target (issue detail, the steering
 * screen, the composer with the action preselected).
 */
@Composable
private fun PinnedSection(rows: List<PinnedRow>, onOpen: (PinnedRow) -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth().testTag("board-switcher-pinned"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                ExpIcons.uiPin,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                "Pinned",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        rows.forEach { row ->
            key(row.pin.id) {
                PinnedRowView(row = row, onClick = { onOpen(row) })
            }
        }
    }
}

@Composable
private fun PinnedRowView(row: PinnedRow, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        when (row) {
            is PinnedRow.Issue -> {
                Text(
                    row.issue.identifier,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    fontFamily = FontFamily.Monospace,
                )
                Spacer(Modifier.width(10.dp))
                Text(
                    row.issue.title.ifBlank { "Untitled issue" },
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
            }
            is PinnedRow.Session -> {
                // The Agent page's row look (AgentSessionsList): the shared
                // identity line with the state dot in front. No device join
                // here — the sheet is a launcher, the list carries the rest.
                val state = codingSessionDisplayState(row.session, row.issue?.prState ?: row.session.prState)
                SessionRowTitle(
                    identifier = sessionRowIdentifier(row.issue),
                    title = sessionRowTitle(row.session, row.issue),
                    modifier = Modifier.weight(1f),
                    dot = {
                        when (state) {
                            // EXP-848: pulses only mid-turn (agent_busy).
                            CodingSessionDisplayState.Running ->
                                LiveDot(busy = row.session.agentBusy)
                            CodingSessionDisplayState.NeedsInput -> StaticDot(NeedsInputAmber)
                            CodingSessionDisplayState.Review -> StaticDot(ReviewGreen)
                            CodingSessionDisplayState.Done -> StaticDot(DoneBlue)
                        }
                    },
                )
            }
            is PinnedRow.Action -> {
                Icon(
                    row.action.icon?.takeIf { it.isNotEmpty() }?.let { ExpIcons.byName(it) }
                        ?: ExpIcons.actionDefault,
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
                Spacer(Modifier.width(10.dp))
                Text(
                    row.action.name,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
            }
        }
        Spacer(Modifier.width(8.dp))
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
            modifier = Modifier.size(16.dp),
        )
    }
}
