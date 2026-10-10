package com.exponential.app.ui.actions

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.domain.ActionTrigger
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.actionRunTitle
import com.exponential.app.domain.SessionTreeNode
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.sessionTree
import com.exponential.app.domain.visibleSessionTreeRows
import com.exponential.app.domain.triggerCaption
import com.exponential.app.ui.components.GlassSwitch
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.CodingSessionRow
import com.exponential.app.ui.components.SessionRowSize
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.TabPager
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.domain.Prompts

// The action page (SLOP-2): ONE action — what it says, when it fires on its
// own, and what it ran. An action row (and an `action` entity ref) opens it; it
// replaces the edit sheet and the Automations segment. The header carries
// back, the action's glyph + name and Run (the Agent page composer with the
// action preselected, EXP-825). On a phone the three parts are TABS on a
// pager — `Prompt | Triggers | Runs`, the Work screen's face strip (wide
// layouts on web/desktop stack them as sections instead):
//
//  * Prompt — the action editor's fields ([ActionPromptTab]).
//  * Triggers — one row per readable trigger, in stored order: its glyph, the
//    trigger sentence, the bound machine + pins, the owner's enabled switch
//    and Edit / Delete. A phone never runs one: the bound machine does.
//  * Runs — every run of this action, newest first, each titled by what
//    started it. Opening one and going Back returns here.

internal const val ACTION_TAB_PROMPT = "prompt"
internal const val ACTION_TAB_TRIGGERS = "triggers"
internal const val ACTION_TAB_RUNS = "runs"

private val ACTION_TABS = listOf(ACTION_TAB_PROMPT, ACTION_TAB_TRIGGERS, ACTION_TAB_RUNS)

