package com.exponential.app.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.MultiAccountTeamRepository
import com.exponential.app.data.db.ServerTeamGroup
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassSection
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val selection: TeamSelection,
    multiAccountTeams: MultiAccountTeamRepository,
) : ViewModel() {
    val accounts: StateFlow<List<ServerAccount>> = auth.accounts
    val serverGroups: StateFlow<List<ServerTeamGroup>> =
        multiAccountTeams.serverGroups.stateIn(
            viewModelScope,
            SharingStarted.WhileSubscribed(5_000),
            emptyList(),
        )

    // startAddServer removed — Settings now navigates to the instance route
    // directly via the onAddServer callback.

    /// Settings → Teams tap. Selects the team and (for cross-server
    /// taps) makes its account active; the caller navigates immediately —
    /// TeamSettingsViewModel scopes to the active account reactively, so
    /// no rebuild/pending-handoff dance is needed.
    fun onTeamSettingsTap(accountId: String, teamId: String) {
        selection.select(teamId)
        if (accountId != auth.activeAccountId.value) {
            auth.switchAccount(accountId)
        }
    }
}

// iOS-parity Settings: a centered nav title, the zinc AppBackground gradient,
// and grouped glass sections (Servers / Teams / General) of
// leading-icon + title (+ optional subtitle) + trailing-chevron rows.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    onOpenServerDetail: (accountId: String) -> Unit,
    onOpenTeamSettings: () -> Unit,
    onOpenSyncDiagnostics: () -> Unit,
    onAddServer: () -> Unit,
    onBack: () -> Unit,
    viewModel: SettingsViewModel = hiltViewModel(),
) {
    val accounts by viewModel.accounts.collectAsStateWithLifecycle()
    val serverGroups by viewModel.serverGroups.collectAsStateWithLifecycle()

    AppBackground {
        Scaffold(
            topBar = {
                CenterAlignedTopAppBar(
                    title = { Text("Settings") },
                    navigationIcon = {
                        IconButton(onClick = onBack) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    },
                    colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                        containerColor = Color.Transparent,
                    ),
                )
            },
            containerColor = Color.Transparent,
        ) { padding ->
            Column(
                modifier = Modifier
                    .padding(padding)
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                // Servers section — one row per server account, then "Add server".
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    SectionHeader("Servers")
                    Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
                        accounts.forEachIndexed { i, account ->
                            if (i > 0) CardDivider()
                            ServerRow(account = account, onClick = { onOpenServerDetail(account.id) })
                        }
                        if (accounts.isNotEmpty()) CardDivider()
                        SettingsRow(
                            icon = Icons.Filled.Add,
                            title = "Add server",
                            showChevron = false,
                            onClick = onAddServer,
                        )
                    }
                }

                // Teams section — the switcher list: tapping a team
                // selects it (and switches the active account for cross-server
                // taps) before opening that team's settings.
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    SectionHeader("Teams")
                    if (serverGroups.isEmpty()) {
                        Text(
                            "No teams synced yet.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            modifier = Modifier.padding(horizontal = 4.dp, vertical = 4.dp),
                        )
                    } else {
                        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            serverGroups.forEach { group ->
                                TeamGroupBlock(
                                    group = group,
                                    onTeamTap = { teamId ->
                                        viewModel.onTeamSettingsTap(group.accountId, teamId)
                                        onOpenTeamSettings()
                                    },
                                )
                            }
                        }
                    }
                }

                // General section.
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    SectionHeader("General")
                    Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
                        SettingsRow(
                            icon = Icons.Filled.Sync,
                            title = "Sync diagnostics",
                            subtitle = "Live Electric shape status",
                            onClick = onOpenSyncDiagnostics,
                        )
                    }
                }
            }
        }
    }
}

// A navigational glass row: leading icon (22dp, secondary), title (+ optional
// subtitle), then a trailing chevron (quaternary) by default. iOS settingsRow.
@Composable
private fun SettingsRow(
    icon: ImageVector,
    title: String,
    subtitle: String? = null,
    trailingIcon: ImageVector = Icons.Filled.ChevronRight,
    showChevron: Boolean = true,
    onClick: () -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Icon(
            icon,
            contentDescription = null,
            modifier = Modifier.size(22.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                title,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            if (subtitle != null) {
                Text(
                    subtitle,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (showChevron) {
            Spacer(Modifier.width(8.dp))
            Icon(
                trailingIcon,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
            )
        }
    }
}

@Composable
private fun ServerRow(account: ServerAccount, onClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Icon(
            Icons.Filled.Dns,
            contentDescription = null,
            modifier = Modifier.size(22.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                account.displayHost,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            when {
                account.token == null -> Text(
                    "Signed out",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.tertiary,
                )
                !account.userEmail.isNullOrBlank() -> Text(
                    account.userEmail!!,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                else -> Text(
                    "Signed in",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        Spacer(Modifier.width(8.dp))
        Icon(
            Icons.Filled.ChevronRight,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
        )
    }
}

// One server's hostname header + a glass card of its teams (the switcher).
@Composable
private fun TeamGroupBlock(
    group: ServerTeamGroup,
    onTeamTap: (teamId: String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Column(modifier = Modifier.padding(horizontal = 4.dp)) {
            Text(
                group.hostname,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (!group.userEmail.isNullOrBlank()) {
                Text(
                    group.userEmail!!,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
            group.teams.forEachIndexed { i, team ->
                if (i > 0) CardDivider()
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onTeamTap(team.id) }
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    TeamAvatar(team, size = 22.dp)
                    Spacer(Modifier.width(12.dp))
                    Text(
                        team.name,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(Modifier.width(8.dp))
                    Icon(
                        Icons.Filled.ChevronRight,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                    )
                }
            }
        }
    }
}

// Hairline divider between grouped-card rows (iOS Divider white@6%).
@Composable
private fun CardDivider() {
    HorizontalDivider(thickness = 0.5.dp, color = Color.White.copy(alpha = 0.06f))
}
