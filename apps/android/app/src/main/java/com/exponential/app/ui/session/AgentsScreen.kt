package com.exponential.app.ui.session

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.DeviceLatestVersions
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.deviceUpdateAvailable
import com.exponential.app.domain.AgentAccountUsageGroup
import com.exponential.app.domain.AgentAccountsRows
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.AgentHealthRules
import com.exponential.app.domain.AgentProfileUsageRow
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.DeviceAccountChip
import com.exponential.app.domain.LaunchDeviceRules
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.agentIconPainter
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import kotlinx.coroutines.delay

/**
 * The Devices tab (EXP-686, the renamed Agents surface): "My devices" — the
 * caller's registered devices (EXP-403: desktop IDEs and headless
 * `exponential` servers, online and offline) plus, since EXP-432, the
 * selected team's shared servers. EXP-825: machines ONLY (web parity,
 * EXP-818) — the Running / Past session lists moved to the Agent page, and a
 * machine's play glyph opens that page with the machine preselected instead
 * of a launcher sheet of its own.
 *
 * EXP-829: below the machines sits "Accounts" (web `AgentAccountsSection`,
 * desktop `accounts_section.rs`) — one row per agent account the machines
 * report, its machines as chips (a check where the account is the ACTIVE
 * login there), the freshest machine's usage windows, refreshed by itself
 * while the page is open.
 *
 * EXP-849 splits the two surfaces deliberately, and they are NOT mirror
 * images:
 *   - **Accounts** is the DECISION surface — one row per account (who it is,
 *     what it may spend, how healthy it is). The machine chips there are QUIET
 *     presence indicators, never a control cluster.
 *   - **My devices** is the SETUP/REPAIR surface — one row per machine, with
 *     an ACCOUNT CHIP per login it holds: the health badge bubbles to the row,
 *     and the chip's menu makes another login this machine's default
 *     (`agent_profile_use`), removes this machine's copy of one
 *     (`agent_profile_remove`, EXP-862), or signs one in — the login sheet
 *     owns the link and the code field, wherever it was opened from.
 *
 * EXP-862: the account CHIPS are controls on BOTH surfaces (same menu, one
 * rule), the Accounts header carries "+ Add account", every account's machines
 * end in a bare `+` ("Sign in on <device>"), and nothing refreshes by hand.
 */
