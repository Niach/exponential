package com.exponential.app.ui.session

import androidx.compose.foundation.clickable
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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.DeviceLatestVersions
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.deviceUpdateAvailable
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuDefaults
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPillButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.PulsingDot
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StartCodingSheet
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.SteerStartState
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSection

/**
 * The Agents tab: "My machines" — the caller's registered devices (EXP-403:
 * desktop IDEs and headless `exponential` servers, online and offline) doubling
 * as the remote-start launcher (EXP-156) — plus the caller's OWN coding
 * sessions currently running (EXP-312: live sessions are owner-only, so a
 * teammate's session never lists here). Tapping a running row jumps straight
 * into the live steer viewer when the relay is configured; otherwise it falls
 * back to the issue detail. The trailing info button always opens the issue
 * detail.
 */
@Composable
fun AgentsScreen(
    onOpenSteer: (codingSessionId: String) -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    onOpenActions: () -> Unit,
    viewModel: AgentsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val latestVersions by viewModel.latestVersions.collectAsStateWithLifecycle()
    val deviceBusy by viewModel.deviceBusy.collectAsStateWithLifecycle()
    val startState by viewModel.startState.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()
    val merging by viewModel.merging.collectAsStateWithLifecycle()
    val mergeErrors by viewModel.mergeErrors.collectAsStateWithLifecycle()

    // The device the launcher sheet was opened from (non-null = sheet open).
    var sheetDevice by remember { mutableStateOf<SteerDevice?>(null) }

    // The machine row whose settings sheet (EXP-481) / Remove dialog is open.
    var settingsTargetId by remember { mutableStateOf<String?>(null) }
    var removeTarget by remember { mutableStateOf<SteerDevice?>(null) }

    // The row whose "Merge and close" is awaiting confirmation (EXP-358).
    var mergeTarget by remember { mutableStateOf<AgentRow?>(null) }

    val steerOn = state.steerEnabled == true

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "Agents",
                    style = MaterialTheme.typography.headlineLarge,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f),
                )
                // Team actions (EXP-253) live behind the Agents surface — the
                // bottom bar is already at capacity (six tabs + compose on a
                // 360dp screen), so the entry rides the Agents header instead
                // of a seventh tab. NOT helpdesk-gated.
                GlassPillButton(
                    label = "Actions",
                    icon = ExpIcons.actionDefault,
                    onClick = onOpenActions,
                    modifier = Modifier.testTag("open-actions"),
                )
            }
            // No steer and nothing running → the full empty state (no machines
            // section to anchor a compact caption).
            if (!steerOn && state.rows.isEmpty()) {
                AgentsEmptyState()
            } else {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    if (steerOn) {
                        item(key = "__machines_header__") { SectionHeader("My machines") }
                        // EXP-432: the team-scoped list appends teammates'
                        // shared servers — they belong under their own header,
                        // never in the caller's "My machines" count.
                        val ownDevices = devices?.filter { it.isMine }
                        val teamDevices = devices?.filterNot { it.isMine }.orEmpty()
                        when {
                            // null = still loading; render nothing under the header.
                            ownDevices == null -> Unit
                            ownDevices.isEmpty() -> item(key = "__no_machine__") {
                                HintRow(
                                    "No machines yet. Open the Exponential desktop app, or add a " +
                                        "server on the web.",
                                )
                            }
                            else -> items(ownDevices, key = { "dev_${it.deviceId}" }) { device ->
                                MachineRow(
                                    device = device,
                                    latestVersions = latestVersions,
                                    busy = device.deviceId in deviceBusy,
                                    onStart = { sheetDevice = device },
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
                                    onStart = { sheetDevice = device },
                                    onEdit = {},
                                    onRemove = {},
                                    onUpdate = {},
                                )
                            }
                        }
                        val caption = startStateCaption(startState)
                        if (caption != null) {
                            item(key = "__start_state__") {
                                StartStateCaptionRow(caption = caption, showSpinner = startState is SteerStartState.Sending)
                            }
                        }
                        item(key = "__running_header__") { SectionHeader("Running") }
                    }

                    if (state.rows.isEmpty()) {
                        item(key = "__no_running__") {
                            // iOS noAgentsRow: caption/tertiary text in a glass row.
                            Text(
                                "No agents running right now.",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .glassRow()
                                    .padding(horizontal = 12.dp, vertical = 12.dp),
                            )
                        }
                    } else {
                        items(state.rows, key = { it.session.id }) { row ->
                            AgentSessionRow(
                                session = row.session,
                                issue = row.issue,
                                merging = row.issue?.id in merging,
                                errorMessage = row.issue?.id?.let(mergeErrors::get),
                                onClick = {
                                    // Every listed row is the caller's own
                                    // (EXP-312), so steer availability alone
                                    // decides the live viewer.
                                    if (state.steerEnabled == true) {
                                        onOpenSteer(row.session.id)
                                    } else {
                                        // Batch multi-issue sessions carry no issue.
                                        row.session.issueId?.let(onOpenIssue)
                                    }
                                },
                                onInfo = { row.session.issueId?.let(onOpenIssue) },
                                onMergeAndClose = { mergeTarget = row },
                            )
                        }
                    }
                }
            }
        }
    }

    val sheetDev = sheetDevice
    if (sheetDev != null) {
        StartCodingSheet(
            devices = devices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = emptySet(),
            preferredDeviceId = sheetDev.deviceId,
            onStart = viewModel::startCoding,
            onRunAction = viewModel::runAction,
            onDismiss = { sheetDevice = null },
        )
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
                    "Removes “${device.displayLabel}” from your machines. A machine with the " +
                        "daemon still running re-registers itself on its next heartbeat.",
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

    // EXP-358: merging alone parks the session on `merged`, so the destructive
    // half ("and close") is confirm-gated — same shape as the Reviews dialog.
    mergeTarget?.let { row ->
        val issueId = row.issue?.id
        AlertDialog(
            onDismissRequest = { mergeTarget = null },
            title = { Text("Merge and close?") },
            text = {
                Text("Merges the pull request, completes every linked issue, and closes the coding session.")
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        issueId?.let(viewModel::mergeAndClose)
                        mergeTarget = null
                    },
                ) { Text("Merge and close") }
            },
            dismissButton = {
                TextButton(onClick = { mergeTarget = null }) { Text("Cancel") }
            },
        )
    }
}

