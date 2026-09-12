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
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.formatAutomationBlock
import com.exponential.app.domain.triggerSummary
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EndedRunRow
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SwitchThumb
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.glassSwitchColors
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.ui.steer.SteerRunCaptionRow
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.theme.glassRow

// The Actions screen (EXP-253, view + run only — no manual edit on mobile):
// the selected team's action prompts, each with a Run affordance. EXP-825:
// Run NAVIGATES to the Agent page composer with the action preselected (the
// ONE launcher — typed inputs, free text and the agent/model/device options
// live there), the "Actions" header's "New action" (web-parity placement per
// EXP-574) lands there on the "Create action" builtin, and a used suggestion
// seeds that builtin with its description (+ automation note) and icon.
// Neither client builtin is listed here: "Create action" left in EXP-431 and
// "Fix merge conflicts" in EXP-686 (both pick from the composer's ▶ menu).
// The Automations segment's Resume still reports back on this screen and
// jumps into the desktop's row once it syncs.
//
// EXP-686 gave it its own bottom-bar tab (it used to be a push off the Agents
// header, now Devices), so the screen is a ROOT surface: a plain header row,
// no back button, and its lists clear the floating bar.
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

@Composable
fun ActionsScreen(
    onOpenSteer: (codingSessionId: String) -> Unit,
    // EXP-825: Run / New action / a suggestion navigate to the composer.
    onOpenAgent: (AgentComposerSeed) -> Unit,
    viewModel: ActionsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val runState by viewModel.runState.collectAsStateWithLifecycle()
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
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

    // The owner-only automation form: true = creating, non-null row = editing.
    var automationForm by remember { mutableStateOf(false) }
    var automationEditTarget by remember { mutableStateOf<AutomationEntity?>(null) }
    // EXP-694: the action whose editor is open (non-null = sheet open).
    var editActionId by remember { mutableStateOf<String?>(null) }

    // The desktop picked a Resume up — jump into the live viewer ONCE.
    LaunchedEffect(startedSessionId) {
        startedSessionId?.let {
            viewModel.consumeStartedSession()
            onOpenSteer(it)
        }
    }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            // The root screens' header (AgentsScreen / SearchScreen parity):
            // a plain large title, no back button — this is a tab now.
            Text(
                "Actions",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            )
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
                testTag = { "actions-segment-$it" },
            )
            Spacer(Modifier.height(4.dp))
            Box(modifier = Modifier.fillMaxSize()) {
                when (segment) {
                    SEGMENT_AUTOMATIONS -> AutomationsContent(
                        automations = automations,
                        onOpenSteer = onOpenSteer,
                        actions = state.actions,
                        devices = syncedDevices,
                        lastRuns = lastRunByAutomation,
                        runs = automationRuns,
                        isOwner = isTeamOwner,
                        busy = automationBusy,
                        error = automationError,
                        // EXP-773: a run's close-out and its Resume live in
                        // the session view a row opens; the send caption for
                        // a manual run still rides this column.
                        runState = runState,
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
                            // EXP-825: the creator run reads the request off
                            // the composer text — the suggestion's
                            // description, plus (EXP-583) the machine-readable
                            // automation note the agent copies verbatim into
                            // exponential_automations_create, bound to the
                            // caller's default automation-capable machine
                            // (EXP-622) when one exists. The icon seeds the
                            // builtin's `icon` pick.
                            val trigger = suggestion.automation
                            val runner = automationDevices.firstOrNull { it.isDefault }
                                ?: automationDevices.firstOrNull()
                            val text = if (trigger != null && runner != null) {
                                suggestion.description +
                                    formatAutomationBlock(trigger, deviceId = runner.deviceId)
                            } else {
                                suggestion.description
                            }
                            onOpenAgent(
                                AgentComposerSeed(
                                    actionId = DomainContract.builtinCreateActionId,
                                    text = text,
                                    icon = suggestion.icon,
                                ),
                            )
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
                            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            // EXP-574 (web parity): the "Actions" header with
                            // the "New action" entry (EXP-431) as its trailing
                            // control.
                            item(key = "__actions_header__") {
                                SectionHeader(title = "Actions") {
                                    GlassPill(
                                        "New action",
                                        icon = ExpIcons.actionCreate,
                                        enabled = selectedTeamId != null,
                                        onClick = {
                                            onOpenAgent(
                                                AgentComposerSeed(
                                                    actionId = DomainContract.builtinCreateActionId,
                                                ),
                                            )
                                        },
                                        modifier = Modifier.testTag("new-action"),
                                    )
                                }
                            }
                            if (runState !is ActionRunState.Idle) {
                                item(key = "__run_state__") { SteerRunCaptionRow(runState) }
                            }
                            // Server order (sort_order, then name) — since
                            // EXP-686 the list carries no client builtins to
                            // pin above it.
                            items(state.actions, key = { it.id }) { action ->
                                ActionRow(
                                    action = action,
                                    // EXP-583: an action only says HOW MANY
                                    // automations point at it — they are their
                                    // own rows on their own tab.
                                    automationCount = automations.count {
                                        it.actionId == action.id
                                    },
                                    onRun = { onOpenAgent(AgentComposerSeed(actionId = action.id)) },
                                    // EXP-694: editing an action is a mobile
                                    // affordance now; the sheet itself is
                                    // read-only for non-owners.
                                    onEdit = { editActionId = action.id },
                                )
                            }
                        }
                    }
                }
            }
        }
    }

    // EXP-694: the full action editor (icon, name, description, repository and
    // the tRPC-fetched prompt body).
    editActionId?.let { id ->
        ActionEditSheet(actionId = id, onDismiss = { editActionId = null })
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

// One action: its curated glyph, name, optional description, how many
// automations point at it (EXP-583), a trailing play button and (EXP-694) the
// row overflow with Edit.
@Composable
private fun ActionRow(
    action: ActionDto,
    automationCount: Int,
    onRun: () -> Unit,
    onEdit: () -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("action-row")
            .flatRow()
            .clickable(onClick = onRun)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            actionGlyph(action),
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            // EXP-697: no repo glyph beside the name — the row says what the
            // action is, not where it runs.
            Text(
                action.name,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
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
        // EXP-694: on the shared glass circle (iOS `CircleIconButton` parity),
        // not a bare primary-tinted glyph in an M3 touch box.
        CircleIconButton(
            ExpIcons.actionRun,
            contentDescription = "Run",
            onClick = onRun,
            modifier = Modifier.padding(start = 8.dp),
        )
        // Builtins are shipped prompts with no team row to edit (the list
        // carries none today — the guard keeps it that way).
        if (!action.isBuiltin) {
            Box {
                // EXP-698: every trailing row action is the one 32dp glass
                // circle — a bare glyph in an M3 touch box read as a hitbox,
                // not a control.
                CircleIconButton(
                    ExpIcons.uiMore,
                    contentDescription = "Action options",
                    onClick = { menuOpen = true },
                    modifier = Modifier.padding(start = 8.dp),
                )
                GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    GlassMenuItem(
                        text = { Text("Edit") },
                        leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                        onClick = {
                            menuOpen = false
                            onEdit()
                        },
                    )
                }
            }
        }
    }
}

