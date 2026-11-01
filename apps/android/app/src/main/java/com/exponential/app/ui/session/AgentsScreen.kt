package com.exponential.app.ui.session

import com.exponential.app.ui.components.ClaimedFailedActionToast
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.domain.EmptyStateCopy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
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
import com.exponential.app.domain.AgentProfileUsageRow
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.DevicesList
import com.exponential.app.domain.LaunchDeviceRules
import com.exponential.app.domain.Prompts
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.AddDeviceSheet
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.agentIconPainter
import com.exponential.app.ui.components.agentIconTint
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.compactRelativeTime
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard
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
 * Remove moved into the settings sheet the gear opens. FEED-149: a team
 * OWNER's teammate rows carry Remove in that slot instead (drop the share).
 *
 * EXP-909 folded the accounts INTO the machines. The cross-device "Accounts"
 * section — logins merged by email, per-agent tabs, a chip per machine with
 * its own menu, a "+" chip — is gone: it answered "what may this account
 * spend" while every repair it offered was per-MACHINE, so the one thing a
 * person came for (which box needs a sign-in) was the thing it hid. Each
 * device row now lists its OWN logins beneath it, one flat sub-row each:
 * the brand mark, the login's identity, its health badge, its numbers in the
 * mini form, and the ⋯ menu (Sign in / Sign out / Remove account). A teammate's shared machine renders its logins READ-ONLY —
 * seeing that a shared server's codex login expired explains a refused start,
 * but only its owner can fix it.
 *
 * A-16 (`devices-list.json`) flattened that again: one LINE per device, its
 * logins as chips, every problem behind ONE attention icon ([DevicesList.row]),
 * online rows first and the offline ones under a muted label.
 *
 * EXP-1560 made each section a GRID of device cards ([DeviceCard]): the
 * logins as brand-mark icons badged where they need a fix, the update line
 * and the settings behind the card's `⋯`; the attention icon and the gear
 * are gone.
 */
