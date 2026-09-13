package com.exponential.app.ui.session

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
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
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.CLI_DEFAULT_MODEL
import com.exponential.app.ui.agent.AgentPickerPill
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.LaunchOptionsSection
import com.exponential.app.ui.components.LaunchOptionsVariant
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.defaultModelFor
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.modelValuesFor
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

// The device-settings sheet (EXP-481) — the mobile twin of the web dialog,
// styled like the Start-coding sheet (EXP-208/EXP-211 chrome: full height,
// status-bar inset, no drag handle). Replaces the Rename menu entry: name,
// the EXP-622 default-machine toggle (which machine every device picker
// prefills), team sharing (server machines only — the toggle was web-only
// before), the
// machine's per-agent launch defaults (SERVER-authoritative: editable while
// the machine is OFFLINE, it converges on return), and the synced worktree
// inventory with remove/prune commands (durable queue — an offline machine
// runs them when it comes back).
//
// EXP-490: settings sheet, not a form — there is no Cancel and no Save. Edits
// AUTO-SAVE (debounced in the ViewModel, flushed on blur and on dismiss), and
// the fields track the LIVE synced row: every delta reseeds them, EXCEPT while
// an edit of that section is pending (a save in flight, or the name field
// focused), which would stomp what the user is doing.

/** `devices.rename` caps the label at 255 chars server-side. */
private const val MAX_DEVICE_LABEL = 255