// ── Automations segment (EXP-583) ───────────────────────────────────────────

@Composable
private fun AutomationsContent(
    automations: List<AutomationEntity>,
    // EXP-686: a run that is still going opens its live session from the row.
    onOpenSteer: (codingSessionId: String) -> Unit,
    actions: List<ActionDto>,
    devices: List<SteerDevice>,
    lastRuns: Map<String, CodingSessionEntity>,
    runs: List<CodingSessionEntity>,
    isOwner: Boolean,
    busy: Boolean,
    error: String?,
    runState: ActionRunState,
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
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // EXP-574 (web parity): section header, with the owner-only create
        // entry as its trailing control.
        item(key = "__automations_header__") {
            SectionHeader(title = "Automations") {
                if (isOwner) {
                    GlassPill(
                        "New automation",
                        icon = ExpIcons.uiAdd,
                        enabled = !busy,
                        onClick = onNew,
                        modifier = Modifier.testTag("new-automation"),
                    )
                }
            }
        }
        // EXP-637: a Resume sent from a run row reports back on this tab.
        if (runState !is ActionRunState.Idle) {
            item(key = "__run_state__") { SteerRunCaptionRow(runState) }
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
                SectionHeader(
                    title = "Recent automated runs",
                    modifier = Modifier.padding(top = 12.dp),
                )
            }
            items(runs, key = { it.id }) { session ->
                // One row shape for both (EXP-686/EXP-773): a plain link.
                // A finished run's close-out summary and its Resume live at
                // the top of the session view it opens, so live and finished
                // rows behave identically.
                val ended = session.status == DomainContract.codingSessionStatusEnded
                EndedRunRow(
                    title = session.actionName ?: "Action run",
                    timeLabel = relativeTime(
                        if (ended) session.endedAt ?: session.startedAt else session.startedAt,
                    ),
                    isLive = !ended,
                    onOpen = { onOpenSteer(session.id) },
                )
            }
        }
    }
}