@Composable
fun AgentsScreen(
    // EXP-825: every play button navigates to the composer with a seed.
    onOpenAgent: (AgentComposerSeed) -> Unit,
    viewModel: AgentsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val latestVersions by viewModel.latestVersions.collectAsStateWithLifecycle()
    val deviceBusy by viewModel.deviceBusy.collectAsStateWithLifecycle()
    val accountSections by viewModel.accountSections.collectAsStateWithLifecycle()
    val accountsError by viewModel.accountsError.collectAsStateWithLifecycle()
    // EXP-849: the account commands the MACHINE rows issued (`agent_profile_use`
    // — "use this account here"), keyed by machine × login: the chip spins
    // while one is in flight and the row captions a refusal. Sign-ins are not
    // here: they round-trip a link and a code, which is the device-settings
    // sheet's job.
    val accountCommandStates by viewModel.accountCommandStates.collectAsStateWithLifecycle()

    // EXP-817: the section's own refresh round — on every change of the rows
    // and on the same 30s clock the countdowns re-read on (web `useNow`).
    LaunchedEffect(accountSections) { viewModel.autoRefreshAccounts() }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000L)
            viewModel.autoRefreshAccounts()
        }
    }

    // The machine row whose settings sheet (EXP-481) / Remove dialog is open.
    var settingsTargetId by remember { mutableStateOf<String?>(null) }
    var removeTarget by remember { mutableStateOf<SteerDevice?>(null) }
    // EXP-862: the sign-in a chip (or the Add-account pill) asked for — the
    // machine runs its agent's OWN login flow and publishes the link back, so
    // the sheet is the ONE place that renders a login, wherever it started.
    var loginTarget by remember { mutableStateOf<AgentLoginTarget?>(null) }
    var addAccountOpen by remember { mutableStateOf(false) }
    // The login whose "Remove account" is waiting on its confirm.
    var removeTargetAccount by remember {
        mutableStateOf<Pair<SteerDevice, AgentProfileUsageRow>?>(null)
    }
    // …and its machine-row twin, which names a chip rather than a usage row.
    var removeTargetChip by remember {
        mutableStateOf<Pair<SteerDevice, DeviceAccountChip>?>(null)
    }
    // EXP-849: the Accounts section's agent TAB (claude | codex) — one agent's
    // rows at a time, so the second agent's accounts never crowd the first's
    // (web/desktop parity). Null = the first reported agent.
    var accountAgentTab by rememberSaveable { mutableStateOf<String?>(null) }

    val steerOn = state.steerEnabled == true
    val listState = rememberLazyListState()
    // EXP-432: the team-scoped list appends teammates' shared servers — they
    // belong under their own header, never in the caller's "My devices" count.
    val ownDevices = devices?.filter { it.isMine }
    val teamDevices = devices?.filterNot { it.isMine }.orEmpty()

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Text(
                "Devices",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            )
            // No relay → nothing here can take a start: the full empty state.
            if (state.steerEnabled == false) {
                AgentsEmptyState()
            } else if (steerOn) {
                LazyColumn(
                    state = listState,
                    modifier = Modifier.fillMaxSize().testTag("devices-list"),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    item(key = "__machines_header__") { SectionHeader("My devices") }
                    when {
                        // null = still loading; render nothing under the header.
                        ownDevices == null -> Unit
                        ownDevices.isEmpty() -> item(key = "__no_machine__") {
                            HintRow(
                                "No devices yet. Open the Exponential desktop app, or add a " +
                                    "device on the web.",
                            )
                        }
                        else -> items(ownDevices, key = { "dev_${it.deviceId}" }) { device ->
                            MachineRow(
                                device = device,
                                latestVersions = latestVersions,
                                busy = device.deviceId in deviceBusy,
                                commandStates = accountCommandStates,
                                onStart = { onOpenAgent(AgentComposerSeed(deviceId = device.deviceId)) },
                                onEdit = { settingsTargetId = device.deviceId },
                                onRemove = { removeTarget = device },
                                onUpdate = { viewModel.requestDeviceUpdate(device.deviceId) },
                                onSetAccountDefault = { chip -> viewModel.useAccountHere(device, chip) },
                                onRemoveAccount = { chip -> removeTargetChip = device to chip },
                                // EXP-862: the sign-in link, its code field and
                                // the waiting state live in ONE sheet, opened
                                // on the login that needs the repair — the
                                // device-settings sheet carries no accounts.
                                onSignInAccount = { chip ->
                                    loginTarget = AgentLoginTarget(
                                        device = device,
                                        agent = chip.agent,
                                        profileId = chip.profileId,
                                    )
                                },
                            )
                        }
                    }
                    // Teammates' machines: startable, but with no rename /
                    // remove / update menu — they are not this user's to
                    // curate (sharing itself is managed on the web).
                    if (teamDevices.isNotEmpty()) {
                        item(key = "__team_machines_header__") { SectionHeader("Team devices") }
                        items(teamDevices, key = { "shared_${it.deviceId}" }) { device ->
                            MachineRow(
                                device = device,
                                latestVersions = latestVersions,
                                busy = false,
                                commandStates = accountCommandStates,
                                onStart = { onOpenAgent(AgentComposerSeed(deviceId = device.deviceId)) },
                                onEdit = {},
                                onRemove = {},
                                onUpdate = {},
                                // A teammate's machine renders its logins
                                // READ-ONLY: seeing that a shared server's
                                // codex login expired explains a refused
                                // start, but only its owner can fix it.
                                onSetAccountDefault = {},
                                onRemoveAccount = {},
                                onSignInAccount = {},
                            )
                        }
                    }
                    // EXP-829: the Accounts section. The header carries the
                    // tag the screenshot flow scrolls to, and (EXP-862) the
                    // "Add account" pill — the Add-device twin. No refresh
                    // note: the numbers refresh themselves, and saying so was
                    // chrome about chrome.
                    item(key = "__accounts_gap__") { Spacer(Modifier.height(10.dp)) }
                    item(key = "__accounts_header__") {
                        SectionHeader(
                            "Accounts",
                            modifier = Modifier.testTag("agent-accounts-section"),
                            trailing = {
                                GlassPill(
                                    "Add account",
                                    size = PillSize.Sm,
                                    icon = ExpIcons.uiAdd,
                                    onClick = { addAccountOpen = true },
                                    modifier = Modifier.testTag("add-account"),
                                )
                            },
                        )
                    }
                    accountsError?.let { message ->
                        item(key = "__accounts_error__") {
                            Text(
                                message,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.error,
                                modifier = Modifier.padding(horizontal = 4.dp),
                            )
                        }
                    }
                    val sections = accountSections
                    when {
                        sections == null -> item(key = "__accounts_loading__") { HintRow("Loading…") }
                        sections.isEmpty() -> item(key = "__no_accounts__") {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                modifier = Modifier.padding(horizontal = 4.dp, vertical = 4.dp),
                            ) {
                                Icon(
                                    ExpIcons.uiDeviceOffline,
                                    contentDescription = null,
                                    modifier = Modifier.size(14.dp),
                                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                )
                                Text(
                                    AgentAccountsRows.EMPTY_STATE,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                )
                            }
                        }
                        else -> {
                            // EXP-849: per-AGENT tabs, not a band per agent —
                            // codex's accounts no longer push claude's off the
                            // screen. Only agents a machine reported get a
                            // segment, and a lone agent gets no strip at all.
                            val agents = sections.map { it.agent }
                            val selectedAgent = accountAgentTab?.takeIf { it in agents }
                                ?: agents.first()
                            if (agents.size >= 2) {
                                item(key = "__accounts_tabs__") {
                                    GlassSegmentedControl(
                                        options = agents,
                                        selected = selectedAgent,
                                        label = ::agentLabel,
                                        onSelect = { accountAgentTab = it },
                                        // EXP-862: the agent is recognised by
                                        // its brand mark everywhere else, so
                                        // the tab carries it too.
                                        leadingIcon = { agent ->
                                            Icon(
                                                agentIconPainter(agent),
                                                contentDescription = null,
                                                modifier = Modifier.size(14.dp),
                                            )
                                        },
                                        modifier = Modifier
                                            .padding(bottom = 4.dp)
                                            .testTag("agent-accounts-tabs"),
                                        testTag = { "agent-accounts-tab-$it" },
                                    )
                                }
                            }
                            val section = sections.first { it.agent == selectedAgent }
                            items(section.groups, key = { "acct_${it.key}" }) { group ->
                                AccountRow(
                                    group = group,
                                    devices = devices.orEmpty(),
                                    commandStates = accountCommandStates,
                                    onSetDefault = { device, row ->
                                        viewModel.setAccountDefault(device, row.agent, row.profileId)
                                    },
                                    onRemove = { device, row -> removeTargetAccount = device to row },
                                    onSignIn = { device, row ->
                                        loginTarget = AgentLoginTarget(
                                            device = device,
                                            agent = row.agent,
                                            profileId = row.profileId,
                                        )
                                    },
                                    onAddHere = { device ->
                                        val target = AgentAccountsRows.addAccountLoginTarget(
                                            device,
                                            group.agent,
                                            AgentAccountsRows.nextProfileLabel(
                                                device,
                                                group.agent,
                                                agentLabel(group.agent),
                                            ),
                                        )
                                        loginTarget = AgentLoginTarget(
                                            device = device,
                                            agent = group.agent,
                                            profileId = target.profileId,
                                            newProfileLabel = target.newProfileLabel,
                                        )
                                    },
                                )
                            }
                        }
                    }
                }
            }
        }
    }

    // EXP-481: the device-settings sheet, re-resolving the LIVE row on every
    // sync delta so saved edits reflect without reopening. Owner-only — the
    // menu only exists on "mine" rows.
    settingsTargetId?.let { targetId ->
        // The row can vanish mid-edit (device removed elsewhere) — the sheet
        // simply stops rendering; the stale id is harmless and replaced on
        // the next Edit tap.
        devices?.firstOrNull { it.deviceId == targetId && it.isMine }?.let { target ->
            DeviceSettingsSheet(
                device = target,
                onDismiss = { settingsTargetId = null },
            )
        }
    }

    // Removing drops the registry row only — say so, or an owner who removes a
    // machine that is still running the daemon reads its return as a bug.
    removeTarget?.let { device ->
        AlertDialog(
            onDismissRequest = { removeTarget = null },
            title = { Text("Remove machine?") },
            text = {
                Text(
                    "Remove “${device.displayLabel}” from your machines? A machine with the " +
                        "daemon still running will re-register itself on its next heartbeat.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        viewModel.removeDevice(device.deviceId)
                        removeTarget = null
                    },
                ) { Text("Remove") }
            },
            dismissButton = {
                TextButton(onClick = { removeTarget = null }) { Text("Cancel") }
            },
        )
    }

    // EXP-862: "Remove account" — the machine deletes ITS copy of the login.
    // The confirm names the login and the machine and says in the same breath
    // that the account itself survives (the pinned sentence ×4).
    removeTargetChip?.let { (device, chip) ->
        RemoveAccountDialog(
            accountLabel = AgentAccountsRows.machineChipLabel(chip, ::agentLabel),
            deviceLabel = device.displayLabel,
            onConfirm = {
                viewModel.removeAccountHere(device, chip.agent, chip.profileId)
                removeTargetChip = null
            },
            onDismiss = { removeTargetChip = null },
        )
    }
    removeTargetAccount?.let { (device, row) ->
        RemoveAccountDialog(
            accountLabel = row.email ?: row.profileLabel,
            deviceLabel = device.displayLabel,
            onConfirm = {
                viewModel.removeAccountHere(device, row.agent, row.profileId)
                removeTargetAccount = null
            },
            onDismiss = { removeTargetAccount = null },
        )
    }

    // EXP-862: "Add account" — pick one of the caller's online machines and an
    // agent installed there, then hand off to the sign-in sheet, which is the
    // ONE place a login renders on this client.
    if (addAccountOpen) {
        AddAccountSheet(
            devices = AgentAccountsRows.addAccountDevices(devices.orEmpty()),
            onPick = { device, agent ->
                addAccountOpen = false
                val target = AgentAccountsRows.addAccountLoginTarget(
                    device,
                    agent,
                    AgentAccountsRows.nextProfileLabel(device, agent, agentLabel(agent)),
                )
                loginTarget = AgentLoginTarget(
                    device = device,
                    agent = agent,
                    profileId = target.profileId,
                    newProfileLabel = target.newProfileLabel,
                )
            },
            onDismiss = { addAccountOpen = false },
        )
    }

    loginTarget?.let { target ->
        AgentLoginSheet(target = target, onDismiss = { loginTarget = null })
    }
}

