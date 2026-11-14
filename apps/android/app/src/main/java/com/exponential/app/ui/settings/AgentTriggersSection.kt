package com.exponential.app.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.AgentTrigger
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.apps.ArgsStore
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSwitch
import com.exponential.app.ui.components.GlassSwitchSize
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerValueRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

// #EXP-1846 (agent-settings.json `page` › `triggers`, `checkIn`): THE trigger
// list of an agent, ONE piece the agent page (the Assistant and every team
// agent) and an app's Events tab (#EXP-1829) mount (web `AgentTriggers`, iOS
// `AgentTriggersSection`, desktop `settings::agent_triggers`): the Triggers
// header with its `+ Add` pill (editors), then one group: the Check in row
// (THE single Picker over `assistant.schedules`, `checkInOf` / `withCheckIn`)
// and a row per `listedTriggers` (sentence · Switch · ⋯ Edit/Delete; the row
// opens the editor). Every write is the WHOLE array, each trigger kept as
// stored (its `preset` included).

/** A whole-array trigger write: [build] maps the current triggers to the
 *  next; [onDone] runs once it landed (closes the editor). */
typealias AgentTriggersWrite = (onDone: () -> Unit, build: (List<AgentTrigger>) -> List<AgentTrigger>) -> Unit

/**
 * The trigger list of agent [agentId] on its own (an app's Events tab):
 * the agent page's view model scoped to [agentId], its failures toasted.
 * [editable] = the agent's editors (a team agent's owners, an Assistant's
 * member); members read every row.
 */
@Composable
fun AgentTriggersSection(agentId: String, editable: Boolean, modifier: Modifier = Modifier) {
    ArgsStore("agentId", agentId) {
        val viewModel: AgentSettingsViewModel = hiltViewModel()
        val triggers by viewModel.triggers.collectAsStateWithLifecycle()
        val busy by viewModel.busy.collectAsStateWithLifecycle()
        val failure by viewModel.failure.collectAsStateWithLifecycle()
        FailedActionToast(failure, onShown = viewModel::clearFailure)
        AgentTriggersSection(
            agentId = agentId,
            editable = editable,
            triggers = triggers,
            busy = busy,
            onWrite = viewModel::writeTriggers,
            modifier = modifier,
        )
    }
}

/**
 * The trigger list over triggers the caller holds ([agentId] null = a new
 * agent's draft): [onWrite] persists (or keeps) the next whole array.
 */
