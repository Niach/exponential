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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.ServerBoardGroup
import com.exponential.app.data.db.TeamBlock
import com.exponential.app.ui.components.BoardRow
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

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
) {
    GlassSheet(title = "Switch board", onDismiss = onDismiss) {
        if (groups.isEmpty()) {
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
                    MutedActionRow(label = "New team", onClick = onCreateTeam)
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
private fun MutedActionRow(label: String, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
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
