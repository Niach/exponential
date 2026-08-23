import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-615: the ONE trigger editor. The automation form sheet and the create-
// action sheet's Automation detail configure the same when-part, so the
// Schedule | On event switch, the schedule fields and the event filters live
// here once. Contextual filters are single-select on mobile — a multi-id list
// (the web allows several) seeds from its first entry and travels as one.

/// The automation copy shared by the form sheet, the create-action sheet and
/// the Automations tab — one wording each, taken from the web dialogs
/// (EXP-615 string parity).
enum AutomationCopy {
    /// Nothing to automate at all (web automation-dialog.tsx).
    static let noAutomatableActions = "No custom actions yet. Create one first, then automate it."
    /// Actions exist, but every one of them demands an input nobody can type
    /// (web `REQUIRED_INPUTS_HINT`).
    static let requiredInputsHint = "This action has required inputs, and an automated run has none to fill them with. Make the inputs optional to enable it."
    /// No machine can run automations (web `AutomationDevicePicker`).
    static let noAutomationDevice = "No automation-capable device. Run the desktop app or the exponential daemon and it will appear here."
    /// The delete confirmation's body (web automations-tab.tsx).
    static func deleteBody(actionName: String?) -> String {
        "Stop automating \"\(actionName ?? "this action")\"? The action itself stays, and runs already going keep going."
    }
}

/// The trigger being edited, in picker-shaped fields.
struct AutomationDraft: Equatable {
    var kind = "schedule"
    var schedInterval = "daily"
    var schedTime = AutomationDraft.defaultScheduleTime
    /// 1 = Monday … 7 = Sunday (the wire convention).
    var schedWeekday = 1
    var schedDayOfMonth = 1
    var eventType = "created"
    var filterBoardId = ""
    var filterLabelId = ""
    var filterPriority = ""
    var filterToStatusId = ""

    static var defaultScheduleTime: Date {
        Calendar.current.date(bySettingHour: 9, minute: 0, second: 0, of: Date()) ?? Date()
    }

    init() {}

    /// Seed the pickers from an existing/suggested trigger.
    init(trigger: AutomationTrigger?) {
        switch trigger {
        case let .schedule(s)?:
            kind = "schedule"
            schedInterval = s.interval
            schedWeekday = s.weekday ?? 1
            schedDayOfMonth = s.dayOfMonth ?? 1
            schedTime = Calendar.current.date(
                bySettingHour: s.minuteOfDay / 60,
                minute: s.minuteOfDay % 60,
                second: 0,
                of: Date()
            ) ?? Self.defaultScheduleTime
        case let .event(e)?:
            kind = "event"
            eventType = e.event
            filterBoardId = e.filters.boardIds.first ?? ""
            filterLabelId = e.filters.labelIds.first ?? ""
            filterPriority = e.filters.priorities.first ?? ""
            filterToStatusId = e.filters.toStatusIds.first ?? ""
        case nil:
            break
        }
    }

    /// The configured trigger in wire form. Contextual filters travel only for
    /// the event they apply to — a stale pick from a previously chosen event
    /// never rides along.
    var trigger: AutomationTrigger {
        if kind == "schedule" {
            let comps = Calendar.current.dateComponents([.hour, .minute], from: schedTime)
            let minuteOfDay = (comps.hour ?? 0) * 60 + (comps.minute ?? 0)
            return .schedule(AutomationScheduleTrigger(
                interval: schedInterval,
                minuteOfDay: minuteOfDay,
                weekday: schedInterval == "weekly" ? schedWeekday : nil,
                dayOfMonth: schedInterval == "monthly" ? schedDayOfMonth : nil
            ))
        }
        return .event(AutomationEventTrigger(
            event: eventType,
            filters: AutomationTriggerFilters(
                boardIds: filterBoardId.isEmpty ? [] : [filterBoardId],
                labelIds: eventType == "label_added" && !filterLabelId.isEmpty
                    ? [filterLabelId] : [],
                priorities: (eventType == "created" || eventType == "priority_changed")
                    && !filterPriority.isEmpty ? [filterPriority] : [],
                toStatusIds: eventType == "status_changed" && !filterToStatusId.isEmpty
                    ? [filterToStatusId] : []
            )
        ))
    }
}

