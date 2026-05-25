package com.exponential.app.ui.integrations

import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IntegrationsApi
import com.exponential.app.data.auth.AuthRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class IntegrationsState(
    val loading: Boolean = true,
    val connected: Boolean = false,
    val connectedAt: String? = null,
    val scope: String? = null,
    val error: String? = null,
    val backfillResult: Int? = null,
    val backfilling: Boolean = false,
    val disconnecting: Boolean = false,
)

@HiltViewModel
class IntegrationsViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val integrationsApi: IntegrationsApi,
) : ViewModel() {
    val instanceUrl: StateFlow<String?> = auth.instanceUrl

    private val _state = MutableStateFlow(IntegrationsState())
    val state: StateFlow<IntegrationsState> = _state.asStateFlow()

    fun refresh() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _state.value = _state.value.copy(loading = true, error = null)
            runCatching { integrationsApi.googleStatus(accountId) }
                .onSuccess { status ->
                    _state.value = _state.value.copy(
                        loading = false,
                        connected = status.connected,
                        connectedAt = status.connectedAt,
                        scope = status.scope,
                    )
                }
                .onFailure {
                    _state.value = _state.value.copy(
                        loading = false,
                        error = it.message,
                    )
                }
        }
    }

    fun disconnect() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _state.value = _state.value.copy(disconnecting = true, error = null)
            runCatching { integrationsApi.googleDisconnect(accountId) }
                .onSuccess {
                    _state.value = _state.value.copy(
                        connected = false,
                        connectedAt = null,
                        scope = null,
                        disconnecting = false,
                    )
                }
                .onFailure {
                    _state.value = _state.value.copy(
                        disconnecting = false,
                        error = it.message,
                    )
                }
        }
    }

    fun backfill() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _state.value = _state.value.copy(backfilling = true, error = null, backfillResult = null)
            runCatching { integrationsApi.googleBackfill(accountId) }
                .onSuccess {
                    _state.value = _state.value.copy(
                        backfilling = false,
                        backfillResult = it.scheduled,
                    )
                }
                .onFailure {
                    _state.value = _state.value.copy(
                        backfilling = false,
                        error = it.message,
                    )
                }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IntegrationsScreen(
    onBack: () -> Unit,
    viewModel: IntegrationsViewModel = hiltViewModel(),
) {
    val context = LocalContext.current
    val instanceUrl by viewModel.instanceUrl.collectAsState()
    val state by viewModel.state.collectAsState()
    var confirmDisconnect by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) { viewModel.refresh() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Integrations") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    IconButton(onClick = { viewModel.refresh() }) {
                        Icon(Icons.Filled.Refresh, contentDescription = "Refresh")
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
            modifier = Modifier
                .padding(padding)
                .padding(16.dp)
                .fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                Card(
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surfaceContainer,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                "Google Calendar",
                                style = MaterialTheme.typography.titleMedium,
                                modifier = Modifier.weight(1f),
                            )
                            if (state.connected) {
                                Icon(
                                    Icons.Filled.CheckCircle,
                                    contentDescription = "Connected",
                                    tint = MaterialTheme.colorScheme.primary,
                                )
                            }
                        }
                        when {
                            state.loading -> CircularProgressIndicator()
                            state.connected -> ConnectedSection(
                                connectedAt = state.connectedAt,
                                backfillScheduled = state.backfillResult,
                                backfilling = state.backfilling,
                                disconnecting = state.disconnecting,
                                onBackfill = viewModel::backfill,
                                onDisconnect = { confirmDisconnect = true },
                            )
                            else -> DisconnectedSection(
                                instanceUrl = instanceUrl,
                                onConnect = {
                                    val url = "${instanceUrl}/account/integrations"
                                    val intent = CustomTabsIntent.Builder().build()
                                    intent.launchUrl(context, Uri.parse(url))
                                },
                            )
                        }
                        state.error?.let { error ->
                            Text(
                                error,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.error,
                            )
                        }
                    }
                }
            }
            item {
                Card(
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surfaceContainer,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(
                            "Push notifications",
                            style = MaterialTheme.typography.titleMedium,
                        )
                        Text(
                            "Push uses Firebase Cloud Messaging. The Android client registers an " +
                                "FCM token automatically when google-services.json is bundled with " +
                                "the build.",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
    }

    if (confirmDisconnect) {
        AlertDialog(
            onDismissRequest = { confirmDisconnect = false },
            title = { Text("Disconnect Google Calendar?") },
            text = { Text("Future issue due dates will no longer sync to your calendar. Already-synced events stay where they are.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDisconnect = false
                    viewModel.disconnect()
                }) { Text("Disconnect") }
            },
            dismissButton = {
                TextButton(onClick = { confirmDisconnect = false }) { Text("Cancel") }
            },
        )
    }
}

@Composable
private fun ConnectedSection(
    connectedAt: String?,
    backfillScheduled: Int?,
    backfilling: Boolean,
    disconnecting: Boolean,
    onBackfill: () -> Unit,
    onDisconnect: () -> Unit,
) {
    Text(
        "Issues with due dates sync as all-day events on your primary Google calendar.",
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    if (connectedAt != null) {
        Text(
            "Connected $connectedAt",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    backfillScheduled?.let {
        Text(
            "Scheduled $it existing issue${if (it == 1) "" else "s"} for sync.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.primary,
        )
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(
            onClick = onBackfill,
            enabled = !backfilling && !disconnecting,
            modifier = Modifier.weight(1f),
        ) {
            if (backfilling) {
                CircularProgressIndicator(modifier = Modifier.height(16.dp))
                Spacer(Modifier.height(0.dp))
                Text("  Syncing…")
            } else {
                Text("Backfill existing")
            }
        }
        OutlinedButton(
            onClick = onDisconnect,
            enabled = !backfilling && !disconnecting,
            modifier = Modifier.weight(1f),
        ) {
            if (disconnecting) Text("Disconnecting…") else Text("Disconnect")
        }
    }
}

@Composable
private fun DisconnectedSection(
    instanceUrl: String?,
    onConnect: () -> Unit,
) {
    Text(
        "Link your Google account to mirror issues with due dates as all-day calendar events.",
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Button(
        enabled = instanceUrl != null,
        onClick = onConnect,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Icon(Icons.AutoMirrored.Filled.OpenInNew, null)
        Spacer(Modifier.height(0.dp))
        Text("  Connect in browser")
    }
}
