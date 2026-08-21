import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-583: the "+ New automation" / "Edit automation" form — the mobile twin
// of the web's automation dialog. An automation binds ONE action to ONE
// device with a schedule/event trigger and its own agent/model/effort
// (unset = the device's launch defaults). Owner-only: the list hides the
// entry points for everyone else, and the server refuses anyway.
//
// The action pool is deliberately narrow: custom actions only (builtins never
// automate) that declare NO required input — an automated run has nobody to
// type one in, so the server refuses to enable such a binding.
struct AutomationFormSheet: View {
    let teamId: String
    /// The team's actions, unfiltered — this sheet applies the eligibility
    /// rules itself so it can explain an empty pool.
    let actions: [ActionDto]
    /// Automation-capable machines (cap `automations`), OFFLINE INCLUDED: a
    /// sleeping machine still owns the binding and fires the missed schedule
    /// when it comes back.
    let devices: [SteerDevice]
    /// nil = create a new automation.
    let editing: AutomationDto?
    let onSubmit: (String, String, AutomationTrigger, AutomationLaunchPatch) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    /// Sentinel for "use the device's launch defaults" (agent/model/effort
    /// travel to the server as null).
    private static let deviceDefault = ""

    @State private var actionId = ""
    @State private var deviceId = ""
    @State private var kind = "schedule"
    @State private var schedInterval = "daily"
    @State private var schedTime = Self.defaultScheduleTime
    /// 1 = Monday … 7 = Sunday (the wire convention).
    @State private var schedWeekday = 1
    @State private var schedDayOfMonth = 1
    @State private var eventType = "created"
    @State private var filterBoardId = ""
    @State private var filterLabelId = ""
    @State private var filterPriority = ""
    @State private var filterToStatusId = ""
    @State private var agent = AutomationFormSheet.deviceDefault
    @State private var model = AutomationFormSheet.deviceDefault
    @State private var effort = AutomationFormSheet.deviceDefault
    @State private var boards: [BoardEntity] = []
    @State private var teamLabels: [LabelEntity] = []
    @State private var teamStatuses: [IssueStatusEntity] = []
    @State private var seeded = false

    private static var defaultScheduleTime: Date {
        Calendar.current.date(bySettingHour: 9, minute: 0, second: 0, of: Date()) ?? Date()
    }

    /// Automatable targets: real actions with no required input.
    private var eligibleActions: [ActionDto] {
        actions.filter { action in
            !action.isBuiltin && !(action.inputs ?? []).contains(where: \.isRequired)
        }
    }

    private var selectedDevice: SteerDevice? {
        devices.first { $0.deviceId == deviceId }
    }

    /// The chosen machine's runnable agents, in contract order.
    private var availableAgents: [String] {
        let supported = selectedDevice?.agentIds ?? []
        return DomainContract.codingAgentValues.filter { supported.contains($0) }
    }

    /// Model/effort are only meaningful against an agent — with none pinned
    /// the device's own per-agent defaults apply, so both pickers hide.
    private var pinnedAgent: String? {
        agent == Self.deviceDefault ? nil : agent
    }

    private var canSave: Bool {
        !actionId.isEmpty && !deviceId.isEmpty
    }

