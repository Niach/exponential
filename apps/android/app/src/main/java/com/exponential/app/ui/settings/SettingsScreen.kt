package com.exponential.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AdminPanelSettings
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.MultiAccountWorkspaceRepository
import com.exponential.app.data.db.ServerWorkspaceGroup
import com.exponential.app.ui.nav.LocalDrawerOpener
import com.exponential.app.ui.nav.WorkspaceAvatar
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val selection: WorkspaceSelection,
    multiAccountWorkspaces: MultiAccountWorkspaceRepository,
) : ViewModel() {
    val isAdmin: StateFlow<Boolean> = auth.isAdmin
    val instanceUrl: StateFlow<String?> = auth.instanceUrl
    val accounts: StateFlow<List<ServerAccount>> = auth.accounts
    val serverGroups: StateFlow<List<ServerWorkspaceGroup>> =
        multiAccountWorkspaces.serverGroups.stateIn(
            viewModelScope,
            SharingStarted.WhileSubscribed(5_000),
            emptyList(),
        )

    // startAddServer removed — Settings now navigates to the instance route
    // directly via the onAddServer callback.

    /// Settings → Workspaces tap. Same-server taps just `select(workspaceId)`
    /// then the caller navigates to "workspace-settings"; cross-server taps
    /// also flip a pending flag that AuthenticatedShell consumes after the
    /// `key(activeAccountId)` rebuild to push the route on the new NavHost.
    /// Returns whether the caller can navigate immediately.
    fun onWorkspaceSettingsTap(accountId: String, workspaceId: String): Boolean {
        selection.select(workspaceId)
        return if (accountId == auth.activeAccountId.value) {
            true
        } else {
            selection.setPendingWorkspaceSettings()
            auth.switchAccount(accountId)
            false
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    onOpenIntegrations: () -> Unit,
    onOpenServerDetail: (accountId: String) -> Unit,
    onOpenWorkspaceSettings: () -> Unit,
    onOpenAdminUsers: () -> Unit,
    onOpenAdminWorkspaces: () -> Unit,
    onAddServer: () -> Unit,
    viewModel: SettingsViewModel = hiltViewModel(),
) {
    val isAdmin by viewModel.isAdmin.collectAsState()
    val instanceUrl by viewModel.instanceUrl.collectAsState()
    val accounts by viewModel.accounts.collectAsState()
    val serverGroups by viewModel.serverGroups.collectAsState()
    val openDrawer = LocalDrawerOpener.current
    val context = androidx.compose.ui.platform.LocalContext.current

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = { openDrawer() }) {
                        Icon(Icons.Filled.Menu, contentDescription = "Menu")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                ),
            )
        },
        containerColor = MaterialTheme.colorScheme.background,
    ) { padding ->
        LazyColumn(
            modifier = Modifier.padding(padding).fillMaxSize(),
            contentPadding = PaddingValues(vertical = 8.dp),
        ) {
            // Servers section.
            item { SectionHeader("Servers") }
            items(accounts.size, key = { idx -> accounts[idx].id }) { idx ->
                val account = accounts[idx]
                ServerRow(
                    account = account,
                    onClick = { onOpenServerDetail(account.id) },
                )
            }
            item {
                ListItem(
                    headlineContent = { Text("Add server") },
                    leadingContent = { Icon(Icons.Filled.Add, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    modifier = Modifier.clickable { onAddServer() },
                )
                HorizontalDivider()
            }

            // Workspaces section.
            item { SectionHeader("Workspaces") }
            if (serverGroups.isEmpty()) {
                item {
                    Text(
                        "No workspaces synced yet.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
                    )
                }
            } else {
                for (group in serverGroups) {
                    item("group-header-${group.accountId}") {
                        WorkspaceGroupHeader(group)
                    }
                    items(
                        group.workspaces.size,
                        key = { idx -> "ws-${group.accountId}-${group.workspaces[idx].id}" },
                    ) { idx ->
                        val workspace = group.workspaces[idx]
                        ListItem(
                            headlineContent = { Text(workspace.name) },
                            leadingContent = { WorkspaceAvatar(workspace, size = 22.dp) },
                            trailingContent = { Icon(Icons.Filled.ChevronRight, contentDescription = null) },
                            colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                            modifier = Modifier.clickable {
                                val sameServer = viewModel.onWorkspaceSettingsTap(group.accountId, workspace.id)
                                if (sameServer) onOpenWorkspaceSettings()
                            },
                        )
                    }
                }
            }
            item { HorizontalDivider() }

            // General section.
            item { SectionHeader("General") }
            item {
                ListItem(
                    headlineContent = { Text("Integrations") },
                    supportingContent = { Text("Google Calendar, push notifications") },
                    leadingContent = { Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null) },
                    trailingContent = { Icon(Icons.Filled.ChevronRight, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    modifier = Modifier.clickable { onOpenIntegrations() },
                )
                HorizontalDivider()
            }
            item {
                val canOpen = instanceUrl != null
                ListItem(
                    headlineContent = { Text("Send feedback") },
                    supportingContent = { Text("Open the feedback workspace") },
                    leadingContent = { Icon(Icons.Filled.Email, contentDescription = null) },
                    trailingContent = { Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    modifier = Modifier.clickable(enabled = canOpen) {
                        val base = instanceUrl ?: return@clickable
                        val url = "$base/w/feedback/projects/feedback"
                        runCatching {
                            val intent = android.content.Intent(
                                android.content.Intent.ACTION_VIEW,
                                android.net.Uri.parse(url),
                            )
                            intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                            context.startActivity(intent)
                        }
                    },
                )
                HorizontalDivider()
            }

            // Admin section (only when current user is admin on the active server).
            if (isAdmin) {
                item { SectionHeader("Admin") }
                item {
                    ListItem(
                        headlineContent = { Text("Users") },
                        leadingContent = { Icon(Icons.Filled.AdminPanelSettings, contentDescription = null) },
                        trailingContent = { Icon(Icons.Filled.ChevronRight, contentDescription = null) },
                        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                        modifier = Modifier.clickable { onOpenAdminUsers() },
                    )
                    HorizontalDivider()
                }
                item {
                    ListItem(
                        headlineContent = { Text("Workspaces") },
                        leadingContent = { Icon(Icons.Filled.AdminPanelSettings, contentDescription = null) },
                        trailingContent = { Icon(Icons.Filled.ChevronRight, contentDescription = null) },
                        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                        modifier = Modifier.clickable { onOpenAdminWorkspaces() },
                    )
                    HorizontalDivider()
                }
            }
        }
    }
}

@Composable
private fun SectionHeader(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(horizontal = 24.dp, vertical = 10.dp),
    )
}

@Composable
private fun ServerRow(account: ServerAccount, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(account.displayHost) },
        supportingContent = {
            when {
                account.token == null -> Text(
                    "Signed out",
                    color = MaterialTheme.colorScheme.tertiary,
                )
                !account.userEmail.isNullOrBlank() -> Text(
                    account.userEmail!!,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                else -> Text("Signed in")
            }
        },
        leadingContent = { Icon(Icons.Filled.Dns, contentDescription = null) },
        trailingContent = { Icon(Icons.Filled.ChevronRight, contentDescription = null) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
        modifier = Modifier.clickable(onClick = onClick),
    )
}

@Composable
private fun WorkspaceGroupHeader(group: ServerWorkspaceGroup) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 24.dp, vertical = 8.dp),
    ) {
        Text(
            group.hostname,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (!group.userEmail.isNullOrBlank()) {
            Text(
                group.userEmail!!,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
