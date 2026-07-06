package com.exponential.app.ui.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.ServerProjectGroup
import com.exponential.app.data.db.WorkspaceBlock
import com.exponential.app.ui.components.ProjectRow
import com.exponential.app.ui.components.WorkspaceAvatar
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The inline project switcher: a bottom sheet presenting every signed-in
 * account's workspaces and projects (server → workspace → project). This is
 * the old Projects home screen's tree, relocated — picking a project swaps the
 * Issues tab's list in place instead of pushing a new destination.
 *
 * The mobile app is a pure companion (masterplan L26): workspaces and projects
 * are created on the web or desktop app, so the sheet only ever reads the
 * synced tree — no create-project entry point lives here.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ProjectSwitcherSheet(
    groups: List<ServerProjectGroup>,
    onSelect: (accountId: String, projectId: String) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = GlassTokens.BackgroundBottom,
    ) {
        if (groups.isEmpty()) {
            Text(
                "No projects yet. Create your first project on the web or desktop app.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp, vertical = 32.dp),
            )
        } else {
            LazyColumn(
                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(18.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                items(groups, key = { it.accountId }) { group ->
                    ServerSection(group = group, onSelect = onSelect)
                }
            }
        }
    }
}

@Composable
private fun ServerSection(
    group: ServerProjectGroup,
    onSelect: (accountId: String, projectId: String) -> Unit,
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
        group.workspaceBlocks.forEach { block ->
            WorkspaceBlockView(
                accountId = group.accountId,
                block = block,
                onSelect = onSelect,
            )
        }
    }
}

@Composable
private fun WorkspaceBlockView(
    accountId: String,
    block: WorkspaceBlock,
    onSelect: (accountId: String, projectId: String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            WorkspaceAvatar(block.workspace, size = 18.dp)
            Spacer(Modifier.width(8.dp))
            Text(
                block.workspace.name,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
            Text(
                "${block.projects.size}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        block.projects.forEach { project ->
            key(project.id) {
                ProjectRow(
                    project = project,
                    onClick = { onSelect(accountId, project.id) },
                )
            }
        }
    }
}
