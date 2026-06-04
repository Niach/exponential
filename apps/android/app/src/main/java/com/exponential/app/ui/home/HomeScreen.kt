package com.exponential.app.ui.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Inbox
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.data.db.ServerProjectGroup
import com.exponential.app.data.db.WorkspaceBlock
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.ProjectRow
import com.exponential.app.ui.components.WorkspaceAvatar
import com.exponential.app.ui.theme.TextEmphasis

/**
 * iOS-style "Projects" home: a single scrolling list of every signed-in
 * account's workspaces and projects (server → workspace → project), shown
 * inline with no drawer and no workspace switcher. Settings + account live in
 * the top bar (gear + avatar), and everything is a push onto the back stack.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    onOpenProject: (accountId: String, projectId: String) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenInbox: () -> Unit = {},
    viewModel: HomeViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val error by viewModel.error.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { viewModel.bootstrap() }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            TopAppBar(
                title = { Text("Projects") },
                // iOS Home shows only a gear; account sign-out lives per-server
                // in Settings → server detail.
                actions = {
                    IconButton(onClick = onOpenInbox) {
                        Icon(Icons.Filled.Inbox, contentDescription = "Inbox")
                    }
                    IconButton(onClick = onOpenSettings) {
                        Icon(Icons.Filled.Settings, contentDescription = "Settings")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
            if (state.projectTree.isEmpty()) {
                when {
                    error != null -> EmptyState(message = error!!, icon = Icons.Filled.Inbox)
                    state.isSyncing -> LoadingState()
                    else -> EmptyState(message = "No projects yet", icon = Icons.Filled.Inbox)
                }
            } else {
                ProjectTree(
                    groups = state.projectTree,
                    onOpenProject = { accountId, projectId ->
                        val sameServer = viewModel.onProjectTap(accountId, projectId)
                        if (sameServer) onOpenProject(accountId, projectId)
                    },
                )
            }
        }
    }
}

@Composable
private fun ProjectTree(
    groups: List<ServerProjectGroup>,
    onOpenProject: (accountId: String, projectId: String) -> Unit,
) {
    LazyColumn(
        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        items(groups, key = { it.accountId }) { group ->
            ServerSection(group = group, onOpenProject = onOpenProject)
        }
    }
}

@Composable
private fun ServerSection(
    group: ServerProjectGroup,
    onOpenProject: (accountId: String, projectId: String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(modifier = Modifier.padding(horizontal = 4.dp)) {
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
        group.workspaceBlocks.forEach { block ->
            WorkspaceBlockView(
                accountId = group.accountId,
                block = block,
                onOpenProject = onOpenProject,
            )
        }
    }
}

@Composable
private fun WorkspaceBlockView(
    accountId: String,
    block: WorkspaceBlock,
    onOpenProject: (accountId: String, projectId: String) -> Unit,
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
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.85f),
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
                    onClick = { onOpenProject(accountId, project.id) },
                )
            }
        }
    }
}
