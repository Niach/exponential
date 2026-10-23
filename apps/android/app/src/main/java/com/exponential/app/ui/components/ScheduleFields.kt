package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.Schedule
import com.exponential.app.domain.ScheduleDraft
import com.exponential.app.domain.SchedulePicker
import com.exponential.app.domain.pickRepeat
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

// NEW-2: THE schedule picker ×4 (`fixtures/schedule-picker.json`; web
// `@exp/ui` `ScheduleFields`, desktop `schedule_fields.rs`, iOS ExpUI
// `ScheduleFields.swift`). Nobody speaks cron: a schedule trigger STORES a
// 5-field cron, these rows edit it. Rendered INSIDE the caller's
// `OptionGroup` (the trigger editor's card, whose first row is the
// Schedule · On event strip; the caller lays the hairline above Repeat), the rows
// separated by `GroupDivider`s:
// Repeat · Days (weekly) | Day (monthly) · Every (minutes, EXP-1363) |
// Minute (hourly) | Time · Time zone (daily, weekly, monthly, custom) · Cron
// (a custom, MCP-written expression only). CONTROLLED: the
// caller holds the [ScheduleDraft].

private val ScheduleCopy = SchedulePicker.Copy

private fun pad2(value: Int): String = value.toString().padStart(2, '0')

private val WEEKDAY_ITEMS: List<PickerItem<Int>> =
    (1..7).map { day -> PickerItem(value = day, label = ScheduleCopy.WEEKDAYS[day - 1]) }
private val HOUR_ITEMS: List<PickerItem<Int>> = (0..23).map { PickerItem(value = it, label = pad2(it)) }
private val MINUTE_ITEMS: List<PickerItem<Int>> = (0..59).map { PickerItem(value = it, label = pad2(it)) }

/** The contract's zones as picker rows; `_` and `/` read as spaces for search. */
private val TIME_ZONE_ITEMS: List<PickerItem<String>> = DomainContract.timeZoneNames.map(::timeZoneItem)

private fun timeZoneItem(zone: String): PickerItem<String> =
    PickerItem(value = zone, label = zone, keywords = listOf(zone, zone.replace('_', ' ').replace('/', ' ')))

