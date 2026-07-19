package com.exponential.app.ui.home

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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.ServerBoardGroup
import com.exponential.app.data.db.TeamBlock
import com.exponential.app.ui.components.BoardRow
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The inline board switcher: a bottom sheet presenting every signed-in
 * account's teams and boards (server → team → board). This is
 * the old Boards home screen's tree, relocated — picking a board swaps the
 * Issues tab's list in place instead of pushing a new destination.
 *
 * A "New board" action at the foot opens the create-board sheet (the mobile
 * app now creates boards directly, with an inline GitHub repo connect).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BoardSwitcherSheet(
    groups: List<ServerBoardGroup>,
    onSelect: (accountId: String, boardId: String) -> Unit,
    onDismiss: () -> Unit,
    onCreateBoard: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = GlassTokens.BackgroundBottom,
    ) {
        if (groups.isEmpty()) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp, vertical = 32.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Text(
                    "No boards yet. Create your first board to get started.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
                Button(onClick = onCreateBoard, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("New board")
                }
            }
        } else {
            LazyColumn(
                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(18.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                items(groups, key = { it.accountId }) { group ->
                    ServerSection(group = group, onSelect = onSelect)
                }
                item(key = "__new_board__") {
                    Button(onClick = onCreateBoard, modifier = Modifier.fillMaxWidth()) {
                        Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(8.dp))
                        Text("New board")
                    }
                }
            }
        }
    }
}

@Composable
private fun ServerSection(
    group: ServerBoardGroup,
    onSelect: (accountId: String, boardId: String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
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
        group.teamBlocks.forEach { block ->
            TeamBlockView(
                accountId = group.accountId,
                block = block,
                onSelect = onSelect,
            )
        }
    }
}

@Composable
private fun TeamBlockView(
    accountId: String,
    block: TeamBlock,
    onSelect: (accountId: String, boardId: String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TeamAvatar(block.team, size = 18.dp)
            Spacer(Modifier.width(8.dp))
            Text(
                block.team.name,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
            Text(
                "${block.boards.size}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        block.boards.forEach { board ->
            key(board.id) {
                BoardRow(
                    board = board,
                    onClick = { onSelect(accountId, board.id) },
                )
            }
        }
    }
}