/** The pinned confirm ×4 — see [AgentAccountsRows.removeAccountConfirm]. */
@Composable
private fun RemoveAccountDialog(
    accountLabel: String,
    deviceLabel: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(AgentAccountsRows.ACTION_REMOVE) },
        text = { Text(AgentAccountsRows.removeAccountConfirm(accountLabel, deviceLabel)) },
        confirmButton = { TextButton(onClick = onConfirm) { Text("Remove") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/**
 * EXP-862: "Add account" — the machine and the agent the sign-in runs on (web
 * `AddAccountDialog`). Only the caller's ONLINE machines that advertise
 * `agent-login` can take one, and only the agents installed there.
 */
@Composable
private fun AddAccountSheet(
    devices: List<SteerDevice>,
    onPick: (SteerDevice, String) -> Unit,
    onDismiss: () -> Unit,
) {
    var deviceId by remember(devices) { mutableStateOf(devices.firstOrNull()?.deviceId ?: "") }
    val device = devices.firstOrNull { it.deviceId == deviceId }
    val agents = device?.let(AgentAccountsRows::addableAgents).orEmpty()
    var agent by remember(agents) { mutableStateOf(agents.firstOrNull() ?: "") }
    GlassSheet(
        title = "Add account",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("add-account-sheet"),
    ) {
        Column(modifier = Modifier.fillMaxWidth()) {
            if (devices.isEmpty()) {
                Text(
                    "None of your devices is online with an agent that can sign in " +
                        "remotely. Open the desktop app or start the daemon there first.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
                )
            } else {
                OptionGroup {
                    PickerRow(
                        label = "Device",
                        value = device?.displayLabel ?: "",
                        options = devices.map { it.deviceId },
                        selected = deviceId,
                        optionLabel = { id ->
                            devices.firstOrNull { it.deviceId == id }?.displayLabel ?: id
                        },
                        optionIcon = { id ->
                            val row = devices.firstOrNull { it.deviceId == id }
                            if (row?.isServer == true) ExpIcons.uiServer else ExpIcons.uiDevice
                        },
                        onSelect = { deviceId = it },
                    )
                    GroupDivider()
                    PickerRow(
                        label = "Agent",
                        value = agent.takeIf { it.isNotEmpty() }?.let(::agentLabel) ?: "",
                        options = agents,
                        selected = agent,
                        optionLabel = ::agentLabel,
                        enabled = agents.isNotEmpty(),
                        onSelect = { agent = it },
                    )
                }
                Spacer(Modifier.height(12.dp))
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp),
                    horizontalArrangement = Arrangement.End,
                ) {
                    GlassPill(
                        "Sign in",
                        icon = ExpIcons.uiSignIn,
                        primary = true,
                        enabled = device != null && agent.isNotEmpty(),
                        onClick = { device?.let { onPick(it, agent) } },
                    )
                }
            }
            Spacer(Modifier.height(16.dp))
        }
    }
}

