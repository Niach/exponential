package com.exponential.app.ui.session

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.AgentLaunchDefaults
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.DeviceWorktreeEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.AgentTab
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.CLI_DEFAULT_MODEL
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.SectionLabel
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.components.defaultModelFor
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.modelValuesFor
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

// The device-settings sheet (EXP-481) — the mobile twin of the web dialog,
// styled like the Start-coding sheet (EXP-208/EXP-211 chrome: full height,
// status-bar inset, Cancel top-left, no drag handle). Replaces the Rename
// menu entry: name, team sharing (server machines only — the toggle was
// web-only before), the machine's per-agent launch defaults (SERVER-
// authoritative: editable while the machine is OFFLINE, it converges on
// return), and the synced worktree inventory with remove/prune commands
// (durable queue — an offline machine runs them when it comes back).

/** `devices.rename` caps the label at 255 chars server-side. */
private const val MAX_DEVICE_LABEL = 255

private const val NOT_SHARED = ""

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DeviceSettingsSheet(
    device: SteerDevice,
    onDismiss: () -> Unit,
    viewModel: DeviceSettingsViewModel = hiltViewModel(),
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    LaunchedEffect(device.rowId) { viewModel.bind(device.rowId) }

    val worktrees by viewModel.worktrees.collectAsStateWithLifecycle()
    val teams by viewModel.teams.collectAsStateWithLifecycle()
    val nameBusy by viewModel.nameBusy.collectAsStateWithLifecycle()
    val nameError by viewModel.nameError.collectAsStateWithLifecycle()
    val shareBusy by viewModel.shareBusy.collectAsStateWithLifecycle()
    val shareError by viewModel.shareError.collectAsStateWithLifecycle()
    val defaultsBusy by viewModel.defaultsBusy.collectAsStateWithLifecycle()
    val defaultsError by viewModel.defaultsError.collectAsStateWithLifecycle()
    val defaultsSaved by viewModel.defaultsSaved.collectAsStateWithLifecycle()
    val commandStates by viewModel.commandStates.collectAsStateWithLifecycle()

    // Drafts latch ONCE per open (the launch-dialog idiom): live sync deltas
    // keep flowing into the read-only parts, but must never stomp in-flight
    // edits.
    var label by remember { mutableStateOf(device.deviceLabel.ifBlank { device.deviceId }) }
    val editableAgents = remember { editableAgents(device) }
    var defaultAgent by remember { mutableStateOf(seededDefaultAgent(device, editableAgents)) }
    var agentTab by remember { mutableStateOf(seededDefaultAgent(device, editableAgents)) }
    var drafts by remember {
        mutableStateOf(editableAgents.associateWith { agentDraft(device, it) })
    }
    var defaultsDirty by remember { mutableStateOf(false) }
    var removeTarget by remember { mutableStateOf<DeviceWorktreeEntity?>(null) }

    fun editDraft(agent: String, edit: (AgentDraft) -> AgentDraft) {
        drafts = drafts + (agent to edit(drafts[agent] ?: agentDraft(device, agent)))
        defaultsDirty = true
        viewModel.clearDefaultsSaved()
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = null,
        modifier = Modifier.statusBarsPadding().testTag("device-settings-sheet"),
    ) {
        Column(modifier = Modifier.fillMaxWidth().fillMaxHeight()) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TextButton(onClick = onDismiss) { Text("Cancel") }
                Spacer(Modifier.weight(1f))
                Text(
                    device.deviceLabel.ifBlank { device.deviceId },
                    style = MaterialTheme.typography.titleSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.weight(1f))
                TextButton(onClick = onDismiss) { Text("Done") }
            }

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
                    .verticalScroll(rememberScrollState()),
            ) {
                // ── Name ─────────────────────────────────────────────────────
                SectionLabel("Name")
                OptionGroup {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        TextField(
                            value = label,
                            onValueChange = { label = it.take(MAX_DEVICE_LABEL) },
                            singleLine = true,
                            modifier = Modifier.weight(1f),
                            colors = TextFieldDefaults.colors(
                                focusedContainerColor = Color.Transparent,
                                unfocusedContainerColor = Color.Transparent,
                                disabledContainerColor = Color.Transparent,
                                focusedIndicatorColor = Color.Transparent,
                                unfocusedIndicatorColor = Color.Transparent,
                                disabledIndicatorColor = Color.Transparent,
                            ),
                        )
                        if (nameBusy) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(14.dp).padding(end = 2.dp),
                                strokeWidth = 2.dp,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                            Spacer(Modifier.width(12.dp))
                        } else {
                            TextButton(
                                onClick = { viewModel.rename(device.deviceId, label) },
                                enabled = label.isNotBlank() &&
                                    label.trim() != device.deviceLabel,
                            ) { Text("Save") }
                        }
                    }
                }
                ErrorCaption(nameError)
                Spacer(Modifier.height(8.dp))

                // ── Sharing (server machines only, EXP-432/EXP-481) ─────────
                if (device.isServer) {
                    SectionLabel("Sharing")
                    OptionGroup {
                        PickerRow(
                            label = "Shared with",
                            value = teams.firstOrNull { it.id == device.sharedTeamId }?.name
                                ?: "Not shared",
                            options = listOf(NOT_SHARED) + teams.map { it.id },
                            selected = device.sharedTeamId ?: NOT_SHARED,
                            optionLabel = { id ->
                                if (id == NOT_SHARED) {
                                    "Not shared"
                                } else {
                                    teams.firstOrNull { it.id == id }?.name ?: id
                                }
                            },
                            enabled = !shareBusy,
                            onSelect = { id ->
                                viewModel.setShared(
                                    device.deviceId,
                                    id.takeIf { it != NOT_SHARED },
                                )
                            },
                        )
                    }
                    Text(
                        "Teammates of the shared team can start coding sessions on this " +
                            "machine. Runs are attributed to whoever starts them.",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
                    )
                    ErrorCaption(shareError)
                    Spacer(Modifier.height(8.dp))
                }

                // ── Agent defaults (server-authoritative, EXP-481) ───────────
                SectionLabel("Agent defaults")
                if (!device.online) {
                    Text(
                        "This machine is offline — changes apply when it comes online.",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
                    )
                }
                OptionGroup {
                    PickerRow(
                        label = "Default agent",
                        value = agentLabel(defaultAgent),
                        options = editableAgents,
                        selected = defaultAgent,
                        optionLabel = ::agentLabel,
                        onSelect = {
                            defaultAgent = it
                            defaultsDirty = true
                            viewModel.clearDefaultsSaved()
                        },
                    )
                }
                Spacer(Modifier.height(4.dp))
                if (editableAgents.size > 1) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 16.dp, vertical = 4.dp),
                        horizontalArrangement = androidx.compose.foundation.layout.Arrangement
                            .spacedBy(8.dp, Alignment.CenterHorizontally),
                    ) {
                        editableAgents.forEach { value ->
                            AgentTab(
                                value = value,
                                selected = agentTab == value,
                                onClick = { agentTab = value },
                            )
                        }
                    }
                    Spacer(Modifier.height(4.dp))
                }
                val draft = drafts[agentTab] ?: agentDraft(device, agentTab)
                OptionGroup {
                    val modelOptions = if (agentTab == DEFAULT_AGENT) {
                        modelValuesFor(agentTab)
                    } else {
                        listOf(CLI_DEFAULT_MODEL) + modelValuesFor(agentTab)
                    }
                    PickerRow(
                        label = "Model",
                        value = modelLabel(draft.model),
                        options = modelOptions,
                        selected = draft.model,
                        optionLabel = ::modelLabel,
                        onSelect = { next -> editDraft(agentTab) { it.copy(model = next) } },
                    )
                    GroupDivider()
                    PickerRow(
                        label = when (agentTab) {
                            "codex" -> "Reasoning"
                            "pi" -> "Thinking"
                            else -> "Effort"
                        },
                        value = effortLabel(draft.effort),
                        options = listOf(CLI_DEFAULT_EFFORT) + effortValuesFor(agentTab),
                        selected = draft.effort,
                        optionLabel = ::effortLabel,
                        enabled = !(agentTab == DEFAULT_AGENT && draft.ultracode),
                        onSelect = { next -> editDraft(agentTab) { it.copy(effort = next) } },
                    )
                    if (agentTab == DEFAULT_AGENT) {
                        GroupDivider()
                        SwitchRow(
                            title = "Ultracode",
                            checked = draft.ultracode,
                            onCheckedChange = { next ->
                                editDraft(agentTab) { it.copy(ultracode = next) }
                            },
                        )
                    }
                    if (supportsPlanMode(agentTab)) {
                        GroupDivider()
                        SwitchRow(
                            title = "Plan mode",
                            checked = draft.planMode,
                            onCheckedChange = { next ->
                                editDraft(agentTab) { it.copy(planMode = next) }
                            },
                        )
                    }
                    if (agentTab != "pi") {
                        GroupDivider()
                        SwitchRow(
                            title = "Skip permissions",
                            checked = draft.skipPermissions,
                            onCheckedChange = { next ->
                                editDraft(agentTab) { it.copy(skipPermissions = next) }
                            },
                        )
                    }
                }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(horizontal = 24.dp),
                ) {
                    TextButton(
                        onClick = {
                            viewModel.saveDefaults(
                                device.deviceId,
                                buildDefaults(defaultAgent, editableAgents, drafts),
                            )
                            defaultsDirty = false
                        },
                        enabled = defaultsDirty && !defaultsBusy,
                    ) { Text("Save defaults") }
                    if (defaultsBusy) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(14.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    } else if (defaultsSaved) {
                        Text(
                            "Saved",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = TextEmphasis.Secondary,
                            ),
                        )
                    }
                }
                ErrorCaption(defaultsError)
                Spacer(Modifier.height(8.dp))

                // ── Worktrees (EXP-481) ──────────────────────────────────────
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth().padding(end = 24.dp),
                ) {
                    SectionLabel("Worktrees")
                    Spacer(Modifier.weight(1f))
                    val pruneState = commandStates[PRUNE_COMMAND_KEY]
                    if (worktrees.isNotEmpty()) {
                        if (pruneState is DeviceCommandUiState.Sending ||
                            pruneState is DeviceCommandUiState.Running
                        ) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(14.dp),
                                strokeWidth = 2.dp,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                        } else {
                            TextButton(
                                onClick = {
                                    viewModel.pruneWorktrees(device.deviceId, device.online)
                                },
                            ) { Text("Prune merged") }
                        }
                    }
                }
                if (!device.online && worktrees.isNotEmpty()) {
                    Text(
                        "This machine is offline — commands run when it comes online.",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
                    )
                }
                CommandCaption(commandStates[PRUNE_COMMAND_KEY])
                if (worktrees.isEmpty()) {
                    Text(
                        "No worktrees reported by this machine.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.padding(horizontal = 32.dp, vertical = 4.dp),
                    )
                } else {
                    OptionGroup {
                        worktrees.forEachIndexed { index, worktree ->
                            if (index > 0) GroupDivider()
                            WorktreeRow(
                                worktree = worktree,
                                state = commandStates["${worktree.repoFullName} ${worktree.branch}"],
                                onRemove = { removeTarget = worktree },
                            )
                        }
                    }
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }

    removeTarget?.let { worktree ->
        AlertDialog(
            onDismissRequest = { removeTarget = null },
            title = { Text("Remove worktree?") },
            text = {
                Text(
                    "Removes ${worktree.branch} from ${worktree.repoFullName} on " +
                        "“${device.deviceLabel.ifBlank { device.deviceId }}”. The machine " +
                        "refuses when uncommitted changes would be lost.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        viewModel.removeWorktree(device.deviceId, worktree, device.online)
                        removeTarget = null
                    },
                ) { Text("Remove") }
            },
            dismissButton = {
                TextButton(onClick = { removeTarget = null }) { Text("Cancel") }
            },
        )
    }
}

