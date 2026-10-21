package com.exponential.app.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.UpdateAgentPatch
import com.exponential.app.domain.AccountOptions
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.AgentTrigger
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.actions.TriggerFormSheet
import com.exponential.app.ui.actions.triggerGlyph
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.ColorPicker
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassSwitch
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.accountOptionsFor
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.components.picker.AccountPicker
import com.exponential.app.ui.components.picker.AccountPickerOption
import com.exponential.app.ui.components.picker.DevicePicker
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.components.PickerValueRow
import com.exponential.app.ui.components.toPickerAccount
import com.exponential.app.ui.components.toPickerDevice
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// EXP-1296 (Threads v2, `agent-settings.json`): Team settings → Agents,
// managed EXACTLY like boards. The list = icon-in-colour + name rows under the
// "Agents" band, then `New agent` (owner-only, like `New board`). An agent's
// page = the board form's ONE identity field (icon picker + colour swatch +
// name) over Device, Account, Triggers, Instructions and the owner-only
// Delete. A NEW agent is a form with a Create button
// (`sessions.send({agent})`); an existing one autosaves like the board page:
// name and instructions on blur, everything else at once.

private const val CREATE_AGENT_LABEL = "Create agent"
private const val OPEN_CONVERSATION = "Open conversation"

/** The agent's glyph, tinted in its colour (the board row's [BoardIcon]). */
@Composable
internal fun AgentGlyph(icon: String, color: String, modifier: Modifier = Modifier) {
    Icon(
        ExpIcons.byName(icon) ?: ExpIcons.byName(SessionTree.ListCopy.AGENT_DEFAULT_ICON) ?: ExpIcons.settingsAgents,
        contentDescription = null,
        tint = remember(color) { parseColor(color) },
        modifier = modifier.size(16.dp),
    )
}

/**
 * Team settings → Agents, the LIST: one row per agent of the selected team
 * then `New agent` (owner-only). Embeddable (a settings section) — no
 * Scaffold of its own.
 */
@Composable
fun AgentsSettingsList(
    onOpenAgent: (agentId: String) -> Unit,
    onNewAgent: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: AgentSettingsViewModel = hiltViewModel(),
) {
    val agents by viewModel.agents.collectAsStateWithLifecycle()
    val isOwner by viewModel.isTeamOwner.collectAsStateWithLifecycle()
    AgentsSettingsListContent(
        agents = agents,
        canCreate = isOwner,
        onOpenAgent = onOpenAgent,
        onNewAgent = onNewAgent,
        modifier = modifier,
    )
}

/** The stateless list. */
@Composable
internal fun AgentsSettingsListContent(
    agents: List<SessionTree.AgentRow>,
    canCreate: Boolean,
    onOpenAgent: (agentId: String) -> Unit,
    onNewAgent: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.testTag(AgentSettings.TEST_ID_LIST),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        SectionHeader(AgentSettings.Copy.TITLE)
        agents.forEach { row ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SessionTree.ListCopy.TEST_ID_AGENT_ROW)
                    .flatRow()
                    .clickable { onOpenAgent(row.agentId) }
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                AgentGlyph(row.icon, row.color)
                Text(
                    row.name,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
            }
        }
        if (canCreate) {
            val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("agents-settings-new")
                    .flatRow()
                    .clickable(onClick = onNewAgent)
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                Icon(ExpIcons.uiAdd, contentDescription = null, tint = muted, modifier = Modifier.size(16.dp))
                Text(AgentSettings.Copy.NEW_AGENT, color = muted)
            }
        }
    }
}

