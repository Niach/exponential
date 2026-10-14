package com.exponential.app.ui.actions

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.AccountOptions
import com.exponential.app.domain.TriggerWhen
import com.exponential.app.domain.TriggerFilters
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.triggerEventLabel
import com.exponential.app.domain.triggerWeekdayName
import com.exponential.app.ui.components.PickerValueRow
import com.exponential.app.ui.components.picker.AccountPicker
import com.exponential.app.ui.components.picker.AccountPickerPillTrigger
import com.exponential.app.ui.components.picker.BoardPickerBoard
import com.exponential.app.ui.components.picker.LabelPickerLabel
import com.exponential.app.ui.components.picker.StatusPickerStatus
import com.exponential.app.ui.components.picker.boardPickerItems
import com.exponential.app.ui.components.picker.labelPickerItems
import com.exponential.app.ui.components.picker.statusPickerItems
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.CLI_DEFAULT_MODEL
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.LaunchOptionsSection
import com.exponential.app.ui.components.LaunchOptionsVariant
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.accountOptionsFor
import com.exponential.app.ui.components.unpinnedAccountOptions
import com.exponential.app.ui.components.availableAgentsFor
import com.exponential.app.ui.components.toPickerAccount
import com.exponential.app.ui.components.defaultAgentFor
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.agent.StartBoardOption
import com.exponential.app.ui.theme.TextEmphasis

// The trigger editor's fields (EXP-583; EXP-1291: triggers live on the team
// AGENT, whose device/account/model are the runner): ONE draft and its
// when-part card (Schedule | On event), rendered by the agent page's
// Schedule "Add" / row tap sheet ([TriggerFormSheet]).

/** The default schedule time, matching the web draft (09:00). */
private const val DEFAULT_MINUTE_OF_DAY = 540

internal const val TRIGGER_KIND_SCHEDULE = "schedule"
internal const val TRIGGER_KIND_EVENT = "event"

/**
 * The editor state; [triggerDraftToWhen] converts at submit (an unknown kind
 * reads as null). The filter picks are SINGLE-select with an "Any" default —
 * the mobile simplification of the web's multi-selects; the wire lists carry
 * one-or-none entries from here.
 */
internal data class TriggerDraft(
    val kind: String = TRIGGER_KIND_SCHEDULE,
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
)

/** `HH:MM` → minutes past midnight; malformed/out-of-range reads 09:00 (the
 * web `timeToMinute` fallback). */
internal fun triggerTimeToMinute(time: String): Int {
    val match = Regex("^(\\d{1,2}):(\\d{2})$").find(time.trim()) ?: return DEFAULT_MINUTE_OF_DAY
    val minute = match.groupValues[1].toInt() * 60 + match.groupValues[2].toInt()
    return if (minute in 0..1439) minute else DEFAULT_MINUTE_OF_DAY
}

/** Minutes past midnight → the `HH:MM` the time field renders. */
internal fun triggerMinuteToTime(minuteOfDay: Int): String =
    String.format(java.util.Locale.ROOT, "%02d:%02d", minuteOfDay / 60, minuteOfDay % 60)

/** A draft seeded from an existing [trigger]; null seeds the plain schedule
 *  default. */
internal fun triggerDraftFor(trigger: TriggerWhen?): TriggerDraft = when (trigger) {
    null -> TriggerDraft()
    is TriggerWhen.Schedule -> TriggerDraft(
        kind = TRIGGER_KIND_SCHEDULE,
        interval = trigger.interval,
        time = triggerMinuteToTime(trigger.minuteOfDay),
        weekday = trigger.weekday ?: 1,
        dayOfMonth = trigger.dayOfMonth ?: 1,
    )
    is TriggerWhen.Event -> TriggerDraft(
        kind = TRIGGER_KIND_EVENT,
        event = trigger.event,
        boardId = trigger.filters.boardIds.firstOrNull().orEmpty(),
        labelId = trigger.filters.labelIds.firstOrNull().orEmpty(),
        priority = trigger.filters.priorities.firstOrNull().orEmpty(),
        toStatusId = trigger.filters.toStatusIds.firstOrNull().orEmpty(),
    )
}

/**
 * The draft's when-part, or null for a kind this build cannot write. Filters
 * irrelevant to the picked event are dropped, matching the server's strict
 * write schema.
 */