@Composable
private fun WorktreeRow(
    worktree: DeviceWorktreeEntity,
    state: DeviceCommandUiState?,
    onRemove: () -> Unit,
) {
    Column {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 16.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        ) {
            Icon(
                ExpIcons.uiBranch,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Spacer(Modifier.width(10.dp))
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        worktree.branch,
                        style = MaterialTheme.typography.bodyMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    if (worktree.busy) {
                        Spacer(Modifier.width(6.dp))
                        Text(
                            "session live",
                            style = MaterialTheme.typography.labelSmall,
                            color = com.exponential.app.ui.issue.ReviewGreen,
                        )
                    } else if (worktree.dirty == "tracked") {
                        Spacer(Modifier.width(6.dp))
                        Text(
                            "uncommitted changes",
                            style = MaterialTheme.typography.labelSmall,
                            color = com.exponential.app.ui.issue.NeedsInputAmber,
                        )
                    }
                }
                Text(
                    worktree.repoFullName +
                        (worktree.issueIdentifier?.let { " · $it" } ?: ""),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            when (state) {
                is DeviceCommandUiState.Sending, is DeviceCommandUiState.Running ->
                    CircularProgressIndicator(
                        modifier = Modifier.size(14.dp).padding(end = 2.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                else -> IconButton(onClick = onRemove, enabled = !worktree.busy) {
                    Icon(
                        ExpIcons.uiDelete,
                        contentDescription = "Remove worktree",
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = if (worktree.busy) {
                                TextEmphasis.Quaternary
                            } else {
                                TextEmphasis.Tertiary
                            },
                        ),
                    )
                }
            }
        }
        CommandCaption(state)
    }
}

/** Inline command feedback (EXP-323 idiom — captions the triggering row). */
@Composable
private fun CommandCaption(state: DeviceCommandUiState?) {
    val (text, isError) = when (state) {
        is DeviceCommandUiState.Queued ->
            "Queued — runs when the machine comes online." to false
        is DeviceCommandUiState.Done -> (state.message ?: "Done.") to false
        is DeviceCommandUiState.Failed -> state.message to true
        else -> return
    }
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = if (isError) {
            MaterialTheme.colorScheme.error
        } else {
            MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        },
        modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
    )
}