private fun actionTabLabel(tab: String): String = when (tab) {
    ACTION_TAB_TRIGGERS -> "Triggers"
    ACTION_TAB_RUNS -> "Runs"
    else -> "Prompt"
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActionDetailScreen(
    onBack: () -> Unit,
    // EXP-825: Run navigates to the composer with this action preselected.
    onOpenAgent: (AgentComposerSeed) -> Unit,
    onOpenSteer: (codingSessionId: String) -> Unit,
    viewModel: ActionDetailViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val triggers by viewModel.triggers.collectAsStateWithLifecycle()
    val syncedDevices by viewModel.syncedDevices.collectAsStateWithLifecycle()
    val triggerDevices by viewModel.triggerDevices.collectAsStateWithLifecycle()
    val runs by viewModel.runs.collectAsStateWithLifecycle()
    val isOwner by viewModel.isTeamOwner.collectAsStateWithLifecycle()
    val busy by viewModel.triggerBusy.collectAsStateWithLifecycle()
    val error by viewModel.triggerError.collectAsStateWithLifecycle()
    val action = state.action

    // Saveable: Back from a run lands on the same tab.
    var tab by rememberSaveable { mutableStateOf(ACTION_TAB_PROMPT) }

    // The owner-only trigger form: true = adding, non-null = editing that one.
    var addingTrigger by remember { mutableStateOf(false) }
    var editingTrigger by remember { mutableStateOf<ActionTrigger?>(null) }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            Column {
                CenterAlignedTopAppBar(
                    title = {
                        if (action != null) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                Icon(
                                    actionGlyph(action),
                                    contentDescription = null,
                                    modifier = Modifier.size(18.dp),
                                    tint = MaterialTheme.colorScheme.onSurface.copy(
                                        alpha = TextEmphasis.Secondary,
                                    ),
                                )
                                Text(
                                    action.name,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.testTag("action-detail-title"),
                                )
                            }
                        }
                    },
                    navigationIcon = { TopBarBackButton(onClick = onBack) },
                    actions = {
                        if (action != null) {
                            CircleIconButton(
                                ExpIcons.actionRun,
                                contentDescription = "Run",
                                onClick = { onOpenAgent(AgentComposerSeed(actionId = action.id)) },
                                modifier = Modifier
                                    .padding(end = 8.dp)
                                    .testTag("action-detail-run"),
                            )
                        }
                    },
                    colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                        containerColor = Color.Transparent,
                    ),
                )
                // The tabs close the header band (the Work screen's strip):
                // title and tabs read as one band that never moves.
                if (action != null) {
                    GlassSegmentedControl(
                        options = ACTION_TABS,
                        selected = tab,
                        label = ::actionTabLabel,
                        onSelect = { tab = it },
                        modifier = Modifier
                            .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 8.dp)
                            .testTag("action-tabs"),
                        testTag = { "action-tab-$it" },
                    )
                    HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
                }
            }
        },
    ) { padding ->
        when {
            action == null && state.loading -> LoadingState(modifier = Modifier.padding(padding))
            // Deleted meanwhile, or a ref to an action this account can't see.
            action == null -> EmptyState(
                message = "This action is no longer available.",
                icon = ExpIcons.actionDefault,
                modifier = Modifier.padding(padding),
            )
            else -> TabPager(
                pages = ACTION_TABS,
                selected = tab,
                onSelect = { tab = it },
                key = { it },
                modifier = Modifier
                    .padding(padding)
                    // consumeWindowInsets keeps imePadding from re-adding the
                    // navigation-bar inset the Scaffold padding already covers.
                    .consumeWindowInsets(padding)
                    .imePadding()
                    .fillMaxSize(),
                // ALL three pages stay composed: the neighbour is ready before
                // a drag starts (Work parity), and an unsaved Prompt edit
                // survives a visit to Runs, two pages away.
                beyondViewportPageCount = ACTION_TABS.size - 1,
            ) { page ->
                when (page) {
                    ACTION_TAB_TRIGGERS -> TriggersTab(
                        triggers = triggers,
                        devices = syncedDevices,
                        isOwner = isOwner,
                        blockedByInputs = action.hasRequiredInputs,
                        busy = busy,
                        // The form shows a refusal itself while it is open.
                        error = error.takeIf { !addingTrigger && editingTrigger == null },
                        onSetEnabled = viewModel::setTriggerEnabled,
                        onDelete = viewModel::deleteTrigger,
                        onEdit = { trigger ->
                            viewModel.clearTriggerError()
                            editingTrigger = trigger
                        },
                        onAdd = {
                            viewModel.clearTriggerError()
                            addingTrigger = true
                        },
                    )
                    ACTION_TAB_RUNS -> RunsTab(
                        runs = runs,
                        devices = syncedDevices,
                        onOpenSteer = onOpenSteer,
                    )
                    else -> ActionPromptTab(actionId = action.id)
                }
            }
        }
    }

    if (addingTrigger || editingTrigger != null) {
        val editing = editingTrigger
        val close = {
            addingTrigger = false
            editingTrigger = null
        }
        TriggerFormSheet(
            devices = triggerDevices,
            busy = busy,
            error = error,
            editing = editing,
            onSubmit = { result -> viewModel.saveTrigger(editing, result, onDone = close) },
            onDismiss = close,
        )
    }
}

// ── Triggers ────────────────────────────────────────────────────────────────

