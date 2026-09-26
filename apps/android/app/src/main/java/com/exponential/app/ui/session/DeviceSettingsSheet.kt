package com.exponential.app.ui.session

import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.AgentLaunchDefaults
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.DeviceWorkflowDefaults
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.deviceUpdateAvailable
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.CLI_DEFAULT_MODEL
import com.exponential.app.ui.components.AccountPickerPill
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.LaunchOptionsSection
import com.exponential.app.ui.components.LaunchOptionsVariant
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SubShell
import com.exponential.app.ui.components.SubShellHost
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.deviceAccountOptions
import com.exponential.app.ui.components.defaultModelFor
import com.exponential.app.ui.components.deviceIconName
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.modelValuesFor
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.components.supportsSubagentModel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.glassGroup
import com.exponential.app.ui.theme.TextEmphasis

// The device-settings sheet (EXP-481) — the mobile twin of the web dialog,
// styled like the Start-coding sheet (EXP-208/EXP-211 chrome: full height,
// status-bar inset, no drag handle). Replaces the Rename menu entry: name,
// the EXP-622 default-machine toggle (which machine every device picker
// prefills), team sharing (server machines only — the toggle was web-only
// before) and the machine's per-agent launch defaults (SERVER-authoritative:
// editable while the machine is OFFLINE, it converges on return). The
// EXP-909 follow-up added its last two sections: Update (server daemons
// only) and Remove, which used to hang off the device row's ⋯ menu — the row
// keeps ONE control now, the gear that opens this sheet.
//
// EXP-1043 (EXP-1020's Android half): ONE layout on the four clients. No
// worktrees — a machine's worktrees are a LOCAL surface, the IDE's own, and
// the remote command queue that drove them from here went with them. The
// agent-defaults block ends in a "Workflow settings" SUB-SHELL row (the
// model pair a new workflow is seeded from, `launch_defaults.workflow`), and
// "Remove device" is a plain row of the same shell rather than a section of
// its own.
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
    /**
     * The registry mutations Update and Remove ride (they were the row's, and
     * moved in here with the rest of curating a machine). On the Devices tab
     * this resolves to the SAME instance the list holds, so a request in
     * flight disables both surfaces at once.
     */
    agentsViewModel: AgentsViewModel = hiltViewModel(),
) {

    LaunchedEffect(device.rowId) { viewModel.bind(device.rowId) }

    val teams by viewModel.teams.collectAsStateWithLifecycle()
    val nameBusy by viewModel.nameBusy.collectAsStateWithLifecycle()
    val nameError by viewModel.nameError.collectAsStateWithLifecycle()
    val shareBusy by viewModel.shareBusy.collectAsStateWithLifecycle()
    val shareError by viewModel.shareError.collectAsStateWithLifecycle()
    val defaultBusy by viewModel.defaultBusy.collectAsStateWithLifecycle()
    val defaultError by viewModel.defaultError.collectAsStateWithLifecycle()
    val defaultsError by viewModel.defaultsError.collectAsStateWithLifecycle()
    val latestVersions by agentsViewModel.latestVersions.collectAsStateWithLifecycle()
    val deviceBusy by agentsViewModel.deviceBusy.collectAsStateWithLifecycle()
    val iconError by agentsViewModel.deviceIconError.collectAsStateWithLifecycle()

    // A rename/remove/update on THIS machine is in flight: its controls stay
    // put but disable until the change lands.
    val deviceMutating = device.deviceId in deviceBusy
    // A server runs the CLI, a desktop the IDE — each compares against its own
    // channel's advertised latest.
    val latestVersion = if (device.isServer) latestVersions.cli else latestVersions.desktop
    val outdated = deviceUpdateAvailable(device.version, latestVersion)

    var label by remember { mutableStateOf(device.deviceLabel.ifBlank { device.deviceId }) }
    var nameFocused by remember { mutableStateOf(false) }
    // EXP-924: the optimistic icon pick, drawn over the synced row until the
    // write's own delta arrives (which clears it) — or until it fails, when the
    // picker hands the previous value back.
    var iconPick by remember(device.rowId) { mutableStateOf<String?>(null) }
    var editableAgents by remember { mutableStateOf(editableAgents(device)) }
    var defaultAgent by remember { mutableStateOf(seededDefaultAgent(device, editableAgents)) }
    // EXP-872: "default agent" became "default ACCOUNT" — the row stores a
    // login's profile id and the agent is derived from it. "" = nothing stored
    // yet (including the AMBIENT login, which is never a stored id), and the
    // picker then sits on that agent's ambient option.
    var defaultAccount by remember {
        mutableStateOf(device.launchDefaults?.defaultAccount.orEmpty())
    }
    var agentTab by remember { mutableStateOf(defaultAgent) }
    var drafts by remember {
        mutableStateOf(editableAgents.associateWith { agentDraft(device, it) })
    }
    // EXP-1043: the STORED workflow pair, exactly as the row carries it. What
    // the pickers show is [workflowDefaults] of it for the DEFAULT agent, so
    // moving the default account to the other agent falls back to that
    // agent's contract pair instead of showing a name it cannot run.
    var workflow by remember { mutableStateOf(device.launchDefaults?.workflow) }
    // "Remove device" waiting on its confirm. The sheet needs no dismiss of
    // its own afterwards: the caller re-resolves the live row, which is gone.
    var confirmRemove by remember { mutableStateOf(false) }

    // Live reseeds. The name only re-seeds while the field is idle, the
    // defaults only while nothing of theirs is queued or in flight — otherwise
    // the echo of the user's own save would land back on top of a newer edit.
    LaunchedEffect(device.deviceLabel) {
        if (!nameFocused && !viewModel.hasPendingRename()) {
            label = device.deviceLabel.ifBlank { device.deviceId }
        }
    }
    // The synced icon moved (our own write landing, or another client's) — the
    // row is the truth again, so the optimistic pick steps aside.
    LaunchedEffect(device.icon) { iconPick = null }
    LaunchedEffect(device.launchDefaults, device.agents, device.unauthedAgents) {
        if (!viewModel.hasPendingDefaults()) {
            editableAgents = editableAgents(device)
            defaultAgent = seededDefaultAgent(device, editableAgents)
            defaultAccount = device.launchDefaults?.defaultAccount.orEmpty()
            drafts = editableAgents.associateWith { agentDraft(device, it) }
            workflow = device.launchDefaults?.workflow
            if (agentTab !in editableAgents) agentTab = editableAgents.first()
        }
    }

    // Last chance for a debounce that hasn't elapsed (the ViewModel flushes on
    // a scope that survives this composable).
    DisposableEffect(Unit) {
        onDispose { viewModel.flushPending() }
    }

    /**
     * Queue the WHOLE edited struct — `setLaunchDefaults` replaces the stored
     * object, so every save carries the workflow pair too (resolved for the
     * agent it is being saved under) or it would wipe it.
     */
    fun queueDefaults(
        agent: String = defaultAgent,
        account: String = defaultAccount,
        next: Map<String, AgentDraft> = drafts,
        stored: DeviceWorkflowDefaults? = workflow,
    ) {
        val (model, strongModel) = workflowDefaults(agent, stored)
        viewModel.queueDefaults(
            device.deviceId,
            buildDefaults(
                agent,
                account,
                editableAgents,
                next,
                DeviceWorkflowDefaults(model = model, strongModel = strongModel),
            ),
        )
    }

    fun editDraft(agent: String, edit: (AgentDraft) -> AgentDraft) {
        val next = drafts + (agent to edit(drafts[agent] ?: agentDraft(device, agent)))
        drafts = next
        queueDefaults(next = next)
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
            // EXP-1043: the whole settings list is ONE sub-shell card — a
            // row that opens (Workflow settings) slides its page in place of
            // all of it, with a back button on top, rather than nesting a
            // second card inside the sheet.
            SubShellHost {
                Column(modifier = Modifier.fillMaxWidth()) {
                    // ── Name ─────────────────────────────────────────────────
                    // EXP-924: ONE identity row — the icon picker then the name, the
                    // board form's layout (a machine IS its glyph and its name). A
                    // pick writes straight through and is drawn optimistically until
                    // the devices shape echoes it back; a failure reverts and captions
                    // this row, exactly where a failed rename lands.
                    SectionHeader("Name", modifier = Modifier.padding(horizontal = 16.dp))
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                    ) {
                        IconPicker(
                            // The RESOLVED name, so a machine that never picked one
                            // shows its kind default selected rather than nothing.
                            selected = deviceIconName(iconPick ?: device.icon, device.isServer),
                            onSelect = { picked ->
                                val previous = iconPick
                                iconPick = picked
                                agentsViewModel.setDeviceIcon(device.deviceId, picked) {
                                    iconPick = previous
                                }
                            },
                            pickable = ExpIcons.devicePickable,
                        )
                        Column(modifier = Modifier.weight(1f).glassGroup()) {
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
                    }
                    ErrorCaption(nameError ?: iconError)
                    Spacer(Modifier.height(8.dp))

                    // ── Default machine (EXP-622) ───────────────────────────
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
                            // EXP-994: a grouped list of rows carries a hairline
                            // between them — a stack of bare switches read as one
                            // control with several thumbs.
                            teams.forEachIndexed { index, team ->
                                if (index > 0) GroupDivider()
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

                    // ── Agent defaults (server-authoritative, EXP-481) ───────
                    if (!device.online) {
                        Text(
                            "Applies when the device comes online.",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
                        )
                    }
                    // EXP-872: the SHARED account picker — the same trigger the
                    // composer's options row wears, so "which login" looks the same
                    // wherever it is asked. The machine's default AGENT rides the
                    // picked login. EXP-1043: every editable agent contributes at
                    // least its AMBIENT login here, so the default account is always
                    // changeable — the setting is about which agent a run starts on,
                    // and a machine that reports a login for one agent only must not
                    // lock the other one away.
                    val accountOptions = deviceAccountOptions(device, editableAgents)
                    OptionGroup {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 16.dp, vertical = 8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                "Default account",
                                style = MaterialTheme.typography.bodyLarge,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                            Spacer(Modifier.weight(1f))
                            AccountPickerPill(
                                options = accountOptions,
                                // An unset pin IS the ambient login, and that is
                                // the option carrying `system` — the same
                                // `agent:id` key every other picker builds.
                                selectedKey =
                                    "$defaultAgent:${defaultAccount.ifEmpty { SYSTEM_PROFILE_ID }}",
                                onSelect = { option ->
                                    // The ambient login is NOT a profile id: it
                                    // stores as "nothing pinned", which is what
                                    // the row echoes back (see [buildDefaults]).
                                    val picked = option.id
                                        .takeIf { it != SYSTEM_PROFILE_ID }
                                        .orEmpty()
                                    defaultAgent = option.agent
                                    defaultAccount = picked
                                    queueDefaults(agent = option.agent, account = picked)
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
                    // ── Workflow settings (EXP-1043) ─────────────────────────
                    // Both agents' workflows run on the DEFAULT agent's models, so
                    // this row is the same one whatever tab is selected above — it
                    // belongs to the machine, not to the tab. A stored name that
                    // belongs to the other agent falls back to the contract pair
                    // (see [workflowDefaults]).
                    val (workflowModel, workflowStrongModel) = workflowDefaults(defaultAgent, workflow)
                    val workflowModels = modelValuesFor(defaultAgent)
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
                        subagentModel = draft.subagentModel,
                        onSubagentModelChange = { next ->
                            editDraft(agentTab) { it.copy(subagentModel = next) }
                        },
                        ultracode = draft.ultracode,
                        onUltracodeChange = { next ->
                            editDraft(agentTab) { it.copy(ultracode = next) }
                        },
                        planMode = draft.planMode,
                        onPlanModeChange = { next ->
                            editDraft(agentTab) { it.copy(planMode = next) }
                        },
                        // EXP-1020: the LAST row of that same card, never a card
                        // of its own — the four clients read alike.
                        trailing = {
                            SubShell(
                                label = "Workflow settings",
                                // The `nav-workflows` CONCEPT, the glyph the web row
                                // carries and the one Workflows wears everywhere.
                                icon = ExpIcons.navWorkflows,
                                value = "${modelLabel(workflowModel)} · " +
                                    modelLabel(workflowStrongModel),
                                title = "Workflow settings",
                            ) {
                                Column(modifier = Modifier.fillMaxWidth()) {
                                    OptionGroup {
                                        PickerRow(
                                            label = "Model",
                                            value = modelLabel(workflowModel),
                                            options = workflowModels,
                                            selected = workflowModel,
                                            optionLabel = ::modelLabel,
                                            onSelect = { next ->
                                                val edited = DeviceWorkflowDefaults(
                                                    model = next,
                                                    strongModel = workflowStrongModel,
                                                )
                                                workflow = edited
                                                queueDefaults(stored = edited)
                                            },
                                        )
                                        GroupDivider()
                                        PickerRow(
                                            label = "Strong model",
                                            value = modelLabel(workflowStrongModel),
                                            options = workflowModels,
                                            selected = workflowStrongModel,
                                            optionLabel = ::modelLabel,
                                            onSelect = { next ->
                                                val edited = DeviceWorkflowDefaults(
                                                    model = workflowModel,
                                                    strongModel = next,
                                                )
                                                workflow = edited
                                                queueDefaults(stored = edited)
                                            },
                                        )
                                    }
                                    // One line per row above, in the same order.
                                    Text(
                                        "Leaf nodes and the subagents inside them.",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onSurface
                                            .copy(alpha = TextEmphasis.Tertiary),
                                        modifier = Modifier
                                            .padding(horizontal = 32.dp, vertical = 2.dp),
                                    )
                                    Text(
                                        "Contract, integration and risky nodes, and every review.",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onSurface
                                            .copy(alpha = TextEmphasis.Tertiary),
                                        modifier = Modifier
                                            .padding(horizontal = 32.dp, vertical = 2.dp),
                                    )
                                }
                            }
                        },
                    )
                    // Whatever follows (Update, else Remove) opens with its own
                    // 8dp gap — the card needs no second one.
                    ErrorCaption(defaultsError)

                    // ── Update (server daemons only) ─────────────────────────
                    // Self-update is a SERVER capability: the desktop app updates
                    // itself through its own channel, so it has no control here. The
                    // row used to carry this; a device list is for curating machines,
                    // and curating one is what this sheet is.
                    if (device.isServer) {
                        Spacer(Modifier.height(8.dp))
                        SectionHeader("Update", modifier = Modifier.padding(horizontal = 16.dp))
                        OptionGroup {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(horizontal = 16.dp, vertical = 8.dp),
                            ) {
                                Column(modifier = Modifier.weight(1f)) {
                                    Text(
                                        device.version?.let { "v$it" } ?: "Version unknown",
                                        style = MaterialTheme.typography.bodyLarge,
                                        color = MaterialTheme.colorScheme.onSurface,
                                    )
                                    if (outdated) {
                                        Text(
                                            "Update available: v$latestVersion",
                                            style = MaterialTheme.typography.labelSmall,
                                            color = NeedsInputAmber,
                                        )
                                    }
                                }
                                // EXP-420 (web `showDeviceUpdateButton`): a control only
                                // where the ask can land — an online, registered server
                                // with a newer version out, or one already updating.
                                // Otherwise the version line alone is the whole answer.
                                if (device.online &&
                                    device.registered &&
                                    (outdated || device.updateRequested)
                                ) {
                                    when {
                                        // EXP-411: parked behind live sessions — say so
                                        // instead of spinning until the last one closes.
                                        device.updateQueued -> GlassPill(
                                            "Queued",
                                            icon = ExpIcons.uiUpdate,
                                            size = PillSize.Sm,
                                            enabled = false,
                                        )
                                        device.updateRequested -> GlassPill(
                                            "Updating…",
                                            size = PillSize.Sm,
                                            enabled = false,
                                            loading = true,
                                        )
                                        else -> GlassPill(
                                            "Update",
                                            icon = ExpIcons.uiUpdate,
                                            size = PillSize.Sm,
                                            tint = NeedsInputAmber,
                                            enabled = !deviceMutating,
                                            onClick = {
                                                agentsViewModel.requestDeviceUpdate(device.deviceId)
                                            },
                                        )
                                    }
                                }
                            }
                        }
                        if (device.updateQueued) {
                            // FEED-36, the pinned sentence ×4 (web QUEUED_UPDATE_TOOLTIP).
                            Text(
                                "Live sessions hold this update — the device restarts itself once " +
                                    "every session ends or sits idle for 2 hours.",
                                style = MaterialTheme.typography.labelSmall,
                                color = NeedsInputAmber,
                                modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
                            )
                        }
                    }

                    // ── Remove ───────────────────────────────────────────────
                    // EXP-1043: a plain row of the same shell — a whole section band
                    // over one destructive control read like a second settings page.
                    Spacer(Modifier.height(8.dp))
                    GlassPill(
                        "Remove device",
                        icon = ExpIcons.uiDelete,
                        onClick = { confirmRemove = true },
                        enabled = !deviceMutating,
                        contentColor = DesignTokens.Semantic.Red,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 16.dp)
                            .testTag("remove-device-button"),
                    )
                    Spacer(Modifier.height(24.dp))
                }
            }
        }
    }

    // Removing drops the registry row only — say so, or an owner who removes a
    // machine that is still running the daemon reads its return as a bug.
    if (confirmRemove) {
        AlertDialog(
            onDismissRequest = { confirmRemove = false },
            title = { Text("Remove device") },
            text = {
                Text(
                    "Remove “${device.deviceLabel.ifBlank { device.deviceId }}” from your " +
                        "devices? A device with the daemon still running will re-register " +
                        "itself on its next heartbeat.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        agentsViewModel.removeDevice(device.deviceId)
                        confirmRemove = false
                    },
                ) { Text("Remove") }
            },
            dismissButton = {
                TextButton(onClick = { confirmRemove = false }) { Text("Cancel") }
            },
        )
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
    /** EXP-981: claude only — "" is the CLI's own default for subagents. */
    val subagentModel: String = CLI_DEFAULT_MODEL,
    val ultracode: Boolean,
    val planMode: Boolean,
    /**
     * EXP-1005: the desktop-owned auto-rotate toggle, carried through the
     * draft ONLY so a save echoes the synced value (the mutation replaces the
     * whole object). No row edits it here; null = the stored row has no key.
     */
    val autoRotateAccounts: Boolean? = null,
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
        ?: return AgentDraft(
            model = defaultModelFor(agent),
            effort = CLI_DEFAULT_EFFORT,
            ultracode = false,
            planMode = false,
        )
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
        // EXP-981: claude only; a stored value on any other agent is dropped.
        subagentModel = defaults.subagentModel
            ?.takeIf { supportsSubagentModel(agent) && (it == CLI_DEFAULT_MODEL || it in models) }
            ?: CLI_DEFAULT_MODEL,
        ultracode = defaults.ultracode && agent == DEFAULT_AGENT,
        planMode = defaults.planMode && supportsPlanMode(agent),
        // EXP-1005: echoed as stored, never clamped or defaulted here.
        autoRotateAccounts = defaults.autoRotateAccounts,
    )
}

