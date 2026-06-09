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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Inbox
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
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
    val scope = rememberCoroutineScope()

    // Create-workspace / create-project sheets. The non-null value carries the
    // account (and workspace) context the new entity belongs to.
    var createWorkspaceFor by remember { mutableStateOf<String?>(null) }
    var createProjectFor by remember { mutableStateOf<Pair<String, String>?>(null) }
    var creating by remember { mutableStateOf(false) }
    var createError by remember { mutableStateOf<String?>(null) }

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
                    onNewWorkspace = { accountId ->
                        createError = null
                        createWorkspaceFor = accountId
                    },
                    onNewProject = { accountId, workspaceId ->
                        createError = null
                        createProjectFor = accountId to workspaceId
                    },
                )
            }
        }
    }

    createWorkspaceFor?.let { accountId ->
        CreateWorkspaceSheet(
            isCreating = creating,
            error = createError,
            onDismiss = { createWorkspaceFor = null; createError = null },
            onCreate = { name ->
                scope.launch {
                    creating = true
                    val err = viewModel.createWorkspace(accountId, name)
                    creating = false
                    if (err == null) {
                        createWorkspaceFor = null
                        createError = null
                    } else {
                        createError = err
                    }
                }
            },
        )
    }

    createProjectFor?.let { (accountId, workspaceId) ->
        CreateProjectSheet(
            isCreating = creating,
            error = createError,
            onDismiss = { createProjectFor = null; createError = null },
            onCreate = { name, prefix, color ->
                scope.launch {
                    creating = true
                    val err = viewModel.createProject(accountId, workspaceId, name, prefix, color)
                    creating = false
                    if (err == null) {
                        createProjectFor = null
                        createError = null
                    } else {
                        createError = err
                    }
                }
            },
        )
    }
}

@Composable
private fun ProjectTree(
    groups: List<ServerProjectGroup>,
    onOpenProject: (accountId: String, projectId: String) -> Unit,
    onNewWorkspace: (accountId: String) -> Unit,
    onNewProject: (accountId: String, workspaceId: String) -> Unit,
) {
    LazyColumn(
        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        items(groups, key = { it.accountId }) { group ->
            ServerSection(
                group = group,
                onOpenProject = onOpenProject,
                onNewWorkspace = onNewWorkspace,
                onNewProject = onNewProject,
            )
        }
    }
}

@Composable
private fun ServerSection(
    group: ServerProjectGroup,
    onOpenProject: (accountId: String, projectId: String) -> Unit,
    onNewWorkspace: (accountId: String) -> Unit,
    onNewProject: (accountId: String, workspaceId: String) -> Unit,
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
            TextButton(onClick = { onNewWorkspace(group.accountId) }) {
                Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(4.dp))
                Text("Workspace")
            }
        }
        group.workspaceBlocks.forEach { block ->
            WorkspaceBlockView(
                accountId = group.accountId,
                block = block,
                onOpenProject = onOpenProject,
                onNewProject = onNewProject,
            )
        }
    }
}

@Composable
private fun WorkspaceBlockView(
    accountId: String,
    block: WorkspaceBlock,
    onOpenProject: (accountId: String, projectId: String) -> Unit,
    onNewProject: (accountId: String, workspaceId: String) -> Unit,
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
            IconButton(
                onClick = { onNewProject(accountId, block.workspace.id) },
                modifier = Modifier.size(28.dp),
            ) {
                Icon(
                    Icons.Filled.Add,
                    contentDescription = "New project",
                    modifier = Modifier.size(16.dp),
                )
            }
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