@Composable
private fun ErrorCaption(message: String?) {
    if (message == null) return
    Text(
        message,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.error,
        modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
    )
}

/** One agent's editable defaults draft. */
data class AgentDraft(
    val model: String,
    val effort: String,
    val ultracode: Boolean,
    val planMode: Boolean,
    val skipPermissions: Boolean,
)

/**
 * The agents the editor offers tabs for: everything the machine runs, has
 * installed-but-signed-out, or already has defaults stored for — contract
 * order; the full contract set when the row knows nothing (a fresh offline
 * machine stays editable).
 */
internal fun editableAgents(device: SteerDevice): List<String> {
    val known = buildSet {
        addAll(device.agents.orEmpty())
        addAll(device.unauthedAgents)
        addAll(device.launchDefaults?.agents?.keys.orEmpty())
    }
    val ordered = DomainContract.codingAgentValues.filter { it in known }
    return ordered.ifEmpty { DomainContract.codingAgentValues }
}

/** The stored default agent, clamped to the editable set. */
internal fun seededDefaultAgent(device: SteerDevice, editable: List<String>): String =
    device.launchDefaults?.defaultAgent?.takeIf { it in editable }
        ?: DEFAULT_AGENT.takeIf { it in editable }
        ?: editable.firstOrNull()
        ?: DEFAULT_AGENT

