package com.exponential.app.ui.session

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.DeviceLatestVersions
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.deviceUpdateAvailable
import com.exponential.app.domain.AgentAccountsRows
import com.exponential.app.domain.AgentHealthRules
import com.exponential.app.domain.AgentProfileUsageRow
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.LaunchDeviceRules
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.agentIconPainter
import com.exponential.app.ui.components.agentIconTint
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
 * EXP-818) — the Running / Past session lists moved to the Agent page.
 *
 * EXP-909 (follow-up) left the rows ONE control: the settings gear. A device
 * list is where machines are CURATED, not where runs are started — the Agent
 * page composer's device picker is the single launcher — so the play glyph,
 * the ⋯ row menu and the inline Update controls are gone, and Update and
 * Remove moved into the settings sheet the gear opens.
 *
 * EXP-909 folded the accounts INTO the machines. The cross-device "Accounts"
 * section — logins merged by email, per-agent tabs, a chip per machine with
 * its own menu, a "+" chip — is gone: it answered "what may this account
 * spend" while every repair it offered was per-MACHINE, so the one thing a
 * person came for (which box needs a sign-in) was the thing it hid. Each
 * device row now lists its OWN logins beneath it, one flat sub-row each:
 * the brand mark, the login's identity, its health badge, its numbers in the
 * mini form, and the unchanged ⋯ menu (Sign in / Set as default / Remove
 * account). A teammate's shared machine renders its logins READ-ONLY —
 * seeing that a shared server's codex login expired explains a refused start,
 * but only its owner can fix it.
 */
