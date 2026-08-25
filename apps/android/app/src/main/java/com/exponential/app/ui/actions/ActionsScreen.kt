package com.exponential.app.ui.actions

import androidx.compose.foundation.background
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.nextScheduleRun
import com.exponential.app.domain.triggerSummary
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPillButton
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.StartCodingSheet
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.ui.steer.SteerRunCaptionRow
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import java.text.DateFormat
import java.util.Date

// The Actions screen (EXP-253, view + run only — no manual edit on mobile):
// the selected team's action prompts, each with a Run affordance that opens
// the unified Start-coding sheet (EXP-257) preselected on that action — one
// launcher for issue runs AND action runs, with typed input fields and the
// full agent/model/effort/toggle options. The "Fix merge conflicts" builtin
// pins first by its flag; "Create action" left the list (EXP-431) — the
// "Actions" section header's "New action" button (web-parity placement,
// EXP-574) opens the dedicated [CreateActionSheet] (EXP-615) instead. After a
// successful send the screen waits for the desktop's synced coding_sessions
// row and jumps into the existing agent session viewer once.
//
// EXP-530 splits the surface into three segments (the PersonalScreen
// GlassSegmentedControl pattern): Actions (the plain list), Automations and
// Suggestions (seed cards whose "Use" opens the create sheet prefilled).
// EXP-583 made automations their OWN entity: an action carries no trigger
// anymore, the Automations segment lists `automations` rows (action + trigger
// + bound machine + agent pins) with an owner-only enable Switch, Edit +
// Delete in the row overflow (EXP-615) and a "New automation" form sheet;
// an action row only says HOW MANY automations point at it.