/** Never render a bare blank row: a label-less machine falls back to its id. */
private val SteerDevice.displayLabel: String get() = deviceLabel.ifBlank { deviceId }

/**
 * One registered machine (EXP-403): kind glyph + label with a hair-small
 * version, a status line (green dot Online / "Last seen …" / Offline), the
 * "Start coding" pill for STARTABLE machines only, and the row menu — Rename
 * and Remove for registered rows, plus Update for an online server daemon. A
 * row that predates the registry (`registered == false`, live off relay
 * presence) has nothing to rename or remove, so it carries no menu.
 *
 * EXP-409/EXP-836: an online machine with NO runnable agent can take no start,
 * so it reads like an offline row (dimmed glyph, no pill) with an amber reason
 * in place of "Online" — the signed-out agents when it named any, else
 * [LaunchDeviceRules.NO_RUNNABLE_AGENT]; a machine that CAN run something but
 * has signed-out agents left over just gets a quiet note.
 *
 * EXP-432: a TEAMMATE's shared server (`owner != null`) renders read-only —
 * "shared by <owner>" in place of the version chip and no row menu at all,
 * since neither the registry row nor the share is the caller's to change. Own
 * rows shared with a team carry a quiet "Shared" chip so the reason teammates
 * can start there is visible from the phone (the toggle stays web-only).
 */