/** The rows of the schedule picker. */
@Composable
internal fun ScheduleFields(
    draft: ScheduleDraft,
    onChange: (ScheduleDraft) -> Unit,
) {
    val schedule = draft.schedule
    val setSchedule: (Schedule) -> Unit = { next -> onChange(draft.copy(schedule = next)) }

    // Repeat: `custom` is offered ONLY while the stored cron is custom.
    val repeatOptions = if (schedule != null) {
        SchedulePicker.REPEAT_VALUES
    } else {
        SchedulePicker.REPEAT_VALUES + SchedulePicker.REPEAT_CUSTOM
    }
    val repeatLabel: (String) -> String = { ScheduleCopy.REPEAT[it] ?: ScheduleCopy.REPEAT_CUSTOM }
    val repeatValue = schedule?.repeat ?: SchedulePicker.REPEAT_CUSTOM
    Box(Modifier.testTag(SchedulePicker.TEST_ID_REPEAT)) {
        PickerRow(
            label = ScheduleCopy.ROW_REPEAT,
            value = repeatLabel(repeatValue),
            options = repeatOptions,
            selected = repeatValue,
            optionLabel = repeatLabel,
            onSelect = { picked ->
                if (picked != SchedulePicker.REPEAT_CUSTOM) onChange(pickRepeat(draft, picked))
            },
        )
    }

    if (schedule?.repeat == SchedulePicker.REPEAT_WEEKLY) {
        GroupDivider()
        Picker(
            items = WEEKDAY_ITEMS,
            mode = PickerMode.Multi,
            value = schedule.weekdays.toSet(),
            onChange = { picked -> setSchedule(schedule.copy(weekdays = picked.sorted())) },
            title = ScheduleCopy.ROW_DAYS,
        ) { open ->
            // The row's value is the fixture's label ("Weekdays", "Mon, Wed,
            // Fri"), never the primitive's "Monday, Tuesday +3".
            PickerValueRow(
                label = ScheduleCopy.ROW_DAYS,
                value = SchedulePicker.weekdaysLabel(schedule.weekdays).ifEmpty { ScheduleCopy.ERROR_NO_DAYS },
                onClick = open,
                modifier = Modifier.testTag(SchedulePicker.TEST_ID_DAYS),
            )
        }
    }

    if (schedule?.repeat == SchedulePicker.REPEAT_MONTHLY) {
        GroupDivider()
        val days = remember { (1..SchedulePicker.DAY_OF_MONTH_MAX).map(Int::toString) }
        Box(Modifier.testTag(SchedulePicker.TEST_ID_DAY)) {
            PickerRow(
                label = ScheduleCopy.ROW_DAY,
                value = SchedulePicker.dayOfMonthLabel(schedule.dayOfMonth),
                options = days,
                selected = schedule.dayOfMonth.toString(),
                optionLabel = { SchedulePicker.dayOfMonthLabel(it.toInt()) },
                onSelect = { setSchedule(schedule.copy(dayOfMonth = it.toInt())) },
            )
        }
    }

    if (schedule?.repeat == SchedulePicker.REPEAT_MINUTES) {
        // EXP-1363: every N minutes — the Every row over `everyValues`; no
        // Time row (and no Time zone row below).
        GroupDivider()
        val everyOptions = remember { SchedulePicker.EVERY_VALUES.map(Int::toString) }
        Box(Modifier.testTag(SchedulePicker.TEST_ID_EVERY)) {
            PickerRow(
                label = ScheduleCopy.ROW_EVERY,
                value = SchedulePicker.everyLabel(schedule.every),
                options = everyOptions,
                selected = schedule.every.toString(),
                optionLabel = { SchedulePicker.everyLabel(it.toInt()) },
                onSelect = { setSchedule(schedule.copy(every = it.toInt())) },
            )
        }
    } else if (schedule?.repeat == SchedulePicker.REPEAT_HOURLY) {
        GroupDivider()
        val minutes = remember { (0..59).map(Int::toString) }
        Box(Modifier.testTag(SchedulePicker.TEST_ID_MINUTE)) {
            PickerRow(
                label = ScheduleCopy.ROW_MINUTE,
                value = SchedulePicker.minuteOfHourLabel(schedule.minute),
                options = minutes,
                selected = schedule.minute.toString(),
                optionLabel = { SchedulePicker.minuteOfHourLabel(it.toInt()) },
                onSelect = { setSchedule(schedule.copy(minute = it.toInt())) },
            )
        }
    } else if (schedule != null) {
        GroupDivider()
        ScheduleTimeRow(
            hour = schedule.hour,
            minute = schedule.minute,
            onChange = { hour, minute -> setSchedule(schedule.copy(hour = hour, minute = minute)) },
        )
    }

    // EXP-1363: a minutes or hourly schedule shows no Time zone row (and
    // keeps whatever zone is stored).
    if (SchedulePicker.showsTimeZone(schedule?.repeat)) {
    GroupDivider()
    val zoneItems = remember(draft.tz) {
        if (TIME_ZONE_ITEMS.any { it.value == draft.tz }) {
            TIME_ZONE_ITEMS
        } else {
            listOf(timeZoneItem(draft.tz)) + TIME_ZONE_ITEMS
        }
    }
    Picker(
        items = zoneItems,
        mode = PickerMode.Single,
        value = setOf(draft.tz),
        onChange = { picked -> picked.firstOrNull()?.let { onChange(draft.copy(tz = it)) } },
        search = true,
        searchPlaceholder = ScheduleCopy.TIME_ZONE_PLACEHOLDER,
        title = ScheduleCopy.ROW_TIME_ZONE,
    ) { open ->
        PickerValueRow(
            label = ScheduleCopy.ROW_TIME_ZONE,
            value = draft.tz,
            onClick = open,
            modifier = Modifier.testTag(SchedulePicker.TEST_ID_TIME_ZONE),
        )
    }
    }

    if (schedule == null) {
        GroupDivider()
        TextFieldRow(
            label = ScheduleCopy.ROW_CRON,
            value = draft.cron,
            onValueChange = { onChange(draft.copy(cron = it.take(DomainContract.agentTriggerCronMax))) },
            placeholder = ScheduleCopy.CRON_PLACEHOLDER,
            textStyle = LocalTextStyle.current.copy(fontFamily = FontFamily.Monospace),
            modifier = Modifier.testTag(SchedulePicker.TEST_ID_CRON),
        )
    }
}

/**
 * The Time row ×4: "Time" leading, the 24h `HH:mm` + chevron trailing, its
 * hour and its minute each the shared [Picker] (every hour, every minute, the
 * picked one marked) — never the platform's own time dialog, which follows
 * the phone's locale. A tap on the label opens the hour.
 */
@Composable
private fun ScheduleTimeRow(
    hour: Int,
    minute: Int,
    onChange: (hour: Int, minute: Int) -> Unit,
) {
    val valueColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Picker(
        items = HOUR_ITEMS,
        mode = PickerMode.Single,
        value = setOf(hour),
        onChange = { picked -> picked.firstOrNull()?.let { onChange(it, minute) } },
        title = "Hour",
    ) { openHour ->
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .testTag(SchedulePicker.TEST_ID_TIME)
                .clickable(onClick = openHour)
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                ScheduleCopy.ROW_TIME,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
            Text(
                pad2(hour),
                style = MaterialTheme.typography.bodyMedium,
                color = valueColor,
                modifier = Modifier.clickable(onClick = openHour).padding(horizontal = 2.dp),
            )
            Text(":", style = MaterialTheme.typography.bodyMedium, color = valueColor)
            Picker(
                items = MINUTE_ITEMS,
                mode = PickerMode.Single,
                value = setOf(minute),
                onChange = { picked -> picked.firstOrNull()?.let { onChange(hour, it) } },
                title = ScheduleCopy.ROW_MINUTE,
            ) { openMinute ->
                Text(
                    pad2(minute),
                    style = MaterialTheme.typography.bodyMedium,
                    color = valueColor,
                    modifier = Modifier.clickable(onClick = openMinute).padding(horizontal = 2.dp),
                )
            }
            Icon(
                ExpIcons.uiChevronRight,
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

/** The rows in their grouped card, for the styleguide's `schedule-picker`
 *  specimen (thread 58cb32df: the `minutes` repeat). */
@Composable
fun ScheduleFieldsSpecimen(draft: ScheduleDraft, onChange: (ScheduleDraft) -> Unit) {
    OptionGroup { ScheduleFields(draft, onChange) }
}
