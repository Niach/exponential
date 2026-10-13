package com.exponential.app.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.AgentSettings
import com.exponential.app.ui.components.TopBarBackButton

/**
 * EXP-1291: Team settings → Agents, the LIST (lane AGENTS builds it): one row
 * per agent of the selected team (icon + color + name like a board row) then
 * `New agent`. Embeddable (a settings section) — no Scaffold of its own.
 *
 * STUB (compiling, minimal body) — final signature.
 */
@Composable
fun AgentsSettingsList(
    onOpenAgent: (agentSessionId: String) -> Unit,
    onNewAgent: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: AgentSettingsViewModel = hiltViewModel(),
) {
    val agents by viewModel.agents.collectAsStateWithLifecycle()
    Column(modifier = modifier.testTag(AgentSettings.TEST_ID_LIST)) {
        agents.forEach { row ->
            Text(
                row.name,
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { onOpenAgent(row.sessionId) }
                    .padding(16.dp),
            )
        }
        Text(
            AgentSettings.Copy.NEW_AGENT,
            modifier = Modifier
                .fillMaxWidth()
                .clickable(onClick = onNewAgent)
                .padding(16.dp),
        )
    }
}

/**
 * EXP-1291: the Agents settings PAGE (`settings/agents`) — a pushed screen
 * hosting [AgentsSettingsList].
 *
 * STUB (compiling, minimal body) — final signature.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentsSettingsScreen(
    onBack: () -> Unit,
    onOpenAgent: (agentSessionId: String) -> Unit,
    onNewAgent: () -> Unit,
) {
    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(AgentSettings.Copy.TITLE) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        AgentsSettingsList(
            onOpenAgent = onOpenAgent,
            onNewAgent = onNewAgent,
            modifier = Modifier.padding(padding).fillMaxSize(),
        )
    }
}

/**
 * EXP-1291: ONE agent's settings page (`settings/agent/{sessionId}`; the
 * `settings/agent-new` route = `New agent`, the model's `sessionId` null) —
 * lane AGENTS builds it off [AgentSettings.FIELDS]: identity (icon + color +
 * name), Device, Account, Schedule (`TriggerFormSheet` for add / edit),
 * Instructions, Delete (owner-only). [onOpenChannel] opens the agent's
 * conversation (`thread/{sessionId}`); [onCreated] lands on a just-created
 * agent's page (replace the `new` entry); [onDeleted] leaves the page.
 *
 * STUB (compiling, minimal body) — final signature.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentSettingsScreen(
    onBack: () -> Unit,
    onOpenChannel: (agentSessionId: String) -> Unit,
    onCreated: (agentSessionId: String) -> Unit,
    onDeleted: () -> Unit,
    viewModel: AgentSettingsViewModel = hiltViewModel(),
) {
    val agent by viewModel.agent.collectAsStateWithLifecycle()
    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(agent?.name ?: AgentSettings.Copy.NEW_AGENT) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize().testTag(AgentSettings.TEST_ID_PAGE)) {
            AgentSettings.FIELDS.forEach { field -> Text(field.label, modifier = Modifier.padding(16.dp)) }
        }
    }
}