@Composable
private fun MachineRow(
    device: SteerDevice,
    latestVersions: DeviceLatestVersions,
    busy: Boolean,
    /** EXP-849: this machine's account commands in flight, keyed by chip. */
    commandStates: Map<String, DeviceCommandUiState>,
    onStart: () -> Unit,
    onEdit: () -> Unit,
    onRemove: () -> Unit,
    onUpdate: () -> Unit,
    /** EXP-862 "Set as default": make this login the machine's ACTIVE one
     *  (`agent_profile_use`). */
    onSetAccountDefault: (DeviceAccountChip) -> Unit,
    /** EXP-862 "Remove account": the machine drops ITS copy of the login. */
    onRemoveAccount: (DeviceAccountChip) -> Unit,
    /** EXP-849: run the agent's own sign-in here — the login sheet owns the flow. */
    onSignInAccount: (DeviceAccountChip) -> Unit,
) {
    val online = device.online
    // Installed-but-signed-out agents (EXP-409): they block a start outright
    // when nothing else is runnable, and are worth a note when something is.
    val unauthed = device.unauthedAgentIds
    // EXP-836: a start needs an online machine WITH a runnable agent. Gating on
    // the signed-out case alone let a machine that reported no agents at all
    // keep its play button, and the composer then dropped the pre-picked
    // machine for the default one without a word.
    val startable = LaunchDeviceRules.startable(device)
    // Online but unstartable for either reason: the row dims and its caption
    // carries the reason in place of "Online".
    val blockedCaption = LaunchDeviceRules.blockedCaption(device)
    // A server runs the CLI, a desktop the IDE — each compares against its own
    // channel's advertised latest.
    val outdated = deviceUpdateAvailable(
        device.version,
        if (device.isServer) latestVersions.cli else latestVersions.desktop,
    )
    // EXP-849: the row is a COLUMN now — its machine line, then the account
    // chips (the repair surface). The whole block keeps the one tap target.
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(enabled = startable, onClick = onStart)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                if (device.isServer) ExpIcons.uiServer else ExpIcons.uiDevice,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (startable) TextEmphasis.Secondary else TextEmphasis.Tertiary,
                ),
            )
            Spacer(Modifier.width(12.dp))
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        device.displayLabel,
                        style = MaterialTheme.typography.bodyMedium,
                        // An unstartable machine greys out: it looks present but
                        // can take nothing, so it must not read as fully available.
                        color = if (blockedCaption != null) {
                            MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                        } else {
                            MaterialTheme.colorScheme.onSurface
                        },
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    val owner = device.owner
                    if (owner != null) {
                        Spacer(Modifier.width(6.dp))
                        Text(
                            "shared by ${owner.name}",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    } else if (device.version != null) {
                        Spacer(Modifier.width(6.dp))
                        Text(
                            "v${device.version}",
                            style = MaterialTheme.typography.labelSmall,
                            color = if (outdated) {
                                NeedsInputAmber
                            } else {
                                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                            },
                            maxLines = 1,
                        )
                    }
                    // EXP-622: the machine every device picker prefills.
                    if (device.isDefault) {
                        Spacer(Modifier.width(6.dp))
                        Icon(
                            ExpIcons.uiDeviceDefault,
                            contentDescription = "Default machine",
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            modifier = Modifier.size(13.dp),
                        )
                    }
                    if (device.isMine && device.sharedTeamIds.isNotEmpty()) {
                        Spacer(Modifier.width(6.dp))
                        Text(
                            "Shared",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                        )
                    }
                    // EXP-849: the machine's WORST account health. "Needs
                    // re-login" (a credential that expired under the user — the
                    // CLI still claims it is signed in) is deliberately distinct
                    // from "Signed out" (a login nobody ever made). EXP-862: it
                    // is the ONLY sign-in notice the row carries — the status
                    // line below no longer names signed-out agents, and the
                    // chips underneath say which login is which.
                    val healthBadge = AgentHealthRules
                        .deviceWorst(device.agentAccounts)
                        ?.let(AgentHealthRules::badgeLabel)
                    if (healthBadge != null) {
                        Spacer(Modifier.width(6.dp))
                        Text(
                            healthBadge,
                            style = MaterialTheme.typography.labelSmall,
                            color = NeedsInputAmber,
                            maxLines = 1,
                            modifier = Modifier.testTag("device-health-badge"),
                        )
                    }
                }
                Spacer(Modifier.height(2.dp))
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    // A pending update outranks the presence caption: the daemon is
                    // about to restart, so "Online" would only read as a lie. But a
                    // request parked behind live sessions (EXP-411) reads "Update
                    // queued" without a spinner — it applies once they close.
                    if (device.updateRequested && !device.updateBlocked) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(10.dp),
                            strokeWidth = 1.5.dp,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                    } else if (online && !device.updateQueued) {
                        StaticDot(if (blockedCaption != null) NeedsInputAmber else ReviewGreen, size = 6.dp)
                    }
                    Text(
                        when {
                            device.updateQueued -> "Update queued"
                            device.updateRequested -> "Updating…"
                            blockedCaption != null -> blockedCaption
                            online -> "Online"
                            device.lastSeenAt != null -> "Last seen ${relativeTime(device.lastSeenAt)}"
                            else -> "Offline"
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = if (blockedCaption != null && !device.updateRequested) {
                            NeedsInputAmber
                        } else {
                            MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                        },
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                }
            }
            // EXP-615: an icon-only play button (one Run/Start affordance across
            // the clients). Offline machines can't take a start (the relay refuses
            // it), nor can ones with no runnable agent (EXP-409/EXP-836), so the
            // affordance is simply absent — the status line above says why.
            // EXP-694: on the shared glass circle (iOS `CircleIconButton` parity),
            // not a bare primary-tinted glyph in an M3 touch box.
            if (startable) {
                CircleIconButton(
                    ExpIcons.actionRun,
                    contentDescription = "Start coding",
                    onClick = onStart,
                    modifier = Modifier.padding(start = 8.dp),
                )
            }
            // Rename / Update / Remove all mutate the OWNER's registry row, so a
            // teammate's shared machine carries no menu at all (EXP-432).
            if (device.registered && device.isMine) {
                var rowMenu by remember { mutableStateOf(false) }
                Box {
                    CircleIconButton(
                        ExpIcons.uiMore,
                        contentDescription = "Machine actions",
                        onClick = { rowMenu = true },
                        modifier = Modifier.padding(start = 8.dp),
                    )
                    GlassDropdownMenu(expanded = rowMenu, onDismissRequest = { rowMenu = false }) {
                        // EXP-481: Rename and the share toggle live INSIDE the
                        // device-settings sheet, so the menu opens that sheet
                        // (EXP-862: "Device settings" and the gear ×4 — "Edit"
                        // with a pencil never said what it edited).
                        GlassMenuItem(
                            text = { Text("Device settings") },
                            leadingIcon = { Icon(ExpIcons.navSettings, contentDescription = null) },
                            enabled = !busy,
                            onClick = {
                                rowMenu = false
                                onEdit()
                            },
                        )
                        // Self-update is a server-daemon capability: the desktop
                        // app updates itself through its own channel, and an
                        // offline machine has nothing listening for the request.
                        // EXP-420: offered only when a newer version really exists.
                        if (device.isServer && online && outdated && !device.updateRequested) {
                            GlassMenuItem(
                                text = { Text("Update") },
                                leadingIcon = { Icon(ExpIcons.uiUpdate, contentDescription = null) },
                                enabled = !busy,
                                onClick = {
                                    rowMenu = false
                                    onUpdate()
                                },
                            )
                        }
                        GlassMenuItem(
                            text = { Text("Remove") },
                            leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                            enabled = !busy,
                            destructive = true,
                            onClick = {
                                rowMenu = false
                                onRemove()
                            },
                        )
                    }
                }
            }
        }
        MachineAccountChips(
            deviceId = device.deviceId,
            chips = AgentAccountsRows.deviceAccountChips(device.agentAccounts),
            // A repair only runs on one of MY machines that is listening and
            // new enough to advertise the cap — the server refuses the
            // commands without it, and an offline machine would hold them
            // until it wakes, which reads as a dead tap.
            actionable = device.isMine && online && device.canAgentLogin,
            // EXP-849: `agent_profile_use` is REFUSED by the server on a
            // machine that does not also advertise `account-switch` (it
            // shipped in desktop/CLI 0.14.38, above the fleet floor), so an
            // older machine never offers the pick — its logins are repaired by
            // signing in, which every `agent-login` build can run.
            canSwitch = device.canSwitchAccount,
            // EXP-862: and `account-remove` for the destructive entry — the
            // server refuses the command on a build without it.
            canRemove = device.canRemoveAccount,
            commandStates = commandStates,
            onSetDefault = onSetAccountDefault,
            onRemove = onRemoveAccount,
            onSignIn = onSignInAccount,
        )
    }
}

