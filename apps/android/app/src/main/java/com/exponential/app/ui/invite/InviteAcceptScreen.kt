package com.exponential.app.ui.invite

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.InvitePreview
import com.exponential.app.data.api.TeamInvitesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.OwnDevices
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
import com.exponential.app.ui.onboarding.OnboardingDevicesStep
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import com.exponential.app.ui.theme.GlassTokens

data class InviteAcceptState(
    val loading: Boolean = true,
    val preview: InvitePreview? = null,
    val accepting: Boolean = false,
    val acceptedTeamName: String? = null,
    /** EXP-1169: accepted by a caller who owns no device, so the screen shows
     *  the devices step before leaving. */
    val showDeviceStep: Boolean = false,
    /** The invite itself could not be read: the page's "Invalid invite" state. */
    val error: String? = null,
    /** FEED-79: a failed accept, raised once as an error toast. */
    val failure: FailedAction? = null,
)

@HiltViewModel
class InviteAcceptViewModel @Inject constructor(
    private val invitesApi: TeamInvitesApi,
    private val auth: com.exponential.app.data.auth.AuthRepository,
    private val ownDevices: OwnDevices,
) : ViewModel() {
    private val _state = MutableStateFlow(InviteAcceptState())
    val state: StateFlow<InviteAcceptState> = _state.asStateFlow()

    val instanceOrigin: StateFlow<String?> = auth.instanceUrl

    fun load(token: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _state.value = _state.value.copy(loading = true, error = null)
            runCatching { invitesApi.getByToken(accountId, token) }
                .onSuccess { _state.value = _state.value.copy(loading = false, preview = it) }
                .onFailure {
                    _state.value = _state.value.copy(
                        loading = false,
                        error = trpcErrorMessage(it, "Couldn't load the invite."),
                    )
                }
        }
    }

    fun clearFailure() {
        _state.value = _state.value.copy(failure = null)
    }

    fun accept(token: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _state.value = _state.value.copy(accepting = true, failure = null)
            runCatching { invitesApi.accept(accountId, token) }
                .onSuccess {
                    val showDeviceStep = ownDevices.needsJoinDeviceStep()
                    _state.value = _state.value.copy(
                        accepting = false,
                        acceptedTeamName = it.team.name,
                        showDeviceStep = showDeviceStep,
                    )
                }
                .onFailure {
                    // An accept is idempotent (a member answers alreadyMember): Retry.
                    _state.value = _state.value.copy(
                        accepting = false,
                        failure = FailedAction("accept", "the invite", trpcErrorMessage(it, "")) { accept(token) },
                    )
                }
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
    val instanceOrigin by viewModel.instanceOrigin.collectAsStateWithLifecycle()

    LaunchedEffect(token) { viewModel.load(token) }
    FailedActionToast(state.failure, onShown = viewModel::clearFailure)
    // EXP-1169: a caller who owns no device sees the devices step first; its
    // Continue leaves instead.
    LaunchedEffect(state.acceptedTeamName, state.showDeviceStep) {
        if (state.acceptedTeamName != null && !state.showDeviceStep) onAccepted()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { TopBarTitle("Team invite") },
                navigationIcon = {
                    TopBarBackButton(onClick = onBack)
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
                state.showDeviceStep -> Column(
                    modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    OnboardingDevicesStep(instanceOrigin = instanceOrigin, onContinue = onAccepted)
                }
                // EXP-1275: skeleton rows, never a centred spinner.
                state.loading -> SkeletonRows(dot = false)
                // The invite could not be read (web parity): a state, not an error line.
                state.error != null -> Column(
                    modifier = Modifier.fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text("Invalid invite", style = MaterialTheme.typography.titleMedium)
                    Text(
                        state.error!!,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
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
                                preview.teamName,
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
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                    Button(
                        onClick = { viewModel.accept(token) },
                        enabled = !state.accepting && preview.acceptedAt == null,
                        shape = GlassTokens.ButtonShape,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        if (state.accepting) Text("Joining…") else Text("Accept invite")
                    }
                }
            }
        }
    }
}
