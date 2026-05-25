package com.exponential.app.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Menu
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
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.ServerProjectGroup
import com.exponential.app.data.db.WorkspaceBlock
import com.exponential.app.ui.nav.AvatarMenuButton
import com.exponential.app.ui.nav.LocalDrawerOpener
import com.exponential.app.ui.nav.WorkspaceAvatar

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    onOpenProject: (accountId: String, projectId: String) -> Unit,
    viewModel: HomeViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsState()
    val error by viewModel.error.collectAsState()
    val openDrawer = LocalDrawerOpener.current

    LaunchedEffect(Unit) { viewModel.bootstrap() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Projects") },
                navigationIcon = {
                    IconButton(onClick = { openDrawer() }) {
                        Icon(Icons.Filled.Menu, contentDescription = "Menu")
                    }
                },
                actions = { AvatarMenuButton() },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                ),
            )
        },
        containerColor = MaterialTheme.colorScheme.background,
    ) { padding ->
        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
            if (state.projectTree.isEmpty()) {
                EmptyState(message = error ?: "No projects yet — create one on the web.")
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
private fun EmptyState(message: String) {
    Box(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            message,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
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
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
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
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
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
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        block.projects.forEach { project ->
            ProjectRow(
                project = project,
                onClick = { onOpenProject(accountId, project.id) },
            )
        }
    }
}

@Composable
private fun ProjectRow(project: ProjectEntity, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(10.dp)
                .background(parseHex(project.color), shape = CircleShape),
        )
        Spacer(Modifier.width(12.dp))
        Text(
            project.name,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(12.dp))
        Text(
            project.prefix,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun parseHex(hex: String): Color {
    val cleaned = hex.removePrefix("#")
    return runCatching {
        Color(
            android.graphics.Color.parseColor(if (cleaned.length == 6) "#$cleaned" else "#FF$cleaned")
        )
    }.getOrElse { Color(0xFF6366F1) }
}