/**
 * The Agents settings PAGE (`settings/agents`) — a pushed screen hosting
 * [AgentsSettingsList].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentsSettingsScreen(
    onBack: () -> Unit,
    onOpenAgent: (agentId: String) -> Unit,
    onNewAgent: () -> Unit,
) {
    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(AgentSettings.Copy.TITLE) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        AgentsSettingsList(
            onOpenAgent = onOpenAgent,
            onNewAgent = onNewAgent,
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
        )
    }
}

/**
 * ONE agent's settings page (`settings/agent/{agentId}`; the
 * `settings/agent-new` route = `New agent`, the model's `agentId` null).
 * [onOpenChannel] opens the caller's conversation with the agent (when one
 * exists); [onCreated] lands on a just-created agent's page; [onDeleted]
 * leaves the page.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentSettingsScreen(
    onBack: () -> Unit,
    onOpenChannel: (conversationSessionId: String) -> Unit,
    onCreated: (agentId: String) -> Unit,
    onDeleted: () -> Unit,
    viewModel: AgentSettingsViewModel = hiltViewModel(),
) {
    val agent by viewModel.agent.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val loadedInstructions by viewModel.instructions.collectAsStateWithLifecycle()
    val syncedTriggers by viewModel.triggers.collectAsStateWithLifecycle()
    val isOwner by viewModel.isTeamOwner.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val conversationId by viewModel.conversationId.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)

    val isNew = viewModel.agentId == null
    // The form's own copy of the row, seeded ONCE (an Electric echo must
    // never fight a person mid-edit).
    var name by remember { mutableStateOf("") }
    var icon by remember { mutableStateOf(SessionTree.ListCopy.AGENT_DEFAULT_ICON) }
    var color by remember { mutableStateOf(SessionTree.ListCopy.AGENT_DEFAULT_COLOR) }
    var deviceId by remember { mutableStateOf<String?>(null) }
    var accountKey by remember { mutableStateOf<String?>(null) }
    var instructions by remember { mutableStateOf("") }
    var instructionsSeeded by remember { mutableStateOf(false) }
    var seeded by remember { mutableStateOf(false) }
    // A new agent's triggers live here until Create sends it.
    var draftTriggers by remember { mutableStateOf<List<AgentTrigger>>(emptyList()) }
    var showErrors by remember { mutableStateOf(false) }

    LaunchedEffect(agent) {
        val row = agent ?: return@LaunchedEffect
        if (seeded) return@LaunchedEffect
        seeded = true
        name = row.name
        icon = row.icon ?: SessionTree.ListCopy.AGENT_DEFAULT_ICON
        color = row.color ?: SessionTree.ListCopy.AGENT_DEFAULT_COLOR
        deviceId = row.deviceId
        accountKey = row.account?.let { account -> row.cli?.let { "$it:$account" } }
    }
    LaunchedEffect(loadedInstructions) {
        val text = loadedInstructions ?: return@LaunchedEffect
        if (instructionsSeeded) return@LaunchedEffect
        instructionsSeeded = true
        instructions = text
    }

    val deviceList = devices.orEmpty()
    val device = deviceList.firstOrNull { it.deviceId == deviceId }
    val accountOptions = remember(device) { accountOptionsFor(device, device?.acpAgents.orEmpty()) }
    // EXP-1306: the Account row's value = `agentAccountValue` over the pin
    // (`<cli>:<profile>` while editing, the synced row's cli otherwise).
    val accountValueKey = remember(accountOptions, accountKey, agent?.cli) {
        val parsed = accountKey?.let(AccountOptions::parseKey)
        AgentSettings.agentAccountValue(
            options = accountOptions.map { AgentSettings.AccountValueOption(it.id, it.agent, it.email, it.isLastUsed) },
            account = parsed?.id,
            cli = parsed?.agent ?: agent?.cli,
        )?.let { "${it.agent}:${it.id}" }
    }
    val triggers = if (isNew) draftTriggers else syncedTriggers
    val errors = viewModel.validate(name, deviceId, instructions)
    val editable = isOwner

    // The Triggers sheet: null = closed; -1 = a new trigger, else its index.
    var triggerSheet by remember { mutableStateOf<Int?>(null) }
    var confirmDelete by remember { mutableStateOf(false) }

    fun patch(update: UpdateAgentPatch) {
        if (!isNew) viewModel.update(update)
    }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = {
                    Text(
                        if (isNew) AgentSettings.Copy.NEW_AGENT else agent?.name ?: name,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                actions = {
                    val id = conversationId
                    if (id != null) {
                        CircleIconButton(
                            ExpIcons.actionChat,
                            contentDescription = OPEN_CONVERSATION,
                            onClick = { onOpenChannel(id) },
                            modifier = Modifier.padding(end = 8.dp).testTag("agent-settings-open-conversation"),
                            borderless = true,
                        )
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        AgentSettingsContent(
            isNew = isNew,
            editable = editable,
            name = name,
            onNameChange = { name = it },
            onNameCommit = {
                val trimmed = name.trim()
                if (errors.none { it.field == AgentSettings.FIELD_IDENTITY } && trimmed != agent?.name) {
                    patch(UpdateAgentPatch(name = trimmed))
                } else if (!isNew) {
                    showErrors = true
                }
            },
            icon = icon,
            onIconChange = {
                icon = it
                patch(UpdateAgentPatch(icon = it))
            },
            color = color,
            onColorChange = {
                color = it
                patch(UpdateAgentPatch(color = it))
            },
            devices = deviceList.map { it.toPickerDevice(startGate = false) },
            deviceId = deviceId,
            onDeviceChange = {
                if (it != deviceId) {
                    deviceId = it
                    accountKey = null
                    patch(UpdateAgentPatch(deviceId = it))
                }
            },
            accounts = accountOptions.map { it.toPickerAccount() },
            accountValueKey = accountValueKey,
            onAccountChange = { key ->
                accountKey = key
                val parsed = AccountOptions.parseKey(key)
                if (parsed != null) {
                    patch(
                        if (parsed.id.isEmpty()) UpdateAgentPatch(clearAccount = true)
                        else UpdateAgentPatch(account = parsed.id),
                    )
                }
            },
            triggers = triggers,
            onToggleTrigger = { index, enabled ->
                if (isNew) {
                    draftTriggers = draftTriggers.mapIndexed { i, t -> if (i == index) t.copy(enabled = enabled) else t }
                } else {
                    triggers.getOrNull(index)?.id?.let { viewModel.setTriggerEnabled(it, enabled) }
                }
            },
            onEditTrigger = { triggerSheet = it },
            onAddTrigger = { triggerSheet = -1 },
            instructions = instructions,
            instructionsLoading = !isNew && loadedInstructions == null,
            onInstructionsChange = { instructions = it },
            onInstructionsCommit = {
                if (!isNew && instructionsSeeded && instructions != loadedInstructions &&
                    errors.none { it.field == AgentSettings.FIELD_INSTRUCTIONS }
                ) {
                    patch(UpdateAgentPatch(instructions = instructions))
                }
            },
            errors = if (showErrors) errors else emptyList(),
            busy = busy,
            onCreate = if (isNew) {
                {
                    showErrors = true
                    val picked = deviceId
                    if (errors.isEmpty() && picked != null) {
                        val parsed = accountKey?.let(AccountOptions::parseKey)
                        viewModel.create(
                            name = name,
                            icon = icon,
                            color = color,
                            deviceId = picked,
                            account = parsed?.id?.takeIf { it.isNotEmpty() },
                            cli = parsed?.agent,
                            instructions = instructions,
                            triggers = draftTriggers,
                            onCreated = onCreated,
                        )
                    }
                }
            } else {
                null
            },
            onDelete = if (!isNew && isOwner) ({ confirmDelete = true }) else null,
            modifier = Modifier
                .padding(padding)
                .consumeWindowInsets(padding)
                .imePadding()
                .fillMaxSize(),
        )
    }

    triggerSheet?.let { index ->
        val editing = triggers.getOrNull(index)
        TriggerFormSheet(
            busy = busy,
            editing = editing,
            onSubmit = { trigger ->
                if (isNew) {
                    draftTriggers = if (editing == null) draftTriggers + trigger
                    else draftTriggers.mapIndexed { i, t -> if (i == index) trigger else t }
                    triggerSheet = null
                } else {
                    viewModel.saveTrigger(trigger) { triggerSheet = null }
                }
            },
            onDelete = if (editing != null && editable) {
                {
                    if (isNew) {
                        draftTriggers = draftTriggers.filterIndexed { i, _ -> i != index }
                    } else {
                        editing.id?.let(viewModel::deleteTrigger)
                    }
                    triggerSheet = null
                }
            } else {
                null
            },
            onDismiss = { triggerSheet = null },
        )
    }

    if (confirmDelete) {
        GlassAlert(
            title = AgentSettings.Copy.DELETE_CONFIRM,
            onDismiss = { confirmDelete = false },
            trailing = listOf(
                GlassAlertAction("Cancel", onClick = { confirmDelete = false }),
                GlassAlertAction(
                    "Delete",
                    onClick = {
                        confirmDelete = false
                        viewModel.delete(onDeleted)
                    },
                    destructive = true,
                    testTag = "agent-settings-delete-confirm",
                ),
            ),
        )
    }
}

/**
 * The stateless agent page, [AgentSettings.FIELDS] top to bottom. [onCreate] non-null = the
 * `New agent` form's submit; [onDelete] non-null = the owner's Delete.
 */