/// The event filters' option pools, off the synced store.
struct AutomationFilterOptions {
    var boards: [BoardEntity] = []
    var labels: [LabelEntity] = []
    var statuses: [IssueStatusEntity] = []

    static func load(
        db: DatabaseManager,
        accountId: String,
        teamId: String
    ) async -> AutomationFilterOptions {
        guard let pool = try? db.pool(forAccountId: accountId) else {
            return AutomationFilterOptions()
        }
        let boardRows = (try? await pool.read { db in
            try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }) ?? []
        let labelRows = (try? await pool.read { db in
            try LabelEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }) ?? []
        let statusRows = (try? await pool.read { db in
            try IssueStatusEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }) ?? []
        return AutomationFilterOptions(
            boards: boardRows.sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) },
            labels: labelRows.sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) },
            statuses: statusRows
                .filter { $0.category != "duplicate" }
                .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
        )
    }
}

/// The Trigger `Section` — one row set shared by both automation surfaces.
struct AutomationTriggerForm: View {
    @Binding var draft: AutomationDraft
    let options: AutomationFilterOptions

    var body: some View {
        Section {
            GlassSegmentedControl(
                options: ["schedule", "event"],
                selection: draft.kind,
                label: { $0 == "schedule" ? "Schedule" : "On event" },
                onSelect: { draft.kind = $0 }
            )
            .accessibilityLabel("Trigger")
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))

            if draft.kind == "schedule" {
                scheduleRows
            } else {
                eventRows
            }
        } header: {
            Text("Trigger")
        } footer: {
            if draft.kind == "schedule" {
                Text("Schedules fire on the machine's own clock.")
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    @ViewBuilder
    private var scheduleRows: some View {
        GlassPickerRow(
            "Every",
            selection: $draft.schedInterval,
            options: ["daily", "weekly", "monthly"],
            label: { value in
                switch value {
                case "weekly": "Week"
                case "monthly": "Month"
                default: "Day"
                }
            }
        )
        if draft.schedInterval == "weekly" {
            GlassPickerRow(
                "Weekday",
                selection: $draft.schedWeekday,
                options: Array(1...7),
                label: { AutomationTriggerDisplay.weekdayNames[$0 - 1] }
            )
        }
        if draft.schedInterval == "monthly" {
            GlassPickerRow(
                "Day of month",
                selection: $draft.schedDayOfMonth,
                options: Array(1...28),
                label: { "Day \($0)" }
            )
        }
        DatePicker("Time", selection: $draft.schedTime, displayedComponents: .hourAndMinute)
    }

    @ViewBuilder
    private var eventRows: some View {
        GlassPickerRow(
            "When",
            selection: $draft.eventType,
            options: DomainContract.actionTriggerEventValues,
            label: { AutomationTriggerDisplay.eventLabel($0) }
        )
        GlassPickerRow(
            "Board",
            selection: $draft.filterBoardId,
            options: [""] + options.boards.map(\.id),
            label: { id in
                guard !id.isEmpty else { return "Any board" }
                return options.boards.first { $0.id == id }?.name ?? id
            }
        )
        if draft.eventType == "label_added" {
            GlassPickerRow(
                "Label",
                selection: $draft.filterLabelId,
                options: [""] + options.labels.map(\.id),
                label: { id in
                    guard !id.isEmpty else { return "Any label" }
                    return options.labels.first { $0.id == id }?.name ?? id
                }
            )
        }
        if draft.eventType == "created" || draft.eventType == "priority_changed" {
            GlassPickerRow(
                "Priority",
                selection: $draft.filterPriority,
                options: [""] + IssuePriority.displayOrder.map(\.rawValue),
                label: { value in
                    guard !value.isEmpty else { return "Any priority" }
                    return IssuePriority.displayOrder
                        .first { $0.rawValue == value }?.label ?? value
                }
            )
        }
        if draft.eventType == "status_changed" {
            GlassPickerRow(
                "To status",
                selection: $draft.filterToStatusId,
                options: [""] + options.statuses.map(\.id),
                label: { id in
                    guard !id.isEmpty else { return "Any status" }
                    return options.statuses.first { $0.id == id }?.name ?? id
                }
            )
        }
    }
}
