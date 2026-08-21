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
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
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
import com.exponential.app.data.api.builtinCreateAction
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.domain.ActionTrigger
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.nextScheduleRun
import com.exponential.app.domain.triggerSummary
import com.exponential.app.ui.components.GlassPillButton
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.TopBarBackButton
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
// EXP-574) opens the sheet in its dedicated create mode instead. After a
// successful send the screen waits for the desktop's synced coding_sessions
// row and jumps into the existing agent session viewer once.
//
// EXP-530 splits the surface into three segments (the PersonalScreen
// GlassSegmentedControl pattern): Actions (today's list, with an inline
// trigger summary on automated rows), Automations (dense per-trigger rows +
// recent automated runs) and Suggestions (seed cards whose "Use" opens the
// create sheet prefilled).

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
    val triggerBusyActionId by viewModel.triggerBusyActionId.collectAsStateWithLifecycle()
    val triggerError by viewModel.triggerError.collectAsStateWithLifecycle()

    var segment by rememberSaveable { mutableStateOf(SEGMENT_ACTIONS) }

    // The action the unified sheet was opened for (non-null = sheet open).
    var sheetAction by remember { mutableStateOf<ActionDto?>(null) }
    // EXP-431: the top bar's "New action" entry opens the SAME sheet locked
    // to the create builtin, presented as a creation flow.
    var createMode by remember { mutableStateOf(false) }
    // EXP-530: a used suggestion's description + icon, seeded into the create
    // sheet's input values (the iOS prefilledInputs pattern).
    var suggestionPrefill by remember { mutableStateOf<Map<String, String>?>(null) }

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
                        actions = state.actions,
                        devices = syncedDevices,
                        runs = automationRuns,
                        isOwner = isTeamOwner,
                        busyActionId = triggerBusyActionId,
                        error = triggerError,
                        onSetEnabled = viewModel::setTriggerEnabled,
                    )
                    SEGMENT_SUGGESTIONS -> SuggestionsContent(
                        onUse = { suggestion ->
                            selectedTeamId?.let {
                                createMode = true
                                suggestionPrefill = mapOf(
                                    "description" to suggestion.description,
                                    "icon" to suggestion.icon,
                                )
                                sheetAction = builtinCreateAction(it)
                            }
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
                                        onClick = {
                                            selectedTeamId?.let {
                                                createMode = true
                                                sheetAction = builtinCreateAction(it)
                                            }
                                        },
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
                                ActionRow(action = action, onRun = { sheetAction = action })
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
            createActionMode = createMode,
            prefilledInputs = suggestionPrefill.takeIf { createMode },
            onStart = viewModel::startCoding,
            onRunAction = viewModel::runAction,
            onDismiss = {
                sheetAction = null
                createMode = false
                suggestionPrefill = null
            },
        )
    }
}

// One action: bolt glyph (a create + for the virtual builtin), name (+ a
// small repo indicator when the action clones a repository), optional
// description, and a trailing Run affordance.
@Composable
private fun ActionRow(action: ActionDto, onRun: () -> Unit) {
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
            // EXP-530: an automated action carries its trigger sentence inline.
            val trigger = remember(action.trigger) { action.parsedTrigger }
            if (trigger != null) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Icon(
                        ExpIcons.actionAutomation,
                        contentDescription = null,
                        modifier = Modifier.size(11.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                    Text(
                        triggerSummary(trigger),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp),
            modifier = Modifier.padding(start = 8.dp),
        ) {
            Icon(
                ExpIcons.actionRun,
                contentDescription = null,
                modifier = Modifier.size(15.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
            Text(
                "Run",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

// ── Automations segment (EXP-530) ────────────────────────────────────────────

@Composable
private fun AutomationsContent(
    actions: List<ActionDto>,
    devices: List<SteerDevice>,
    runs: List<CodingSessionEntity>,
    isOwner: Boolean,
    busyActionId: String?,
    error: String?,
    onSetEnabled: (ActionDto, Boolean) -> Unit,
) {
    val automated = remember(actions) {
        actions.filter { !it.isBuiltin && it.parsedTrigger != null }
    }
    if (automated.isEmpty() && runs.isEmpty()) {
        AutomationsEmptyState()
        return
    }
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // EXP-574 (web parity): counted section header.
        item(key = "__automations_header__") {
            SectionLabel(title = "Automations", count = automated.size)
        }
        if (error != null) {
            item(key = "__trigger_error__") {
                Text(
                    error,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(horizontal = 4.dp),
                )
            }
        }
        if (automated.isEmpty()) {
            item(key = "__no_automations__") {
                Text(
                    "No automations yet. Add a schedule or event trigger to an action.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp),
                )
            }
        } else {
            items(automated, key = { it.id }) { action ->
                AutomationRow(
                    action = action,
                    devices = devices,
                    // The owner toggles; while ANY flip is in flight every
                    // switch parks (one mutation at a time, iOS parity).
                    toggleEnabled = isOwner && busyActionId == null,
                    onSetEnabled = { enabled -> onSetEnabled(action, enabled) },
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

// One triggered action: glyph + name, the trigger sentence, the bound device
// (label + online dot off the synced devices rows; raw id when the row isn't
// visible to us), the next schedule run in the VIEWER's timezone (hence the
// "(device time)" label — the device fires on its own clock), and the
// owner-only enabled toggle.
@Composable
private fun AutomationRow(
    action: ActionDto,
    devices: List<SteerDevice>,
    toggleEnabled: Boolean,
    onSetEnabled: (Boolean) -> Unit,
) {
    val trigger = remember(action.trigger) { action.parsedTrigger } ?: return
    val boundDevice = devices.firstOrNull { it.deviceId == trigger.deviceId }
    // An automated run has nobody to type required inputs, so the server
    // refuses to ENABLE such a trigger — but a legacy row that is already on
    // must stay switchable OFF.
    val hasRequired = action.inputs.orEmpty().any { it.required }
    val locked = hasRequired && !trigger.enabled
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("automation-row")
            .glassRow()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            action.icon?.let { ExpIcons.byName(it) } ?: ExpIcons.actionDefault,
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
                action.name,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                triggerSummary(trigger),
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
                    deviceDisplayLabel(boundDevice, trigger.deviceId),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (trigger is ActionTrigger.Schedule && trigger.enabled) {
                nextScheduleRun(trigger)?.let { nextMs ->
                    Text(
                        "Next run ${formatRunTime(nextMs)} (device time)",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
            if (locked) {
                Text(
                    "This action has required inputs, and an automated run has none to " +
                        "fill them with. Make the inputs optional to enable it.",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        Spacer(Modifier.width(8.dp))
        // Owner-only (actions.update is owner-gated server-side).
        Switch(
            checked = trigger.enabled,
            onCheckedChange = onSetEnabled,
            enabled = toggleEnabled && !locked,
        )
    }
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
    DomainContract.codingSessionStatusMerged -> "Merged"
    DomainContract.codingSessionStatusEnded -> "Ended"
    else -> status.replace('_', ' ').replaceFirstChar { it.uppercaseChar() }
}

@Composable
private fun AutomationsEmptyState() {
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
                "No automations yet. Add a schedule or event trigger to an action.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                textAlign = TextAlign.Center,
            )
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
            Text(
                suggestion.title,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
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
