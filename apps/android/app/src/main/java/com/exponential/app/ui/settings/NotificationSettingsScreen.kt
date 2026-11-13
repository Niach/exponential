package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.NotificationsApi
import com.exponential.app.data.api.SourcePrefsDto
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.NotificationDelivery
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.glassGroup
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * #EXP-1815 (notification-delivery.json SETTINGS): Settings › Notifications,
 * the member's source switches. Rows = [NotificationDelivery.settingsRows]
 * over the synced teams + agents; the switches live server-side
 * (`notifications.emailPrefs` / `updateEmailPrefs`, the WHOLE object).
 */
@HiltViewModel
class NotificationSettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val api: NotificationsApi,
    holder: DatabaseHolder,
) : ViewModel() {
    private val db = accountDatabaseFlow(auth, holder)

    val groups: StateFlow<List<NotificationDelivery.SettingsGroup>> = combine(
        db.scopedQuery(emptyList()) { it.teamDao().observeAll() },
        db.scopedQuery(emptyList()) { it.agentDefinitionDao().observeAll() },
    ) { teams, agents ->
        NotificationDelivery.settingsRows(
            teams.map { NotificationDelivery.SettingsTeam(it.id, it.name) },
            // S-3: a missing role reads as `agent`.
            agents.map { NotificationDelivery.SettingsAgent(it.id, it.teamId, it.name, it.role ?: "agent") },
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), NotificationDelivery.settingsRows(emptyList(), emptyList()))

    /** null = not loaded yet (the switches wait, disabled). */
    private val _prefs = MutableStateFlow<NotificationDelivery.SourcePrefs?>(null)
    val prefs: StateFlow<NotificationDelivery.SourcePrefs?> = _prefs.asStateFlow()

    private val _failures = MutableSharedFlow<String>(extraBufferCapacity = 1)
    val failures: SharedFlow<String> = _failures.asSharedFlow()

    init {
        load()
    }

    private fun load() {
        val account = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            try {
                _prefs.value = api.sourcePrefs(account).toDomain()
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                _failures.tryEmit("Couldn't load notification settings")
            }
        }
    }

    fun toggle(key: String, on: Boolean) {
        val account = auth.activeAccountId.value ?: return
        val before = _prefs.value ?: return
        val after = NotificationDelivery.toggleSourcePref(before, key, on)
        _prefs.value = after
        viewModelScope.launch {
            try {
                api.updateSourcePrefs(account, after.toDto())
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                // A failed action = THE toast, and the switch goes back.
                if (_prefs.value == after) _prefs.value = before
                _failures.tryEmit("Couldn't save notification settings")
            }
        }
    }
}

private fun SourcePrefsDto.toDomain() = NotificationDelivery.SourcePrefs(assistant, temporary, agents)
private fun NotificationDelivery.SourcePrefs.toDto() = SourcePrefsDto(assistant, temporary, agents)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NotificationSettingsScreen(
    onBack: () -> Unit,
    viewModel: NotificationSettingsViewModel = hiltViewModel(),
) {
    val groups by viewModel.groups.collectAsStateWithLifecycle()
    val prefs by viewModel.prefs.collectAsStateWithLifecycle()
    val toaster = LocalToaster.current
    LaunchedEffect(viewModel) { viewModel.failures.collect { toaster.error(it) } }

    NotificationSettingsContent(groups, prefs, onBack, viewModel::toggle)
}

/** The page itself: the bands of [groups], each row a switch over [prefs] (null = loading). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NotificationSettingsContent(
    groups: List<NotificationDelivery.SettingsGroup>,
    prefs: NotificationDelivery.SourcePrefs?,
    onBack: () -> Unit,
    onToggle: (key: String, on: Boolean) -> Unit,
) {
    AppBackground {
        Scaffold(
            topBar = {
                CenterAlignedTopAppBar(
                    title = { TopBarTitle("Notifications") },
                    navigationIcon = { TopBarBackButton(onClick = onBack) },
                    colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
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
                groups.forEach { group ->
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        SectionHeader(group.band, loading = prefs == null)
                        Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                            group.rows.forEach { row ->
                                SwitchRow(
                                    title = row.label,
                                    description = row.detail,
                                    checked = NotificationDelivery.switchOn(prefs, row.key),
                                    enabled = prefs != null,
                                    onCheckedChange = { on -> onToggle(row.key, on) },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}