    var body: some View {
        NavigationStack {
            Form {
                actionSection
                triggerSection
                deviceSection
                if pinnedAgent != nil {
                    launchSection
                }
            }
            .navigationTitle(editing == nil ? "New automation" : "Edit automation")
            .navigationBarTitleDisplayMode(.inline)
            .listSectionSpacing(12)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { submit() }
                        .disabled(!canSave)
                }
            }
        }
        .presentationDetents([.large])
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("automation-form-sheet")
        .onAppear { seed() }
        .task { await loadFilterOptions() }
    }

    // MARK: - Sections

    @ViewBuilder
    private var actionSection: some View {
        Section {
            if eligibleActions.isEmpty {
                Text("No action can be automated yet. An automation runs an action with no required inputs.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                Picker("Action", selection: $actionId) {
                    ForEach(eligibleActions) { action in
                        Text(action.name).tag(action.id)
                    }
                }
            }
        } header: {
            Text("Action")
        }
    }

    private var triggerSection: some View {
        Section {
            Picker("Trigger", selection: $kind) {
                Text("Schedule").tag("schedule")
                Text("On event").tag("event")
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            if kind == "schedule" {
                Picker("Every", selection: $schedInterval) {
                    Text("Day").tag("daily")
                    Text("Week").tag("weekly")
                    Text("Month").tag("monthly")
                }
                if schedInterval == "weekly" {
                    Picker("Weekday", selection: $schedWeekday) {
                        ForEach(1...7, id: \.self) { day in
                            Text(AutomationTriggerDisplay.weekdayNames[day - 1]).tag(day)
                        }
                    }
                }
                if schedInterval == "monthly" {
                    Picker("Day of month", selection: $schedDayOfMonth) {
                        ForEach(1...28, id: \.self) { day in
                            Text("\(day)").tag(day)
                        }
                    }
                }
                DatePicker("Time", selection: $schedTime, displayedComponents: .hourAndMinute)
            } else {
                Picker("Event", selection: $eventType) {
                    ForEach(DomainContract.actionTriggerEventValues, id: \.self) { value in
                        Text(AutomationTriggerDisplay.eventLabel(value)).tag(value)
                    }
                }
                Picker("Board", selection: $filterBoardId) {
                    Text("Any").tag("")
                    ForEach(boards) { board in
                        Text(board.name).tag(board.id)
                    }
                }
                if eventType == "label_added" {
                    Picker("Label", selection: $filterLabelId) {
                        Text("Any").tag("")
                        ForEach(teamLabels) { label in
                            Text(label.name).tag(label.id)
                        }
                    }
                }
                if eventType == "created" || eventType == "priority_changed" {
                    Picker("Priority", selection: $filterPriority) {
                        Text("Any").tag("")
                        ForEach(IssuePriority.displayOrder) { priority in
                            Text(priority.label).tag(priority.rawValue)
                        }
                    }
                }
                if eventType == "status_changed" {
                    Picker("To status", selection: $filterToStatusId) {
                        Text("Any").tag("")
                        ForEach(teamStatuses) { status in
                            Text(status.name).tag(status.id)
                        }
                    }
                }
            }
        } header: {
            Text("Trigger")
        } footer: {
            if kind == "schedule" {
                Text("Schedules fire on the machine's own clock.")
            }
        }
    }

    @ViewBuilder
    private var deviceSection: some View {
        Section {
            if devices.isEmpty {
                Text("No machine of yours runs automations yet. Update the desktop app or CLI and try again.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                Picker("Runs on", selection: deviceBinding) {
                    ForEach(devices) { device in
                        Text(Self.deviceCaption(device)).tag(device.deviceId)
                    }
                }
                Picker("Agent", selection: agentBinding) {
                    Text("Device default").tag(Self.deviceDefault)
                    ForEach(availableAgents, id: \.self) { value in
                        Text(StartCodingSheet.agentLabel(value)).tag(value)
                    }
                }
            }
        } header: {
            Text("Runs on")
        }
    }

    @ViewBuilder
    private var launchSection: some View {
        if let pinnedAgent {
            Section {
                Picker("Model", selection: $model) {
                    Text("Device default").tag(Self.deviceDefault)
                    ForEach(Self.modelOptions(for: pinnedAgent), id: \.self) { value in
                        Text(StartCodingSheet.modelLabel(value)).tag(value)
                    }
                }
                Picker(Self.effortTitle(for: pinnedAgent), selection: $effort) {
                    Text("Device default").tag(Self.deviceDefault)
                    ForEach(StartCodingSheet.effortValues(for: pinnedAgent), id: \.self) { value in
                        Text(StartCodingSheet.effortLabel(value)).tag(value)
                    }
                }
            }
        }
    }

    // MARK: - Bindings

    /// Both writes clear DOWNSTREAM picks the way an explicit user switch
    /// must (and `seed()`'s prefill deliberately must not — hence bindings
    /// rather than onChange): a machine that doesn't advertise the pinned
    /// agent would be rejected server-side, and model/effort vocabularies are
    /// per-agent, so a stale pick can never ride along.
    private var deviceBinding: Binding<String> {
        Binding(
            get: { deviceId },
            set: { value in
                guard value != deviceId else { return }
                deviceId = value
                if agent != Self.deviceDefault, !availableAgents.contains(agent) {
                    agent = Self.deviceDefault
                    model = Self.deviceDefault
                    effort = Self.deviceDefault
                }
            }
        )
    }

    private var agentBinding: Binding<String> {
        Binding(
            get: { agent },
            set: { value in
                guard value != agent else { return }
                agent = value
                model = Self.deviceDefault
                effort = Self.deviceDefault
            }
        )
    }

    // MARK: - Option lists

    /// The agent's contract models. The sheet's own "Device default" row
    /// replaces the run sheet's "CLI default" sentinel, so drop that entry.
    private static func modelOptions(for agent: String) -> [String] {
        StartCodingSheet.modelValues(for: agent).filter { $0 != StartCodingSheet.cliDefault }
    }

    private static func effortTitle(for agent: String) -> String {
        switch agent {
        case "codex": "Reasoning"
        case "pi": "Thinking"
        default: "Effort"
        }
    }

    /// A teammate's shared machine is attributed to its owner — the run lands
    /// on somebody else's hardware, so the picker says whose.
    private static func deviceCaption(_ device: SteerDevice) -> String {
        let name = device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
        let base = device.isOnline ? name : "\(name) (offline)"
        guard let owner = device.owner else { return base }
        return "\(base) — \(owner.name)"
    }

    // MARK: - Seed / submit

    private func seed() {
        guard !seeded else { return }
        seeded = true
        if let editing {
            actionId = editing.actionId
            deviceId = editing.deviceId
            agent = editing.agent ?? Self.deviceDefault
            model = editing.model ?? Self.deviceDefault
            effort = editing.effort ?? Self.deviceDefault
            applyTrigger(editing.parsedTrigger)
        }
        if actionId.isEmpty { actionId = eligibleActions.first?.id ?? "" }
        if deviceId.isEmpty { deviceId = devices.first?.deviceId ?? "" }
    }

    /// Seed the pickers from an existing trigger. Contextual filters are
    /// single-select on mobile, so a multi-id list seeds from its first entry.
    private func applyTrigger(_ trigger: AutomationTrigger?) {
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

    /// The event filters' option pools, off the synced store.
    @MainActor
    private func loadFilterOptions() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let boardRows = (try? await pool.read { db in
            try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }) ?? []
        boards = boardRows.sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
        let labelRows = (try? await pool.read { db in
            try LabelEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }) ?? []
        teamLabels = labelRows.sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
        let statusRows = (try? await pool.read { db in
            try IssueStatusEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }) ?? []
        teamStatuses = statusRows
            .filter { $0.category != "duplicate" }
            .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
    }

    /// The configured trigger in wire form. Contextual filters travel only
    /// for the event they apply to — a stale pick from a previously chosen
    /// event never rides along.
    private var configuredTrigger: AutomationTrigger {
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

    private func submit() {
        guard canSave else { return }
        // Snapshot before dismissing — the payload must not depend on what
        // the teardown does to the sheet's state.
        let launch = AutomationLaunchPatch(
            agent: pinnedAgent,
            model: pinnedAgent == nil || model == Self.deviceDefault ? nil : model,
            effort: pinnedAgent == nil || effort == Self.deviceDefault ? nil : effort
        )
        let payload = (actionId, deviceId, configuredTrigger, launch)
        dismiss()
        onSubmit(payload.0, payload.1, payload.2, payload.3)
    }
}
