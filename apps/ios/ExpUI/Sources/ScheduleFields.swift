import ExpCore
import SwiftUI

// NEW-2: THE schedule picker ×4 (`fixtures/schedule-picker.json`; web
// `@exp/ui` `ScheduleFields`, desktop `schedule_fields.rs`, Android
// `ScheduleFields.kt`). Nobody speaks cron: a schedule trigger STORES a 5-field
// cron, these rows edit it. Rendered INSIDE the caller's `Section` (the trigger
// card, whose first row is the Schedule · On event strip), every row an
// existing control: Repeat · Days (weekly) | Day (monthly) · Minute (hourly) |
// Time · Time zone · Cron (a custom, MCP-written expression only).
public struct ScheduleFields: View {
    @Binding var draft: ScheduleDraft

    public init(draft: Binding<ScheduleDraft>) {
        self._draft = draft
    }

    private typealias Copy = SchedulePicker.Copy
    private typealias Ids = SchedulePicker.TestIds
    private static let custom = "custom"

    public var body: some View {
        repeatRow
        if let schedule = draft.schedule {
            switch schedule.repeat {
            case "weekly": daysRow(schedule)
            case "monthly": dayRow
            default: EmptyView()
            }
            if schedule.repeat == "hourly" {
                minuteRow
            } else {
                timeRow
            }
        }
        timeZoneRow
        if draft.schedule == nil {
            cronRow
        }
    }

    // MARK: - Rows

    private var repeatRow: some View {
        GlassPickerRow(
            Copy.rowRepeat,
            selection: Binding(
                get: { draft.schedule?.repeat ?? Self.custom },
                set: { picked in
                    guard picked != Self.custom else { return }
                    draft.pick(repeat: picked)
                }
            ),
            // `custom` is offered ONLY while the schedule is custom.
            options: SchedulePicker.repeatValues + (draft.schedule == nil ? [Self.custom] : []),
            label: { Copy.repeatLabels[$0] ?? $0 }
        )
        .accessibilityIdentifier(Ids.repeat)
    }

    private func daysRow(_ schedule: Schedule) -> some View {
        let label = SchedulePicker.weekdaysLabel(schedule.weekdays)
        return GlassPicker(
            items: (1...7).map { day in
                PickerItem(value: day, label: Copy.weekdays[day - 1])
            },
            mode: .multi,
            value: Set(schedule.weekdays),
            onChange: { picked in draft.schedule?.weekdays = picked.sorted() },
            title: Copy.rowDays
        ) {
            // The row's value is the fixture's label ("Weekdays", "Mon, Wed,
            // Fri"), never the picker's own summary.
            GlassPickerRowLabel(Copy.rowDays, value: label.isEmpty ? Copy.errorNoDays : label)
        }
        .accessibilityIdentifier(Ids.days)
    }

    private var dayRow: some View {
        GlassPickerRow(
            Copy.rowDay,
            selection: Binding(
                get: { draft.schedule?.dayOfMonth ?? 1 },
                set: { draft.schedule?.dayOfMonth = $0 }
            ),
            options: Array(1...SchedulePicker.dayOfMonthMax),
            label: SchedulePicker.dayOfMonthLabel
        )
        .accessibilityIdentifier(Ids.day)
    }

    private var minuteRow: some View {
        GlassPickerRow(
            Copy.rowMinute,
            selection: Binding(
                get: { draft.schedule?.minute ?? 0 },
                set: { draft.schedule?.minute = $0 }
            ),
            options: Array(0...59),
            label: SchedulePicker.minuteOfHourLabel
        )
        .accessibilityIdentifier(Ids.minute)
    }

    /// The 24h `HH:mm` row; the wheel is pinned to a 24-hour locale so it
    /// never follows the phone's AM/PM setting.
    private var timeRow: some View {
        GlassTimeRow(Copy.rowTime, selection: timeBinding)
            .environment(\.locale, Locale(identifier: "en_GB"))
            .accessibilityIdentifier(Ids.time)
    }

    private var timeBinding: Binding<Date> {
        Binding(
            get: {
                let calendar = Calendar.current
                let start = calendar.startOfDay(for: Date())
                return calendar.date(
                    bySettingHour: draft.schedule?.hour ?? 0,
                    minute: draft.schedule?.minute ?? 0,
                    second: 0,
                    of: start
                ) ?? start
            },
            set: { date in
                let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
                draft.schedule?.hour = parts.hour ?? 0
                draft.schedule?.minute = parts.minute ?? 0
            }
        )
    }

    private var timeZoneRow: some View {
        let names = DomainContract.timeZoneNames
        // A stored zone off the list is offered as its own (first) row.
        let zones = names.contains(draft.tz) ? names : [draft.tz] + names
        return GlassPicker(
            items: zones.map { zone in
                PickerItem(
                    value: zone,
                    label: zone,
                    keywords: [
                        zone,
                        zone.replacingOccurrences(of: "_", with: " ")
                            .replacingOccurrences(of: "/", with: " "),
                    ]
                )
            },
            mode: .single,
            value: [draft.tz],
            onChange: { picked in
                if let zone = picked.first { draft.tz = zone }
            },
            search: true,
            title: Copy.rowTimeZone,
            searchPlaceholder: Copy.timeZonePlaceholder
        ) {
            GlassPickerRowLabel(Copy.rowTimeZone, value: draft.tz)
        }
        .accessibilityIdentifier(Ids.timeZone)
    }

    /// The only place cron is ever visible: a custom expression, monospace.
    private var cronRow: some View {
        GlassTextField(
            Copy.cronPlaceholder,
            text: $draft.cron,
            bordered: false,
            accessibilityIdentifier: Ids.cron,
            leading: {
                Text(Copy.rowCron)
                    .foregroundStyle(.white)
                    .frame(minWidth: 88, alignment: .leading)
            },
            trailing: { EmptyView() }
        )
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
        .font(.system(.body, design: .monospaced))
    }
}
