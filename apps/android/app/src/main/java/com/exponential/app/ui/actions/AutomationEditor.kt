package com.exponential.app.ui.actions

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.AutomationTriggerFilters
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.triggerEventLabel
import com.exponential.app.domain.triggerWeekdayName
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.CLI_DEFAULT_MODEL
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.modelValuesFor
import com.exponential.app.ui.issue.StartBoardOption
import com.exponential.app.ui.issue.StartFilterOption
import com.exponential.app.ui.theme.TextEmphasis

// The shared automation editor (EXP-583). TWO hosts render these fields: the
// Automations tab's "New automation" sheet (where a trigger is mandatory) and
// the create-action sheet's Automation block, which only appears when the flow
// came from an action+automation suggestion (the trigger arrives prefilled and
// the note the creator agent follows is built from the same draft). Keeping
// one draft + one set of rows is what stops the two from drifting.

/** The default schedule time, matching the web draft (09:00). */
private const val DEFAULT_MINUTE_OF_DAY = 540

/** Kind sentinel for "no automation" — only the suggestion host offers it. */
internal const val AUTOMATION_KIND_NONE = "none"
internal const val AUTOMATION_KIND_SCHEDULE = "schedule"
internal const val AUTOMATION_KIND_EVENT = "event"

/**
 * The editor state. A draft can represent things a valid trigger cannot (an
 * event kind with no machine bound yet), so [automationDraftToTrigger]
 * converts at submit and incomplete reads as null. The filter picks are
 * SINGLE-select with an "Any" default — the mobile simplification of the web's
 * multi-selects; the wire lists carry one-or-none entries from here.
 * [agent]/[model]/[effort] are empty for "the machine's own launch defaults".
 */
internal data class AutomationDraft(
    val kind: String = AUTOMATION_KIND_SCHEDULE,
    /** Steer deviceId of the bound machine; null = nothing picked yet. */
    val deviceId: String? = null,
    val interval: String = "daily",
    /** `HH:MM` wall-clock string, straight off the time field. */
    val time: String = "09:00",
    val weekday: Int = 1,
    val dayOfMonth: Int = 1,
    val event: String = "created",
    val boardId: String = "",
    val labelId: String = "",
    val priority: String = "",
    val toStatusId: String = "",
    val agent: String = "",
    val model: String = CLI_DEFAULT_MODEL,
    val effort: String = CLI_DEFAULT_EFFORT,
)

/** `HH:MM` → minutes past midnight; malformed/out-of-range reads 09:00 (the
 * web `timeToMinute` fallback). */
internal fun automationTimeToMinute(time: String): Int {
    val match = Regex("^(\\d{1,2}):(\\d{2})$").find(time.trim()) ?: return DEFAULT_MINUTE_OF_DAY
    val minute = match.groupValues[1].toInt() * 60 + match.groupValues[2].toInt()
    return if (minute in 0..1439) minute else DEFAULT_MINUTE_OF_DAY
}

/** Minutes past midnight → the `HH:MM` the time field renders. */
internal fun automationMinuteToTime(minuteOfDay: Int): String =
    String.format(java.util.Locale.ROOT, "%02d:%02d", minuteOfDay / 60, minuteOfDay % 60)

/**
 * A draft seeded from an existing/suggested [trigger] (the suggestion host);
 * null seeds the plain schedule default. [deviceId] pre-picks the machine.
 */
internal fun automationDraftFor(
    trigger: AutomationTrigger?,
    deviceId: String? = null,
): AutomationDraft = when (trigger) {
    null -> AutomationDraft(deviceId = deviceId)
    is AutomationTrigger.Schedule -> AutomationDraft(
        kind = AUTOMATION_KIND_SCHEDULE,
        deviceId = deviceId,
        interval = trigger.interval,
        time = automationMinuteToTime(trigger.minuteOfDay),
        weekday = trigger.weekday ?: 1,
        dayOfMonth = trigger.dayOfMonth ?: 1,
    )
    is AutomationTrigger.Event -> AutomationDraft(
        kind = AUTOMATION_KIND_EVENT,
        deviceId = deviceId,
        event = trigger.event,
        boardId = trigger.filters.boardIds.firstOrNull().orEmpty(),
        labelId = trigger.filters.labelIds.firstOrNull().orEmpty(),
        priority = trigger.filters.priorities.firstOrNull().orEmpty(),
        toStatusId = trigger.filters.toStatusIds.firstOrNull().orEmpty(),
    )
}

/**
 * The draft's when-part trigger, or null when the kind is "none". Filters
 * irrelevant to the picked event are dropped, matching the server's strict
 * write schema.
 */