/**
 * EXP-849: the logins ONE machine holds, as the chips on its row — the
 * setup/repair half of the accounts split (web `MachineAccountChips`, iOS
 * `DeviceAccountChips`).
 *
 * The Accounts section decides WHICH account to run on; a machine row is where
 * a broken or missing login gets fixed. So each chip names the agent and the
 * login, badges THIS machine's health for it, and carries the repairs that
 * machine owes it (EXP-862): sign in, make it the machine's default
 * (`agent_profile_use` — no login flow, no logout, no credential touched), or
 * remove this machine's copy of it.
 */
@Composable
private fun MachineAccountChips(
    deviceId: String,
    chips: List<DeviceAccountChip>,
    actionable: Boolean,
    /** The machine advertises `account-switch` — see [AgentAccountsRows.chipActions]. */
    canSwitch: Boolean,
    /** …and `account-remove`, for the destructive entry. */
    canRemove: Boolean,
    commandStates: Map<String, DeviceCommandUiState>,
    onSetDefault: (DeviceAccountChip) -> Unit,
    onRemove: (DeviceAccountChip) -> Unit,
    onSignIn: (DeviceAccountChip) -> Unit,
) {
    if (chips.isEmpty()) return
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        chips.forEach { chip ->
            key(chip.key) {
                MachineAccountChip(
                    chip = chip,
                    actionable = actionable,
                    canSwitch = canSwitch,
                    canRemove = canRemove,
                    state = commandStates[deviceAccountCommandKey(deviceId, chip)],
                    onSetDefault = { onSetDefault(chip) },
                    onRemove = { onRemove(chip) },
                    onSignIn = { onSignIn(chip) },
                )
            }
        }
    }
    // The material outcome of a pick arrives by SYNC (the machine re-reports
    // its accounts, which moves the check), but a refusal would otherwise be
    // silent — including the honest one a machine too old to know the command
    // answers with.
    chips.forEach { chip ->
        val failure = commandStates[deviceAccountCommandKey(deviceId, chip)]
            as? DeviceCommandUiState.Failed ?: return@forEach
        Text(
            failure.message,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.error,
        )
    }
}

/** One login of one machine: the agent, the login, its state, its menu. */
@Composable
private fun MachineAccountChip(
    chip: DeviceAccountChip,
    actionable: Boolean,
    canSwitch: Boolean,
    canRemove: Boolean,
    state: DeviceCommandUiState?,
    onSetDefault: () -> Unit,
    onRemove: () -> Unit,
    onSignIn: () -> Unit,
) {
    val label = AgentAccountsRows.machineChipLabel(chip, ::agentLabel)
    val badge = AgentHealthRules.badgeLabel(chip.health)
    val busy = state is DeviceCommandUiState.Sending || state is DeviceCommandUiState.Running
    var menuOpen by remember { mutableStateOf(false) }
    // EXP-862: the entries, decided in ONE place ×4 — a signed-out or expired
    // login offers a sign-in and nothing else; a healthy one can become the
    // machine's default and can be removed from it. Empty = the chip is a
    // statement (a teammate's machine, an offline one, the ambient login).
    val actions = if (actionable) {
        AgentAccountsRows.chipActions(chip, canSwitchAccount = canSwitch, canRemoveAccount = canRemove)
    } else {
        emptyList()
    }
    val description = buildString {
        append(label)
        if (chip.active) append(", the account this machine uses")
        badge?.let { append(", ${it.lowercase()}") }
    }
    val pill: @Composable () -> Unit = {
        GlassPill(
            label,
            size = PillSize.Sm,
            onClick = if (actions.isEmpty()) null else ({ menuOpen = true }),
            trailing = when {
                busy -> null
                badge != null -> {
                    {
                        Icon(
                            ExpIcons.uiWarning,
                            contentDescription = badge,
                            tint = NeedsInputAmber,
                            modifier = Modifier.size(12.dp),
                        )
                    }
                }
                chip.signedIn && chip.active -> {
                    {
                        Icon(
                            ExpIcons.uiCheck,
                            contentDescription = "The account this machine uses",
                            tint = ReviewGreen,
                            modifier = Modifier.size(12.dp),
                        )
                    }
                }
                else -> null
            },
            loading = busy,
            contentDescription = description,
            modifier = Modifier.testTag("device-account-chip"),
        )
    }
    if (actions.isEmpty()) {
        pill()
        return
    }
    Box {
        pill()
        GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            AccountChipMenuItems(
                actions = actions,
                busy = busy,
                onSignIn = {
                    menuOpen = false
                    onSignIn()
                },
                onSetDefault = {
                    menuOpen = false
                    onSetDefault()
                },
                onRemove = {
                    menuOpen = false
                    onRemove()
                },
            )
        }
    }
}