@Composable
fun AgentsScreen(
    viewModel: AgentsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val latestVersions by viewModel.latestVersions.collectAsStateWithLifecycle()
    // EXP-909: the logins each machine holds, keyed by device id. null until
    // the device list has landed, which is what makes a row say "Checking…"
    // rather than flashing "No login reported".
    val deviceLogins by viewModel.deviceLogins.collectAsStateWithLifecycle()
    // EXP-849: the account commands the MACHINE rows issued (`agent_profile_use`
    // — "use this account here"), keyed by machine × login: the row spins
    // while one is in flight and captions a refusal. Sign-ins are not here:
    // they round-trip a link and a code, which the login sheet owns.
    val accountCommandStates by viewModel.accountCommandStates.collectAsStateWithLifecycle()

    // EXP-817: the machines list's own refresh round — on every change of the
    // logins and on the same 30s clock the countdowns re-read on (web `useNow`).
    LaunchedEffect(deviceLogins) { viewModel.autoRefreshAccounts() }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000L)
            viewModel.autoRefreshAccounts()
        }
    }

    // The machine row whose settings sheet (EXP-481) is open — the sheet owns
    // Update and Remove now, confirm dialog included.
    var settingsTargetId by remember { mutableStateOf<String?>(null) }
    // EXP-862: the sign-in a chip (or the Add-account pill) asked for — the
    // machine runs its agent's OWN login flow and publishes the link back, so
    // the sheet is the ONE place that renders a login, wherever it started.
    var loginTarget by remember { mutableStateOf<AgentLoginTarget?>(null) }
    // EXP-909: "Add account" is DEVICE-BOUND now — the sheet is opened on the
    // machine the sign-in will run on, so it keeps its agent picker and drops
    // its device picker.
    var addAccountDevice by remember { mutableStateOf<SteerDevice?>(null) }
    // The login whose "Remove account" is waiting on its confirm.
    var removeTargetAccount by remember {
        mutableStateOf<Pair<SteerDevice, AgentProfileUsageRow>?>(null)
    }

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
                                commandStates = accountCommandStates,
                                onOpenSettings = { settingsTargetId = device.deviceId },
                                logins = deviceLogins?.get(device.deviceId),
                                onSetAccountDefault = { row -> viewModel.useAccountHere(device, row) },
                                onRemoveAccount = { row -> removeTargetAccount = device to row },
                                // EXP-862: the sign-in link, its code field and
                                // the waiting state live in ONE sheet, opened
                                // on the login that needs the repair — the
                                // device-settings sheet carries no accounts.
                                onSignInAccount = { row ->
                                    loginTarget = AgentLoginTarget(
                                        device = device,
                                        agent = row.agent,
                                        profileId = row.profileId,
                                    )
                                },
                                onAddAccount = { addAccountDevice = device },
                            )
                        }
                    }
                    // Teammates' machines carry no control at all — neither the
                    // registry row nor the share is this user's to curate
                    // (sharing itself is managed on the web).
                    if (teamDevices.isNotEmpty()) {
                        item(key = "__team_machines_header__") { SectionHeader("Team devices") }
                        items(teamDevices, key = { "shared_${it.deviceId}" }) { device ->
                            MachineRow(
                                device = device,
                                latestVersions = latestVersions,
                                commandStates = accountCommandStates,
                                onOpenSettings = {},
                                logins = deviceLogins?.get(device.deviceId),
                                // A teammate's machine renders its logins
                                // READ-ONLY: seeing that a shared server's
                                // codex login expired explains a refused
                                // start, but only its owner can fix it.
                                onSetAccountDefault = {},
                                onRemoveAccount = {},
                                onSignInAccount = {},
                                onAddAccount = {},
                            )
                        }
                    }
                }
            }
        }
    }

    // EXP-481: the device-settings sheet, re-resolving the LIVE row on every
    // sync delta so saved edits reflect without reopening. Owner-only — the
    // gear only exists on "mine" rows.
    settingsTargetId?.let { targetId ->
        // The row can vanish mid-edit (device removed from the sheet itself,
        // or elsewhere) — the sheet simply stops rendering; the stale id is
        // harmless and replaced on the next gear tap.
        devices?.firstOrNull { it.deviceId == targetId && it.isMine }?.let { target ->
            DeviceSettingsSheet(
                device = target,
                onDismiss = { settingsTargetId = null },
            )
        }
    }

    // EXP-862: "Remove account" — the machine deletes ITS copy of the login.
    // The confirm names the login and the machine and says in the same breath
    // that the account itself survives (the pinned sentence ×4).
    removeTargetAccount?.let { (device, row) ->
        RemoveAccountDialog(
            accountLabel = AgentAccountsRows.loginLabel(row),
            deviceLabel = device.displayLabel,
            onConfirm = {
                viewModel.removeAccountHere(device, row.agent, row.profileId)
                removeTargetAccount = null
            },
            onDismiss = { removeTargetAccount = null },
        )
    }

    // EXP-862/EXP-909: "Add account" — pick the agent to sign in on THIS
    // machine, then hand off to the sign-in sheet, which is the ONE place a
    // login renders on this client. The machine is no longer a pick: the row
    // the control sits under already named it.
    addAccountDevice?.let { device ->
        AddAccountSheet(
            device = device,
            onPick = { agent ->
                addAccountDevice = null
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
            onDismiss = { addAccountDevice = null },
        )
    }

    loginTarget?.let { target ->
        AgentLoginSheet(
            target = target,
            onDismiss = { loginTarget = null },
            liveDevice = devices.orEmpty().firstOrNull { it.deviceId == target.device.deviceId },
        )
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
 * EXP-862/EXP-909: "Add account" — the agent the sign-in runs on. DEVICE-BOUND
 * since EXP-909: the control that opens it lives under one machine's row, so
 * the sheet dropped its device picker and keeps only the agent one (web
 * `AddAccountDialog`). Only the agents INSTALLED on that machine are offered.
 */
@Composable
private fun AddAccountSheet(
    device: SteerDevice,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    // EXP-862: the pick is remembered as the user's CHOICE, never keyed on the
    // device row. Those re-emit on every heartbeat (~30s) with a fresh
    // `lastSeenAt`, so a `remember(device)` key would re-initialise mid-sheet
    // and silently snap the pick back to the first agent.
    var pickedAgent by remember { mutableStateOf("") }
    val agents = AgentAccountsRows.addableAgents(device)
    val agent = pickedAgent.takeIf { it in agents } ?: agents.firstOrNull() ?: ""
    GlassSheet(
        title = "Add account",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("add-account-sheet"),
    ) {
        Column(modifier = Modifier.fillMaxWidth()) {
            if (agents.isEmpty()) {
                Text(
                    "This device reports no agent that can sign in remotely. Open the " +
                        "desktop app or start the daemon there first.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
                )
            } else {
                OptionGroup {
                    PickerRow(
                        label = "Agent",
                        value = agent.takeIf { it.isNotEmpty() }?.let(::agentLabel) ?: "",
                        options = agents,
                        selected = agent,
                        optionLabel = ::agentLabel,
                        onSelect = { pickedAgent = it },
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
                        enabled = agent.isNotEmpty(),
                        onClick = { onPick(agent) },
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
 * version, a status line (green dot Online / "Last seen …" / Offline), and —
 * since the EXP-909 follow-up — exactly ONE trailing control, the settings
 * gear, on the caller's own REGISTERED rows. A row that predates the registry
 * (`registered == false`, live off relay presence) has nothing to configure,
 * so it carries no control; neither does a teammate's shared machine. Update
 * and Remove live inside the sheet the gear opens, and starting a run is the
 * Agent page composer's job alone.
 *
 * EXP-409/EXP-836: an online machine with NO runnable agent can take no start,
 * so it reads dimmed with an amber reason in place of "Online" — the signed-out
 * agents when it named any, else [LaunchDeviceRules.NO_RUNNABLE_AGENT]; a
 * machine that CAN run something but has signed-out agents left over just gets
 * a quiet note.
 *
 * EXP-432: a TEAMMATE's shared server (`owner != null`) renders read-only —
 * "shared by <owner>" in place of the version chip, since neither the registry
 * row nor the share is the caller's to change. Own rows shared with a team
 * carry a quiet "Shared" chip so the reason teammates can start there is
 * visible from the phone (the toggle stays web-only).
 */
@Composable
private fun MachineRow(
    device: SteerDevice,
    latestVersions: DeviceLatestVersions,
    /** EXP-849: this machine's account commands in flight, keyed by chip. */
    commandStates: Map<String, DeviceCommandUiState>,
    /** The gear: opens this machine's settings sheet (own registered rows). */
    onOpenSettings: () -> Unit,
    /** EXP-909: the logins this machine holds — null while the list is still
     *  loading, which is what makes the row say "Checking…". */
    logins: List<AgentProfileUsageRow>?,
    /** EXP-862 "Set as default": make this login the machine's ACTIVE one
     *  (`agent_profile_use`). */
    onSetAccountDefault: (AgentProfileUsageRow) -> Unit,
    /** EXP-862 "Remove account": the machine drops ITS copy of the login. */
    onRemoveAccount: (AgentProfileUsageRow) -> Unit,
    /** EXP-849: run the agent's own sign-in here — the login sheet owns the flow. */
    onSignInAccount: (AgentProfileUsageRow) -> Unit,
    /** EXP-909: sign a NEW login in on this machine (the device-bound sheet). */
    onAddAccount: () -> Unit,
) {
    val online = device.online
    // EXP-836: a machine that could take a start is an online one WITH a
    // runnable agent. No control hangs off it any more — it only decides how
    // present the row READS (full-emphasis glyph and label vs dimmed).
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
    // EXP-849: the row is a COLUMN — its machine line, then the account chips
    // (the repair surface). The row itself is inert: it used to take a start,
    // and a device list is not where runs begin.
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
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
                            contentDescription = "Default device",
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
            // The ONE control a device row carries (EXP-909 follow-up): the
            // settings gear, ghost-drawn the way the retired "⋯" was. Name,
            // sharing, launch defaults, worktrees, Update and Remove all live
            // behind it, and every one of them mutates the OWNER's registry
            // row — so a teammate's shared machine, and a row that predates the
            // registry, carry nothing at all (EXP-432). On a phone it is always
            // visible; there is no hover to reveal it.
            if (device.registered && device.isMine) {
                CircleIconButton(
                    ExpIcons.navSettings,
                    contentDescription = "Device settings",
                    onClick = onOpenSettings,
                    borderless = true,
                    modifier = Modifier
                        .padding(start = 8.dp)
                        .testTag("device-settings-button"),
                )
            }
        }
        DeviceLoginRows(
            device = device,
            logins = logins,
            // A repair only runs on one of MY machines that is listening and
            // new enough to advertise the cap — the server refuses the
            // commands without it, and an offline machine would hold them
            // until it wakes, which reads as a dead tap.
            actionable = device.isMine && online && device.canAgentLogin,
            commandStates = commandStates,
            onSetDefault = onSetAccountDefault,
            onRemove = onRemoveAccount,
            onSignIn = onSignInAccount,
            onAddAccount = onAddAccount,
        )
    }
}

/**
 * EXP-849/EXP-909: the logins ONE machine holds, listed UNDER its row (web
 * `DeviceLogins`, iOS `DeviceLogins`, desktop `machines::render_login_rows`).
 *
 * A machine row is where a broken or missing login gets fixed, so each row
 * names the agent by its brand mark, the login by its identity alone
 * ([AgentAccountsRows.loginLabel] — never a status), badges THIS machine's
 * health for it, shows its numbers in the mini form, and carries the repairs
 * that machine owes it (EXP-862): sign in, make it the machine's default
 * (`agent_profile_use` — no login flow, no logout, no credential touched), or
 * remove this machine's copy of it.
 */
@Composable
private fun DeviceLoginRows(
    device: SteerDevice,
    logins: List<AgentProfileUsageRow>?,
    actionable: Boolean,
    commandStates: Map<String, DeviceCommandUiState>,
    onSetDefault: (AgentProfileUsageRow) -> Unit,
    onRemove: (AgentProfileUsageRow) -> Unit,
    onSignIn: (AgentProfileUsageRow) -> Unit,
    onAddAccount: () -> Unit,
) {
    // EXP-909: the "Add account" control is OFFERED or ABSENT, never visible
    // and disabled — this deliberately reverses EXP-845, which showed a dead
    // control on every machine that could not take a sign-in and made the
    // page read as broken. Never on a teammate's machine.
    val canAdd = device.isMine &&
        device.online &&
        device.canAgentLogin &&
        AgentAccountsRows.addableAgents(device).isNotEmpty()
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        when {
            // Still loading the device list: say so rather than claiming the
            // machine holds nothing.
            logins == null -> LoginHint("Checking…")
            logins.isEmpty() -> LoginHint(AgentUsagePresentation.NO_LOGIN_REPORTED)
            else -> logins.forEach { login ->
                key(login.key) {
                    DeviceLoginRow(
                        row = login,
                        actionable = actionable,
                        canSwitch = device.canSwitchAccount,
                        canRemove = device.canRemoveAccount,
                        canAgentLogin = device.canAgentLogin,
                        state = commandStates[deviceLoginCommandKey(login)],
                        onSetDefault = { onSetDefault(login) },
                        onRemove = { onRemove(login) },
                        onSignIn = { onSignIn(login) },
                    )
                }
            }
        }
        if (canAdd) {
            GlassPill(
                "Add account",
                size = PillSize.Sm,
                icon = ExpIcons.uiAdd,
                onClick = onAddAccount,
                modifier = Modifier.testTag("add-account"),
            )
        }
    }
}

/** The per-device caption ladder: `Checking…` / `No login reported`. */
@Composable
private fun LoginHint(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
}

/** ONE login of one machine: the agent, the login, its state, its menu. */
@Composable
private fun DeviceLoginRow(
    row: AgentProfileUsageRow,
    actionable: Boolean,
    /** The machine advertises `account-switch` — see [AgentAccountsRows.chipActions]. */
    canSwitch: Boolean,
    /** …and `account-remove`, for the destructive entry. */
    canRemove: Boolean,
    /** …and `agent-login`, which a removal needs as well. */
    canAgentLogin: Boolean,
    state: DeviceCommandUiState?,
    onSetDefault: () -> Unit,
    onRemove: () -> Unit,
    onSignIn: () -> Unit,
) {
    val nowMs = rememberUsageClock()
    val label = AgentAccountsRows.loginLabel(row)
    val badge = AgentAccountsRows.healthBadge(row)
    val busy = state is DeviceCommandUiState.Sending || state is DeviceCommandUiState.Running
    var menuOpen by remember { mutableStateOf(false) }
    // EXP-862: the entries, decided in ONE place ×4 — a signed-out or expired
    // login offers a sign-in and nothing else; a healthy one can become the
    // machine's default and can be removed from it. Empty = the row is a
    // statement (a teammate's machine, an offline one, the ambient login).
    val actions = if (actionable) {
        AgentAccountsRows.chipActions(
            row,
            canSwitchAccount = canSwitch,
            canRemoveAccount = canRemove,
            canAgentLogin = canAgentLogin,
        )
    } else {
        emptyList()
    }
    // The `· plan` tail only when the label is already the EMAIL: a login that
    // has nothing but its plan must not read "max · max".
    val planTail = row.plan?.takeIf { row.email != null }
    val description = buildString {
        append("${agentLabel(row.agent)}, $label")
        planTail?.let { append(", $it") }
        if (row.active) append(", the account this device uses")
        badge?.let { append(", ${it.lowercase()}") }
    }
    Column(
        modifier = Modifier.fillMaxWidth().testTag("device-login-row"),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .fillMaxWidth()
                .semantics(mergeDescendants = true) { contentDescription = description },
        ) {
            Icon(
                agentIconPainter(row.agent),
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = agentIconTint(row.agent),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                label,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            if (planTail != null) {
                Text(
                    " · $planTail",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                )
            }
            Spacer(Modifier.weight(1f))
            if (badge != null) {
                Text(
                    badge,
                    style = MaterialTheme.typography.labelSmall,
                    color = NeedsInputAmber,
                    maxLines = 1,
                    modifier = Modifier.testTag("device-login-health"),
                )
            }
            if (busy) {
                Spacer(Modifier.width(6.dp))
                CircularProgressIndicator(
                    modifier = Modifier.size(12.dp),
                    strokeWidth = 1.5.dp,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            } else if (actions.isNotEmpty()) {
                Box {
                    CircleIconButton(
                        ExpIcons.uiMore,
                        contentDescription = "Account menu",
                        onClick = { menuOpen = true },
                        // EXP-862: every "⋯" is a ghost rung — no circle, no
                        // hairline (×4).
                        borderless = true,
                        size = 28.dp,
                        glyphSize = 15.dp,
                        modifier = Modifier.padding(start = 4.dp),
                    )
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
        }
        // The numbers, or the caption ladder when there are none: a signed-in
        // login nothing has probed YET reads "Checking…" — "No usage reported"
        // made a device that is simply still working read as broken (×4).
        val age = AgentUsagePresentation.usageAge(row.usage, nowMs)
        val asOf = age ?: row.checkedAt?.takeIf { it.isNotBlank() }
            ?.let(::relativeTime)?.takeIf { it.isNotEmpty() }?.let { "as of $it" }
        Column(
            modifier = Modifier.padding(start = 22.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            if (row.usage?.windows?.isNotEmpty() == true) {
                Column(modifier = Modifier.alpha(if (age != null) 0.5f else 1f)) {
                    AgentUsageMini(usage = row.usage)
                }
                if (age != null) LoginHint(age)
            } else if (row.signedIn && asOf == null) {
                LoginHint("Checking…")
            } else {
                LoginHint(if (asOf != null) "No usage reported · $asOf" else "No usage reported")
            }
        }
        // The material outcome of a pick arrives by SYNC (the machine
        // re-reports its accounts, which moves the check), but a refusal would
        // otherwise be silent — including the honest one a machine too old to
        // know the command answers with.
        (state as? DeviceCommandUiState.Failed)?.let { failure ->
            Text(
                failure.message,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
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