// rememberSaveable-friendly segment keys (plain strings, no custom Saver).
private const val SEGMENT_ACTIONS = "actions"
private const val SEGMENT_AUTOMATIONS = "automations"
private const val SEGMENT_SUGGESTIONS = "suggestions"

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActionsScreen(
    onBack: () -> Unit,
    onOpenSteer: (codingSessionId: String) -> Unit,
    viewModel: ActionsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val runState by viewModel.runState.collectAsStateWithLifecycle()
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()
    val selectedTeamId by viewModel.selectedTeamId.collectAsStateWithLifecycle()
    val syncedDevices by viewModel.syncedDevices.collectAsStateWithLifecycle()
    val automationRuns by viewModel.automationRuns.collectAsStateWithLifecycle()
    val isTeamOwner by viewModel.isTeamOwner.collectAsStateWithLifecycle()
    val automations by viewModel.automations.collectAsStateWithLifecycle()
    val automationDevices by viewModel.automationDevices.collectAsStateWithLifecycle()
    val lastRunByAutomation by viewModel.lastRunByAutomation.collectAsStateWithLifecycle()
    val automationBusy by viewModel.automationBusy.collectAsStateWithLifecycle()
    val automationError by viewModel.automationError.collectAsStateWithLifecycle()

    var segment by rememberSaveable { mutableStateOf(SEGMENT_ACTIONS) }

    // The action the unified sheet was opened for (non-null = sheet open).
    var sheetAction by remember { mutableStateOf<ActionDto?>(null) }
    // EXP-615: authoring lives in its own sheet now (true = open).
    var createOpen by remember { mutableStateOf(false) }
    // EXP-530: a used suggestion's description + icon, seeded into the create
    // sheet's input values (the iOS prefilledInputs pattern).
    var suggestionPrefill by remember { mutableStateOf<Map<String, String>?>(null) }
    // EXP-583: an "Action + automation" suggestion's proposed trigger, which
    // seeds the create sheet's Automation row.
    var suggestionAutomation by remember { mutableStateOf<AutomationTrigger?>(null) }
    // The owner-only automation form: true = creating, non-null row = editing.
    var automationForm by remember { mutableStateOf(false) }
    var automationEditTarget by remember { mutableStateOf<AutomationEntity?>(null) }

    // Re-poll device presence each time the screen comes to the foreground.
    LifecycleResumeEffect(Unit) {
        viewModel.refreshDevices()
        onPauseOrDispose { }
    }

    // The desktop picked the start up — jump into the live viewer ONCE.
    LaunchedEffect(startedSessionId) {
        startedSessionId?.let {
            viewModel.consumeStartedSession()
            onOpenSteer(it)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Actions") },
                navigationIcon = {
                    TopBarBackButton(onClick = onBack)
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent),
            )
        },
        containerColor = Color.Transparent,
    ) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            GlassSegmentedControl(
                options = listOf(SEGMENT_ACTIONS, SEGMENT_AUTOMATIONS, SEGMENT_SUGGESTIONS),
                selected = when (segment) {
                    SEGMENT_AUTOMATIONS -> SEGMENT_AUTOMATIONS
                    SEGMENT_SUGGESTIONS -> SEGMENT_SUGGESTIONS
                    else -> SEGMENT_ACTIONS
                },
                label = {
                    when (it) {
                        SEGMENT_AUTOMATIONS -> "Automations"
                        SEGMENT_SUGGESTIONS -> "Suggestions"
                        else -> "Actions"
                    }
                },
                onSelect = { segment = it },
                modifier = Modifier.padding(horizontal = 16.dp),
            )
            Spacer(Modifier.height(4.dp))
            Box(modifier = Modifier.fillMaxSize()) {
                when (segment) {
                    SEGMENT_AUTOMATIONS -> AutomationsContent(
                        automations = automations,
                        actions = state.actions,
                        devices = syncedDevices,
                        lastRuns = lastRunByAutomation,
                        runs = automationRuns,
                        isOwner = isTeamOwner,
                        busy = automationBusy,
                        error = automationError,
                        onSetEnabled = viewModel::setAutomationEnabled,
                        onDelete = viewModel::deleteAutomation,
                        onEdit = { automation ->
                            viewModel.clearAutomationError()
                            automationEditTarget = automation
                        },
                        onNew = {
                            viewModel.clearAutomationError()
                            automationForm = true
                        },
                    )
                    SEGMENT_SUGGESTIONS -> SuggestionsContent(
                        onUse = { suggestion ->
                            suggestionPrefill = mapOf(
                                "description" to suggestion.description,
                                "icon" to suggestion.icon,
                            )
                            suggestionAutomation = suggestion.automation
                            createOpen = true
                        },
                    )
                    else -> when {
                        state.actions.isEmpty() && state.loading ->
                            CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
                        state.actions.isEmpty() && state.error != null ->
                            CenteredCaption(state.error ?: "")
                        state.actions.isEmpty() -> ActionsEmptyState()
                        else -> LazyColumn(
                            modifier = Modifier.fillMaxSize(),
                            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            // EXP-574 (web parity): "Actions · count" header
                            // with the "New action" entry (EXP-431) as its
                            // trailing control.
                            item(key = "__actions_header__") {
                                SectionLabel(title = "Actions", count = state.actions.size) {
                                    GlassPillButton(
                                        label = "New action",
                                        icon = ExpIcons.actionCreate,
                                        enabled = selectedTeamId != null,
                                        onClick = { createOpen = true },
                                        modifier = Modifier.testTag("new-action"),
                                    )
                                }
                            }
                            if (runState !is ActionRunState.Idle) {
                                item(key = "__run_state__") { SteerRunCaptionRow(runState) }
                            }
                            // Builtin rows pin FIRST by the flag, never by sort order
                            // (EXP-257; the stable sort keeps server order otherwise).
                            items(
                                state.actions.sortedByDescending { it.isBuiltin },
                                key = { it.id },
                            ) { action ->
                                ActionRow(
                                    action = action,
                                    // EXP-583: an action only says HOW MANY
                                    // automations point at it — they are their
                                    // own rows on their own tab.
                                    automationCount = automations.count {
                                        it.actionId == action.id
                                    },
                                    onRun = { sheetAction = action },
                                )
                            }
                        }
                    }
                }
            }
        }
    }

    val action = sheetAction
    if (action != null) {
        StartCodingSheet(
            devices = devices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = emptySet(),
            preselectedActionId = action.id,
            onStart = viewModel::startCoding,
            onRunAction = viewModel::runAction,
            onDismiss = { sheetAction = null },
        )
    }

    // EXP-615: authoring an action is its own sheet — a form, not a run picker.
    if (createOpen) {
        selectedTeamId?.let { teamId ->
            CreateActionSheet(
                teamId = teamId,
                devices = devices ?: emptyList(),
                prefilledInputs = suggestionPrefill,
                suggestionAutomation = suggestionAutomation,
                onRunAction = viewModel::runAction,
                onDismiss = {
                    createOpen = false
                    suggestionPrefill = null
                    suggestionAutomation = null
                },
            )
        }
    }

    if (automationForm || automationEditTarget != null) {
        val editing = automationEditTarget
        AutomationFormSheet(
            actions = state.actions,
            devices = automationDevices,
            busy = automationBusy,
            error = automationError,
            editing = editing,
            onSubmit = { actionId, deviceId, trigger, agent, model, effort ->
                val close = {
                    automationForm = false
                    automationEditTarget = null
                }
                if (editing == null) {
                    viewModel.createAutomation(
                        actionId = actionId,
                        deviceId = deviceId,
                        trigger = trigger,
                        agent = agent,
                        model = model,
                        effort = effort,
                        onDone = close,
                    )
                } else {
                    viewModel.updateAutomation(
                        automationId = editing.id,
                        actionId = actionId,
                        deviceId = deviceId,
                        trigger = trigger,
                        agent = agent,
                        model = model,
                        effort = effort,
                        onDone = close,
                    )
                }
            },
            onDismiss = {
                automationForm = false
                automationEditTarget = null
            },
        )
    }
}