internal fun automationDraftToTrigger(draft: AutomationDraft): AutomationTrigger? =
    when (draft.kind) {
        AUTOMATION_KIND_SCHEDULE -> AutomationTrigger.Schedule(
            interval = draft.interval,
            minuteOfDay = automationTimeToMinute(draft.time),
            weekday = draft.weekday.takeIf { draft.interval == "weekly" },
            dayOfMonth = draft.dayOfMonth.takeIf { draft.interval == "monthly" },
        )
        AUTOMATION_KIND_EVENT -> AutomationTrigger.Event(
            event = draft.event,
            filters = AutomationTriggerFilters(
                boardIds = listOfNotNull(draft.boardId.takeIf { it.isNotEmpty() }),
                labelIds = listOfNotNull(
                    draft.labelId.takeIf { it.isNotEmpty() && draft.event == "label_added" },
                ),
                priorities = listOfNotNull(
                    draft.priority.takeIf {
                        it.isNotEmpty() &&
                            (draft.event == "created" || draft.event == "priority_changed")
                    },
                ),
                toStatusIds = listOfNotNull(
                    draft.toStatusId.takeIf { it.isNotEmpty() && draft.event == "status_changed" },
                ),
            ),
        )
        else -> null
    }

private fun intervalLabel(interval: String): String = when (interval) {
    "weekly" -> "Week"
    "monthly" -> "Month"
    else -> "Day"
}

/**
 * The trigger half of the editor: the segmented kind switch (with a leading
 * "None" only when [allowNone]), the schedule pane (interval + contextual
 * weekday/day pickers + an HH:MM time field) and the event pane (event picker
 * + contextual filter pickers off the synced tables).
 */
@Composable
internal fun AutomationTriggerFields(
    draft: AutomationDraft,
    boards: List<StartBoardOption>,
    labels: List<StartFilterOption>,
    statuses: List<StartFilterOption>,
    onChange: (AutomationDraft) -> Unit,
    allowNone: Boolean = false,
) {
    GlassSegmentedControl(
        options = if (allowNone) {
            listOf(AUTOMATION_KIND_NONE, AUTOMATION_KIND_SCHEDULE, AUTOMATION_KIND_EVENT)
        } else {
            listOf(AUTOMATION_KIND_SCHEDULE, AUTOMATION_KIND_EVENT)
        },
        selected = draft.kind,
        label = {
            when (it) {
                AUTOMATION_KIND_SCHEDULE -> "Schedule"
                AUTOMATION_KIND_EVENT -> "On event"
                else -> "None"
            }
        },
        onSelect = { onChange(draft.copy(kind = it)) },
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
    )

    if (draft.kind == AUTOMATION_KIND_SCHEDULE) {
        Spacer(Modifier.height(4.dp))
        OptionGroup {
            PickerRow(
                label = "Every",
                value = intervalLabel(draft.interval),
                options = DomainContract.actionScheduleIntervalValues,
                selected = draft.interval,
                optionLabel = ::intervalLabel,
                onSelect = { onChange(draft.copy(interval = it)) },
            )
            if (draft.interval == "weekly") {
                GroupDivider()
                PickerRow(
                    label = "Weekday",
                    value = triggerWeekdayName(draft.weekday),
                    options = (1..7).map(Int::toString),
                    selected = draft.weekday.toString(),
                    optionLabel = { triggerWeekdayName(it.toInt()) },
                    onSelect = { onChange(draft.copy(weekday = it.toInt())) },
                )
            }
            if (draft.interval == "monthly") {
                GroupDivider()
                PickerRow(
                    label = "Day of month",
                    value = "Day ${draft.dayOfMonth}",
                    options = (1..28).map(Int::toString),
                    selected = draft.dayOfMonth.toString(),
                    optionLabel = { "Day $it" },
                    onSelect = { onChange(draft.copy(dayOfMonth = it.toInt())) },
                )
            }
            GroupDivider()
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "Time",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f),
                )
                GlassTextField(
                    value = draft.time,
                    onValueChange = { onChange(draft.copy(time = it.take(5))) },
                    modifier = Modifier.width(104.dp),
                    placeholder = "09:00",
                    singleLine = true,
                )
            }
        }
    } else if (draft.kind == AUTOMATION_KIND_EVENT) {
        Spacer(Modifier.height(4.dp))
        OptionGroup {
            PickerRow(
                label = "When",
                value = triggerEventLabel(draft.event),
                options = DomainContract.actionTriggerEventValues,
                selected = draft.event,
                optionLabel = ::triggerEventLabel,
                onSelect = { onChange(draft.copy(event = it)) },
            )
            GroupDivider()
            // Board filter applies to every event; the rest are contextual
            // (labels for label_added, priorities for created/priority
            // changes, target status for status_changed).
            AutomationFilterPicker(
                label = "Board",
                anyLabel = "Any board",
                options = boards.map { StartFilterOption(it.id, it.name) },
                selected = draft.boardId,
                onSelect = { onChange(draft.copy(boardId = it)) },
            )
            if (draft.event == "label_added") {
                GroupDivider()
                AutomationFilterPicker(
                    label = "Label",
                    anyLabel = "Any label",
                    options = labels,
                    selected = draft.labelId,
                    onSelect = { onChange(draft.copy(labelId = it)) },
                )
            }
            if (draft.event == "created" || draft.event == "priority_changed") {
                GroupDivider()
                AutomationFilterPicker(
                    label = "Priority",
                    anyLabel = "Any priority",
                    options = DomainContract.issuePriorityValues.map {
                        StartFilterOption(it, IssuePriority.fromWire(it).label)
                    },
                    selected = draft.priority,
                    onSelect = { onChange(draft.copy(priority = it)) },
                )
            }
            if (draft.event == "status_changed") {
                GroupDivider()
                AutomationFilterPicker(
                    label = "To status",
                    anyLabel = "Any status",
                    options = statuses,
                    selected = draft.toStatusId,
                    onSelect = { onChange(draft.copy(toStatusId = it)) },
                )
            }
        }
    }
}