/**
 * The whole-object payload `devices.setLaunchDefaults` replaces the row with:
 * every editable agent's draft, capability-masked per agent (an unsupported
 * toggle never rides).
 */
internal fun buildDefaults(
    defaultAgent: String,
    /**
     * EXP-872: the profile id of [defaultAgent]'s login the machine should
     * start on; "" (nothing picked yet) builds a null, which the request
     * sends as an explicit `defaultAccount: null` (the clear), and the
     * agent's ACTIVE login stays the default.
     *
     * [SYSTEM_PROFILE_ID] means the same thing and clears too: the AMBIENT
     * login is a PICKER sentinel, never a stored id (the server clamp takes
     * any non-empty string, so a leaked `"system"` would pin a profile that
     * does not exist). The mapping lives here rather than at the pick site
     * alone so the next writer cannot forget it.
     */
    defaultAccount: String,
    agents: List<String>,
    drafts: Map<String, AgentDraft>,
    /**
     * EXP-1043: the machine's WORKFLOW model pair, already resolved for
     * [defaultAgent] ([workflowDefaults]). It rides EVERY save because the
     * mutation REPLACES the stored object; null (a caller with nothing to
     * say about it) leaves the key off, which the server reads as an older
     * client and keeps what is stored.
     */
    workflow: DeviceWorkflowDefaults? = null,
): DeviceLaunchDefaults = DeviceLaunchDefaults(
    defaultAgent = defaultAgent,
    defaultAccount = defaultAccount.takeIf { it.isNotEmpty() && it != SYSTEM_PROFILE_ID },
    workflow = workflow,
    agents = agents.associateWith { agent ->
        val draft = drafts[agent]
            ?: AgentDraft(
                model = defaultModelFor(agent),
                effort = CLI_DEFAULT_EFFORT,
                ultracode = false,
                planMode = false,
            )
        AgentLaunchDefaults(
            model = draft.model,
            effort = draft.effort,
            subagentModel = draft.subagentModel.takeIf { supportsSubagentModel(agent) },
            ultracode = draft.ultracode && agent == DEFAULT_AGENT,
            planMode = draft.planMode && supportsPlanMode(agent),
            // EXP-1005: the synced value rides back unchanged; absent stays
            // absent so the server keeps whatever the desktop stored.
            autoRotateAccounts = draft.autoRotateAccounts,
        )
    },
)