@Composable
private fun SectionHeader(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.padding(top = 8.dp, bottom = 2.dp),
    )
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
 * EXP-409: an online machine whose every installed agent is signed out can
 * take no start, so it reads like an offline row (dimmed glyph, no pill) with
 * an amber "<agents> not signed in" status instead of "Online"; a machine that
 * CAN run something but has signed-out agents left over just gets a quiet note.
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
    val signInNeeded = online && !device.hasRunnableAgent && unauthed.isNotEmpty()
    val startable = online && !signInNeeded
    // A server runs the CLI, a desktop the IDE — each compares against its own
    // channel's advertised latest.
    val outdated = deviceUpdateAvailable(
        device.version,
        if (device.isServer) latestVersions.cli else latestVersions.desktop,
    )
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
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
                    // A signed-out machine greys out: it looks present but can
                    // take nothing, so it must not read as fully available.
                    color = if (signInNeeded) {
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
                if (device.isMine && device.sharedTeamId != null) {
                    Spacer(Modifier.width(6.dp))
                    Text(
                        "Shared",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
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
                    StaticDot(if (signInNeeded) NeedsInputAmber else ReviewGreen, size = 6.dp)
                }
                val signedOutCaption = "${unauthed.joinToString(", ")} not signed in"
                Text(
                    when {
                        device.updateQueued -> "Update queued"
                        device.updateRequested -> "Updating…"
                        signInNeeded -> signedOutCaption
                        online -> "Online"
                        device.lastSeenAt != null -> "Last seen ${relativeTime(device.lastSeenAt)}"
                        else -> "Offline"
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = if (signInNeeded && !device.updateRequested) {
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
                if (online && !signInNeeded && !device.updateRequested && unauthed.isNotEmpty()) {
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
        // Text-only glass chip, matching the iOS AgentsView "Start coding"
        // button (EXP-331 — no play glyph). Offline machines can't take a
        // start (the relay refuses it), nor can ones with every agent signed
        // out (EXP-409), so the affordance is simply absent.
        if (startable) {
            GlassPillButton(
                label = "Start coding",
                onClick = onStart,
                modifier = Modifier.padding(start = 8.dp),
            )
        }
        // Rename / Update / Remove all mutate the OWNER's registry row, so a
        // teammate's shared machine carries no menu at all (EXP-432).
        if (device.registered && device.isMine) {
            var rowMenu by remember { mutableStateOf(false) }
            Box {
                IconButton(onClick = { rowMenu = true }) {
                    Icon(
                        ExpIcons.uiMoreVertical,
                        contentDescription = "Machine actions",
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
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
                    HorizontalDivider(color = GlassMenuDefaults.DividerColor)
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
private fun StartStateCaptionRow(caption: StartCaption, showSpinner: Boolean) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier.padding(vertical = 2.dp),
    ) {
        if (showSpinner) {
            CircularProgressIndicator(
                modifier = Modifier.size(12.dp),
                strokeWidth = 2.dp,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        Text(
            caption.text,
            style = MaterialTheme.typography.labelSmall,
            color = if (caption.isError) {
                MaterialTheme.colorScheme.error
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
            },
        )
    }
}

private data class StartCaption(val text: String, val isError: Boolean)

private fun startStateCaption(state: SteerStartState): StartCaption? = when (state) {
    is SteerStartState.Idle -> null
    is SteerStartState.Sending -> StartCaption("Sending start command…", false)
    is SteerStartState.Sent ->
        if (state.isBatch) {
            StartCaption("Batch start sent to ${state.deviceLabel}. It'll appear below when the desktop picks up.", false)
        } else {
            StartCaption("Start sent to ${state.deviceLabel}. Waiting for the desktop…", false)
        }
    is SteerStartState.Failed -> StartCaption(state.message, true)
}

@Composable
private fun AgentSessionRow(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    merging: Boolean,
    errorMessage: String?,
    onClick: () -> Unit,
    onInfo: () -> Unit,
    onMergeAndClose: () -> Unit,
) {
    val state = codingSessionDisplayState(session, issue?.prState)
    // EXP-358: "Merge and close" only shows while the linked PR is still open —
    // a batch row (no issue) or an already-merged PR has nothing to merge.
    val canMerge = issue?.prState == DomainContract.prStateOpen
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .testTag("agent-session-row")
                .glassRow()
                .clickable(onClick = onClick)
                .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // running → pulsing green; parked states → static dot: review green,
            // done/merged blue, needs-input amber (EXP-194/EXP-214/EXP-358).
            when (state) {
                CodingSessionDisplayState.Running -> PulsingDot()
                CodingSessionDisplayState.NeedsInput -> StaticDot(NeedsInputAmber)
                CodingSessionDisplayState.Review -> StaticDot(ReviewGreen)
                CodingSessionDisplayState.Done -> StaticDot(DoneBlue)
                CodingSessionDisplayState.Merged -> StaticDot(DoneBlue)
            }
            Spacer(Modifier.width(12.dp))
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        issue?.identifier ?: "…",
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        // An issueless run is an action run when the row carries
                        // its action_name snapshot (EXP-253), else a batch run —
                        // never "not synced". Keep the not-yet-synced case for an
                        // issue-scoped session whose issue hasn't landed.
                        when {
                            issue != null -> issue.title
                            session.issueId == null -> session.actionName ?: "Batch run"
                            else -> "Issue not synced yet"
                        },
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                val device = session.deviceLabel?.takeIf { it.isNotBlank() } ?: "Desktop"
                Text(
                    when (state) {
                        CodingSessionDisplayState.NeedsInput -> "Needs input · $device"
                        CodingSessionDisplayState.Review -> "Ready for review · $device"
                        CodingSessionDisplayState.Done -> "Done · $device"
                        CodingSessionDisplayState.Merged -> "Merged · $device"
                        CodingSessionDisplayState.Running ->
                            "$device · started ${relativeTime(session.startedAt)}"
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = when (state) {
                        CodingSessionDisplayState.NeedsInput -> NeedsInputAmber
                        CodingSessionDisplayState.Review -> ReviewGreen
                        CodingSessionDisplayState.Done -> DoneBlue
                        CodingSessionDisplayState.Merged -> DoneBlue
                        CodingSessionDisplayState.Running ->
                            MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                    },
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            // EXP-358: a plain merge leaves the session alive on `merged`, so
            // the list carries the explicit "and close" variant — confirm-gated,
            // and only while the PR is actually open.
            if (canMerge) {
                IconButton(onClick = onMergeAndClose, enabled = !merging) {
                    if (merging) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(18.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    } else {
                        Icon(
                            ExpIcons.prMerged,
                            contentDescription = "Merge and close",
                            modifier = Modifier.size(18.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
            }
            IconButton(onClick = onInfo) {
                Icon(
                    ExpIcons.uiInfo,
                    contentDescription = "Open issue",
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }

        // A refused merge (conflicts, branch protection, GitHub App errors)
        // captions THIS row (EXP-323 pattern) — inside the list, which already
        // clears the floating nav pill, so the reason is always readable.
        if (errorMessage != null) {
            Text(
                errorMessage,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 3.dp)
                    .glassSection()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            )
        }
    }
}

@Composable
private fun AgentsEmptyState() {
    // Mirrors iOS AgentsView.emptyState: tertiary icon, secondary subheadline
    // title, tertiary caption body.
    Box(Modifier.fillMaxSize().padding(horizontal = 40.dp), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(
                ExpIcons.navAgents,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(28.dp),
            )
            Text(
                "No agents running",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Text(
                "Start coding on an issue from the desktop IDE. Live sessions show up here.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                textAlign = TextAlign.Center,
            )
        }
    }
}
