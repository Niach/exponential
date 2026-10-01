import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-615: the ONE trigger editor — the Schedule | On event switch, the
// schedule fields and the event filters of a trigger's when-part (SLOP-2: the
// trigger form sheet on an action's Triggers tab hosts it). Contextual filters
// are single-select on mobile — a multi-id list (the web allows several) seeds
// from its first entry and travels as one.

/// The trigger copy shared by the action page's Triggers tab and the trigger
/// form — one wording each, byte-equal to the other three clients.
enum TriggerCopy {
    /// The Triggers tab with no trigger on the action.
    static let empty = "No triggers. This action runs when someone starts it."
    /// The action demands an input nobody can type on a triggered run, so the
    /// server refuses to ENABLE a trigger on it.
    static let requiredInputsHint = "This action has required inputs, and a triggered run has none to fill them with. Make the inputs optional to enable it."
    /// No machine can run triggers.
    static let noTriggerDevice = "No device can run triggers. Run the desktop app or the exponential daemon and it will appear here."
    static let deleteTitle = "Delete trigger?"
    static let deleteBody = "It stops firing. Past runs stay in Runs."
}

/// The trigger being edited, in picker-shaped fields.
struct TriggerDraft: Equatable {
    var kind = "schedule"
    var schedInterval = "daily"
    var schedTime = TriggerDraft.defaultScheduleTime
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
struct TriggerFilterOptions {
    var boards: [BoardEntity] = []
    var labels: [LabelEntity] = []
    var statuses: [IssueStatusEntity] = []

    static func load(
        db: DatabaseManager,
        accountId: String,
        teamId: String
    ) async -> TriggerFilterOptions {
        guard let pool = try? db.pool(forAccountId: accountId) else {
            return TriggerFilterOptions()
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
        return TriggerFilterOptions(
            boards: boardRows.sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) },
            labels: labelRows.sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) },
            statuses: statusRows
                .filter { $0.category != "duplicate" }
                .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
        )
    }
}

/// The when-part `Section` of the trigger form.
struct TriggerForm: View {
    @Binding var draft: TriggerDraft
    let options: TriggerFilterOptions

    @ViewBuilder
    var body: some View {
        // EXP-721 (EXP-698 r4 parity): the Schedule / On event tabs are the
        // FIRST ROW of the trigger card — the embedded strip the agent card
        // wears — not a capsule under a "Trigger" heading. Web, desktop and
        // Android already read this way. No footer note about the machine's
        // own clock (EXP-615): the "Runs on" row already names the machine the
        // schedule belongs to.
        Section {
            GlassSegmentedControl(
                options: ["schedule", "event"],
                selection: draft.kind,
                label: { $0 == "schedule" ? "Schedule" : "On event" },
                style: .embedded,
                onSelect: { draft.kind = $0 }
            )
            .listRowInsets(EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))
            if draft.kind == "schedule" {
                scheduleRows
            } else {
                eventRows
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
        // EXP-721: the shared glass row, not the stock compact `DatePicker` —
        // its grey pill was the one system control left in this card, and its
        // 12-hour rendering disagreed with every other client's "HH:mm".
        GlassTimeRow("Time", selection: $draft.schedTime)
    }

    @ViewBuilder
    private var eventRows: some View {
        GlassPickerRow(
            "When",
            selection: $draft.eventType,
            options: DomainContract.actionTriggerEventValues,
            label: { AutomationTriggerDisplay.eventLabel($0) }
        )
        filterRow(
            "Board",
            anyLabel: "Any board",
            rows: BoardPicker<EmptyView>.items(options.boards.map(BoardPickerBoard.init)),
            selection: $draft.filterBoardId
        )
        if draft.eventType == "label_added" {
            filterRow(
                "Label",
                anyLabel: "Any label",
                rows: LabelPicker<EmptyView>.items(options.labels.map(LabelPickerLabel.init)),
                selection: $draft.filterLabelId
            )
        }
        if draft.eventType == "created" || draft.eventType == "priority_changed" {
            filterRow(
                "Priority",
                anyLabel: "Any priority",
                rows: PriorityPicker<EmptyView>.items(
                    IssuePriority.displayOrder.map(PriorityPickerOption.init)
                ),
                selection: $draft.filterPriority
            )
        }
        if draft.eventType == "status_changed" {
            // EXP-314: the team's rows resolved the way every list resolves
            // them, so a custom status keeps its own glyph and colour and the
            // order matches the board it filters.
            filterRow(
                "To status",
                anyLabel: "Any status",
                rows: StatusPicker<EmptyView>.items(
                    IssueStatusResolver.teamStatuses(options.statuses)
                        .map(StatusPickerStatus.init)
                ),
                selection: $draft.filterToStatusId
            )
        }
    }

    /// One event filter: the TYPED pickers' own rows — a board wears its glyph
    /// in its colour, a label its dot, a status its glyph — on the shared
    /// sheet, with the "Any X" reset as the FIRST row.
    ///
    /// Built from each typed picker's `items` rather than from the typed VIEW
    /// on purpose, which is also what the IDE's filter rows do
    /// (`board_picker::board_items` + its own row) and what Android's filter
    /// picker reads as: "Any board" is not a
    /// board, and a mobile filter is SINGLE-select (a multi-id list seeds from
    /// its first entry and travels as one, see this file's header), so there
    /// is no empty multi-selection for a cleared filter to be — it is a row.
    private func filterRow(
        _ title: String,
        anyLabel: String,
        rows: [PickerItem<String>],
        selection: Binding<String>
    ) -> some View {
        GlassPicker(
            items: [PickerItem(value: "", label: anyLabel)] + rows,
            mode: .single,
            value: [selection.wrappedValue],
            onChange: { picked in
                guard let value = picked.first else { return }
                selection.wrappedValue = value
            },
            title: title,
            trigger: {
                GlassPickerRowLabel(
                    title,
                    value: rows.first { $0.value == selection.wrappedValue }?.label ?? anyLabel
                )
            }
        )
    }
}
