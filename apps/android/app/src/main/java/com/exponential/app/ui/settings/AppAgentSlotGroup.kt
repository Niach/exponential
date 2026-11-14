package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.UpdateAgentPatch
import com.exponential.app.domain.AccountOptions
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.Apps
import com.exponential.app.ui.apps.ArgsStore
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.accountOptionsFor
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.components.toPickerAccount
import com.exponential.app.ui.components.toPickerDevice

/**
 * #EXP-1849 (apps.json `agentSlot`): the App settings' AGENT group, right
 * after the identity: a section header over THE agent page's Runs on rows
 * ([AgentRunsOnRows]) for the app's agent. Device = the app's slot (`Each
 * member's own device` or a team-shared device), Account and Model only
 * while a device is set, then Skills and MCP servers. Writes =
 * `agents.update {id, deviceId, …}`; [editable] = the app's creator or a
 * team owner, everyone else reads.
 */
@Composable
fun AppAgentSlotGroup(agentId: String, editable: Boolean) {
    ArgsStore("agentId", agentId) { AppAgentSlotBody(editable) }
}

@Composable
private fun AppAgentSlotBody(editable: Boolean, viewModel: AgentSettingsViewModel = hiltViewModel()) {
    val agent by viewModel.agent.collectAsStateWithLifecycle()
    val teamDevices by viewModel.teamDevices.collectAsStateWithLifecycle()
    val teamSkills by viewModel.skills.collectAsStateWithLifecycle()
    val teamMcpServers by viewModel.mcpServers.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    val row = agent ?: return

    // The form's copy of the slot, re-seeded on a synced change.
    var slot by remember { mutableStateOf<String?>(null) }
    var accountKey by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(row.deviceId) { slot = row.deviceId.takeIf { it.isNotEmpty() } }

    val all = teamDevices.orEmpty()
    val rows = Apps.AgentSlot.rows(
        all.map { Apps.AgentSlot.Device(it.deviceId, it.online, row.teamId in it.sharedTeamIds) },
    )
    val byId = all.associateBy { it.deviceId }
    val pickerDevices = rows.map { entry ->
        byId[entry.id]?.toPickerDevice(startGate = false)?.copy(
            disabled = entry.disabled,
            description = if (entry.disabled) "Offline" else null,
        ) ?: DevicePickerDevice(id = entry.id, name = Apps.AgentSlot.EACH_MEMBER)
    }
    val device = slot?.let(byId::get)
    val accountOptions = remember(device) { accountOptionsFor(device, device?.acpAgents.orEmpty()) }
    val syncedCli = row.runnerDefaults.cli
    val accountValueKey = remember(accountOptions, accountKey, syncedCli) {
        val parsed = accountKey?.let(AccountOptions::parseKey)
        AgentSettings.agentAccountValue(
            options = accountOptions.map { AgentSettings.AccountValueOption(it.id, it.agent, it.email, it.isLastUsed) },
            account = parsed?.id,
            cli = parsed?.agent ?: syncedCli,
        )?.let { "${it.agent}:${it.id}" }
    }
    val patch: (UpdateAgentPatch) -> Unit = { if (editable) viewModel.update(it) }

    Column(Modifier.fillMaxWidth().testTag(Apps.AgentSlot.TEST_ID)) {
        SectionHeader(Apps.AgentSlot.TITLE, modifier = Modifier.padding(horizontal = 16.dp))
        OptionGroup {
            AgentRunsOnRows(
                devices = pickerDevices,
                deviceId = Apps.AgentSlot.pick(slot),
                onDeviceChange = { pick ->
                    val next = Apps.AgentSlot.deviceIdFor(pick)
                    if (next != (slot ?: "")) {
                        slot = next.takeIf { it.isNotEmpty() }
                        accountKey = null
                        patch(UpdateAgentPatch(deviceId = next))
                    }
                },
                deviceValue = device?.let { slotDeviceName(it) } ?: slot ?: Apps.AgentSlot.EACH_MEMBER,
                deviceTitle = Apps.AgentSlot.RUNS_ON,
                accounts = accountOptions.map { it.toPickerAccount() },
                accountValueKey = accountValueKey,
                onAccountChange = { key ->
                    accountKey = key
                    val parsed = AccountOptions.parseKey(key)
                    if (parsed != null) {
                        patch(
                            if (parsed.id.isEmpty()) UpdateAgentPatch(clearAccount = true)
                            else UpdateAgentPatch(account = parsed.id),
                        )
                    }
                },
                editable = editable,
                model = row.runnerDefaults.model,
                modelOptions = com.exponential.app.ui.components.modelOptionsFor(syncedCli ?: "claude"),
                modelCli = syncedCli,
                onModelChange = { patch(UpdateAgentPatch(model = it)) },
                skillsRow = { AgentSkillsRow(row, teamSkills, editable, patch) },
                mcpRow = if (AgentSettings.McpRow.visible(teamMcpServers.size, row.mcpServerIdList.size)) {
                    { AgentMcpRow(row, teamMcpServers, editable, patch) }
                } else {
                    null
                },
                runner = Apps.AgentSlot.showsRunner(slot),
            )
        }
        Spacer(Modifier.height(16.dp))
    }
}

/** EXP-1554: the device row's value is the machine alone (×4). */
private fun slotDeviceName(device: com.exponential.app.data.api.SteerDevice): String =
    device.toPickerDevice(startGate = false).name.substringBefore(" — ")