@Composable
fun DeviceSettingsSheet(
    device: SteerDevice,
    onDismiss: () -> Unit,
    viewModel: DeviceSettingsViewModel = hiltViewModel(),
) {

    LaunchedEffect(device.rowId) { viewModel.bind(device.rowId) }

    val worktrees by viewModel.worktrees.collectAsStateWithLifecycle()
    val teams by viewModel.teams.collectAsStateWithLifecycle()
    val nameBusy by viewModel.nameBusy.collectAsStateWithLifecycle()
    val nameError by viewModel.nameError.collectAsStateWithLifecycle()
    val shareBusy by viewModel.shareBusy.collectAsStateWithLifecycle()
    val shareError by viewModel.shareError.collectAsStateWithLifecycle()
    val defaultBusy by viewModel.defaultBusy.collectAsStateWithLifecycle()
    val defaultError by viewModel.defaultError.collectAsStateWithLifecycle()
    val defaultsError by viewModel.defaultsError.collectAsStateWithLifecycle()
    val commandStates by viewModel.commandStates.collectAsStateWithLifecycle()

    var label by remember { mutableStateOf(device.deviceLabel.ifBlank { device.deviceId }) }
    var nameFocused by remember { mutableStateOf(false) }
    var editableAgents by remember { mutableStateOf(editableAgents(device)) }
    var defaultAgent by remember { mutableStateOf(seededDefaultAgent(device, editableAgents)) }
    var agentTab by remember { mutableStateOf(defaultAgent) }
    var drafts by remember {
        mutableStateOf(editableAgents.associateWith { agentDraft(device, it) })
    }
    var removeTarget by remember { mutableStateOf<DeviceWorktreeEntity?>(null) }

    // Live reseeds. The name only re-seeds while the field is idle, the
    // defaults only while nothing of theirs is queued or in flight — otherwise
    // the echo of the user's own save would land back on top of a newer edit.
    LaunchedEffect(device.deviceLabel) {
        if (!nameFocused && !viewModel.hasPendingRename()) {
            label = device.deviceLabel.ifBlank { device.deviceId }
        }
    }
    LaunchedEffect(device.launchDefaults, device.agents, device.unauthedAgents) {
        if (!viewModel.hasPendingDefaults()) {
            editableAgents = editableAgents(device)
            defaultAgent = seededDefaultAgent(device, editableAgents)
            drafts = editableAgents.associateWith { agentDraft(device, it) }
            if (agentTab !in editableAgents) agentTab = editableAgents.first()
        }
    }

    // Last chance for a debounce that hasn't elapsed (the ViewModel flushes on
    // a scope that survives this composable).
    DisposableEffect(Unit) {
        onDispose { viewModel.flushPending() }
    }

    fun editDraft(agent: String, edit: (AgentDraft) -> AgentDraft) {
        val next = drafts + (agent to edit(drafts[agent] ?: agentDraft(device, agent)))
        drafts = next
        viewModel.queueDefaults(
            device.deviceId,
            buildDefaults(defaultAgent, editableAgents, next),
        )
    }

    // EXP-686: the static sheet title — the machine's own name is the editable
    // field right below it. EXP-694: no bottom button at all — edits save
    // themselves, so a "Done" that only dismissed duplicated the drag handle.
    GlassSheet(
        title = "Device settings",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("device-settings-sheet"),
        height = SheetHeight.Full,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            // ── Name ─────────────────────────────────────────────────────
            SectionHeader("Name", modifier = Modifier.padding(horizontal = 16.dp))
            OptionGroup {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    GlassTextField(
                        value = label,
                        onValueChange = { next ->
                            label = next.take(MAX_DEVICE_LABEL)
                            viewModel.queueRename(
                                device.deviceId,
                                label.trim()
                                    .takeIf { it.isNotEmpty() && it != device.deviceLabel },
                            )
                        },
                        singleLine = true,
                        // Inside the group — the group owns the chrome.
                        bordered = false,
                        modifier = Modifier
                            .weight(1f)
                            .onFocusChanged {
                                nameFocused = it.isFocused
                                if (!it.isFocused) {
                                    // A rename that arrived while focused
                                    // was deliberately skipped — catch up
                                    // unless an edit is owed.
                                    val hadPending = viewModel.hasPendingRename()
                                    viewModel.flushPending()
                                    if (!hadPending) {
                                        label = device.deviceLabel
                                            .ifBlank { device.deviceId }
                                    }
                                }
                            },
                    )
                    if (nameBusy) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(14.dp).padding(end = 2.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Spacer(Modifier.width(12.dp))
                    }
                }
            }
            ErrorCaption(nameError)
            Spacer(Modifier.height(8.dp))

            // ── Default machine (EXP-622) ───────────────────────────────
            OptionGroup {
                SwitchRow(
                    title = "Default device",
                    checked = device.isDefault,
                    onCheckedChange = { next ->
                        viewModel.setDefault(device.deviceId, next)
                    },
                    enabled = !defaultBusy,
                )
            }
            ErrorCaption(defaultError)
            Spacer(Modifier.height(8.dp))

            // ── Sharing (server machines only, EXP-432/EXP-481/FEED-33) ─
            // One switch per team, rendered straight off the live row like
            // the default-device toggle: a machine may be shared with
            // several teams at once.
            if (device.isServer) {
                SectionHeader("Sharing", modifier = Modifier.padding(horizontal = 16.dp))
                OptionGroup {
                    teams.forEach { team ->
                        SwitchRow(
                            title = team.name,
                            checked = device.sharedTeamIds.contains(team.id),
                            onCheckedChange = { next ->
                                viewModel.setShared(device.deviceId, team.id, next)
                            },
                            enabled = !shareBusy,
                        )
                    }
                }
                Text(
                    "Teammates of a shared team can start coding sessions on this " +
                        "device. Runs are attributed to whoever starts them.",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
                )
                ErrorCaption(shareError)
                Spacer(Modifier.height(8.dp))
            }

            // ── Agent defaults (server-authoritative, EXP-481) ───────────
            if (!device.online) {
                Text(
                    "Applies when the device comes online.",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
                )
            }
            // EXP-862: the SHARED agent picker — the same icon-only trigger
            // the composer's options row wears, so "which agent" looks the
            // same wherever it is asked.
            OptionGroup {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        "Default agent",
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Spacer(Modifier.weight(1f))
                    AgentPickerPill(
                        agent = defaultAgent,
                        agents = editableAgents,
                        onSelect = {
                            defaultAgent = it
                            viewModel.queueDefaults(
                                device.deviceId,
                                buildDefaults(it, editableAgents, drafts),
                            )
                        },
                    )
                }
            }
            Spacer(Modifier.height(8.dp))
            // EXP-694: the SAME agent card every launch surface renders — the
            // embedded agent tabs, model/effort and the toggles, in one
            // inset-grouped card. EXP-862: NO accounts or usage in here — those
            // belong to Devices → Accounts, the one surface that owns them.
            val draft = drafts[agentTab] ?: agentDraft(device, agentTab)
            LaunchOptionsSection(
                variant = LaunchOptionsVariant.Device,
                // The sheet already IS the machine — no "Runs on" row.
                devices = emptyList(),
                device = null,
                onDeviceChange = {},
                agent = agentTab,
                availableAgents = editableAgents,
                onAgentChange = { agentTab = it },
                model = draft.model,
                onModelChange = { next -> editDraft(agentTab) { it.copy(model = next) } },
                effort = draft.effort,
                onEffortChange = { next -> editDraft(agentTab) { it.copy(effort = next) } },
                ultracode = draft.ultracode,
                onUltracodeChange = { next ->
                    editDraft(agentTab) { it.copy(ultracode = next) }
                },
                planMode = draft.planMode,
                onPlanModeChange = { next ->
                    editDraft(agentTab) { it.copy(planMode = next) }
                },
            )
            ErrorCaption(defaultsError)
            Spacer(Modifier.height(8.dp))

            // ── Worktrees (EXP-481) ──────────────────────────────────────
            // 12dp + the header's own 4dp = 16: the label sits 4dp inside the
            // OptionGroup edge below it, like web and iOS.
            SectionHeader("Worktrees", modifier = Modifier.padding(horizontal = 16.dp)) {
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
                        // EXP-688: icon-only, at the trailing edge of the
                        // section header (web/desktop/iOS parity), on the one
                        // 32dp control circle (EXP-698).
                        CircleIconButton(
                            ExpIcons.uiClean,
                            "Prune merged worktrees",
                            onClick = {
                                viewModel.pruneWorktrees(device.deviceId, device.online)
                            },
                            borderless = true,
                        )
                    }
                }
            }
            if (!device.online && worktrees.isNotEmpty()) {
                Text(
                    "This device is offline — queued changes run when it comes online.",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
                )
            }
            CommandCaption(commandStates[PRUNE_COMMAND_KEY])
            if (worktrees.isEmpty()) {
                Text(
                    "No worktrees reported by this device.",
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
internal fun CommandCaption(state: DeviceCommandUiState?) {
    val (text, isError) = when (state) {
        is DeviceCommandUiState.Queued ->
            "Queued — runs when the device comes online." to false
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
        // EXP-688: an agent the machine only reports an ACCOUNT or USAGE for
        // still needs its tab — that block is where its sign-in and limits
        // live now.
        addAll(device.agentAccounts?.keys.orEmpty())
        addAll(device.agentUsage?.keys.orEmpty())
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
        ?: return AgentDraft(defaultModelFor(agent), CLI_DEFAULT_EFFORT, false, false)
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
            ?: AgentDraft(defaultModelFor(agent), CLI_DEFAULT_EFFORT, false, false)
        AgentLaunchDefaults(
            model = draft.model,
            effort = draft.effort,
            ultracode = draft.ultracode && agent == DEFAULT_AGENT,
            planMode = draft.planMode && supportsPlanMode(agent),
        )
    },
)
