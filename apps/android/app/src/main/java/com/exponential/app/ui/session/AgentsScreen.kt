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
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.DeviceLatestVersions
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.deviceUpdateAvailable
import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.MergeTarget
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.canOfferFixConflicts
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.ui.actions.ActionEditSheet
import com.exponential.app.ui.actions.ActionsViewModel
import com.exponential.app.ui.actions.AutomationFormSheet
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.PulsingDot
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StartCodingSheet
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.SteerStartState
import com.exponential.app.ui.issue.SubjectTab
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard
import com.exponential.app.ui.theme.glassRow

/**
 * The Devices tab (EXP-686, the renamed Agents surface): "My machines" — the
 * caller's registered devices (EXP-403:
 * desktop IDEs and headless `exponential` servers, online and offline) doubling
 * as the remote-start launcher (EXP-156) — plus the caller's OWN coding
 * sessions currently running (EXP-312: live sessions are owner-only, so a
 * teammate's session never lists here). Tapping a running row jumps straight
 * into the live steer viewer when the relay is configured; otherwise it falls
 * back to the issue detail. EXP-694: the trailing control names what the run is
 * about — an issue's identifier pill opening the issue, or the action /
 * automation glyph opening its editor.
 */
@Composable
fun AgentsScreen(
    onOpenSteer: (codingSessionId: String) -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    // EXP-631: the bottom bar's Chat FAB bumps this counter (the bar lives in
    // AppNavHost, the launcher lives here) — every change opens the Start
    // coding sheet on its Chat tab.
    chatRequest: Int = 0,
    viewModel: AgentsViewModel = hiltViewModel(),
    // EXP-694: a session row's trailing control opens the run's ACTION or its
    // AUTOMATION, so the list needs the Actions surface's rows and its
    // owner-gated automation mutation. Reused rather than mirrored into
    // AgentsViewModel (both models read the same synced shapes).
    actionsViewModel: ActionsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val latestVersions by viewModel.latestVersions.collectAsStateWithLifecycle()
    val deviceBusy by viewModel.deviceBusy.collectAsStateWithLifecycle()
    val startState by viewModel.startState.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()
    val merging by viewModel.merging.collectAsStateWithLifecycle()
    val mergeErrors by viewModel.mergeErrors.collectAsStateWithLifecycle()
    // EXP-694 (S6): the rows behind a session's trailing action/automation
    // button, plus the automation form's own plumbing.
    val actionsState by actionsViewModel.state.collectAsStateWithLifecycle()
    val automations by actionsViewModel.automations.collectAsStateWithLifecycle()
    val automationDevices by actionsViewModel.automationDevices.collectAsStateWithLifecycle()
    val automationBusy by actionsViewModel.automationBusy.collectAsStateWithLifecycle()
    val automationError by actionsViewModel.automationError.collectAsStateWithLifecycle()
    val isTeamOwner by actionsViewModel.isTeamOwner.collectAsStateWithLifecycle()

    // The action / automation whose editor is open (non-null = sheet open).
    var editActionId by remember { mutableStateOf<String?>(null) }
    var editAutomation by remember { mutableStateOf<AutomationEntity?>(null) }

    // The device the launcher sheet was opened from (non-null = sheet open).
    var sheetDevice by remember { mutableStateOf<SteerDevice?>(null) }

    // The tab bar's Chat launcher (EXP-631) — the same sheet, opened on its
    // Chat tab with no machine preference. Skips the initial composition so a
    // return to the tab doesn't re-open it.
    var chatSheetOpen by remember { mutableStateOf(false) }
    var seenChatRequest by remember { mutableStateOf(chatRequest) }
    LaunchedEffect(chatRequest) {
        if (chatRequest != seenChatRequest) {
            seenChatRequest = chatRequest
            chatSheetOpen = true
        }
    }

    // The machine row whose settings sheet (EXP-481) / Remove dialog is open.
    var settingsTargetId by remember { mutableStateOf<String?>(null) }
    var removeTarget by remember { mutableStateOf<SteerDevice?>(null) }

    // The row whose merge is awaiting confirmation (EXP-498: merging always
    // closes the session). Named for the ROW, not the target — EXP-734 gave
    // the merge itself a MergeTarget (issue or session) the row carries.
    var mergeConfirmRow by remember { mutableStateOf<AgentRow?>(null) }

    // The issue whose "Fix conflicts" sheet is open (EXP-486, Reviews parity
    // EXP-323): a refused merge is usually a conflict, so the failing row's
    // caption offers the builtin recovery run.
    var fixTargetIssueId by remember { mutableStateOf<String?>(null) }

    val steerOn = state.steerEnabled == true

    // EXP-536: the desktop picked the start up — open the live session ONCE,
    // for a single-issue run, a batch and an action run alike.
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
    LaunchedEffect(startedSessionId) {
        startedSessionId?.let {
            viewModel.consumeStartedSession()
            onOpenSteer(it)
        }
    }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Text(
                "Devices",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            )
            // No steer and nothing running → the full empty state (no
            // machines section to anchor a compact caption).
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
                                        "device on the web.",
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
                                // EXP-536: the spinner spans the whole wait —
                                // Sent means "waiting for the desktop" now,
                                // not "it'll show up below eventually".
                                StartStateCaptionRow(
                                    caption = caption,
                                    showSpinner = startState is SteerStartState.Sending ||
                                        startState is SteerStartState.Sent,
                                )
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
                            // EXP-535: batch rows merge (and fix conflicts)
                            // through their resolved PR's representative issue
                            // — the server resolves a batch PR to ALL linked
                            // issues (Reviews pattern).
                            val mergeIssue = row.issue ?: row.batchPrIssue
                            // EXP-734: only an issue target can be handed to
                            // the "Fix merge conflicts" run (its input IS an
                            // issue-linked PR); a run's own PR has no issue.
                            val issueMergeTarget = row.mergeTarget as? MergeTarget.Issue
                            // EXP-694 (S6): the trailing control names what the
                            // run is about — an issue's identifier, or the
                            // action/automation's own glyph. A chat or batch
                            // run has neither, and gets no button at all.
                            val rowAction = row.session.actionId?.let { id ->
                                actionsState.actions.firstOrNull { it.id == id }
                            }
                            val rowAutomation = row.session.automationId?.let { id ->
                                automations.firstOrNull { it.id == id }
                            }
                            // An automated run edits the AUTOMATION (owner-only,
                            // like the Automations tab); everything else — an
                            // unresolved automation, or a member — edits the
                            // action, whose sheet is read-only for members
                            // anyway. Destination AND label come off this one
                            // resolution (iOS AgentsView.editTarget), so the
                            // button can never announce what it doesn't open.
                            val editsAutomation = rowAutomation != null && isTeamOwner
                            AgentSessionRow(
                                session = row.session,
                                issue = row.issue,
                                device = row.device,
                                // EXP-734: what Merge acts on — the issue, or
                                // (an action/chat run's own PR) the session.
                                mergeTarget = row.mergeTarget,
                                merging = row.mergeTarget?.key in merging,
                                failure = row.mergeTarget?.key?.let(mergeErrors::get),
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
                                // The pill shows only once the issue itself has
                                // synced — it IS the identifier.
                                issueIdentifier = row.issue?.identifier,
                                actionIcon = row.session.actionId?.let {
                                    actionGlyph(rowAction)
                                },
                                actionLabel = if (editsAutomation) {
                                    "Edit automation"
                                } else {
                                    "Edit action"
                                },
                                onOpenIssue = { row.session.issueId?.let(onOpenIssue) },
                                onOpenAction = {
                                    if (editsAutomation) {
                                        actionsViewModel.clearAutomationError()
                                        editAutomation = rowAutomation
                                    } else {
                                        row.session.actionId?.let { editActionId = it }
                                    }
                                },
                                onMerge = { mergeConfirmRow = row },
                                // A REAL conflict only (EXP-533); the recovery
                                // run rebases the PR's branch, so it needs one
                                // recorded — the same gate as the Reviews rows
                                // (EXP-323), plus a reachable machine to run on.
                                // EXP-734: only an ISSUE target can be
                                // rebased by the recovery run — it takes the
                                // PR's representative issue as its input.
                                canFixConflicts = issueMergeTarget != null &&
                                    canOfferFixConflicts(
                                        mergeErrors[issueMergeTarget.key],
                                        mergeIssue?.branch,
                                        steerEnabled = steerOn,
                                    ),
                                onFixConflicts = {
                                    fixTargetIssueId = issueMergeTarget?.issueId
                                },
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

    if (chatSheetOpen) {
        StartCodingSheet(
            devices = devices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = emptySet(),
            initialTab = SubjectTab.Chat,
            onStart = viewModel::startCoding,
            onRunAction = viewModel::runAction,
            onDismiss = { chatSheetOpen = false },
        )
    }

    // "Fix conflicts" (EXP-486, Reviews parity EXP-323): the unified sheet
    // opened on the builtin action with THIS row's pull request already picked.
    fixTargetIssueId?.let { issueId ->
        StartCodingSheet(
            devices = devices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = emptySet(),
            preselectedActionId = DomainContract.builtinFixConflictsId,
            preselectedPrIssueId = issueId,
            onStart = viewModel::startCoding,
            onRunAction = viewModel::runAction,
            onDismiss = { fixTargetIssueId = null },
        )
    }

    // EXP-694 (S6): the run's action, opened straight off its session row —
    // the full editor, read-only for members.
    editActionId?.let { id ->
        ActionEditSheet(actionId = id, onDismiss = { editActionId = null })
    }

    // An automated run opens the automation that fired it instead (the
    // Automations tab's own form, owner-gated).
    editAutomation?.let { automation ->
        AutomationFormSheet(
            actions = actionsState.actions,
            devices = automationDevices,
            busy = automationBusy,
            error = automationError,
            editing = automation,
            onSubmit = { actionId, deviceId, trigger, agent, model, effort ->
                actionsViewModel.updateAutomation(
                    automationId = automation.id,
                    actionId = actionId,
                    deviceId = deviceId,
                    trigger = trigger,
                    agent = agent,
                    model = model,
                    effort = effort,
                    onDone = { editAutomation = null },
                )
            },
            onDismiss = { editAutomation = null },
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

    // EXP-498: merging always closes the session too, so the merge is
    // confirm-gated — same shape as the Reviews dialog.
    mergeConfirmRow?.let { row ->
        // EXP-535: a batch row merges through its resolved representative
        // issue. EXP-734: an action or chat run merges its OWN PR, which
        // completes no issue at all — so the copy follows the target.
        val target = row.mergeTarget
        AlertDialog(
            onDismissRequest = { mergeConfirmRow = null },
            title = { Text("Merge pull request?") },
            text = {
                Text(
                    when (target) {
                        is MergeTarget.Session ->
                            "Merges this run's pull request and closes the coding session."
                        else ->
                            "Merges the pull request, completes every linked issue, " +
                                "and closes the coding session."
                    },
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        target?.let(viewModel::merge)
                        mergeConfirmRow = null
                    },
                ) { Text("Merge") }
            },
            dismissButton = {
                TextButton(onClick = { mergeConfirmRow = null }) { Text("Cancel") }
            },
        )
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
        // EXP-615: an icon-only play button (one Run/Start affordance across
        // the clients). Offline machines can't take a start (the relay refuses
        // it), nor can ones with every agent signed out (EXP-409), so the
        // affordance is simply absent.
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
    // EXP-536: single and batch read the same — the screen opens the session
    // itself the moment the desktop's row syncs in.
    is SteerStartState.Sent ->
        StartCaption("Start sent to ${state.deviceLabel}. Waiting for the desktop…", false)
    is SteerStartState.Failed -> StartCaption(state.message, true)
}

@Composable
private fun AgentSessionRow(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    // EXP-549/550: the host machine resolved against its live devices row —
    // the current label, and offline = the run is paused until it returns.
    device: SessionDevicePresentation,
    // EXP-734: what the merge shortcut acts on — the linked issue, a batch
    // row's client-resolved representative (EXP-535), or, for an action/chat
    // run that opened a PR of its own, the SESSION. Null = nothing to merge.
    // The label/status rendering keeps reading [issue] alone.
    mergeTarget: MergeTarget?,
    merging: Boolean,
    failure: MergeFailure?,
    onClick: () -> Unit,
    // EXP-694 (S6): the trailing control. An issue run wears a glass pill with
    // its IDENTIFIER, an action / automation run the circular button with that
    // action's glyph; a chat or batch run gets neither (it opens nothing the
    // row doesn't already open). This replaced the unconditional info glyph,
    // which was a plain no-op on every issueless run.
    issueIdentifier: String?,
    actionIcon: ImageVector?,
    actionLabel: String,
    onOpenIssue: () -> Unit,
    onOpenAction: () -> Unit,
    onMerge: () -> Unit,
    canFixConflicts: Boolean,
    onFixConflicts: () -> Unit,
) {
    // EXP-734: an issueless run carries its own PR state, so "in review with
    // a merged PR" reads as Done there too.
    val state = codingSessionDisplayState(session, issue?.prState ?: session.prState)
    // The merge shortcut only shows while there IS something open to merge —
    // the linked issue's PR, a batch row's resolved representative (EXP-535),
    // or the run's own issueless PR (EXP-734).
    val canMerge = mergeTarget != null
    // EXP-550: the machine went away (lid closed) — the run is not lost and
    // not ended, it continues when the machine comes back. Grey, never a live
    // dot.
    val paused = device.isPaused(state)
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
            Column(modifier = Modifier.weight(1f)) {
                // EXP-688: the identity line is shared with the steering
                // screen's header (SessionRowTitle) so the two can't drift.
                // running → pulsing green; parked states → static dot: review
                // green, done blue, needs-input amber (EXP-194/EXP-214);
                // paused-on-an-offline-machine → static grey (EXP-550).
                SessionRowTitle(
                    identifier = sessionRowIdentifier(issue),
                    title = sessionRowTitle(session, issue),
                    dot = {
                        when {
                            paused -> StaticDot(LostGray)
                            else -> when (state) {
                                CodingSessionDisplayState.Running -> PulsingDot()
                                CodingSessionDisplayState.NeedsInput -> StaticDot(NeedsInputAmber)
                                CodingSessionDisplayState.Review -> StaticDot(ReviewGreen)
                                CodingSessionDisplayState.Done -> StaticDot(DoneBlue)
                            }
                        }
                    },
                )
                // EXP-549: the LIVE machine label, so a rename lands here
                // instead of the row keeping the original hostname forever.
                val deviceName = device.displayLabel
                Text(
                    when {
                        paused -> "Paused · $deviceName"
                        else -> when (state) {
                            CodingSessionDisplayState.NeedsInput -> "Needs input · $deviceName"
                            CodingSessionDisplayState.Review -> "Ready for review · $deviceName"
                            CodingSessionDisplayState.Done -> "Done · $deviceName"
                            CodingSessionDisplayState.Running ->
                                "$deviceName · started ${relativeTime(session.startedAt)}"
                        }
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = when {
                        paused -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                        else -> when (state) {
                            CodingSessionDisplayState.NeedsInput -> NeedsInputAmber
                            CodingSessionDisplayState.Review -> ReviewGreen
                            CodingSessionDisplayState.Done -> DoneBlue
                            CodingSessionDisplayState.Running ->
                                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                        }
                    },
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    // Aligned under the identifier: the dot (8dp) plus its
                    // 12dp gap now live inside the identity line above.
                    modifier = Modifier.padding(start = 20.dp),
                )
            }
            // EXP-498: merging always closes the session too — confirm-gated,
            // and only while the PR is actually open. EXP-706: a
            // conflict-refused merge REPLACES this control with the recovery
            // run instead of stacking a second button under the message.
            if (canMerge) {
                if (merging) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                } else {
                    // EXP-698: the one 32dp glass circle every trailing row
                    // action draws on.
                    CircleIconButton(
                        if (canFixConflicts) ExpIcons.uiBranch else ExpIcons.prMerged,
                        contentDescription = if (canFixConflicts) "Fix conflicts" else "Merge",
                        onClick = if (canFixConflicts) onFixConflicts else onMerge,
                    )
                }
            }
            when {
                // EXP-698: the identity line above already prints the
                // identifier, so a trailing pill repeating it said "APP-5"
                // twice in one row — and at [PillSize.Sm] it also sat a rung
                // below the 32dp merge circle beside it. The trailing control
                // is now the same 32dp circle in BOTH cases: open the issue,
                // or open the action.
                issueIdentifier != null -> CircleIconButton(
                    ExpIcons.uiIssue,
                    contentDescription = "Open $issueIdentifier",
                    onClick = onOpenIssue,
                    modifier = Modifier.padding(start = 8.dp),
                )
                actionIcon != null -> CircleIconButton(
                    actionIcon,
                    contentDescription = actionLabel,
                    onClick = onOpenAction,
                    modifier = Modifier.padding(start = 8.dp),
                )
            }
        }

        // A refused merge (conflicts, branch protection, GitHub App errors)
        // captions THIS row (EXP-323 pattern) — inside the list, which already
        // clears the floating nav pill, so the reason is always readable.
        // EXP-706: the message ONLY; the recovery run took the merge control's
        // place in the row above.
        if (failure != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 3.dp)
                    .glassCard()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Text(
                    failure.message,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
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
                ExpIcons.navDevices,
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
