package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.statusBarsPadding
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PinnedScaffold
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AgentsApi
import com.exponential.app.data.api.AssistantEnsurer
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.AgentSettings
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.icons.ExpIcons
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * #EXP-1987 (settings-nav.json, agent-settings.json `assistant`): Settings ›
 * Assistant is an ACCOUNT page: the viewer's ONE Assistant, the one in their personal team (a team none of the synced teams rows
 * name). `agents.ensureAssistant` goes out without a team, once per account.
 */
@HiltViewModel
class AssistantSettingsRouteViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    agentsApi: AgentsApi,
) : ViewModel() {
    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The ensure answered (or failed) and the row had its moment to sync. */
    private val settled = MutableStateFlow(false)

    init {
        viewModelScope.launch {
            AssistantEnsurer.ensureForAccount(agentsApi, auth.activeAccountId.value)
            delay(SETTLE_MS)
            settled.value = true
        }
    }

    sealed interface State {
        data object Loading : State
        data object NoAssistant : State
        data class Ready(val agentId: String) : State
    }

    val state: StateFlow<State> = combine(
        dbFlow.scopedQuery(emptyList<AgentDefinitionEntity>()) { it.agentDefinitionDao().observeAll() },
        dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() },
        auth.userId,
        settled,
    ) { agents, teams, me, done ->
        when (val id = assistantOf(agents, teams.map { it.id }, me)) {
            null -> if (done) State.NoAssistant else State.Loading
            else -> State.Ready(id)
        }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), State.Loading)

    companion object {
        private const val SETTLE_MS = 2_000L

        /** The viewer's own Assistant: the one in no synced team (their personal
         *  team), else any of theirs. */
        fun assistantOf(agents: List<AgentDefinitionEntity>, teamIds: List<String>, me: String?): String? {
            if (me.isNullOrEmpty()) return null
            val mine = agents.filter { it.isAssistant && it.ownerId == me }
            return (mine.firstOrNull { it.teamId !in teamIds } ?: mine.firstOrNull())?.id
        }
    }
}

/**
 * `settings/assistant`: the Assistant's own agent page as soon as its row is
 * known; until then the page's skeleton, and `copy.noDevice` when the
 * Assistant cannot be created yet (no device).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AssistantSettingsRoute(
    onBack: () -> Unit,
    viewModel: AssistantSettingsRouteViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    (state as? AssistantSettingsRouteViewModel.State.Ready)?.let { ready ->
        com.exponential.app.ui.apps.ArgsStore("agentId", ready.agentId) {
            AgentSettingsScreen(onBack = onBack, onCreated = {}, onDeleted = onBack)
        }
        return
    }
    PinnedScaffold(
        topBar = {
            PageHeader(
                title = AgentSettings.Assistant.TITLE,
                onBack = onBack,
                modifier = Modifier.statusBarsPadding(),
            )
        },
    ) { padding ->
        Box(Modifier.padding(padding).fillMaxSize()) {
            when (state) {
                AssistantSettingsRouteViewModel.State.NoAssistant -> EmptyState(
                    title = AgentSettings.Assistant.NO_DEVICE,
                    icon = ExpIcons.settingsAgents,
                )
                else -> SkeletonRows(modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
            }
        }
    }
}
