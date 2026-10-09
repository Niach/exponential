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
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.AutomationTriggerFilters
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
import com.exponential.app.ui.components.ambientAccountOptions
import com.exponential.app.ui.components.availableAgentsFor
import com.exponential.app.ui.components.toPickerAccount
import com.exponential.app.ui.components.defaultAgentFor
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.agent.StartBoardOption
import com.exponential.app.ui.theme.TextEmphasis

// The trigger editor's fields (EXP-583; SLOP-2: a trigger lives on its
// action): ONE draft and its two cards — the when-part (Schedule | On event)
// and the binding (the machine it runs on plus the account/model/effort pins)
// — rendered by the action page's "New trigger" / "Edit trigger" sheet
// ([TriggerFormSheet]).

/** The default schedule time, matching the web draft (09:00). */
private const val DEFAULT_MINUTE_OF_DAY = 540

internal const val TRIGGER_KIND_SCHEDULE = "schedule"
internal const val TRIGGER_KIND_EVENT = "event"

/**
 * The editor state. A draft can represent things a valid trigger cannot (an
 * event kind with no machine bound yet), so [triggerDraftToWhen]
 * converts at submit and incomplete reads as null. The filter picks are
 * SINGLE-select with an "Any" default — the mobile simplification of the web's
 * multi-selects; the wire lists carry one-or-none entries from here.
 * [agent] is seeded from the bound machine's last used agent (EXP-615 — empty
 * only before one is bound); EXP-995: [account] is the agent profile the run
 * spends, picked WITH its agent off the account row as the option id
 * VERBATIM (`system` = the ambient login); "" is unpinned, saved as NULL, and
 * runs on the machine's LAST USED login (EXP-1158). An empty [model]/[effort]
 * is the "CLI default" that saves as NULL.
 */