@Composable
private fun TriggersTab(
    triggers: List<ActionTrigger>,
    devices: List<SteerDevice>,
    isOwner: Boolean,
    // The action declares a required input: a trigger that is off stays off.
    blockedByInputs: Boolean,
    busy: Boolean,
    error: String?,
    onSetEnabled: (String, Boolean) -> Unit,
    onDelete: (String) -> Unit,
    onEdit: (ActionTrigger) -> Unit,
    onAdd: () -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize().testTag("action-triggers-tab"),
        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
    ) {
        // No band on a phone — the tab already says "Triggers" — so the
        // owner-only add entry trails a bare row (web < md parity).
        if (isOwner) {
            item(key = "__add_trigger__") {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
                    horizontalArrangement = Arrangement.End,
                ) {
                    // P68: no leading glyph ×4.
                    GlassPill(
                        "Add trigger",
                        enabled = !busy,
                        onClick = onAdd,
                        modifier = Modifier.testTag("add-trigger"),
                    )
                }
            }
        }
        if (error != null) {
            item(key = "__trigger_error__") {
                Text(
                    error,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(horizontal = 4.dp, vertical = 4.dp),
                )
            }
        }
        if (triggers.isEmpty()) {
            item(key = "__no_triggers__") {
                Text(
                    "No triggers. This action runs when someone starts it.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier.padding(horizontal = 4.dp, vertical = 12.dp),
                )
            }
        } else {
            items(triggers, key = { it.id }) { trigger ->
                TriggerRow(
                    trigger = trigger,
                    devices = devices,
                    // The owner toggles, edits and deletes; while ANY write is
                    // in flight every control parks (each write replaces the
                    // whole array).
                    isOwner = isOwner,
                    busy = busy,
                    // One that is already on must stay switchable OFF.
                    locked = blockedByInputs && !trigger.enabled,
                    onSetEnabled = { enabled -> onSetEnabled(trigger.id, enabled) },
                    onDelete = { onDelete(trigger.id) },
                    onEdit = { onEdit(trigger) },
                )
            }
        }
    }
}

/** The glyph of a trigger kind — the Triggers rows. */
internal fun triggerGlyph(whenPart: AutomationTrigger): ImageVector = when (whenPart) {
    is AutomationTrigger.Schedule -> ExpIcons.triggerSchedule
    is AutomationTrigger.Event -> ExpIcons.triggerEvent
}