/**
 * The binding half: which machine runs the automation, and the optional agent
 * pin with its model/effort. The pins are per-AGENT vocabularies server-side
 * (validated against `agent ?? claude`), so Model and Effort park while the
 * agent stays at the machine's own default — a run then simply uses whatever
 * that machine is configured for.
 */
@Composable
internal fun AutomationBindingFields(
    draft: AutomationDraft,
    devices: List<SteerDevice>,
    onChange: (AutomationDraft) -> Unit,
) {
    if (devices.isEmpty()) {
        OptionGroup {
            Text(
                "No machine can run automations yet. Open or update the Exponential " +
                    "desktop app on the machine that should run this.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
            )
        }
        return
    }
    val device = devices.firstOrNull { it.deviceId == draft.deviceId }
    val agents = device?.runnableAgents.orEmpty()
    val agentPinned = draft.agent.isNotEmpty()
    OptionGroup {
        PickerRow(
            label = "Runs on",
            value = device?.let(::automationDeviceLabel) ?: "Select",
            options = devices.map { it.deviceId },
            selected = draft.deviceId,
            optionLabel = { id ->
                devices.firstOrNull { it.deviceId == id }?.let(::automationDeviceLabel) ?: id
            },
            onSelect = { id ->
                // A machine that can't run the pinned agent drops the pin.
                val next = devices.firstOrNull { it.deviceId == id }
                val keepsAgent = draft.agent.isEmpty() ||
                    next?.runnableAgents.orEmpty().contains(draft.agent)
                onChange(
                    if (keepsAgent) {
                        draft.copy(deviceId = id)
                    } else {
                        draft.copy(
                            deviceId = id,
                            agent = "",
                            model = CLI_DEFAULT_MODEL,
                            effort = CLI_DEFAULT_EFFORT,
                        )
                    },
                )
            },
        )
        GroupDivider()
        PickerRow(
            label = "Agent",
            value = if (agentPinned) agentLabel(draft.agent) else "Device default",
            options = listOf("") + agents,
            selected = draft.agent,
            optionLabel = { if (it.isEmpty()) "Device default" else agentLabel(it) },
            enabled = agents.isNotEmpty(),
            onSelect = { next ->
                // The model/effort vocabularies are per agent — a switch has
                // to reset them, or a stale value hits a server refusal.
                onChange(
                    draft.copy(
                        agent = next,
                        model = CLI_DEFAULT_MODEL,
                        effort = CLI_DEFAULT_EFFORT,
                    ),
                )
            },
        )
        GroupDivider()
        PickerRow(
            label = "Model",
            value = if (agentPinned) modelLabel(draft.model) else "Device default",
            options = listOf(CLI_DEFAULT_MODEL) + modelValuesFor(draft.agent),
            selected = draft.model,
            optionLabel = { if (it.isEmpty()) "Device default" else modelLabel(it) },
            enabled = agentPinned,
            onSelect = { onChange(draft.copy(model = it)) },
        )
        GroupDivider()
        PickerRow(
            label = when (draft.agent) {
                "codex" -> "Reasoning"
                "pi" -> "Thinking"
                else -> "Effort"
            },
            value = if (agentPinned) effortLabel(draft.effort) else "Device default",
            options = listOf(CLI_DEFAULT_EFFORT) + effortValuesFor(draft.agent),
            selected = draft.effort,
            optionLabel = { if (it.isEmpty()) "Device default" else effortLabel(it) },
            enabled = agentPinned,
            onSelect = { onChange(draft.copy(effort = it)) },
        )
    }
}

/** How a machine reads in the automation pickers — the owner disambiguates a
 * teammate's shared server from a similarly named own box. */
internal fun automationDeviceLabel(device: SteerDevice): String {
    val base = device.deviceLabel.ifBlank { device.deviceId }
    return device.owner?.let { "$base — ${it.name}" } ?: base
}

// One "Any"-defaulted single-select filter row: the empty value is the
// no-filter state and stays re-pickable as the first option.
@Composable
private fun AutomationFilterPicker(
    label: String,
    anyLabel: String,
    options: List<StartFilterOption>,
    selected: String,
    onSelect: (String) -> Unit,
) {
    PickerRow(
        label = label,
        value = options.firstOrNull { it.id == selected }?.name ?: anyLabel,
        options = listOf("") + options.map { it.id },
        selected = selected,
        optionLabel = { id ->
            if (id.isEmpty()) anyLabel else options.firstOrNull { it.id == id }?.name ?: id
        },
        onSelect = onSelect,
    )
}