internal data class TriggerDraft(
    val kind: String = TRIGGER_KIND_SCHEDULE,
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
    val account: String = "",
    val model: String = CLI_DEFAULT_MODEL,
    val effort: String = CLI_DEFAULT_EFFORT,
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

/**
 * A draft seeded from an existing/suggested [trigger] (the suggestion host);
 * null seeds the plain schedule default. [deviceId] pre-picks the machine.
 */
internal fun triggerDraftFor(
    trigger: AutomationTrigger?,
    deviceId: String? = null,
): TriggerDraft = when (trigger) {
    null -> TriggerDraft(deviceId = deviceId)
    is AutomationTrigger.Schedule -> TriggerDraft(
        kind = TRIGGER_KIND_SCHEDULE,
        deviceId = deviceId,
        interval = trigger.interval,
        time = triggerMinuteToTime(trigger.minuteOfDay),
        weekday = trigger.weekday ?: 1,
        dayOfMonth = trigger.dayOfMonth ?: 1,
    )
    is AutomationTrigger.Event -> TriggerDraft(
        kind = TRIGGER_KIND_EVENT,
        deviceId = deviceId,
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
internal fun triggerDraftToWhen(draft: TriggerDraft): AutomationTrigger? =
    when (draft.kind) {
        TRIGGER_KIND_SCHEDULE -> AutomationTrigger.Schedule(
            interval = draft.interval,
            minuteOfDay = triggerTimeToMinute(draft.time),
            weekday = draft.weekday.takeIf { draft.interval == "weekly" },
            dayOfMonth = draft.dayOfMonth.takeIf { draft.interval == "monthly" },
        )
        TRIGGER_KIND_EVENT -> AutomationTrigger.Event(
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
            TriggerTimeRow(
                time = draft.time,
                onChange = { onChange(draft.copy(time = it)) },
            )
        } else if (draft.kind == TRIGGER_KIND_EVENT) {
            GroupDivider()
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
 * Seed (and re-seed) the pin off the bound machine — what the editor SHOWS is
 * what Save STORES. Null = leave the draft alone.
 *
 *  - An unset pin (a row saved before EXP-615 carries a NULL agent) or one
 *    [bound] cannot run falls back to that machine's LAST USED login, which
 *    names the agent, clamped to what it advertises; model/effort reset to
 *    the "CLI default" blank since their vocabularies are per agent.
 *  - A runnable pin on the SAME machine is left alone, so a manual pick sticks.
 *  - EXP-995: a profile id is DEVICE-LOCAL, so on a switch ([deviceSwitched])
 *    a runnable pin moves onto the new machine's last used login of the SAME
 *    agent ([accountOptionsFor] lists that agent's active login first),
 *    keeping model/effort; an agent the new machine reports no login for
 *    falls back to its last used login.
 *
 * Every seeded pin is the option id VERBATIM, `system` included.
 */
internal fun seedTriggerPin(
    draft: TriggerDraft,
    bound: SteerDevice,
    deviceSwitched: Boolean,
): TriggerDraft? {
    val agents = availableAgentsFor(bound)
    val options = accountOptionsFor(bound, agents)
    val runnable = draft.agent.isNotEmpty() && draft.agent in agents
    if (runnable && !deviceSwitched) return null
    if (runnable) {
        val sameAgent = options.firstOrNull { it.agent == draft.agent }
        if (sameAgent != null) {
            return draft.copy(account = sameAgent.id)
        }
    }
    val fallback = AccountOptions.lastUsed(options)
    return draft.copy(
        agent = fallback?.agent ?: defaultAgentFor(bound),
        account = fallback?.id.orEmpty(),
        model = CLI_DEFAULT_MODEL,
        effort = CLI_DEFAULT_EFFORT,
    )
}

/**
 * The binding half: which machine runs the trigger, and the account pin
 * with its model/effort. EXP-615 retired the "Device default" agent option;
 * EXP-995 retired the agent strip itself — the card's first row is THE
 * account picker ([AccountPicker], brand mark + email over the bound
 * machine's logins, its last used first) and a pick names the agent too. The
 * pin seeds to the bound machine's LAST USED login (the composer's seed) and
 * the row saves that concrete agent + profile. Model/Effort speak the launch
 * "CLI default" sentinel, which is what writes NULL.
 */
@Composable
internal fun TriggerBindingFields(
    draft: TriggerDraft,
    devices: List<SteerDevice>,
    onChange: (TriggerDraft) -> Unit,
) {
    val device = devices.firstOrNull { it.deviceId == draft.deviceId }
    // EXP-995: every signed-in login the bound machine reports, across
    // agents; a machine that reports none (or none bound yet) offers one
    // ambient row per runnable agent so the pin can be made before the
    // heartbeat lands (the workflow runner block's rule).
    val accountOptions = if (device == null) {
        ambientAccountOptions(DomainContract.codingAgentValues, draft.agent.takeIf { it.isNotEmpty() })
    } else {
        accountOptionsFor(device, availableAgentsFor(device))
    }
    // The machine the draft's pin was made on: an edited row's stored pin
    // belongs to its stored machine (never re-seeded on open), a fresh draft's
    // to nothing yet. Only a REAL switch away from it re-seeds a runnable pin —
    // the pool arriving late, or a heartbeat re-listing the same machine, is
    // not a switch.
    var pinDeviceId by remember { mutableStateOf(draft.deviceId) }
    LaunchedEffect(device?.deviceId, devices) {
        val bound = device ?: return@LaunchedEffect
        val switched = pinDeviceId != null && pinDeviceId != bound.deviceId
        pinDeviceId = bound.deviceId
        seedTriggerPin(draft, bound, deviceSwitched = switched)?.let(onChange)
    }
    // EXP-615: the same block the launch dialogs render, in its Trigger
    // variant — no launch toggles; EXP-995: the account row leads it.
    LaunchOptionsSection(
        variant = LaunchOptionsVariant.Trigger,
        devices = devices,
        device = device,
        // The re-seed above moves the pin onto the new machine (its own
        // profile for the same agent, else its last used login).
        onDeviceChange = { id -> onChange(draft.copy(deviceId = id)) },
        agent = draft.agent,
        availableAgents = device?.runnableAgents.orEmpty(),
        onAgentChange = { next ->
            // A profile is ONE agent's, and the model/effort vocabularies are
            // per agent — a switch has to reset them, or a stale value hits a
            // server refusal.
            onChange(
                draft.copy(
                    agent = next,
                    account = "",
                    model = CLI_DEFAULT_MODEL,
                    effort = CLI_DEFAULT_EFFORT,
                ),
            )
        },
        accountRow = if (accountOptions.isEmpty()) {
            null
        } else {
            {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        "Account",
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Primary,
                        ),
                    )
                    Spacer(Modifier.weight(1f))
                    // The pin's own key; an unpinned draft shows its agent's
                    // last used login (that agent's first option), and a pin
                    // this machine no longer reports falls back to the first.
                    val current = accountOptions.firstOrNull { it.key == "${draft.agent}:${draft.account}" }
                        ?: accountOptions.firstOrNull { draft.account.isEmpty() && it.agent == draft.agent }
                        ?: accountOptions.first()
                    // EXP-1021: the same capsule over the SHARED account sheet
                    // — the login rows, their EXP-992 limit bars and the
                    // highlight are the picker's now, not a menu of this
                    // surface's own. A lone login is a statement, not a choice,
                    // so it opens nothing.
                    AccountPicker(
                        options = accountOptions.map { it.toPickerAccount() },
                        value = current.key,
                        onChange = { key ->
                            accountOptions.firstOrNull { it.key == key }?.let { option ->
                                // Only an AGENT change invalidates the
                                // vocabularies below; another login of the same
                                // agent keeps model and effort exactly as
                                // picked. The id rides VERBATIM, `system`
                                // naming the ambient login.
                                val agentChanged = option.agent != draft.agent
                                onChange(
                                    draft.copy(
                                        agent = option.agent,
                                        account = option.id,
                                        model = if (agentChanged) CLI_DEFAULT_MODEL else draft.model,
                                        effort = if (agentChanged) CLI_DEFAULT_EFFORT else draft.effort,
                                    ),
                                )
                            }
                        },
                        trigger = { open ->
                            AccountPickerPillTrigger(
                                option = current.toPickerAccount(),
                                onOpen = if (accountOptions.size > 1) open else null,
                                modifier = Modifier.testTag("trigger-account-pill"),
                            )
                        },
                    )
                }
            }
        },
        model = draft.model,
        onModelChange = { onChange(draft.copy(model = it)) },
        effort = draft.effort,
        onEffortChange = { onChange(draft.copy(effort = it)) },
        // One wording with the web dialog (EXP-615).
        noDeviceNote = "No device can run triggers. Run the desktop app or the " +
            "exponential daemon and it will appear here.",
    )
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
