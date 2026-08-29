package com.exponential.app.ui.session

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentLaunchDefaults
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.DeviceWorktreeEntity
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.parseAgentLoginResult
import com.exponential.app.ui.components.AgentSegmentedTabs
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
import com.exponential.app.ui.issue.relativeTime
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
    val defaultBusy by viewModel.defaultBusy.collectAsStateWithLifecycle()
    val defaultError by viewModel.defaultError.collectAsStateWithLifecycle()
    val defaultsError by viewModel.defaultsError.collectAsStateWithLifecycle()
    val commandStates by viewModel.commandStates.collectAsStateWithLifecycle()
    // EXP-484: bumped on every window pick — the prefs store isn't observable,
    // so this is what re-reads it for the Agents section's rows.
    val usageWindowVersion by viewModel.usageWindowVersion.collectAsStateWithLifecycle()

    var label by remember { mutableStateOf(device.deviceLabel.ifBlank { device.deviceId }) }
    var nameFocused by remember { mutableStateOf(false) }
    var editableAgents by remember { mutableStateOf(editableAgents(device)) }
    var defaultAgent by remember { mutableStateOf(seededDefaultAgent(device, editableAgents)) }
    var agentTab by remember { mutableStateOf(defaultAgent) }
    var drafts by remember {
        mutableStateOf(editableAgents.associateWith { agentDraft(device, it) })
    }
    var removeTarget by remember { mutableStateOf<DeviceWorktreeEntity?>(null) }
    // Codex's logout revokes the token server-side, so switching accounts
    // there is confirmed first (EXP-484); claude just re-runs its login.
    var switchConfirmAgent by remember { mutableStateOf<String?>(null) }

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
        viewModel.queueDefaults(device.deviceId, buildDefaults(defaultAgent, editableAgents, next))
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = null,
        modifier = Modifier.statusBarsPadding().testTag("device-settings-sheet"),
    ) {
        Column(modifier = Modifier.fillMaxWidth().fillMaxHeight()) {
            // Done is the only chrome button now (edits save themselves), so
            // the title centers on the sheet rather than between two buttons.
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 4.dp),
                contentAlignment = Alignment.Center,
            ) {
                // EXP-686: the static sheet title — the machine's own name is
                // the editable field right below it.
                Text(
                    "Device settings",
                    style = MaterialTheme.typography.titleSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(horizontal = 72.dp),
                )
                TextButton(
                    onClick = onDismiss,
                    modifier = Modifier.align(Alignment.CenterEnd),
                ) { Text("Done") }
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
                            onValueChange = { next ->
                                label = next.take(MAX_DEVICE_LABEL)
                                viewModel.queueRename(
                                    device.deviceId,
                                    label.trim()
                                        .takeIf { it.isNotEmpty() && it != device.deviceLabel },
                                )
                            },
                            singleLine = true,
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
                            viewModel.queueDefaults(
                                device.deviceId,
                                buildDefaults(it, editableAgents, drafts),
                            )
                        },
                    )
                }
                Spacer(Modifier.height(4.dp))
                if (editableAgents.size > 1) {
                    // EXP-615: the same one-capsule agent strip the launch
                    // dialogs render.
                    AgentSegmentedTabs(
                        agents = editableAgents,
                        selected = agentTab,
                        onSelect = { agentTab = it },
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                    )
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
                ErrorCaption(defaultsError)
                Spacer(Modifier.height(8.dp))

                // ── Agents (EXP-484) ─────────────────────────────────────────
                // Read-only sign-in + usage status per agent, plus the login
                // the machine runs for itself. Only agents the machine
                // actually reported on are listed — a machine older than
                // EXP-484 reports none, and the whole section disappears
                // rather than claiming everything is signed out.
                val statusAgents = editableAgents.filter { agent ->
                    device.agentAccounts?.containsKey(agent) == true ||
                        device.agentUsage?.containsKey(agent) == true
                }
                if (statusAgents.isNotEmpty()) {
                    SectionLabel("Agents")
                    OptionGroup {
                        statusAgents.forEachIndexed { index, agent ->
                            if (index > 0) GroupDivider()
                            AgentAccountRow(
                                agent = agent,
                                account = device.agentAccounts?.get(agent),
                                usage = device.agentUsage?.get(agent),
                                usageAt = device.agentUsageAt,
                                state = commandStates[agentLoginCommandKey(agent)],
                                // The command opens a login flow ON the machine
                                // and publishes its URL back, so it needs a
                                // machine that is ours, online, and new enough
                                // to advertise the cap. pi has no remote
                                // sign-in at all (the server refuses it).
                                canLogin = device.online && device.canAgentLogin &&
                                    device.isMine && agent != "pi",
                                selectedWindow = remember(agent, usageWindowVersion) {
                                    viewModel.readUsageWindow(agent)
                                },
                                onSelectWindow = { key ->
                                    viewModel.rememberUsageWindow(agent, key)
                                },
                                onLogin = { switchAccount ->
                                    if (switchAccount && agent == "codex") {
                                        switchConfirmAgent = agent
                                    } else {
                                        viewModel.agentLogin(
                                            device.deviceId,
                                            agent,
                                            switchAccount,
                                            device.online,
                                        )
                                    }
                                },
                            )
                        }
                    }
                    Spacer(Modifier.height(8.dp))
                }

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

    switchConfirmAgent?.let { agent ->
        AlertDialog(
            onDismissRequest = { switchConfirmAgent = null },
            title = { Text("Switch ${agentLabel(agent)} account?") },
            text = {
                Text(
                    "Codex logout revokes the token server-side. You'll sign in " +
                        "again on that machine.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        viewModel.agentLogin(device.deviceId, agent, true, device.online)
                        switchConfirmAgent = null
                    },
                ) { Text("Switch account") }
            },
            dismissButton = {
                TextButton(onClick = { switchConfirmAgent = null }) { Text("Cancel") }
            },
        )
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

/**
 * One agent's status row in the Agents section (EXP-484): who is signed in on
 * that machine, its usage windows while they are fresh, and the button that
 * asks the machine to run the agent's OWN sign-in flow. No credential is ever
 * carried here — the machine publishes a login URL and the user finishes on
 * whatever device they are holding.
 */
@Composable
private fun AgentAccountRow(
    agent: String,
    account: AgentAccount?,
    usage: AgentUsage?,
    usageAt: String?,
    state: DeviceCommandUiState?,
    canLogin: Boolean,
    selectedWindow: String?,
    onSelectWindow: (String) -> Unit,
    onLogin: (Boolean) -> Unit,
) {
    val busy = state is DeviceCommandUiState.Sending || state is DeviceCommandUiState.Running
    // Freshness is decided once, on the same clock the strip uses: numbers
    // older than the window are simply not shown (fail closed).
    val fresh = usage != null &&
        AgentUsagePresentation.isFresh(usage.fetchedAt, System.currentTimeMillis())
    Column(modifier = Modifier.padding(start = 16.dp, end = 4.dp, top = 8.dp, bottom = 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Text(
                AgentUsagePresentation.accountRow(agent, account),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            when {
                busy -> CircularProgressIndicator(
                    modifier = Modifier.size(14.dp).padding(end = 2.dp),
                    strokeWidth = 2.dp,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                canLogin -> {
                    val switching = account?.signedIn == true
                    TextButton(onClick = { onLogin(switching) }) {
                        Icon(
                            if (switching) ExpIcons.uiSwap else ExpIcons.uiSignIn,
                            contentDescription = null,
                            modifier = Modifier.size(14.dp),
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(if (switching) "Switch account" else "Login")
                    }
                }
            }
        }
        if (fresh && usage != null) {
            Spacer(Modifier.height(6.dp))
            AgentUsageWindowRows(
                agent = agent,
                usage = usage,
                selectedKey = selectedWindow,
                onSelect = onSelectWindow,
                modifier = Modifier.padding(end = 12.dp),
            )
        }
        // No button here means nothing can be done from this screen, so the row
        // says how old what it shows is instead of looking live.
        if (!canLogin) {
            (account?.checkedAt ?: usageAt)?.let { at ->
                val relative = relativeTime(at)
                if (relative.isNotEmpty()) {
                    Text(
                        "as of $relative",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Tertiary,
                        ),
                        modifier = Modifier.padding(top = 2.dp),
                    )
                }
            }
        }
        LoginResultCaption(state)
    }
}

/**
 * What a queued `agent_login` command is doing (EXP-484). A completed one
 * publishes JSON — the machine's login URL, plus codex's device code — which
 * renders as an openable link and a copyable code; anything else (queued,
 * failed, a result that isn't a login publication) falls through to the
 * ordinary command caption.
 */
@Composable
private fun LoginResultCaption(state: DeviceCommandUiState?) {
    val login = (state as? DeviceCommandUiState.Done)?.let { parseAgentLoginResult(it.message) }
    when {
        state is DeviceCommandUiState.Sending || state is DeviceCommandUiState.Running ->
            Text(
                "Waiting for the sign-in link…",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.padding(top = 4.dp),
            )
        login != null -> {
            val uriHandler = LocalUriHandler.current
            val clipboard = LocalClipboardManager.current
            Spacer(Modifier.height(4.dp))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth().clickable { uriHandler.openUri(login.url) },
            ) {
                Icon(
                    ExpIcons.uiExternalLink,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    login.url,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            login.code?.let { code ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        code,
                        style = MaterialTheme.typography.bodyMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    IconButton(onClick = { clipboard.setText(AnnotatedString(code)) }) {
                        Icon(
                            ExpIcons.uiCopy,
                            contentDescription = "Copy code",
                            modifier = Modifier.size(14.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = TextEmphasis.Tertiary,
                            ),
                        )
                    }
                }
            }
            Text(
                if (login.code == null) {
                    "Open the link on any device."
                } else {
                    "Open the link on any device and enter the code on the machine."
                },
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        else -> CommandCaption(state)
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
