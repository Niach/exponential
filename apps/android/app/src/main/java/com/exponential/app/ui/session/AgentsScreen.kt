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
import androidx.compose.runtime.mutableIntStateOf
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
import com.exponential.app.domain.AgentHealth
import com.exponential.app.domain.AgentHealthRules
import com.exponential.app.domain.AgentProfileUsageRow
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.LaunchDeviceRules
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
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
 * The Devices tab (EXP-686, the renamed Agents surface): "My machines" — the
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
 * while the page is open. EXP-849: a chip of one of the caller's own machines
 * carries the ONE repair that machine owes the login — make it the machine's
 * active one (`agent_profile_use`) or sign it in — plus the way into the
 * machine's own settings sheet.
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
    val refreshingAccounts by viewModel.refreshingAccounts.collectAsStateWithLifecycle()
    val accountsError by viewModel.accountsError.collectAsStateWithLifecycle()
    // EXP-849: the remote sign-ins the Accounts section itself issued, keyed by
    // account row — the published login link and its code field caption inline,
    // instead of sending the user off to the device sheet.
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
    // EXP-849: the Accounts section's agent TAB (claude | codex) — one agent's
    // rows at a time, so the second agent's accounts never crowd the first's
    // (web/desktop parity). Null = the first reported agent.
    var accountAgentTab by rememberSaveable { mutableStateOf<String?>(null) }

    val steerOn = state.steerEnabled == true
    // EXP-827: the device sheet's round Usage button lands on the Accounts
    // section — the ONE surface the usage bars live on (iOS scrolls to the same
    // anchor, web hashes to `#accounts`). The sheet closes itself, then the
    // counter below moves this list.
    val listState = rememberLazyListState()
    var usageRequest by remember { mutableIntStateOf(0) }
    // EXP-432: the team-scoped list appends teammates' shared servers — they
    // belong under their own header, never in the caller's "My machines" count.
    // Hoisted out of the LazyColumn: the Accounts scroll target is counted off
    // exactly these two groups.
    val ownDevices = devices?.filter { it.isMine }
    val teamDevices = devices?.filterNot { it.isMine }.orEmpty()
    val accountsIndex = accountsHeaderIndex(ownDevices?.size, teamDevices.size)
    LaunchedEffect(usageRequest) {
        if (usageRequest > 0) listState.animateScrollToItem(accountsIndex)
    }

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
                    item(key = "__machines_header__") { SectionHeader("My machines") }
                    when {
                        // null = still loading; render nothing under the header.
                        ownDevices == null -> Unit
                        ownDevices.isEmpty() -> item(key = "__no_machine__") {
                            HintRow(
                                "No machines yet. Open the Exponential desktop app, or add a " +
                                    "device on the web.",
                            )
                        }
                        else -> items(ownDevices, key = { "dev_${it.deviceId}" }) { device ->
                            MachineRow(
                                device = device,
                                latestVersions = latestVersions,
                                busy = device.deviceId in deviceBusy,
                                onStart = { onOpenAgent(AgentComposerSeed(deviceId = device.deviceId)) },
                                onEdit = { settingsTargetId = device.deviceId },
                                onRemove = { removeTarget = device },
                                onUpdate = { viewModel.requestDeviceUpdate(device.deviceId) },
                            )
                        }
                    }
                    // Teammates' machines: startable, but with no rename /
                    // remove / update menu — they are not this user's to
                    // curate (sharing itself is managed on the web).
                    if (teamDevices.isNotEmpty()) {
                        item(key = "__team_machines_header__") { SectionHeader("Team machines") }
                        items(teamDevices, key = { "shared_${it.deviceId}" }) { device ->
                            MachineRow(
                                device = device,
                                latestVersions = latestVersions,
                                busy = false,
                                onStart = { onOpenAgent(AgentComposerSeed(deviceId = device.deviceId)) },
                                onEdit = {},
                                onRemove = {},
                                onUpdate = {},
                            )
                        }
                    }
                    // EXP-829: the Accounts section. The header carries the
                    // tag the screenshot flow scrolls to; the trailing note
                    // only shows when some machine can take a refresh.
                    item(key = "__accounts_gap__") { Spacer(Modifier.height(10.dp)) }
                    item(key = "__accounts_header__") {
                        val autoRefreshes = accountSections.orEmpty()
                            .any { section -> section.groups.any { it.refreshTarget != null } }
                        SectionHeader(
                            "Accounts",
                            modifier = Modifier.testTag("agent-accounts-section"),
                            trailing = if (autoRefreshes) {
                                {
                                    Text(
                                        "Refreshes every 5 minutes",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.onSurface.copy(
                                            alpha = TextEmphasis.Tertiary,
                                        ),
                                    )
                                }
                            } else {
                                null
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
                                    devicesById = devices.orEmpty().associateBy { it.deviceId },
                                    refreshing = group.key in refreshingAccounts,
                                    commandStates = accountCommandStates,
                                    onRefresh = { viewModel.refreshAccount(group) },
                                    onLogin = viewModel::accountLogin,
                                    onUseHere = viewModel::useAccountHere,
                                    onEnterCode = { row, code ->
                                        viewModel.accountLoginCode(row, code)
                                    },
                                    // Machine setup still lives in the
                                    // device-settings sheet — the chip just
                                    // isn't limited to opening it any more.
                                    onOpenDevice = { settingsTargetId = it },
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
                // EXP-827: the sheet dismisses itself, then this page scrolls
                // to Accounts.
                onOpenUsage = { usageRequest += 1 },
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
}

/**
 * EXP-827: which LazyColumn index the "Accounts" header sits at, counted off
 * the two machine groups above it — the device sheet's Usage button scrolls
 * here. [ownCount] null = the machines are still loading (no rows under the
 * header yet), 0 = the one hint row.
 *
 * It mirrors the list built above it; moving an item there means moving this.
 */
internal fun accountsHeaderIndex(ownCount: Int?, teamCount: Int): Int {
    var index = 1 // "My machines"
    index += when {
        ownCount == null -> 0
        ownCount == 0 -> 1 // the "No machines yet" hint row
        else -> ownCount
    }
    if (teamCount > 0) index += 1 + teamCount // "Team machines" + its rows
    return index + 1 // the spacer above the header
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
    onStart: () -> Unit,
    onEdit: () -> Unit,
    onRemove: () -> Unit,
    onUpdate: () -> Unit,
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
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(enabled = startable, onClick = onStart)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
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
                // from "Signed out" (a login nobody ever made); the
                // signed-out half is suppressed when the status line below
                // already names the signed-out agents, so the row says it
                // once.
                val worstHealth = AgentHealthRules.deviceWorst(device.agentAccounts)
                val healthBadge = worstHealth
                    ?.let(AgentHealthRules::badgeLabel)
                    ?.takeIf { worstHealth == AgentHealth.NeedsRelogin || unauthed.isEmpty() }
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
                val signedOutCaption = "${unauthed.joinToString(", ")} not signed in"
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
                // Runnable, but something installed is signed out: a footnote
                // next to Online, never the headline.
                if (online && blockedCaption == null && !device.updateRequested && unauthed.isNotEmpty()) {
                    Text(
                        "· $signedOutCaption",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
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
                    // EXP-481: Rename and the share toggle moved INTO the
                    // device-settings sheet — the menu carries one Edit entry.
                    GlassMenuItem(
                        text = { Text("Edit") },
                        leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
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
}

/**
 * EXP-829: one account ROW under its agent band — the identity line (amber
 * "Not signed in", else the email · plan), the refresh glyph when one of the
 * caller's machines can re-read the numbers (greyed inside the device's own
 * five-minute floor, a spinner while a refresh is in flight), the machine
 * chips, and the freshest machine's usage windows (the same cards the device
 * settings sheet renders; dimmed with an "as of …" line once they are older
 * than the freshness window). Web `AccountCard`, desktop `render_row`.
 */
@Composable
private fun AccountRow(
    group: AgentAccountUsageGroup,
    devicesById: Map<String, SteerDevice>,
    refreshing: Boolean,
    commandStates: Map<String, DeviceCommandUiState>,
    onRefresh: () -> Unit,
    onLogin: (AgentProfileUsageRow) -> Unit,
    onUseHere: (AgentProfileUsageRow) -> Unit,
    onEnterCode: (AgentProfileUsageRow, String) -> Unit,
    onOpenDevice: (String) -> Unit,
) {
    val nowMs = rememberUsageClock()
    val usage = group.usage
    val fresh = AgentUsagePresentation.isFresh(usage?.fetchedAt, nowMs)
    val nextAllowed = AgentAccountsRows.refreshAllowedAt(usage, nowMs)
    // The "as of …" fallback: the numbers' own stamp, else when a machine last
    // probed the account.
    val asOf = (usage?.fetchedAt?.takeIf { it.isNotBlank() } ?: group.checkedAt)
        ?.let(::relativeTime)?.takeIf { it.isNotEmpty() }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
            .testTag("agent-account-row"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.weight(1f),
            ) {
                Text(
                    AgentAccountsRows.caption(group),
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (group.signedIn) MaterialTheme.colorScheme.onSurface else NeedsInputAmber,
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
                // EXP-849: the expired-credential badge. A signed-OUT account
                // already says so in the caption above, so the badge carries
                // the one thing the caption cannot express.
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
            if (group.refreshTarget != null) {
                if (refreshing) {
                    CircularProgressIndicator(
                        modifier = Modifier.padding(start = 8.dp).size(16.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                } else {
                    CircleIconButton(
                        ExpIcons.uiRefresh,
                        contentDescription = "Refresh usage",
                        onClick = onRefresh,
                        enabled = nextAllowed == null,
                        modifier = Modifier.padding(start = 8.dp),
                    )
                }
            }
        }
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            group.rows.forEach { row ->
                // Keyed: the rows re-sort as health and usage move, and a chip's
                // open menu must not carry over to whoever takes its slot.
                key(row.key) {
                    DeviceChip(
                        row = row,
                        device = devicesById[row.deviceId],
                        busy = listOf(
                            commandStates[row.key],
                            commandStates[accountProfileUseKey(row)],
                        ).any {
                            it is DeviceCommandUiState.Sending ||
                                it is DeviceCommandUiState.Running
                        },
                        onLogin = { onLogin(row) },
                        onUseHere = { onUseHere(row) },
                        onOpenDevice = { onOpenDevice(row.deviceId) },
                    )
                }
            }
        }
        // EXP-849: whatever sign-in this row started, captioned HERE — the link
        // the machine published, its code field, the refusal. The chips are the
        // trigger, so the answer belongs beside them.
        group.rows.forEach { row ->
            val state = commandStates[row.key]
            val codeState = commandStates[accountLoginCodeKey(row)]
            // "Use this account here" keeps its own slot: its result is plain
            // text, never a login publication, so it captions as a command.
            val useHereState = commandStates[accountProfileUseKey(row)]
            if (state == null && codeState == null && useHereState == null) return@forEach
            // Keyed like the chips: a half-typed login code belongs to the row
            // it was typed under, whatever the sort does next.
            key(row.key) {
                Column {
                    Text(
                        AgentAccountsRows.chipLabel(row),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Tertiary,
                        ),
                    )
                    LoginResultCaption(
                        agent = row.agent,
                        state = state,
                        codeState = codeState,
                        // The code goes back to the machine that is waiting for
                        // it; only our OWN machines take one.
                        canEnterCode = row.mine && row.online,
                        onEnterCode = { code -> onEnterCode(row, code) },
                    )
                    if (codeState != null) CommandCaption(codeState)
                    if (useHereState is DeviceCommandUiState.Sending ||
                        useHereState is DeviceCommandUiState.Running
                    ) {
                        Text(
                            "Pointing the machine at this account…",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = TextEmphasis.Secondary,
                            ),
                        )
                    } else {
                        CommandCaption(useHereState)
                    }
                }
            }
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
 * EXP-829/EXP-849: one machine chip — the online dot, the machine (· profile),
 * and a CHECK when the account is the ACTIVE login on that machine. A chip of
 * one of the caller's own machines opens a MENU, not just the device sheet
 * (web `MachineAccountChip`): the ONE repair that machine needs (Use this
 * account here / Sign in / Re-login / Sign in again), plus Device settings. A
 * teammate's machine is read-only.
 */
@Composable
private fun DeviceChip(
    row: AgentProfileUsageRow,
    device: SteerDevice?,
    busy: Boolean,
    onLogin: () -> Unit,
    onUseHere: () -> Unit,
    onOpenDevice: () -> Unit,
) {
    val description = buildString {
        append(AgentAccountsRows.chipLabel(row))
        append(if (row.online) ", online" else ", offline")
        if (!row.signedIn) append(", not signed in") else if (row.active) append(", active here")
        AgentHealthRules.badgeLabel(row.health)?.let { append(", ${it.lowercase()}") }
    }
    // The machine runs the sign-in itself (`agent_login`), so the entry only
    // exists for a machine that is ours, online and new enough to advertise
    // the cap — the same gate the device sheet applies.
    val canLogin = row.mine && row.online && device?.canAgentLogin == true
    var menuOpen by remember { mutableStateOf(false) }
    val chip: @Composable () -> Unit = {
        GlassPill(
            AgentAccountsRows.chipLabel(row),
            size = PillSize.Sm,
            onClick = if (row.mine) {
                { menuOpen = true }
            } else {
                null
            },
            dot = if (row.online) {
                ReviewGreen
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
            },
            trailing = when {
                busy -> null
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
                AgentHealthRules.badgeLabel(row.health) != null -> {
                    {
                        Icon(
                            ExpIcons.uiWarning,
                            contentDescription = AgentHealthRules.badgeLabel(row.health),
                            tint = NeedsInputAmber,
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
    if (!row.mine) {
        chip()
        return
    }
    Box {
        chip()
        GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            if (canLogin) {
                // ONE action per state, web `MachineAccountChip` parity: a
                // healthy login this machine is not using simply BECOMES its
                // login (`agent_profile_use` — no credential touched, nothing
                // signed out), with the sign-in under it for a login that
                // turns out to be dead after all. Everything else is a
                // profile-scoped `agent_login`, which lands in that profile's
                // own config dir and so never signs the current one out (the
                // destructive logout form stays on the device sheet's ambient
                // button, where a codex token revoke is confirmed).
                val usesHere = row.signedIn && !row.active &&
                    row.health != AgentHealth.NeedsRelogin
                if (usesHere) {
                    GlassMenuItem(
                        text = { Text("Use this account here") },
                        leadingIcon = { Icon(ExpIcons.uiSwap, contentDescription = null) },
                        enabled = !busy,
                        onClick = {
                            menuOpen = false
                            onUseHere()
                        },
                    )
                }
                val signInLabel = when {
                    !row.signedIn -> "Sign in"
                    row.health == AgentHealth.NeedsRelogin -> "Re-login"
                    else -> "Sign in again"
                }
                GlassMenuItem(
                    text = { Text(signInLabel) },
                    leadingIcon = { Icon(ExpIcons.uiSignIn, contentDescription = null) },
                    enabled = !busy,
                    onClick = {
                        menuOpen = false
                        onLogin()
                    },
                )
            }
            GlassMenuItem(
                text = { Text("Device settings") },
                leadingIcon = { Icon(ExpIcons.navSettings, contentDescription = null) },
                onClick = {
                    menuOpen = false
                    onOpenDevice()
                },
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