// One trigger: its kind's glyph, the trigger sentence (a schedule carries
// "(device time)" — the machine fires on its own clock, EXP-812), the bound
// machine (label + online dot off the synced devices rows; the raw id when the
// row isn't visible to us) with the agent pins, the owner-only enabled switch
// and Edit / Delete in the overflow. No "last run": Runs shows it.
@Composable
private fun TriggerRow(
    trigger: ActionTrigger,
    devices: List<SteerDevice>,
    isOwner: Boolean,
    busy: Boolean,
    locked: Boolean,
    onSetEnabled: (Boolean) -> Unit,
    onDelete: () -> Unit,
    onEdit: () -> Unit,
) {
    val boundDevice = devices.firstOrNull { it.deviceId == trigger.deviceId }
    var menuOpen by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("trigger-row")
            .flatRow()
            .then(if (isOwner) Modifier.clickable(enabled = !busy, onClick = onEdit) else Modifier)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            triggerGlyph(trigger.whenPart),
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
                triggerCaption(trigger.whenPart),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
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
                    modifier = Modifier.weight(1f, fill = false),
                )
                val launchLabel = triggerLaunchLabel(trigger)
                if (launchLabel.isNotEmpty()) {
                    // P67: "{device} · {agent}" ×4.
                    Text(
                        "· $launchLabel",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            if (locked) {
                Text(
                    REQUIRED_INPUTS_HINT,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        Spacer(Modifier.width(8.dp))
        // EXP-698: the trailing controls are their OWN cluster, CENTRED against
        // the row — the body wraps to several lines on a locked trigger, and a
        // top-aligned toggle then floated beside the sentence while every other
        // list row in the app lines its actions up on the row's middle.
        Row(
            modifier = Modifier.align(Alignment.CenterVertically),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Owner-only (every `actions.update` is owner-gated server-side).
            GlassSwitch(
                checked = trigger.enabled,
                onCheckedChange = onSetEnabled,
                enabled = isOwner && !busy && !locked,
                modifier = Modifier.testTag("trigger-enabled"),
            )
            if (isOwner) {
                Box {
                    CircleIconButton(
                        ExpIcons.uiMore,
                        contentDescription = "Trigger options",
                        onClick = { menuOpen = true },
                        enabled = !busy,
                        modifier = Modifier.padding(start = 8.dp),
                        borderless = true,
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
        PromptAlert(
            prompt = Prompts.DeleteTrigger.prompt(),
            onDismiss = { confirmDelete = false },
            handlers = mapOf(
                "delete" to {
                    confirmDelete = false
                    onDelete()
                },
            ),
        )
    }
}

/** The row's agent pins, `agent · model` (web `action-triggers-section.tsx`
 * ×4, release train 2026-10-10 F44: the effort never rides); blank for an
 * unpinned trigger (no pin text). */
internal fun triggerLaunchLabel(trigger: ActionTrigger): String {
    val agent = trigger.agent
    if (agent.isNullOrEmpty()) return ""
    val model = trigger.model?.takeIf { it.isNotEmpty() }?.let(::modelLabel)
    return listOfNotNull(agentLabel(agent), model).joinToString(" · ")
}

private fun deviceDisplayLabel(device: SteerDevice?, deviceId: String): String {
    if (device == null) return deviceId
    val name = device.deviceLabel.ifBlank { device.deviceId }
    val owner = device.owner ?: return name
    return if (owner.name.isBlank()) name else "$name — ${owner.name}"
}

// ── Runs ────────────────────────────────────────────────────────────────────

@Composable
private fun RunsTab(
    runs: List<CodingSessionEntity>,
    devices: List<SteerDevice>,
    onOpenSteer: (codingSessionId: String) -> Unit,
) {
    if (runs.isEmpty()) {
        Box(
            modifier = Modifier.fillMaxSize().testTag("action-runs-tab"),
            contentAlignment = Alignment.TopStart,
        ) {
            Text(
                "No runs yet.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 20.dp),
            )
        }
        return
    }
    // EXP-1248: nested like the Agent page — a run started by another run
    // hangs off its parent with the tree guides, children always shown, the
    // BIG session row ×4 on a gapless list.
    val tree = remember(runs) { visibleSessionTreeRows(sessionTree(runs)) }
    val guides = remember(tree) { TreeGuides.compute(tree.map { it.depth }) }
    LazyColumn(
        modifier = Modifier.fillMaxSize().testTag("action-runs-tab"),
        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
    ) {
        itemsIndexed(tree, key = { _, entry -> entry.key }) { index, entry ->
            when (val node = entry.node) {
                is SessionTreeNode.Session -> {
                    val session = node.session
                    val steerDevice = steerDeviceOf(session, devices)
                    // Every row here ran THIS action, so its title says what
                    // started it ([actionRunTitle]) instead of the action's name.
                    CodingSessionRow(
                        session = session,
                        issue = null,
                        device = sessionDevice(session, devices),
                        size = SessionRowSize.Big,
                        titleOverride = actionRunTitle(session.startedReason),
                        depth = entry.depth,
                        guide = guides.getOrNull(index),
                        deviceIcon = steerDevice?.let { deviceIcon(it) } ?: ExpIcons.uiDevice,
                        onClick = { onOpenSteer(session.id) },
                    )
                }
            }
        }
    }
}

/** The steer device a run ran on: the owner's own row first. */
private fun steerDeviceOf(session: CodingSessionEntity, devices: List<SteerDevice>): SteerDevice? {
    val matches = session.deviceId?.let { id -> devices.filter { it.deviceId == id } }.orEmpty()
    return matches.firstOrNull { it.owner?.id == session.userId }
        ?: matches.firstOrNull { it.isMine } ?: matches.firstOrNull()
}

/** EXP-874: a run's host machine off the synced devices (steer `device_id`,
 *  the owner's own row first) — the stamped label and UNKNOWN presence when
 *  the row isn't visible, mirroring `resolveSessionDevice`. */
private fun sessionDevice(session: CodingSessionEntity, devices: List<SteerDevice>): SessionDevicePresentation {
    val row = steerDeviceOf(session, devices)
        ?: return SessionDevicePresentation(label = session.deviceLabel, online = null)
    return SessionDevicePresentation(
        label = row.deviceLabel.takeIf { it.isNotBlank() } ?: session.deviceLabel,
        online = row.online,
    )
}
