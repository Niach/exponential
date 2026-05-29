package com.exponential.app.ui.nav

import com.exponential.app.ui.parseColor

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.ServerWorkspaceGroup
import com.exponential.app.data.db.WorkspaceEntity

@Composable
fun AppDrawer(
    serverGroups: List<ServerWorkspaceGroup>,
    activeAccountId: String?,
    selectedWorkspace: WorkspaceEntity?,
    projects: List<ProjectEntity>,
    email: String?,
    activeProjectId: String?,
    onSelectWorkspace: (accountId: String, workspaceId: String) -> Unit,
    onOpenProject: (String) -> Unit,
    onOpenIntegrations: () -> Unit,
    onSignOut: () -> Unit,
) {
    var workspaceMenuOpen by remember { mutableStateOf(false) }

    ModalDrawerSheet(
        modifier = Modifier.fillMaxHeight(),
        drawerContainerColor = MaterialTheme.colorScheme.background,
    ) {
        Column(modifier = Modifier.fillMaxWidth()) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 12.dp),
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { workspaceMenuOpen = true },
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    WorkspaceAvatar(selectedWorkspace)
                    Spacer(Modifier.width(10.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            selectedWorkspace?.name ?: "Workspace",
                            style = MaterialTheme.typography.titleMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (email != null) {
                            Text(
                                email,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    Icon(Icons.Filled.KeyboardArrowDown, contentDescription = null)
                }
                DropdownMenu(
                    expanded = workspaceMenuOpen,
                    onDismissRequest = { workspaceMenuOpen = false },
                ) {
                    serverGroups.forEachIndexed { idx, group ->
                        if (idx > 0) HorizontalDivider()
                        ServerGroupHeader(hostname = group.hostname, userEmail = group.userEmail)
                        group.workspaces.forEach { ws ->
                            val isActive = group.accountId == activeAccountId
                                && ws.id == selectedWorkspace?.id
                            DropdownMenuItem(
                                text = { Text(ws.name) },
                                leadingIcon = {
                                    if (isActive) {
                                        Icon(Icons.Filled.Check, null)
                                    } else {
                                        WorkspaceAvatar(ws, size = 20.dp)
                                    }
                                },
                                onClick = {
                                    onSelectWorkspace(group.accountId, ws.id)
                                    workspaceMenuOpen = false
                                },
                            )
                        }
                    }
                }
            }

            HorizontalDivider()

            Text(
                "Projects",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            )

            LazyColumn(modifier = Modifier.weight(1f)) {
                items(projects, key = { it.id }) { project ->
                    DrawerProjectRow(
                        project = project,
                        active = project.id == activeProjectId,
                        onClick = { onOpenProject(project.id) },
                    )
                }
            }

            HorizontalDivider()

            DrawerActionRow(
                label = "Integrations",
                icon = Icons.AutoMirrored.Filled.OpenInNew,
                onClick = onOpenIntegrations,
            )
            DrawerActionRow(
                label = "Sign out",
                icon = Icons.AutoMirrored.Filled.Logout,
                onClick = onSignOut,
            )
            Spacer(Modifier.height(8.dp))
        }
    }
}

@Composable
private fun ServerGroupHeader(hostname: String, userEmail: String?) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 6.dp),
    ) {
        Text(
            hostname,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (!userEmail.isNullOrBlank()) {
            Text(
                userEmail,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun DrawerProjectRow(
    project: ProjectEntity,
    active: Boolean,
    onClick: () -> Unit,
) {
    val bg = if (active)
        MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.6f)
    else Color.Transparent
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(bg)
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(10.dp)
                .background(parseColor(project.color), CircleShape),
        )
        Spacer(Modifier.width(12.dp))
        Text(
            project.name,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Text(
            project.prefix,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun DrawerActionRow(
    label: String,
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(icon, contentDescription = null)
        Text(label, style = MaterialTheme.typography.bodyMedium)
    }
}

// Workspace tile: shows the icon_url image when set, otherwise the first
// letter of the workspace name on a colored chip. Keeps the workspace
// switcher visually grounded once an org sets a brand mark.
@Composable
fun WorkspaceAvatar(workspace: WorkspaceEntity?, size: androidx.compose.ui.unit.Dp = 28.dp) {
    val initial = (workspace?.name?.firstOrNull()?.toString() ?: "?").uppercase()
    val url = workspace?.iconUrl?.takeIf { it.isNotBlank() }
    Box(
        modifier = Modifier
            .size(size)
            .background(
                MaterialTheme.colorScheme.primary.copy(alpha = 0.7f),
                androidx.compose.foundation.shape.RoundedCornerShape(size / 4),
            ),
        contentAlignment = Alignment.Center,
    ) {
        if (url != null) {
            coil3.compose.AsyncImage(
                model = url,
                contentDescription = null,
                modifier = Modifier
                    .size(size)
                    .background(
                        Color.Transparent,
                        androidx.compose.foundation.shape.RoundedCornerShape(size / 4),
                    ),
            )
        } else {
            Text(
                initial,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onPrimary,
            )
        }
    }
}