/**
 * One agent's draft seeded from the stored defaults, vocabulary-validated and
 * capability-clamped the way the start sheet seeds (a stored value from a
 * different app version must not render an un-pickable state).
 */
internal fun agentDraft(device: SteerDevice, agent: String): AgentDraft {
    val defaults = device.launchDefaults?.agents?.get(agent)
        ?: return AgentDraft(defaultModelFor(agent), CLI_DEFAULT_EFFORT, false, false, false)
    val models = modelValuesFor(agent)
    return AgentDraft(
        model = defaults.model
            ?.takeIf {
                if (agent == DEFAULT_AGENT) it in models else it == CLI_DEFAULT_MODEL || it in models
            }
            ?: defaultModelFor(agent),
        effort = defaults.effort
            ?.takeIf { it == CLI_DEFAULT_EFFORT || it in effortValuesFor(agent) }
            ?: CLI_DEFAULT_EFFORT,
        ultracode = defaults.ultracode && agent == DEFAULT_AGENT,
        planMode = defaults.planMode && supportsPlanMode(agent),
        skipPermissions = defaults.skipPermissions && agent != "pi",
    )
}

/**
 * The whole-object payload `devices.setLaunchDefaults` replaces the row with:
 * every editable agent's draft, capability-masked per agent (an unsupported
 * toggle never rides).
 */
internal fun buildDefaults(
    defaultAgent: String,
    agents: List<String>,
    drafts: Map<String, AgentDraft>,
): DeviceLaunchDefaults = DeviceLaunchDefaults(
    defaultAgent = defaultAgent,
    agents = agents.associateWith { agent ->
        val draft = drafts[agent]
            ?: AgentDraft(defaultModelFor(agent), CLI_DEFAULT_EFFORT, false, false, false)
        AgentLaunchDefaults(
            model = draft.model,
            effort = draft.effort,
            ultracode = draft.ultracode && agent == DEFAULT_AGENT,
            planMode = draft.planMode && supportsPlanMode(agent),
            skipPermissions = draft.skipPermissions && agent != "pi",
        )
    },
)