// One action: its curated glyph, name (+ a small repo indicator when the
// action clones a repository), optional description, how many automations
// point at it (EXP-583), and a trailing play button.
@Composable
private fun ActionRow(action: ActionDto, automationCount: Int, onRun: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("action-row")
            .glassRow()
            .clickable(onClick = onRun)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            // EXP-273: the action's own curated glyph (the builtins set one
            // too), falling back to the generic action mark.
            action.icon?.let { ExpIcons.byName(it) } ?: ExpIcons.actionDefault,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    action.name,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (action.repositoryId != null) {
                    Spacer(Modifier.width(6.dp))
                    Icon(
                        ExpIcons.actionRepository,
                        contentDescription = "Runs in a repository",
                        modifier = Modifier.size(12.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
            val description = action.description
            if (!description.isNullOrBlank()) {
                Text(
                    description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (automationCount > 0) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Icon(
                        ExpIcons.actionAutomation,
                        contentDescription = null,
                        modifier = Modifier.size(10.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                    Text(
                        "$automationCount " +
                            if (automationCount == 1) "automation" else "automations",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                    )
                }
            }
        }
        // EXP-615: an icon-only play button, like every other Run affordance.
        IconButton(onClick = onRun, modifier = Modifier.padding(start = 8.dp)) {
            Icon(
                ExpIcons.actionRun,
                contentDescription = "Run",
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

// ── Automations segment (EXP-583) ───────────────────────────────────────────

@Composable
private fun AutomationsContent(
    automations: List<AutomationEntity>,
    actions: List<ActionDto>,
    devices: List<SteerDevice>,
    lastRuns: Map<String, CodingSessionEntity>,
    runs: List<CodingSessionEntity>,
    isOwner: Boolean,
    busy: Boolean,
    error: String?,
    onSetEnabled: (String, Boolean) -> Unit,
    onDelete: (String) -> Unit,
    onEdit: (AutomationEntity) -> Unit,
    onNew: () -> Unit,
) {
    val actionsById = remember(actions) { actions.associateBy { it.id } }
    if (automations.isEmpty() && runs.isEmpty()) {
        AutomationsEmptyState(isOwner = isOwner, onNew = onNew)
        return
    }
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // EXP-574 (web parity): counted section header, with the owner-only
        // create entry as its trailing control.
        item(key = "__automations_header__") {
            SectionLabel(title = "Automations", count = automations.size) {
                if (isOwner) {
                    GlassPillButton(
                        label = "New automation",
                        icon = ExpIcons.uiAdd,
                        enabled = !busy,
                        onClick = onNew,
                        modifier = Modifier.testTag("new-automation"),
                    )
                }
            }
        }
        if (error != null) {
            item(key = "__automation_error__") {
                Text(
                    error,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(horizontal = 4.dp),
                )
            }
        }
        if (automations.isEmpty()) {
            item(key = "__no_automations__") {
                Text(
                    "No automations yet.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp),
                )
            }
        } else {
            items(automations, key = { it.id }) { automation ->
                AutomationRow(
                    automation = automation,
                    action = actionsById[automation.actionId],
                    devices = devices,
                    lastRun = lastRuns[automation.id],
                    // The owner toggles and deletes; while ANY mutation is in
                    // flight every control parks (one at a time, iOS parity).
                    isOwner = isOwner,
                    busy = busy,
                    onSetEnabled = { enabled -> onSetEnabled(automation.id, enabled) },
                    onDelete = { onDelete(automation.id) },
                    onEdit = { onEdit(automation) },
                )
            }
        }
        if (runs.isNotEmpty()) {
            item(key = "__recent_runs_header__") {
                SectionLabel(
                    title = "Recent automated runs",
                    count = runs.size,
                    modifier = Modifier.padding(top = 12.dp),
                )
            }
            items(runs, key = { it.id }) { session ->
                AutomatedRunRow(session)
            }
        }
    }
}

// One automation: the target action's glyph + name, the trigger sentence, the
// bound machine (label + online dot off the synced devices rows; the raw id
// when the row isn't visible to us), the agent pins, the next schedule run in
// the VIEWER's timezone (hence "(device time)" — the machine fires on its own
// clock), the last run it produced, the owner-only enabled toggle and a
// Delete in the overflow.
@Composable
private fun AutomationRow(
    automation: AutomationEntity,
    action: ActionDto?,
    devices: List<SteerDevice>,
    lastRun: CodingSessionEntity?,
    isOwner: Boolean,
    busy: Boolean,
    onSetEnabled: (Boolean) -> Unit,
    onDelete: () -> Unit,
    onEdit: () -> Unit,
) {
    val trigger = remember(automation.trigger) { AutomationTrigger.parse(automation.trigger) }
    val boundDevice = devices.firstOrNull { it.deviceId == automation.deviceId }
    var menuOpen by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("automation-row")
            .glassRow()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            action?.icon?.let { ExpIcons.byName(it) } ?: ExpIcons.actionDefault,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(
                // A deleted action cascades its automations away, so a missing
                // row here only means the actions shape hasn't caught up.
                action?.name ?: "Action",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                trigger?.let(::triggerSummary) ?: "Unsupported trigger",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                Box(
                    modifier = Modifier
                        .size(6.dp)
                        .clip(CircleShape)
                        .background(
                            if (boundDevice?.online == true) {
                                DesignTokens.Semantic.Green
                            } else {
                                Color.White.copy(alpha = 0.25f)
                            },
                        ),
                )
                Text(
                    deviceDisplayLabel(boundDevice, automation.deviceId),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                val launchLabel = automationLaunchLabel(automation)
                if (launchLabel.isNotEmpty()) {
                    Text(
                        launchLabel,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            if (trigger is AutomationTrigger.Schedule && automation.enabled) {
                nextScheduleRun(trigger)?.let { nextMs ->
                    Text(
                        "Next run ${formatRunTime(nextMs)} (device time)",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
            if (lastRun != null) {
                val when_ = relativeTime(lastRun.startedAt)
                Text(
                    "Last run ${sessionStatusLabel(lastRun.status)}" +
                        if (when_.isEmpty()) "" else " · $when_",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        Spacer(Modifier.width(8.dp))
        // Owner-only (every `automations` write is owner-gated server-side).
        Switch(
            checked = automation.enabled,
            onCheckedChange = onSetEnabled,
            enabled = isOwner && !busy,
        )
        if (isOwner) {
            Box {
                IconButton(onClick = { menuOpen = true }, enabled = !busy) {
                    Icon(
                        ExpIcons.uiMore,
                        contentDescription = "Automation options",
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                }
                GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    GlassMenuItem(
                        text = { Text("Edit") },
                        leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                        onClick = {
                            menuOpen = false
                            onEdit()
                        },
                    )
                    GlassMenuItem(
                        text = { Text("Delete") },
                        leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                        destructive = true,
                        onClick = {
                            menuOpen = false
                            confirmDelete = true
                        },
                    )
                }
            }
        }
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete automation?") },
            text = {
                Text(
                    "Stop automating \"${action?.name ?: "this action"}\"? The action itself " +
                        "stays, and runs already going keep going.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    onDelete()
                }) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
}

/** The row's agent pins: the pinned agent plus whatever model/effort rides
 * with it; blank for a legacy NULL-agent row (web parity — no pin text). */
private fun automationLaunchLabel(automation: AutomationEntity): String {
    val agent = automation.agent
    if (agent.isNullOrEmpty()) return ""
    val extras = listOfNotNull(
        automation.model?.takeIf { it.isNotEmpty() }?.let(::modelLabel),
        automation.effort?.takeIf { it.isNotEmpty() }?.let(::effortLabel),
    )
    return (listOf(agentLabel(agent)) + extras).joinToString(" · ")
}


private fun deviceDisplayLabel(device: SteerDevice?, deviceId: String): String {
    if (device == null) return deviceId
    val name = device.deviceLabel.ifBlank { device.deviceId }
    val owner = device.owner ?: return name
    return if (owner.name.isBlank()) name else "$name — ${owner.name}"
}

private fun formatRunTime(epochMs: Long): String =
    DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(epochMs))

// One automation-started coding_sessions row (started_reason non-null):
// "Automated" badge, action-name snapshot, status, relative start time.
@Composable
private fun AutomatedRunRow(session: CodingSessionEntity) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("automated-run-row")
            .glassRow()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            modifier = Modifier
                .clip(CircleShape)
                .background(Color.White.copy(alpha = 0.08f))
                .padding(horizontal = 8.dp, vertical = 3.dp),
        ) {
            Icon(
                ExpIcons.actionAutomation,
                contentDescription = null,
                modifier = Modifier.size(10.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Text(
                "Automated",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
        Text(
            session.actionName ?: "Action run",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Text(
            sessionStatusLabel(session.status),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        val time = relativeTime(session.startedAt)
        if (time.isNotEmpty()) {
            Text(
                time,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

private fun sessionStatusLabel(status: String): String = when (status) {
    DomainContract.codingSessionStatusRunning -> "Running"
    DomainContract.codingSessionStatusInReview -> "In review"
    DomainContract.codingSessionStatusEnded -> "Ended"
    else -> status.replace('_', ' ').replaceFirstChar { it.uppercaseChar() }
}

@Composable
private fun AutomationsEmptyState(isOwner: Boolean, onNew: () -> Unit) {
    Box(Modifier.fillMaxSize().padding(horizontal = 40.dp), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(
                ExpIcons.actionAutomation,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(28.dp),
            )
            Text(
                "No automations yet.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                textAlign = TextAlign.Center,
            )
            if (isOwner) {
                GlassPillButton(
                    label = "New automation",
                    icon = ExpIcons.uiAdd,
                    onClick = onNew,
                    modifier = Modifier.testTag("new-automation"),
                )
            }
        }
    }
}

// ── Suggestions segment (EXP-530) ────────────────────────────────────────────

@Composable
private fun SuggestionsContent(onUse: (ActionSuggestion) -> Unit) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        items(ACTION_SUGGESTIONS, key = { it.id }) { suggestion ->
            SuggestionRow(suggestion = suggestion, onUse = { onUse(suggestion) })
        }
    }
}

@Composable
private fun SuggestionRow(suggestion: ActionSuggestion, onUse: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("suggestion-row")
            .glassRow()
            .clickable(onClick = onUse)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            ExpIcons.byName(suggestion.icon) ?: ExpIcons.actionSuggestion,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(
                    suggestion.title,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                // EXP-583: what "Use suggestion" will set up — an action, or
                // an action plus the automation that runs it.
                SuggestionKindChip(
                    if (suggestion.automation != null) "Action + automation" else "Action",
                )
            }
            Text(
                suggestion.description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Text(
            "Use suggestion",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.primary,
            maxLines = 1,
            modifier = Modifier.padding(start = 8.dp),
        )
    }
}

// The small "Action" / "Action + automation" pill on a suggestion row.
@Composable
private fun SuggestionKindChip(label: String) {
    Text(
        label,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        maxLines = 1,
        modifier = Modifier
            .clip(CircleShape)
            .background(Color.White.copy(alpha = 0.08f))
            .padding(horizontal = 8.dp, vertical = 2.dp),
    )
}

// The web `SectionLabel` (agent-session-row.tsx): title · count · spacer ·
// optional trailing control (EXP-574 layout parity).
@Composable
private fun SectionLabel(
    title: String,
    count: Int,
    modifier: Modifier = Modifier,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier = modifier.fillMaxWidth().padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            title,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Text(
            "$count",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.weight(1f))
        trailing?.invoke()
    }
}

@Composable
private fun CenteredCaption(text: String) {
    Box(Modifier.fillMaxSize().padding(horizontal = 40.dp), contentAlignment = Alignment.Center) {
        Text(
            text,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun ActionsEmptyState() {
    Box(Modifier.fillMaxSize().padding(horizontal = 40.dp), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(
                ExpIcons.actionDefault,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(28.dp),
            )
            Text(
                "No actions yet",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Text(
                "Actions are reusable prompts your team runs on a desktop. " +
                    "Team owners create them on the web or desktop app.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                textAlign = TextAlign.Center,
            )
        }
    }
}
