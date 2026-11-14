import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-1846 (agent-settings.json `page` + `checkIn`): THE TRIGGER LIST, one
// reusable Form section taking an agent id (web AgentTriggers, desktop
// settings::agent_triggers, Android AgentTriggersSection): the `Triggers`
// header with the editor's `+ Add` pill trailing, then the Check in row (THE
// Picker over `assistantSchedules`, value `checkInOf`; no empty-state row),
// then one row per `listedTriggers` (summary ·
// switch · ⋯ Edit/Delete; the row opens the editor). Every write replaces
// the WHOLE array (`agents.update {triggers}`), the Check in tag included.
// The agent page and the app's Events tab (#EXP-1829) mount the same rows.

/// The triggers as the section draws them. The agent page hoists it into
/// its draft (a new agent keeps them locally until Create); a standalone
/// mount owns one and follows the synced row.
@MainActor
@Observable
final class AgentTriggerList {
    /// Stored order. A trigger added before the agent exists (or before the
    /// echo lands) carries a `local-` id and is sent WITHOUT one.
    var triggers: [AgentTrigger] = []
    /// One write at a time: each replaces the whole array.
    var busy = false

    static let localPrefix = "local-"

    /// The whole-array write of `list`: a local (unsaved) trigger travels
    /// without an id, so the server mints one; the preset tag rides along.
    func inputs(_ list: [AgentTrigger]) -> [AgentTriggerInput] {
        list.map { trigger in
            var input = AgentTriggerInput(trigger)
            if trigger.id.hasPrefix(Self.localPrefix) { input.id = nil }
            return input
        }
    }

    /// A write's elements as rows until the echo lands (a new one local).
    func adopting(_ inputs: [AgentTriggerInput]) -> [AgentTrigger] {
        inputs.map { input in
            AgentTrigger(
                id: input.id ?? Self.localPrefix + UUID().uuidString,
                enabled: input.enabled,
                when: input.when,
                preset: input.preset
            )
        }
    }

    /// The list with `input` added (`editing` nil) or swapped in.
    func applying(_ input: AgentTriggerInput, editing: AgentTrigger?) -> [AgentTrigger] {
        guard let editing else {
            return triggers + adopting([AgentTriggerInput(enabled: input.enabled, when: input.when)])
        }
        return triggers.map { trigger in
            trigger.id == editing.id
                ? AgentTrigger(id: trigger.id, enabled: input.enabled, when: input.when, preset: trigger.preset)
                : trigger
        }
    }

    func settingEnabled(_ trigger: AgentTrigger, _ enabled: Bool) -> [AgentTrigger] {
        triggers.map {
            $0.id == trigger.id ? AgentTrigger(id: $0.id, enabled: enabled, when: $0.when, preset: $0.preset) : $0
        }
    }

    func removing(_ trigger: AgentTrigger) -> [AgentTrigger] {
        triggers.filter { $0.id != trigger.id }
    }
}

struct AgentTriggersSection: View {
    /// nil = a new agent (the page's list, written on Create).
    let agentId: String?
    /// Editors add, toggle, edit and delete; everyone else reads.
    let editable: Bool
    private let hoisted: AgentTriggerList?
    private let pageTeamId: String?

    /// Anywhere: an existing agent's triggers, read from its synced row.
    init(agentId: String, editable: Bool) {
        self.agentId = agentId
        self.editable = editable
        hoisted = nil
        pageTeamId = nil
    }

    /// The agent page: its draft's list (the page follows the synced row).
    init(agentId: String?, editable: Bool, teamId: String, list: AgentTriggerList) {
        self.agentId = agentId
        self.editable = editable
        hoisted = list
        pageTeamId = teamId
    }

    @Environment(AppDependencies.self) private var deps: AppDependencies?
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster

    @State private var own = AgentTriggerList()
    @State private var rowTeamId: String?
    /// The editor: the trigger's id, or `new`.
    @State private var editorTarget: EditorTarget?
    @State private var pendingDelete: AgentTrigger?

    private struct EditorTarget: Identifiable {
        let id: String
        let trigger: AgentTrigger?
    }

    private var list: AgentTriggerList { hoisted ?? own }
    private var teamId: String? { pageTeamId ?? rowTeamId }
    private var listed: [AgentTrigger] { AgentSettings.listedTriggers(list.triggers) }
    private var checkIn: AgentSettings.AssistantScheduleId { AgentSettings.checkInOf(list.triggers) }

