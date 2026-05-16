package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.AdminPanelSettings
import androidx.compose.material.icons.filled.Group
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Person
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.ui.nav.LocalDrawerOpener
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.StateFlow

@HiltViewModel
class SettingsViewModel @Inject constructor(
    auth: AuthRepository,
) : ViewModel() {
    val email: StateFlow<String?> = auth.userEmail
    val isAdmin: StateFlow<Boolean> = auth.isAdmin
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    onOpenIntegrations: () -> Unit,
    onOpenWorkspaceSettings: () -> Unit,
    onOpenAdminUsers: () -> Unit,
    onOpenAdminWorkspaces: () -> Unit,
    onSignOut: () -> Unit,
    viewModel: SettingsViewModel = hiltViewModel(),
) {
    val email by viewModel.email.collectAsState()
    val isAdmin by viewModel.isAdmin.collectAsState()
    val openDrawer = LocalDrawerOpener.current

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
            item {
                ListItem(
                    headlineContent = { Text("Account") },
                    supportingContent = email?.let { { Text(it) } },
                    leadingContent = { Icon(Icons.Filled.Person, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                )
                HorizontalDivider()
            }
            item {
                ListItem(
                    headlineContent = { Text("Workspace") },
                    supportingContent = { Text("Members, invites, labels") },
                    leadingContent = { Icon(Icons.Filled.Group, contentDescription = null) },
                    modifier = Modifier.padding(0.dp),
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    trailingContent = {
                        IconButton(onClick = onOpenWorkspaceSettings) {
                            Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                        }
                    },
                )
                HorizontalDivider()
            }
            item {
                ListItem(
                    headlineContent = { Text("Integrations") },
                    supportingContent = { Text("Google Calendar, push notifications") },
                    leadingContent = { Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    trailingContent = {
                        IconButton(onClick = onOpenIntegrations) {
                            Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                        }
                    },
                )
                HorizontalDivider()
            }
            if (isAdmin) {
                item {
                    ListItem(
                        headlineContent = { Text("Admin · Users") },
                        leadingContent = { Icon(Icons.Filled.AdminPanelSettings, contentDescription = null) },
                        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                        trailingContent = {
                            IconButton(onClick = onOpenAdminUsers) {
                                Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                            }
                        },
                    )
                    HorizontalDivider()
                }
                item {
                    ListItem(
                        headlineContent = { Text("Admin · Workspaces") },
                        leadingContent = { Icon(Icons.Filled.AdminPanelSettings, contentDescription = null) },
                        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                        trailingContent = {
                            IconButton(onClick = onOpenAdminWorkspaces) {
                                Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                            }
                        },
                    )
                    HorizontalDivider()
                }
            }
            item {
                ListItem(
                    headlineContent = { Text("Sign out") },
                    leadingContent = { Icon(Icons.AutoMirrored.Filled.Logout, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    modifier = Modifier.padding(0.dp),
                    trailingContent = {
                        IconButton(onClick = onSignOut) {
                            Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Sign out")
                        }
                    },
                )
            }
        }
    }
}