// One automation: the target action's glyph + name, the trigger sentence, the
// bound machine (label + online dot off the synced devices rows; the raw id
// when the row isn't visible to us), the agent pins, the last run it produced,
// the owner-only enabled toggle and a Delete in the overflow. A schedule's
// sentence carries "(device time)" because the machine fires on its own clock;
// the row prints no absolute next-run date (EXP-812 — the calendar moved it
// under every screenshot, and the recurrence says the same thing).
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
            .flatRow()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            actionGlyph(action),
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
                trigger?.let(::triggerCaption) ?: "Unsupported trigger",
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
            if (lastRun != null) {
                // EXP-686: no status word — the run's own row below carries
                // "Running", and a finished one says nothing (iOS parity).
                val when_ = relativeTime(lastRun.startedAt)
                if (when_.isNotEmpty()) {
                    Text(
                        "Last run $when_",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
        }
        Spacer(Modifier.width(8.dp))
        // EXP-698: the trailing controls are their OWN cluster, CENTRED against
        // the row — the body wraps to four lines on a scheduled automation, and
        // a top-aligned toggle then floated beside the title while every other
        // list row in the app lines its actions up on the row's middle.
        Row(
            modifier = Modifier.align(Alignment.CenterVertically),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Owner-only (every `automations` write is owner-gated server-side).
            Switch(
                checked = automation.enabled,
                onCheckedChange = onSetEnabled,
                enabled = isOwner && !busy,
                colors = glassSwitchColors(),
                thumbContent = SwitchThumb,
            )
            if (isOwner) {
                Box {
                    CircleIconButton(
                        ExpIcons.uiMore,
                        contentDescription = "Automation options",
                        onClick = { menuOpen = true },
                        enabled = !busy,
                        modifier = Modifier.padding(start = 8.dp),
                    )
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

/**
 * The trigger sentence as the row prints it. A schedule fires on the BOUND
 * MACHINE's wall clock, so the recurrence carries the caveat the row used to
 * hang off an absolute next-run date (EXP-812).
 */
private fun triggerCaption(trigger: AutomationTrigger): String =
    if (trigger is AutomationTrigger.Schedule) {
        "${triggerSummary(trigger)} (device time)"
    } else {
        triggerSummary(trigger)
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
                GlassPill(
                    "New automation",
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
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
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
                    // EXP-677: yield to the chip instead of pushing it off the row.
                    modifier = Modifier.weight(1f, fill = false),
                )
                // EXP-583: what "Use" will set up — an action, or
                // the automation that runs one (EXP-677: short labels, the
                // chip was cut off).
                SuggestionKindChip(
                    if (suggestion.automation != null) "Automation" else "Action",
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
        // EXP-694: no trailing "Use" text — the whole row IS the affordance
        // (it always was; the label only looked like a second button).
    }
}

// The small "Action" / "Automation" pill on a suggestion row.
@Composable
private fun SuggestionKindChip(label: String) {
    GlassPill(label, size = PillSize.Sm, mode = PillMode.Readonly, maxLines = 1)
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