@Composable
fun AgentTriggersSection(
    agentId: String?,
    editable: Boolean,
    triggers: List<AgentTrigger>,
    busy: Boolean,
    onWrite: AgentTriggersWrite,
    modifier: Modifier = Modifier,
) {
    // The editor: null = closed; [TriggerSheet.editing] null = a new trigger.
    var sheet by remember { mutableStateOf<TriggerSheet?>(null) }
    var deleting by remember { mutableStateOf<AgentTrigger?>(null) }
    val checkIn = AgentSettings.checkInOf(triggers)
    val listed = AgentSettings.listedTriggers(triggers)

    Column(modifier = modifier.testTag(agentId?.let { "agent-triggers-$it" } ?: "agent-triggers")) {
        SectionHeader(AgentSettings.Copy.TRIGGERS, modifier = Modifier.padding(horizontal = 16.dp)) {
            if (editable) {
                GlassPill(
                    AgentSettings.Copy.ADD_TRIGGER,
                    icon = ExpIcons.uiAdd,
                    size = PillSize.Sm,
                    onClick = { sheet = TriggerSheet(null) },
                    modifier = Modifier.testTag("agent-settings-add-trigger"),
                )
            }
        }
        OptionGroup {
            Picker(
                items = AgentSettings.Assistant.SCHEDULES.map { PickerItem(it.id, it.label) },
                mode = PickerMode.Single,
                value = setOf(checkIn),
                onChange = { picked ->
                    picked.firstOrNull()?.takeIf { it != checkIn }?.let { id ->
                        // A new Check in takes the device's zone; a kept one its own.
                        val tz = java.util.TimeZone.getDefault().id
                        onWrite({}) { current -> AgentSettings.withCheckIn(current, id, tz) }
                    }
                },
                title = AgentSettings.Copy.CHECK_IN,
                enabled = editable,
            ) { open ->
                PickerValueRow(
                    label = AgentSettings.Copy.CHECK_IN,
                    value = AgentSettings.Assistant.SCHEDULES.firstOrNull { it.id == checkIn }?.label
                        ?: AgentSettings.Assistant.SCHEDULES.first().label,
                    onClick = open,
                    enabled = editable,
                    modifier = Modifier.testTag("agent-check-in"),
                )
            }
            listed.forEach { trigger ->
                GroupDivider()
                AgentTriggerRow(
                    trigger = trigger,
                    editable = editable,
                    onToggle = { enabled ->
                        onWrite({}) { current -> current.replacing(trigger, trigger.copy(enabled = enabled)) }
                    },
                    onEdit = { sheet = TriggerSheet(trigger) },
                    onDelete = { deleting = trigger },
                )
            }
        }
    }

    sheet?.let { open ->
        val editing = open.editing
        TriggerFormSheet(
            busy = busy,
            editing = editing,
            onSubmit = { trigger ->
                onWrite({ sheet = null }) { current ->
                    if (editing == null) current + trigger else current.replacing(editing, trigger)
                }
            },
            onDelete = if (editing != null && editable) {
                { onWrite({ sheet = null }) { current -> current.replacing(editing, null) } }
            } else {
                null
            },
            onDismiss = { sheet = null },
        )
    }

    deleting?.let { target ->
        val prompt = Prompts.DeleteTrigger.prompt()
        PromptAlert(
            prompt = prompt,
            onDismiss = { deleting = null },
            handlers = prompt.actions
                .filter { it.role != Prompts.Role.Cancel }
                .associate { action ->
                    action.id to {
                        deleting = null
                        onWrite({}) { current -> current.replacing(target, null) }
                    }
                },
        )
    }
}

private data class TriggerSheet(val editing: AgentTrigger?)

/** [target] replaced by [with] (null = removed): by id when it has one,
 *  else (a new agent's draft) the first equal trigger. */
private fun List<AgentTrigger>.replacing(target: AgentTrigger, with: AgentTrigger?): List<AgentTrigger> {
    val index = if (target.id != null) indexOfFirst { it.id == target.id } else indexOf(target)
    if (index < 0) return this
    return toMutableList().apply { if (with == null) removeAt(index) else set(index, with) }
}

/** One Triggers row (web `AgentTriggerRow`): its sentence (muted while
 *  paused), the switch and the editors' ⋯ (Edit, Delete); a tap edits. */
@Composable
private fun AgentTriggerRow(
    trigger: AgentTrigger,
    editable: Boolean,
    onToggle: (Boolean) -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    // The switch says paused; the sentence stays the trigger's own.
    val sentence = AgentSettings.triggerSummary(trigger.copy(enabled = true))
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier
            .fillMaxWidth()
            .testTag("agent-settings-trigger")
            .clickable(enabled = editable, onClick = onEdit)
            .padding(start = 16.dp, end = 8.dp, top = 6.dp, bottom = 6.dp),
    ) {
        Text(
            sentence,
            style = MaterialTheme.typography.bodyMedium,
            color = if (trigger.enabled) MaterialTheme.colorScheme.onSurface else muted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        GlassSwitch(
            checked = trigger.enabled,
            onCheckedChange = if (editable) onToggle else null,
            enabled = editable,
            size = GlassSwitchSize.Compact,
        )
        if (editable) {
            var menu by remember { mutableStateOf(false) }
            Box {
                CircleIconButton(
                    ExpIcons.uiMore,
                    contentDescription = "Trigger menu for $sentence",
                    onClick = { menu = true },
                    glyphSize = 16.dp,
                    borderless = true,
                    size = 32.dp,
                )
                GlassDropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    GlassMenuItem(
                        leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                        text = { Text("Edit") },
                        onClick = {
                            menu = false
                            onEdit()
                        },
                    )
                    GlassMenuItem(
                        leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                        text = { Text("Delete") },
                        destructive = true,
                        onClick = {
                            menu = false
                            onDelete()
                        },
                    )
                }
            }
        }
    }
}