/**
 * EXP-1043: the machine's WORKFLOW model pair for [agent] — the cheap `model`
 * (leaf nodes and the subagents inside them) and the `strongModel` (contract,
 * integration and risky nodes, and every review) a new workflow is seeded
 * from (`launch_defaults.workflow`).
 *
 * The two agents' vocabularies do not overlap, so a [stored] name only counts
 * for the agent it belongs to: a machine that was on claude and moved its
 * default account to codex reads as CODEX's contract pair rather than showing
 * `opus` in a codex picker.
 */
internal fun workflowDefaults(
    agent: String,
    stored: DeviceWorkflowDefaults?,
): Pair<String, String> {
    val models = modelValuesFor(agent)
    val (model, strongModel) = workflowFallback(agent)
    return Pair(
        stored?.model?.takeIf { it in models } ?: model,
        stored?.strongModel?.takeIf { it in models } ?: strongModel,
    )
}

/** Contract `workflowLaunch`'s per-agent pair; claude's for anything else. */
private fun workflowFallback(agent: String): Pair<String, String> =
    if (agent == "codex") {
        DomainContract.workflowLaunchCodexModel to DomainContract.workflowLaunchCodexStrongModel
    } else {
        DomainContract.workflowLaunchClaudeModel to DomainContract.workflowLaunchClaudeStrongModel
    }