/**
 * EXP-862: the account chip menu, identical on a machine row and on an account
 * row's machine chip — never a logout (signing codex out would revoke the
 * account server-wide) and never a credential copy: the files stay where the
 * CLI wrote them, and "Remove account" deletes only THIS machine's copy.
 */
@Composable
private fun AccountChipMenuItems(
    actions: List<String>,
    busy: Boolean,
    onSignIn: () -> Unit,
    onSetDefault: () -> Unit,
    onRemove: () -> Unit,
) {
    actions.forEach { action ->
        when (action) {
            AgentAccountsRows.ACTION_SIGN_IN -> GlassMenuItem(
                text = { Text(action) },
                leadingIcon = { Icon(ExpIcons.uiSignIn, contentDescription = null) },
                enabled = !busy,
                onClick = onSignIn,
            )
            AgentAccountsRows.ACTION_SET_DEFAULT -> GlassMenuItem(
                text = { Text(action) },
                leadingIcon = { Icon(ExpIcons.uiSwap, contentDescription = null) },
                enabled = !busy,
                onClick = onSetDefault,
            )
            AgentAccountsRows.ACTION_REMOVE -> GlassMenuItem(
                text = { Text(action) },
                leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                enabled = !busy,
                destructive = true,
                onClick = onRemove,
            )
        }
    }
}

/**
 * EXP-829: one account ROW under its agent band — the identity line (who the
 * account is, plus its plan and the health badge when there is one), the
 * machine chips, and the freshest machine's usage windows (the same cards the
 * device settings sheet used to render; dimmed with an "as of …" line once
 * they are older than the freshness window). Web `AccountCard`, desktop
 * `render_row`.
 *
 * EXP-862: no refresh button — the numbers re-read themselves while the page
 * is open, so a control that mostly said "not yet" was chrome. The chips are
 * controls now: each carries the same menu a machine row's chip does, and a
 * bare `+` offers the account to a machine that does not hold it yet.
 */