internal fun triggerDraftToWhen(draft: TriggerDraft): TriggerWhen? =
    when (draft.kind) {
        TRIGGER_KIND_SCHEDULE -> TriggerWhen.Schedule(
            interval = draft.interval,
            minuteOfDay = triggerTimeToMinute(draft.time),
            weekday = draft.weekday.takeIf { draft.interval == "weekly" },
            dayOfMonth = draft.dayOfMonth.takeIf { draft.interval == "monthly" },
        )
        TRIGGER_KIND_EVENT -> TriggerWhen.Event(
            event = draft.event,
            filters = TriggerFilters(
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
 * The when-part half of the editor, as ONE grouped card (EXP-698): the
 * segmented kind switch is its first row, and under a hairline come the picked
 * kind's rows — the schedule pane (interval + contextual weekday/day pickers +
 * the wall-clock row) or the event pane (event picker + contextual filter
 * pickers off the synced tables).
 */
@Composable
internal fun TriggerWhenFields(
    draft: TriggerDraft,
    boards: List<StartBoardOption>,
    labels: List<LabelPickerLabel>,
    statuses: List<StatusPickerStatus>,
    onChange: (TriggerDraft) -> Unit,
) {
    // EXP-698: ONE card, exactly like the agent card in
    // [LaunchOptionsSection] — the kind strip is the card's FIRST ROW and the
    // picked kind's rows follow it under a hairline, instead of a loose
    // capsule floating above a second, separate group.
    OptionGroup {
        GlassSegmentedControl(
            options = listOf(TRIGGER_KIND_SCHEDULE, TRIGGER_KIND_EVENT),
            selected = draft.kind,
            label = { if (it == TRIGGER_KIND_EVENT) "On event" else "Schedule" },
            onSelect = { onChange(draft.copy(kind = it)) },
            modifier = Modifier.padding(8.dp),
            embedded = true,
        )

        if (draft.kind == TRIGGER_KIND_SCHEDULE) {
            GroupDivider()
            PickerRow(
                label = "Every",
                value = intervalLabel(draft.interval),
                options = DomainContract.agentScheduleIntervalValues,
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
            TriggerTimeRow(
                time = draft.time,
                onChange = { onChange(draft.copy(time = it)) },
            )
        } else if (draft.kind == TRIGGER_KIND_EVENT) {
            GroupDivider()
            PickerRow(
                label = "When",
                value = triggerEventLabel(draft.event),
                options = DomainContract.agentTriggerEventValues,
                selected = draft.event,
                optionLabel = ::triggerEventLabel,
                onSelect = { onChange(draft.copy(event = it)) },
            )
            GroupDivider()
            // Board filter applies to every event; the rest are contextual
            // (labels for label_added, priorities for created/priority
            // changes, target status for status_changed).
            TriggerFilterPicker(
                label = "Board",
                anyLabel = "Any board",
                // The board rows the rest of the app draws: glyph + colour.
                options = boardPickerItems(
                    boards.map { BoardPickerBoard(it.id, it.name, it.icon, it.colorHex) },
                ),
                selected = draft.boardId,
                onSelect = { onChange(draft.copy(boardId = it)) },
            )
            if (draft.event == "label_added") {
                GroupDivider()
                TriggerFilterPicker(
                    label = "Label",
                    anyLabel = "Any label",
                    // A label is a coloured DOT wherever it is listed.
                    options = labelPickerItems(labels),
                    selected = draft.labelId,
                    onSelect = { onChange(draft.copy(labelId = it)) },
                )
            }
            if (draft.event == "created" || draft.event == "priority_changed") {
                GroupDivider()
                TriggerFilterPicker(
                    label = "Priority",
                    anyLabel = "Any priority",
                    options = DomainContract.issuePriorityValues.map {
                        PickerItem(value = it, label = IssuePriority.fromWire(it).label)
                    },
                    selected = draft.priority,
                    onSelect = { onChange(draft.copy(priority = it)) },
                )
            }
            if (draft.event == "status_changed") {
                GroupDivider()
                TriggerFilterPicker(
                    label = "To status",
                    anyLabel = "Any status",
                    // A status is its glyph in its tone, resolved (EXP-314).
                    options = statusPickerItems(statuses),
                    selected = draft.toStatusId,
                    onSelect = { onChange(draft.copy(toStatusId = it)) },
                )
            }
        }
    }
}

/**
 * The schedule's wall-clock row: a PickerRow-shaped row that opens the
 * Material 3 time picker (EXP-615 — the free-text HH:MM field accepted
 * nonsense and read nothing like the iOS `DatePicker(.hourAndMinute)`). The
 * draft keeps its `"HH:MM"` string, so the wire format is unchanged.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TriggerTimeRow(time: String, onChange: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { open = true }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "Time",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
        )
        Text(
            time,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
    if (open) {
        val minuteOfDay = triggerTimeToMinute(time)
        val state = rememberTimePickerState(
            initialHour = minuteOfDay / 60,
            initialMinute = minuteOfDay % 60,
            // The stored string is 24h — showing a 12h dial would disagree
            // with the row's own value.
            is24Hour = true,
        )
        AlertDialog(
            onDismissRequest = { open = false },
            text = { TimePicker(state = state) },
            confirmButton = {
                TextButton(
                    onClick = {
                        open = false
                        onChange(triggerMinuteToTime(state.hour * 60 + state.minute))
                    },
                ) { Text("Done") }
            },
            dismissButton = {
                TextButton(onClick = { open = false }) { Text("Cancel") }
            },
        )
    }
}

// One "Any"-defaulted single-select filter row: the empty value is the
// no-filter state and stays re-pickable as the first option.
//
// [options] arrive as picker ROWS rather than bare names, so every filter
// hands over the rows its subject already has — a board's glyph in its colour,
// a label's dot, a status's resolved glyph. A filter sheet is no place to
// start drawing any of them as a bare label (EXP-1021).
@Composable
private fun TriggerFilterPicker(
    label: String,
    anyLabel: String,
    options: List<PickerItem<String>>,
    selected: String,
    onSelect: (String) -> Unit,
) {
    // EXP-1021: one sheet language for every filter — the shared [Picker],
    // with the "Any X" reset as its first ROW (a filter's cleared state is a
    // choice, not a missing one).
    val rows = listOf(PickerItem(value = "", label = anyLabel)) + options
    Picker(
        items = rows,
        mode = PickerMode.Single,
        value = setOf(selected),
        onChange = { picked -> picked.firstOrNull()?.let(onSelect) },
        title = label,
        trigger = { open ->
            PickerValueRow(
                label = label,
                value = options.firstOrNull { it.value == selected }?.label ?: anyLabel,
                onClick = open,
            )
        },
    )
}