    var body: some View {
        Section {
            // No empty-state row: Check in `Off` says it.
            checkInRow
            ForEach(listed) { trigger in
                triggerRow(trigger)
            }
        } header: {
            GlassSectionHeader(AgentSettings.Copy.triggers) {
                if editable, list.triggers.count < DomainContract.agentTriggerMaxPerAgent {
                    GlassPill(AgentSettings.Copy.addTrigger, icon: AppIcons.uiAdd, mode: .action {
                        editorTarget = EditorTarget(id: "new", trigger: nil)
                    }, enabled: !list.busy)
                    .fixedSize()
                    .accessibilityIdentifier("add-trigger")
                    .sheet(item: Binding(
                        get: { editorTarget?.trigger == nil ? editorTarget : nil },
                        set: { editorTarget = $0 }
                    )) { _ in editor(nil) }
                }
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    // MARK: - Rows

    /// THE single-select Picker over the presets, written at once.
    private var checkInRow: some View {
        GlassPickerRow(
            AgentSettings.Copy.checkIn,
            selection: Binding<AgentSettings.AssistantScheduleId?>(
                get: { checkIn },
                set: { picked in if let picked { setCheckIn(picked) } }
            ),
            options: AgentSettings.assistantSchedules.map { Optional($0.id) },
            label: { id in
                AgentSettings.assistantSchedules.first { $0.id == id }?.label ?? ""
            },
            enabled: editable && !list.busy
        )
        .accessibilityIdentifier("agent-check-in-row")
        // A standalone mount follows the synced row itself.
        .task(id: "\(accountId)/\(agentId ?? "")") {
            if hoisted == nil { await observe() }
        }
    }

    /// The row's body opens the editor; the switch and the ⋯ menu (Edit,
    /// Delete) sit BESIDE it (a control nested in a button's label has its
    /// tap swallowed). Each row presents its own editor and confirm, so the
    /// presenter is always the row on screen.
    private func triggerRow(_ trigger: AgentTrigger) -> some View {
        let sentence = AgentSettings.triggerSummary(trigger)
        return HStack(spacing: 8) {
            Button { editorTarget = EditorTarget(id: trigger.id, trigger: trigger) } label: {
                HStack(spacing: 0) {
                    Text(sentence)
                        .foregroundStyle(.white.opacity(trigger.enabled ? TextOpacity.primary : TextOpacity.tertiary))
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!editable)

            GlassToggleRow(nil, isOn: Binding(
                get: { trigger.enabled },
                set: { write(list.settingEnabled(trigger, $0), verb: "save") }
            ), style: .compact)
            .disabled(!editable || list.busy)
            .accessibilityLabel("Enabled: \(sentence)")
            .accessibilityIdentifier("trigger-enabled")

            if editable {
                GlassMenu {
                    GlassMenuItem("Edit", icon: AppIcons.uiEdit) {
                        editorTarget = EditorTarget(id: trigger.id, trigger: trigger)
                    }
                    GlassMenuItem("Delete", icon: AppIcons.uiDelete, destructive: true) {
                        pendingDelete = trigger
                    }
                } label: {
                    GhostIconLabel(AppIcons.uiMore)
                        .accessibilityLabel("Trigger menu for \(sentence)")
                }
                .accessibilityIdentifier("trigger-menu")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("trigger-row")
        .sheet(item: Binding(
            get: { editorTarget?.trigger?.id == trigger.id ? editorTarget : nil },
            set: { editorTarget = $0 }
        )) { _ in editor(trigger) }
        // EXP-1215: the app's own alert card, on a zero-size node of its own.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: Binding(
                    get: { pendingDelete?.id == trigger.id ? pendingDelete : nil },
                    set: { pendingDelete = $0 }
                )) { trigger in
                    GlassAlert(
                        prompt: Prompts.DeleteTrigger.copy(),
                        handlers: ["delete": { write(list.removing(trigger), verb: "delete") }]
                    )
                }
        }
    }

    @ViewBuilder
    private func editor(_ editing: AgentTrigger?) -> some View {
        if let teamId {
            TriggerFormSheet(teamId: teamId, editing: editing) { input in
                write(list.applying(input, editing: editing), verb: "save")
            }
            .environment(\.accountId, accountId)
        }
    }

    // MARK: - Writes

    /// A pick replaces the preset trigger in front (`withCheckIn`), a new
    /// one in the device's zone.
    private func setCheckIn(_ id: AgentSettings.AssistantScheduleId) {
        guard editable, !list.busy, id != checkIn else { return }
        let next = AgentSettings.withCheckIn(list.inputs(list.triggers), id, tz: TimeZone.current.identifier)
        write(list.adopting(next), verb: "save", thing: "the check-in")
    }

    /// A new agent keeps its triggers locally until Create; an existing one
    /// writes the WHOLE array at once (optimistic, rolled back on refusal).
    private func write(_ next: [AgentTrigger], verb: String, thing: String = "the trigger") {
        guard editable else { return }
        guard let agentId, let deps else {
            list.triggers = next
            return
        }
        guard !list.busy else { return }
        let previous = list.triggers
        list.triggers = next
        list.busy = true
        let inputs = list.inputs(next)
        let target = list
        Task {
            do {
                try await deps.agentsApi.update(
                    accountId: accountId, id: agentId, patch: AgentPatch(triggers: inputs)
                )
            } catch {
                target.triggers = previous
                toaster.error(
                    FeedbackStates.Errors.title(verb: verb, thing: thing),
                    description: error.trpcUserMessage
                )
            }
            target.busy = false
        }
    }

    // MARK: - Load (standalone)

    private func observe() async {
        guard let agentId, let deps, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try AgentDefinitionEntity.fetchOne(db, key: agentId)
        }
        do {
            for try await row in observation.values(in: pool) {
                guard let row else { continue }
                rowTeamId = row.teamId
                // The synced row is the truth once no write of ours flies.
                if !own.busy { own.triggers = AgentTrigger.parseList(row.triggers) }
            }
        } catch {}
    }
}