@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun AgentsScreen(
    // #EXP-1381: pushed from the Team page's Devices row (settings-nav.json)
    // the page wears a back + "Devices" top bar instead of the tab headline.
    onBack: (() -> Unit)? = null,
    // #EXP-1534: the `devices` row id whose settings sheet opens once the
    // list lands (an agent_signed_out inbox row / push); null = none.
    openDeviceRowId: String? = null,
    viewModel: AgentsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val latestVersions by viewModel.latestVersions.collectAsStateWithLifecycle()
    // EXP-909: the logins each machine holds, keyed by device id. null until
    // the device list has landed, which is what makes a row say "Checking…"
    // rather than flashing "No login reported".
    val deviceLogins by viewModel.deviceLogins.collectAsStateWithLifecycle()
    // EXP-849: the account commands the MACHINE rows issued (sign-out,
    // removal), keyed by machine × login: the row spins
    // while one is in flight and captions a refusal. Sign-ins are not here:
    // they round-trip a link and a code, which the login sheet owns.
    val accountCommandStates by viewModel.accountCommandStates.collectAsStateWithLifecycle()
    // A rename/remove/update on a machine is in flight: its Update pill waits.
    val deviceBusy by viewModel.deviceBusy.collectAsStateWithLifecycle()
    // FEED-79: a failed device write or login command is ONE error toast.
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    ClaimedFailedActionToast(failure, viewModel::claimFailure)
    // FEED-149: the selected team when the caller OWNS it — its Remove drops a
    // teammate's share from that team; null = no Remove on any row.
    val ownedTeamId by viewModel.ownedTeamId.collectAsStateWithLifecycle()
    // FEED-149: the teammate's shared row whose Remove waits on its confirm.
    var dropShareTarget by remember { mutableStateOf<SteerDevice?>(null) }

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
    // #EXP-1534: [openDeviceRowId]'s sheet opens ONCE (closing it stays closed),
    // when the caller's own row with that `devices` id has synced.
    var openedDeviceRow by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(openDeviceRowId, devices) {
        if (openDeviceRowId == null || openedDeviceRow) return@LaunchedEffect
        val target = devices?.firstOrNull { it.isMine && it.rowId == openDeviceRowId } ?: return@LaunchedEffect
        settingsTargetId = target.deviceId
        openedDeviceRow = true
    }
    // EXP-1169: the "My devices" header's Add device pill (web + desktop twin).
    var showAddDevice by remember { mutableStateOf(false) }
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
    // EXP-1137: the login whose "Sign out" is waiting on its confirm.
    var signOutTargetAccount by remember {
        mutableStateOf<Pair<SteerDevice, AgentProfileUsageRow>?>(null)
    }
    val steerOn = state.steerEnabled == true
    val listState = rememberLazyListState()
    // EXP-432: the team-scoped list appends teammates' shared servers — they
    // belong under their own header, never in the caller's "My devices" count.
    val ownDevices = devices?.filter { it.isMine }
    val teamDevices = devices?.filterNot { it.isMine }.orEmpty()

    Scaffold(
        containerColor = Color.Transparent,
    ) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            // #EXP-1384 (chats.json `shell.header`): the page header; pushed
            // from the Team page's Devices row, the back control first.
            com.exponential.app.ui.components.PageHeader(
                title = com.exponential.app.domain.Chats.Copy.HEADERS.getValue("devices"),
                onBack = onBack,
            )
            // No relay → nothing here can take a start: the full empty state.
            if (state.steerEnabled == false) {
                // No relay → nothing here can take a start.
                EmptyState(
                    title = "Remote start unavailable",
                    description = "Start sessions from the desktop app. Live sessions show up in Chats.",
                    icon = ExpIcons.navDevices,
                )
            } else if (steerOn) {
                // EXP-1560: the grid's columns come from the section's
                // content width (the page less its 16 + 16 inset).
                BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
                val columns = DevicesList.columns((maxWidth - 32.dp).value.toInt())
                LazyColumn(
                    state = listState,
                    modifier = Modifier.fillMaxSize().testTag("devices-list"),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    item(key = "__machines_header__") {
                        SectionHeader(DevicesList.Copy.MINE) {
                            GlassPill(
                                "Add device",
                                icon = ExpIcons.uiAdd,
                                onClick = { showAddDevice = true },
                            )
                        }
                    }
                    when {
                        // null = still loading: skeleton rows (EXP-1275).
                        ownDevices == null -> item(key = "__machines_loading__") {
                            SkeletonRows(rows = 2, rowHeight = 56.dp, dot = false)
                        }
                        ownDevices.isEmpty() -> item(key = "__no_machine__") {
                            EmptyState(EmptyStateCopy.devices)
                        }
                        else -> {
                            // A-16: online cards first in the list's own order,
                            // then the offline ones under a muted label, dimmed.
                            val (onlineOwn, offlineOwn) = DevicesList.grouped(ownDevices) { it.online }
                            val ownCard: @Composable (SteerDevice) -> Unit = { device ->
                                DeviceCard(
                                    device = device,
                                    latestVersions = latestVersions,
                                    commandStates = accountCommandStates,
                                    deviceBusy = device.deviceId in deviceBusy,
                                    onOpenSettings = { settingsTargetId = device.deviceId },
                                    logins = deviceLogins?.get(device.deviceId),
                                    onRemoveAccount = { row -> removeTargetAccount = device to row },
                                    onSignOutAccount = { row -> signOutTargetAccount = device to row },
                                    // EXP-862: the sign-in link, its code field and
                                    // the waiting state live in ONE sheet, opened
                                    // on the login that needs the repair.
                                    onSignIn = { agent, profileId ->
                                        loginTarget = AgentLoginTarget(
                                            device = device,
                                            agent = agent,
                                            profileId = profileId,
                                        )
                                    },
                                    // EXP-1533: the same sheet, queuing a long-lived login.
                                    onKeepSignedIn = { row ->
                                        loginTarget = AgentLoginTarget(
                                            device = device,
                                            agent = row.agent,
                                            profileId = row.profileId,
                                            longLived = true,
                                        )
                                    },
                                    onAddAccount = { addAccountDevice = device },
                                    onUpdate = { viewModel.requestDeviceUpdate(device.deviceId) },
                                )
                            }
                            if (onlineOwn.isNotEmpty()) {
                                item(key = "__machines_online_grid__") {
                                    DeviceGrid(onlineOwn, columns, key = { "dev_${it.deviceId}" }, card = ownCard)
                                }
                            }
                            if (offlineOwn.isNotEmpty()) {
                                item(key = "__machines_offline__") { OfflineLabel() }
                                item(key = "__machines_offline_grid__") {
                                    DeviceGrid(offlineOwn, columns, key = { "dev_${it.deviceId}" }, card = ownCard)
                                }
                            }
                        }
                    }
                    // Teammates' cards are read-only: the account icons only
                    // preview, no `+`, no actions. FEED-149: a team OWNER's ⋯
                    // holds Remove on a card shared into the SELECTED team (the
                    // list is cross-team, EXP-1186), which drops that share.
                    if (teamDevices.isNotEmpty()) {
                        item(key = "__team_machines_header__") { SectionHeader(DevicesList.Copy.TEAM) }
                        val (onlineTeam, offlineTeam) = DevicesList.grouped(teamDevices) { it.online }
                        val teamCard: @Composable (SteerDevice) -> Unit = { device ->
                            DeviceCard(
                                device = device,
                                latestVersions = latestVersions,
                                commandStates = accountCommandStates,
                                deviceBusy = false,
                                onOpenSettings = {},
                                logins = deviceLogins?.get(device.deviceId),
                                onRemoveAccount = {},
                                onSignOutAccount = {},
                                onSignIn = { _, _ -> },
                                onAddAccount = {},
                                onUpdate = {},
                                onRemoveShare = ownedTeamId
                                    ?.takeIf { device.rowId != null && it in device.sharedTeamIds }
                                    ?.let { { dropShareTarget = device } },
                            )
                        }
                        if (onlineTeam.isNotEmpty()) {
                            item(key = "__team_machines_online_grid__") {
                                DeviceGrid(onlineTeam, columns, key = { "shared_${it.deviceId}" }, card = teamCard)
                            }
                        }
                        if (offlineTeam.isNotEmpty()) {
                            item(key = "__team_machines_offline__") { OfflineLabel() }
                            item(key = "__team_machines_offline_grid__") {
                                DeviceGrid(offlineTeam, columns, key = { "shared_${it.deviceId}" }, card = teamCard)
                            }
                        }
                    }
                }
                }
            }
        }
    }

    if (showAddDevice) {
        AddDeviceSheet(onDismiss = { showAddDevice = false })
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

    // FEED-149: the owner's Remove — the prompt names the machine; Remove drops
    // its share from the selected team (the shape delta removes the row, a
    // failure is the remove toast).
    dropShareTarget?.let { target ->
        PromptAlert(
            prompt = Prompts.RemoveDeviceShare.prompt(target.deviceLabel.ifBlank { target.deviceId }),
            onDismiss = { dropShareTarget = null },
            handlers = mapOf(
                "remove" to {
                    ownedTeamId?.let { teamId -> viewModel.dropDeviceShare(target, teamId) }
                    dropShareTarget = null
                },
            ),
        )
    }

    // EXP-862: "Remove account" — the machine deletes ITS copy of the login.
    // The confirm names the login and the machine and says in the same breath
    // that the account itself survives (the pinned sentence ×4).
    removeTargetAccount?.let { (device, row) ->
        val text = AgentAccountsRows.removeAccountConfirm(AgentAccountsRows.loginLabel(row), device.displayLabel)
        AccountConfirmDialog(
            // The question form of the menu label, as web/iOS ask it.
            title = "${AgentAccountsRows.ACTION_REMOVE}?",
            text = text,
            confirmLabel = "Remove",
            onConfirm = {
                viewModel.removeAccountHere(device, row.agent, row.profileId)
                removeTargetAccount = null
            },
            onDismiss = { removeTargetAccount = null },
        )
    }

    // EXP-1137: "Sign out" — the machine signs the login out and keeps the row.
    signOutTargetAccount?.let { (device, row) ->
        AccountConfirmDialog(
            title = "${AgentAccountsRows.ACTION_SIGN_OUT}?",
            text = AgentAccountsRows.signOutConfirm(
                AgentAccountsRows.loginLabel(row),
                device.displayLabel,
            ),
            confirmLabel = "Sign out",
            onConfirm = {
                viewModel.signOutAccountHere(device, row.agent, row.profileId)
                signOutTargetAccount = null
            },
            onDismiss = { signOutTargetAccount = null },
        )
    }

    // EXP-862/EXP-909: "Add account" — pick the agent to sign in on THIS
    // machine, then hand off to the sign-in sheet, which is the ONE place a
    // login renders on this client. The machine is no longer a pick: the row
    // the control sits under already named it. No intended profile: the login
    // lands on whichever profile its email names (a new email adds one).
    addAccountDevice?.let { device ->
        AddAccountSheet(
            device = device,
            onPick = { agent ->
                addAccountDevice = null
                loginTarget = AgentLoginTarget(device = device, agent = agent)
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

/**
 * The pinned confirms ×4 — see [AgentAccountsRows.removeAccountConfirm] and
 * [AgentAccountsRows.signOutConfirm]: a destructive account entry never fires
 * straight off a menu row.
 */
@Composable
private fun AccountConfirmDialog(
    title: String,
    text: String,
    confirmLabel: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    GlassAlert(
        title = title,
        body = text,
        onDismiss = onDismiss,
        trailing = listOf(
            GlassAlertAction("Cancel", onClick = onDismiss),
            GlassAlertAction(confirmLabel, destructive = true, onClick = onConfirm),
        ),
        defaultAction = 0,
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

/** A-16: the muted "Offline" label the offline rows sit under. */
@Composable
private fun OfflineLabel() {
    Text(
        DevicesList.Copy.OFFLINE,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier
            .padding(start = GlassTokens.RowPaddingH, top = 6.dp)
            .testTag("devices-offline-label"),
    )
}

/**
 * EXP-1560 (`devices-list.json` `card`): one section's devices as a GRID of
 * [columns] equal cards, [DevicesList.Card.GAP] both ways; a short last row
 * keeps its cards at the column width.
 */
@Composable
private fun DeviceGrid(
    devices: List<SteerDevice>,
    columns: Int,
    key: (SteerDevice) -> String,
    card: @Composable (SteerDevice) -> Unit,
) {
    val gap = DevicesList.Card.GAP.dp
    Column(verticalArrangement = Arrangement.spacedBy(gap)) {
        devices.chunked(columns).forEach { row ->
            Row(
                modifier = Modifier.fillMaxWidth().height(IntrinsicSize.Min),
                horizontalArrangement = Arrangement.spacedBy(gap),
            ) {
                row.forEach { device ->
                    key(key(device)) {
                        Box(modifier = Modifier.weight(1f).fillMaxHeight()) { card(device) }
                    }
                }
                repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

/**
 * EXP-1560 (`devices-list.json`): THE device card — the glass card surface,
 * padding [DevicesList.Card.PADDING], two rows.
 *
 * TOP: the name (truncating) + `v{version}` (warning tone when the caller's
 * own device is outdated) + the default star, over the status line and the
 * `Shared` tag (a phone is never a registered device, so `This device` never
 * applies on this client); top-right the ghost `⋯` opening THE menu: the
 * update line (disabled, muted), `Update` (a server's update action), Device
 * settings (own registered). A teammate's card holds only a team owner's
 * Remove from this team; with nothing to hold, no `⋯`.
 *
 * BOTTOM: the logins as brand-mark icons ([DeviceLoginIcon]), the run-mark
 * amber badge on every login with a login attention item; past
 * [DevicesList.Card.MAX_ACCOUNT_ICONS] a ghost `⋯` reveals the rest in place;
 * own, online cards with an agent to add end with a ghost `+`.
 *
 * The device's health is never summarised from its agents: an agent with no
 * login profile (a CLI installed, never signed in) badges nothing.
 */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun DeviceCard(
    device: SteerDevice,
    latestVersions: DeviceLatestVersions,
    /** EXP-849: this machine's account commands in flight, keyed by login. */
    commandStates: Map<String, DeviceCommandUiState>,
    /** A rename/remove/update on this machine is in flight. */
    deviceBusy: Boolean,
    /** Device settings: opens this machine's settings sheet (own registered). */
    onOpenSettings: () -> Unit,
    /** The logins this machine holds — null while the list is still loading. */
    logins: List<AgentProfileUsageRow>?,
    onRemoveAccount: (AgentProfileUsageRow) -> Unit,
    onSignOutAccount: (AgentProfileUsageRow) -> Unit,
    /** Run the agent's own sign-in here (agent, profile) — the login sheet owns the flow. */
    onSignIn: (String, String?) -> Unit,
    /** EXP-1533: a claude login's "Keep signed in" / "Renew long-lived login". */
    onKeepSignedIn: (AgentProfileUsageRow) -> Unit = {},
    onAddAccount: () -> Unit,
    /** The ⋯ menu's Update: the existing update-now request. */
    onUpdate: () -> Unit,
    /** FEED-149: a team owner's Remove on a teammate's shared card; null = none. */
    onRemoveShare: (() -> Unit)? = null,
) {
    val online = device.online
    val mine = device.isMine
    val blockedCaption = LaunchDeviceRules.blockedCaption(device)
    val nowMs = rememberUsageClock()
    // A server runs the CLI, a desktop the IDE — each compares against its own
    // channel's advertised latest.
    val latest = if (device.isServer) latestVersions.cli else latestVersions.desktop
    val outdated = deviceUpdateAvailable(device.version, latest)
    val listRow = DevicesList.row(
        DevicesList.Input(
            online = online,
            mine = mine,
            server = device.isServer,
            registered = device.registered,
            outdated = outdated,
            latest = latest,
            updateQueued = device.updateQueued,
            accountsReported = device.agentAccounts != null,
            logins = logins.orEmpty().map { login ->
                DevicesList.DeviceListLogin(
                    agent = login.agent,
                    profileId = login.profileId,
                    label = AgentAccountsRows.loginLabel(login),
                    health = login.health,
                    limitHit = DevicesList.limitHit(login.usage, nowMs),
                )
            },
        ),
    )
    // A repair only runs on one of MY machines that is listening — an offline
    // machine would hold the commands until it wakes, which reads as a dead tap.
    val actionable = mine && online
    val canAdd = actionable && AgentAccountsRows.addableAgents(device).isNotEmpty()
    val updateItem = listRow.attention.firstOrNull {
        it.kind == DevicesList.Kind.Update || it.kind == DevicesList.Kind.UpdateQueued
    }
    val canSettings = mine && device.registered
    val hasMenu = if (mine) updateItem != null || canSettings else onRemoveShare != null
    var expanded by remember(device.deviceId) { mutableStateOf(false) }
    val gap = 4.dp
    Column(
        modifier = Modifier
            .fillMaxSize()
            .alpha(if (online) 1f else 0.6f)
            .glassCard()
            .padding(DevicesList.Card.PADDING.dp)
            .testTag("device-card"),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        device.displayLabel,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        // P30: the owner of a team device rides the
                        // accessibility label only, never an inline caption.
                        modifier = Modifier
                            .weight(1f, fill = false)
                            .then(
                                device.owner?.let { owner ->
                                    Modifier.semantics {
                                        contentDescription = "${device.displayLabel}, shared by ${owner.name}"
                                    }
                                } ?: Modifier,
                            ),
                    )
                    if (device.version != null) {
                        Spacer(Modifier.width(6.dp))
                        Text(
                            "v${device.version}",
                            style = MaterialTheme.typography.labelSmall,
                            color = if (outdated && mine) {
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
                }
                Spacer(Modifier.height(2.dp))
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    // A pending update outranks the presence caption: the daemon is
                    // about to restart, so "Online" would only read as a lie.
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
                            online -> "Online"
                            device.lastSeenAt != null -> "Last seen ${compactRelativeTime(device.lastSeenAt)}"
                            else -> DevicesList.Copy.OFFLINE
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    if (mine && device.sharedTeamIds.isNotEmpty()) {
                        Text(
                            "Shared",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                        )
                    }
                }
            }
            if (hasMenu) {
                DeviceActionsMenu(
                    updateItem = updateItem,
                    updateBusy = deviceBusy || device.updateRequested,
                    onUpdate = onUpdate,
                    onOpenSettings = onOpenSettings.takeIf { canSettings },
                    onRemoveShare = onRemoveShare.takeIf { !mine },
                )
            }
        }
        FlowRow(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(gap),
            verticalArrangement = Arrangement.spacedBy(gap),
        ) {
            when {
                // Never reported (or the list is still loading): say so rather
                // than claiming the machine holds nothing.
                logins == null || device.agentAccounts == null -> LoginHint(DevicesList.Copy.CHECKING)
                listRow.noAccount -> LoginHint(DevicesList.Copy.NO_ACCOUNT)
                else -> {
                    val icons = DevicesList.accountIcons(logins.size, expanded)
                    logins.take(icons.visible).forEach { login ->
                        key(login.key) {
                            DeviceLoginIcon(
                                row = login,
                                item = listRow.attention.firstOrNull {
                                    it.kind == DevicesList.Kind.Login &&
                                        it.agent == login.agent && it.profileId == login.profileId
                                },
                                actionable = actionable,
                                state = commandStates[deviceLoginCommandKey(login)],
                                onRemove = { onRemoveAccount(login) },
                                onSignOut = { onSignOutAccount(login) },
                                onSignIn = { onSignIn(login.agent, login.profileId) },
                                onKeepSignedIn = { onKeepSignedIn(login) },
                            )
                        }
                    }
                    if (icons.more) {
                        CircleIconButton(
                            ExpIcons.uiMore,
                            contentDescription = DevicesList.Copy.MORE_ACCOUNTS,
                            onClick = { expanded = true },
                            borderless = true,
                            size = DevicesList.Card.ICON_SIZE.dp,
                            glyphSize = DevicesList.Card.MARK_SIZE.dp,
                            modifier = Modifier.testTag("device-more-accounts"),
                        )
                    }
                }
            }
            // EXP-909: "Add account" is OFFERED or ABSENT, never disabled.
            if (canAdd) {
                CircleIconButton(
                    ExpIcons.uiAdd,
                    contentDescription = DevicesList.Copy.ADD_ACCOUNT,
                    onClick = onAddAccount,
                    borderless = true,
                    size = DevicesList.Card.ICON_SIZE.dp,
                    glyphSize = DevicesList.Card.MARK_SIZE.dp,
                    modifier = Modifier.testTag("add-account"),
                )
            }
        }
    }
}

/** The muted caption in the icon row: `Checking…` / `No account`. */
@Composable
private fun LoginHint(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(vertical = 4.dp),
    )
}

/**
 * EXP-1560: the card's ghost `⋯` (`copy.deviceActions`) and THE menu — the
 * update line as a disabled muted row, `Update` (only for the update action;
 * waits while one is requested), Device settings; on a teammate's card only
 * the owner's Remove from this team.
 */
@Composable
private fun DeviceActionsMenu(
    updateItem: DevicesList.AttentionItem?,
    updateBusy: Boolean,
    onUpdate: () -> Unit,
    onOpenSettings: (() -> Unit)?,
    onRemoveShare: (() -> Unit)?,
) {
    var open by remember { mutableStateOf(false) }
    Box {
        CircleIconButton(
            ExpIcons.uiMore,
            contentDescription = DevicesList.Copy.DEVICE_ACTIONS,
            onClick = { open = true },
            borderless = true,
            size = DevicesList.Card.ICON_SIZE.dp,
            glyphSize = DevicesList.Card.MARK_SIZE.dp,
            modifier = Modifier.testTag("device-actions"),
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            updateItem?.let { item ->
                GlassMenuItem(
                    text = {
                        Text(
                            item.text,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    },
                    enabled = false,
                    onClick = {},
                )
                if (item.action == DevicesList.Action.Update) {
                    GlassMenuItem(
                        text = { Text(DevicesList.Copy.UPDATE) },
                        leadingIcon = { Icon(ExpIcons.uiUpdate, contentDescription = null) },
                        enabled = !updateBusy,
                        onClick = {
                            open = false
                            onUpdate()
                        },
                        modifier = Modifier.testTag("device-update"),
                    )
                }
            }
            onOpenSettings?.let { open1 ->
                GlassMenuItem(
                    text = { Text(DevicesList.Copy.SETTINGS) },
                    leadingIcon = { Icon(ExpIcons.navSettings, contentDescription = null) },
                    onClick = {
                        open = false
                        open1()
                    },
                    modifier = Modifier.testTag("device-settings-button"),
                )
            }
            onRemoveShare?.let { remove ->
                GlassMenuItem(
                    text = { Text(DevicesList.Copy.REMOVE_FROM_TEAM) },
                    leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                    destructive = true,
                    onClick = {
                        open = false
                        remove()
                    },
                    modifier = Modifier.testTag("device-remove-share-button"),
                )
            }
        }
    }
}

/**
 * EXP-1560: ONE login of one machine as an icon — a ghost
 * [DevicesList.Card.ICON_SIZE] square holding the agent's brand mark, THE
 * run-mark amber badge when the login has a login attention [item]. A tap
 * opens the account menu headed by the email (else the label), the plan
 * muted, the item's status line in warning tone, then the usage windows; on
 * a teammate's (or an offline) machine the menu is that preview alone.
 */
@Composable
private fun DeviceLoginIcon(
    row: AgentProfileUsageRow,
    item: DevicesList.AttentionItem?,
    actionable: Boolean,
    state: DeviceCommandUiState?,
    onRemove: () -> Unit,
    onSignOut: () -> Unit,
    onSignIn: () -> Unit,
    onKeepSignedIn: () -> Unit,
) {
    val nowMs = rememberUsageClock()
    val label = AgentAccountsRows.loginLabel(row)
    val busy = state is DeviceCommandUiState.Sending || state is DeviceCommandUiState.Running
    var menuOpen by remember { mutableStateOf(false) }
    val actions = if (actionable) AgentAccountsRows.chipActions(row) else emptyList()
    // The plan line only when the header is the EMAIL: a login that has
    // nothing but its plan must not read "max" twice.
    val plan = row.plan?.takeIf { row.email != null }
    val status = item?.status?.let { DevicesList.Copy.STATUS[it] }
    val description = buildString {
        append("${agentLabel(row.agent)}, $label")
        plan?.let { append(", $it") }
        status?.let { append(", ${it.lowercase()}") }
    }
    Box {
        Box(
            modifier = Modifier
                .size(DevicesList.Card.ICON_SIZE.dp)
                .clip(CircleShape)
                .clickable(role = Role.Button) { menuOpen = true }
                .semantics { contentDescription = description }
                .testTag("device-login-icon"),
            contentAlignment = Alignment.Center,
        ) {
            if (busy) {
                CircularProgressIndicator(
                    modifier = Modifier.size(DevicesList.Card.MARK_SIZE.dp - 2.dp),
                    strokeWidth = 1.5.dp,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            } else {
                AgentRunMark(
                    agent = row.agent,
                    state = if (item != null) SessionTree.SessionState.NeedsYou else null,
                    size = DevicesList.Card.MARK_SIZE.dp,
                    badgeSize = 7.dp,
                    badgeRing = GlassTokens.OpaqueCardFill,
                )
            }
        }
        GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            val age = AgentUsagePresentation.usageAge(row.usage, nowMs)
            val asOf = age ?: row.checkedAt?.takeIf { it.isNotBlank() }
                ?.let(::relativeTime)?.takeIf { it.isNotEmpty() }?.let { "as of $it" }
            Column(
                modifier = Modifier
                    .width(260.dp)
                    .padding(horizontal = 14.dp, vertical = 6.dp)
                    .testTag("device-login-preview"),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                Text(
                    row.email ?: label,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                plan?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                    )
                }
                status?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodySmall,
                        color = NeedsInputAmber,
                        maxLines = 1,
                    )
                }
                Spacer(Modifier.height(8.dp))
                val usage = row.usage
                if (usage != null && usage.windows.isNotEmpty()) {
                    UsageWindows(usage = usage)
                } else if (row.signedIn && asOf == null) {
                    LoginHint(DevicesList.Copy.CHECKING)
                } else {
                    LoginHint(if (asOf != null) "No usage reported · $asOf" else "No usage reported")
                }
            }
            if (actions.isNotEmpty()) {
                AccountChipMenuItems(
                    actions = actions,
                    busy = busy,
                    onSignIn = {
                        menuOpen = false
                        onSignIn()
                    },
                    onKeepSignedIn = {
                        menuOpen = false
                        onKeepSignedIn()
                    },
                    onRemove = {
                        menuOpen = false
                        onRemove()
                    },
                    onSignOut = {
                        menuOpen = false
                        onSignOut()
                    },
                )
            }
        }
    }
}

/**
 * EXP-862: the account chip menu, identical on a machine row and on an account
 * row's machine chip — never `codex logout` (it would revoke the account
 * server-wide) and never a credential copy: the files stay where the CLI wrote
 * them. "Remove account" deletes only THIS machine's copy; EXP-1137's "Sign
 * out" is the agent's own way out (claude's `auth logout`, codex's credential
 * file deleted) and keeps the row.
 */
@Composable
private fun AccountChipMenuItems(
    actions: List<String>,
    busy: Boolean,
    onSignIn: () -> Unit,
    onKeepSignedIn: () -> Unit,
    onRemove: () -> Unit,
    onSignOut: () -> Unit,
) {
    actions.forEach { action ->
        when (action) {
            AgentAccountsRows.ACTION_SIGN_IN -> GlassMenuItem(
                text = { Text(action) },
                leadingIcon = { Icon(ExpIcons.uiSignIn, contentDescription = null) },
                enabled = !busy,
                onClick = onSignIn,
            )
            AgentAccountsRows.ACTION_KEEP_SIGNED_IN,
            AgentAccountsRows.ACTION_RENEW_LONG_LIVED,
            -> GlassMenuItem(
                text = { Text(action) },
                leadingIcon = { Icon(ExpIcons.uiClock, contentDescription = null) },
                enabled = !busy,
                onClick = onKeepSignedIn,
            )
            AgentAccountsRows.ACTION_SIGN_OUT -> GlassMenuItem(
                text = { Text(action) },
                leadingIcon = { Icon(ExpIcons.uiSignOut, contentDescription = null) },
                enabled = !busy,
                destructive = true,
                onClick = onSignOut,
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