@Composable
internal fun AgentSettingsContent(
    isNew: Boolean,
    editable: Boolean,
    name: String,
    onNameChange: (String) -> Unit,
    onNameCommit: () -> Unit,
    icon: String,
    onIconChange: (String) -> Unit,
    color: String,
    onColorChange: (String) -> Unit,
    devices: List<DevicePickerDevice>,
    deviceId: String?,
    onDeviceChange: (String) -> Unit,
    accounts: List<AccountPickerOption>,
    /** The option the Account row reads ([AgentSettings.agentAccountValue]'s
     *  pick, as an option key); null = no login reported. */
    accountValueKey: String?,
    onAccountChange: (String) -> Unit,
    triggers: List<AgentTrigger>,
    onToggleTrigger: (index: Int, enabled: Boolean) -> Unit,
    onEditTrigger: (index: Int) -> Unit,
    onAddTrigger: () -> Unit,
    instructions: String,
    instructionsLoading: Boolean,
    onInstructionsChange: (String) -> Unit,
    onInstructionsCommit: () -> Unit,
    errors: List<AgentSettings.FormError>,
    busy: Boolean,
    onCreate: (() -> Unit)?,
    onDelete: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    fun errorOf(field: String) = errors.firstOrNull { it.field == field }?.message
    fun label(field: String) = AgentSettings.FIELDS.first { it.key == field }.label
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val device = devices.firstOrNull { it.id == deviceId }
    val account = accountValueKey?.let { key -> accounts.firstOrNull { it.key == key } }

    Column(
        modifier = modifier
            .testTag(AgentSettings.TEST_ID_PAGE)
            .verticalScroll(rememberScrollState())
            .padding(vertical = 8.dp),
    ) {
        // ── identity + Device ────────────────────────────────────────────
        OptionGroup {
            // The board form's ONE identity field: icon picker, colour
            // swatch and the name, all one row.
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth().padding(start = 12.dp, top = 6.dp, bottom = 6.dp),
            ) {
                if (editable) {
                    IconPicker(selected = icon, onSelect = onIconChange, accentColor = parseColor(color))
                    ColorPicker(selected = color, onSelect = onColorChange)
                } else {
                    Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { AgentGlyph(icon, color) }
                }
                // No caption: the glyph + colour ARE the label (web/iOS/desktop).
                GlassTextField(
                    value = name,
                    onValueChange = onNameChange,
                    placeholder = AgentSettings.Copy.NAME_PLACEHOLDER,
                    enabled = editable,
                    singleLine = true,
                    bordered = false,
                    containerColor = Color.Transparent,
                    modifier = Modifier
                        .weight(1f)
                        .testTag("agent-settings-name")
                        .commitOnBlur { if (!isNew) onNameCommit() },
                )
            }
            GroupDivider()
            DevicePicker(
                devices = devices,
                value = deviceId,
                onChange = onDeviceChange,
                title = label(AgentSettings.FIELD_DEVICE),
                trigger = { open ->
                    PickerValueRow(
                        label = label(AgentSettings.FIELD_DEVICE),
                        value = device?.name ?: "Select",
                        valueIcon = device?.let { deviceIcon(it.icon, it.isServer) },
                        onClick = open,
                        enabled = editable,
                        modifier = Modifier.testTag("agent-settings-device"),
                    )
                },
            )
        }
        FieldErrors(listOfNotNull(errorOf(AgentSettings.FIELD_IDENTITY), errorOf(AgentSettings.FIELD_DEVICE)))
        Spacer(Modifier.height(12.dp))

        // ── Account + Triggers ───────────────────────────────────────────
        OptionGroup {
            AccountPicker(
                options = accounts,
                value = account?.key,
                onChange = onAccountChange,
                trigger = { open ->
                    // EXP-1306: the composer chip's value — the brand mark + the
                    // email; no login reported = `Last used`, muted.
                    PickerValueRow(
                        label = label(AgentSettings.FIELD_ACCOUNT),
                        value = account?.email ?: AgentSettings.Copy.ACCOUNT_LAST_USED,
                        valueAgent = account?.agent,
                        onClick = open,
                        enabled = editable && accounts.isNotEmpty(),
                        modifier = Modifier.testTag("agent-settings-account"),
                    )
                },
            )
            GroupDivider()
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 12.dp, top = 10.dp, bottom = 10.dp),
            ) {
                Text(
                    label(AgentSettings.FIELD_TRIGGERS),
                    style = MaterialTheme.typography.bodyLarge,
                    modifier = Modifier.weight(1f),
                )
                if (triggers.isEmpty()) {
                    Text(
                        AgentSettings.Copy.NO_SCHEDULE,
                        style = MaterialTheme.typography.bodyMedium,
                        color = muted,
                        modifier = Modifier.padding(end = 8.dp),
                    )
                }
                if (editable) {
                    GlassPill(
                        AgentSettings.Copy.ADD_TRIGGER,
                        icon = ExpIcons.uiAdd,
                        size = PillSize.Sm,
                        onClick = onAddTrigger,
                        modifier = Modifier.testTag("agent-settings-add-trigger"),
                    )
                }
            }
            triggers.forEachIndexed { index, trigger ->
                GroupDivider()
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("agent-settings-trigger")
                        .clickable(enabled = editable) { onEditTrigger(index) }
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                ) {
                    Icon(triggerGlyph(trigger.whenPart), contentDescription = null, tint = muted, modifier = Modifier.size(16.dp))
                    Text(
                        AgentSettings.triggerSummary(trigger),
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.weight(1f),
                    )
                    GlassSwitch(
                        checked = trigger.enabled,
                        onCheckedChange = if (editable) ({ onToggleTrigger(index, it) }) else null,
                        enabled = editable,
                    )
                }
            }
        }
        Spacer(Modifier.height(12.dp))

        // ── Instructions (markdown, server-only) ─────────────────────────
        OptionGroup {
            GlassTextField(
                value = instructions,
                onValueChange = onInstructionsChange,
                placeholder = if (instructionsLoading) null else AgentSettings.Copy.INSTRUCTIONS_PLACEHOLDER,
                minLines = 5,
                enabled = editable && !instructionsLoading,
                bordered = false,
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("agent-settings-instructions")
                    .commitOnBlur(onInstructionsCommit),
            )
        }
        Text(
            label(AgentSettings.FIELD_INSTRUCTIONS),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            modifier = Modifier.padding(start = 20.dp, top = 6.dp),
        )
        FieldErrors(listOfNotNull(errorOf(AgentSettings.FIELD_INSTRUCTIONS)))
        Spacer(Modifier.height(20.dp))

        if (onCreate != null) {
            GlassSubmitButton(
                label = CREATE_AGENT_LABEL,
                enabled = editable && !busy,
                onClick = onCreate,
                modifier = Modifier.padding(horizontal = 16.dp).testTag("agent-settings-create"),
            )
        }
        if (onDelete != null) {
            GlassPill(
                label(AgentSettings.FIELD_DELETE),
                icon = ExpIcons.uiDelete,
                onClick = onDelete,
                contentColor = DesignTokens.Semantic.Red,
                modifier = Modifier.padding(horizontal = 16.dp).fillMaxWidth().testTag("agent-settings-delete"),
            )
        }
        Spacer(Modifier.height(24.dp))
    }
}

/** [onBlur] fires when focus LEAVES the field (never on the first,
 *  unfocused report), the board page's save-on-blur. */
@Composable
private fun Modifier.commitOnBlur(onBlur: () -> Unit): Modifier {
    var hadFocus by remember { mutableStateOf(false) }
    return onFocusChanged { state ->
        if (hadFocus && !state.hasFocus) onBlur()
        hadFocus = state.hasFocus
    }
}

@Composable
private fun FieldErrors(messages: List<String>) {
    messages.forEach { FieldError(it, modifier = Modifier.padding(start = 20.dp, top = 6.dp)) }
}
