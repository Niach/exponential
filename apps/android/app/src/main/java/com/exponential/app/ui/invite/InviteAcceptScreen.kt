package com.exponential.app.ui.invite

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
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
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.InvitePreview
import com.exponential.app.data.api.WorkspaceInvitesApi
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class InviteAcceptState(
    val loading: Boolean = true,
    val preview: InvitePreview? = null,
    val accepting: Boolean = false,
    val acceptedWorkspaceName: String? = null,
    val error: String? = null,
)

@HiltViewModel
class InviteAcceptViewModel @Inject constructor(
    private val invitesApi: WorkspaceInvitesApi,
    private val auth: com.exponential.app.data.auth.AuthRepository,
) : ViewModel() {
    private val _state = MutableStateFlow(InviteAcceptState())
    val state: StateFlow<InviteAcceptState> = _state.asStateFlow()

    fun load(token: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _state.value = _state.value.copy(loading = true, error = null)
            runCatching { invitesApi.getByToken(accountId, token) }
                .onSuccess { _state.value = _state.value.copy(loading = false, preview = it) }
                .onFailure { _state.value = _state.value.copy(loading = false, error = it.message) }
        }
    }

    fun accept(token: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _state.value = _state.value.copy(accepting = true, error = null)
            runCatching { invitesApi.accept(accountId, token) }
                .onSuccess {
                    _state.value = _state.value.copy(
                        accepting = false,
                        acceptedWorkspaceName = it.workspace.name,
                    )
                }
                .onFailure { _state.value = _state.value.copy(accepting = false, error = it.message) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun InviteAcceptScreen(
    token: String,
    onBack: () -> Unit,
    onAccepted: () -> Unit,
    viewModel: InviteAcceptViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    LaunchedEffect(token) { viewModel.load(token) }
    LaunchedEffect(state.acceptedWorkspaceName) {
        if (state.acceptedWorkspaceName != null) onAccepted()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Team invite") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = androidx.compose.ui.graphics.Color.Transparent,
                ),
            )
        },
        containerColor = androidx.compose.ui.graphics.Color.Transparent,
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .padding(24.dp)
                .fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            when {
                state.loading -> CircularProgressIndicator()
                state.error != null -> Text(
                    state.error!!,
                    color = MaterialTheme.colorScheme.error,
                )
                state.preview != null -> {
                    val preview = state.preview!!
                    Card(
                        colors = CardDefaults.cardColors(
                            containerColor = MaterialTheme.colorScheme.surfaceContainer,
                        ),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(
                                "You've been invited to",
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            Text(
                                preview.workspaceName,
                                style = MaterialTheme.typography.headlineSmall,
                            )
                            Text(
                                "as ${preview.role}",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            if (preview.acceptedAt != null) {
                                Spacer(Modifier.height(4.dp))
                                Text(
                                    "This invite has already been used.",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.error,
                                )
                            }
                        }
                    }
                    Button(
                        onClick = { viewModel.accept(token) },
                        enabled = !state.accepting && preview.acceptedAt == null,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        if (state.accepting) Text("Joining…") else Text("Accept invite")
                    }
                }
            }
        }
    }
}