@Composable
private fun AccountRow(
    group: AgentAccountUsageGroup,
    /** Every machine the caller can see — the chips resolve their caps here. */
    devices: List<SteerDevice>,
    commandStates: Map<String, DeviceCommandUiState>,
    onSetDefault: (SteerDevice, AgentProfileUsageRow) -> Unit,
    onRemove: (SteerDevice, AgentProfileUsageRow) -> Unit,
    onSignIn: (SteerDevice, AgentProfileUsageRow) -> Unit,
    /** "Sign in on <device>" — the account gains a machine. */
    onAddHere: (SteerDevice) -> Unit,
) {
    val nowMs = rememberUsageClock()
    val usage = group.usage
    val fresh = AgentUsagePresentation.isFresh(usage?.fetchedAt, nowMs)
    // The "as of …" fallback: the numbers' own stamp, else when a machine last
    // probed the account.
    val asOf = (usage?.fetchedAt?.takeIf { it.isNotBlank() } ?: group.checkedAt)
        ?.let(::relativeTime)?.takeIf { it.isNotEmpty() }
    // The machines this account is NOT on yet — where the `+` chip can put it.
    val holders = group.rows.map { it.deviceId }.toSet()
    val addTargets = AgentAccountsRows.addAccountDevices(devices, group.agent, holders)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
            .testTag("agent-account-row"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Text(
                AgentAccountsRows.caption(group),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            if (group.signedIn && group.email != null && group.plan != null) {
                Text(
                    " · ${group.plan}",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                )
            }
            // EXP-862: the ONE sign-in notice this row carries — a signed-out
            // or expired credential, in the badge the caption no longer says.
            AgentAccountsRows.healthBadge(group)?.let { badge ->
                Spacer(Modifier.width(6.dp))
                Text(
                    badge,
                    style = MaterialTheme.typography.labelSmall,
                    color = NeedsInputAmber,
                    maxLines = 1,
                    modifier = Modifier.testTag("agent-account-health"),
                )
            }
        }
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            group.rows.forEach { row ->
                // Keyed: the rows re-sort as health and usage move.
                key(row.key) {
                    val device = devices.firstOrNull { it.deviceId == row.deviceId }
                    DeviceChip(
                        row = row,
                        device = device,
                        state = commandStates[
                            accountCommandKey(row.deviceId, row.agent, row.profileId),
                        ],
                        onSetDefault = { device?.let { onSetDefault(it, row) } },
                        onRemove = { device?.let { onRemove(it, row) } },
                        onSignIn = { device?.let { onSignIn(it, row) } },
                    )
                }
            }
            if (addTargets.isNotEmpty()) {
                key("__add__") { AddAccountHereChip(devices = addTargets, onPick = onAddHere) }
            }
        }
        // A refused command captions the row that triggered it.
        group.rows.forEach { row ->
            val failure = commandStates[accountCommandKey(row.deviceId, row.agent, row.profileId)]
                as? DeviceCommandUiState.Failed ?: return@forEach
            Text(
                failure.message,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
            )
        }
        if (usage != null && usage.windows.isNotEmpty()) {
            Column(
                modifier = Modifier.alpha(if (fresh) 1f else 0.5f),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                AgentUsageCards(usage = usage, compact = true)
                // The cards caption their own staleness (the device's flag);
                // an aged-out but never-failed report gets the line here.
                if (!fresh && !usage.stale && asOf != null) {
                    Text(
                        "as of $asOf",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
        } else {
            Text(
                if (asOf != null) "No usage reported · as of $asOf" else "No usage reported",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

/**
 * EXP-829/EXP-849/EXP-862: one machine chip on an ACCOUNT row — the online
 * dot, the machine (· profile), a CHECK when the account is that machine's
 * ACTIVE login, an amber warning glyph when THAT machine's copy is broken, and
 * the same menu the machine row's chip carries (Sign in / Set as default /
 * Remove account). The rule that decides the entries is shared ×4, so a chip
 * cannot offer different repairs depending on which list it is in.
 */
@Composable
private fun DeviceChip(
    row: AgentProfileUsageRow,
    device: SteerDevice?,
    state: DeviceCommandUiState?,
    onSetDefault: () -> Unit,
    onRemove: () -> Unit,
    onSignIn: () -> Unit,
) {
    val busy = state is DeviceCommandUiState.Sending || state is DeviceCommandUiState.Running
    var menuOpen by remember { mutableStateOf(false) }
    // A command only runs on one of MY machines that is listening and new
    // enough to advertise the cap — otherwise the chip is a statement.
    val actions = if (device != null && row.mine && row.online && device.canAgentLogin) {
        AgentAccountsRows.chipActions(
            row,
            canSwitchAccount = device.canSwitchAccount,
            canRemoveAccount = device.canRemoveAccount,
        )
    } else {
        emptyList()
    }
    val badge = AgentHealthRules.badgeLabel(row.health)
    val description = buildString {
        append(AgentAccountsRows.chipLabel(row))
        append(if (row.online) ", online" else ", offline")
        if (row.signedIn && row.active) append(", active here")
        badge?.let { append(", ${it.lowercase()}") }
    }
    val pill: @Composable () -> Unit = {
        GlassPill(
            AgentAccountsRows.chipLabel(row),
            size = PillSize.Sm,
            onClick = if (actions.isEmpty()) null else ({ menuOpen = true }),
            dot = if (row.online) {
                ReviewGreen
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
            },
            trailing = when {
                busy -> null
                badge != null -> {
                    {
                        Icon(
                            ExpIcons.uiWarning,
                            contentDescription = badge,
                            tint = NeedsInputAmber,
                            modifier = Modifier.size(12.dp),
                        )
                    }
                }
                row.signedIn && row.active -> {
                    {
                        Icon(
                            ExpIcons.uiCheck,
                            contentDescription = "Active on this machine",
                            tint = ReviewGreen,
                            modifier = Modifier.size(12.dp),
                        )
                    }
                }
                else -> null
            },
            loading = busy,
            contentDescription = description,
            modifier = Modifier.testTag("agent-account-chip"),
        )
    }
    if (actions.isEmpty()) {
        pill()
        return
    }
    Box {
        pill()
        GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            AccountChipMenuItems(
                actions = actions,
                busy = busy,
                onSignIn = {
                    menuOpen = false
                    onSignIn()
                },
                onSetDefault = {
                    menuOpen = false
                    onSetDefault()
                },
                onRemove = {
                    menuOpen = false
                    onRemove()
                },
            )
        }
    }
}

/**
 * EXP-862: the bare `+` chip at the end of an account's machines — "Sign in on
 * <device>" for every machine of the caller's that could hold this account and
 * does not yet. Same chip ×4.
 */
@Composable
private fun AddAccountHereChip(devices: List<SteerDevice>, onPick: (SteerDevice) -> Unit) {
    var menuOpen by remember { mutableStateOf(false) }
    Box {
        GlassPill(
            "",
            size = PillSize.Sm,
            icon = ExpIcons.uiAdd,
            onClick = { menuOpen = true },
            contentDescription = "Add this account to a device",
            modifier = Modifier.testTag("agent-account-add-chip"),
        )
        GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            devices.forEach { device ->
                GlassMenuItem(
                    text = { Text("Sign in on ${device.displayLabel}") },
                    leadingIcon = {
                        Icon(
                            if (device.isServer) ExpIcons.uiServer else ExpIcons.uiDevice,
                            contentDescription = null,
                        )
                    },
                    onClick = {
                        menuOpen = false
                        onPick(device)
                    },
                )
            }
        }
    }
}


@Composable
private fun HintRow(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.padding(vertical = 4.dp),
    )
}

@Composable
private fun AgentsEmptyState() {
    // Mirrors iOS AgentsView.emptyState: tertiary icon, secondary subheadline
    // title, tertiary caption body. EXP-825: the relay-off wording — the
    // sessions themselves list on the Agent page now.
    Box(Modifier.fillMaxSize().padding(horizontal = 40.dp), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(
                ExpIcons.navDevices,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(28.dp),
            )
            Text(
                "Remote start isn't available on this server",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Text(
                "Start runs from the desktop app. Live sessions show up on the Agent page.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                textAlign = TextAlign.Center,
            )
        }
    }
}
