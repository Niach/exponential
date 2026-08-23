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
//
// EXP-615: the trigger rows are the shared `AutomationTriggerForm` and the
// machine/agent/model/effort rows the shared `LaunchOptionsSection` in its
// automation variant, so this sheet and the create-action sheet's Automation
// detail cannot drift apart.
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

    @State private var actionId = ""
    @State private var deviceId = ""
    @State private var draft = AutomationDraft()
    @State private var agent = LaunchVocabulary.deviceDefault
    @State private var model = LaunchVocabulary.deviceDefault
    @State private var effort = LaunchVocabulary.deviceDefault
    @State private var filterOptions = AutomationFilterOptions()
    @State private var seeded = false

    /// Real actions only — builtins never automate.
    private var customActions: [ActionDto] {
        actions.filter { !$0.isBuiltin }
    }

    /// Automatable targets: real actions with no required input.
    private var eligibleActions: [ActionDto] {
        customActions.filter { action in
            !(action.inputs ?? []).contains(where: \.isRequired)
        }
    }

    private var selectedDevice: SteerDevice? {
        devices.first { $0.deviceId == deviceId }
    }

    /// The chosen machine's runnable agents, in contract order.
    private var availableAgents: [String] {
        LaunchVocabulary.agents(of: selectedDevice)
    }

    /// Model/effort are only meaningful against an agent — with none pinned
    /// the device's own per-agent defaults apply.
    private var pinnedAgent: String? {
        agent == LaunchVocabulary.deviceDefault ? nil : agent
    }

    private var canSave: Bool {
        !actionId.isEmpty && !deviceId.isEmpty
    }

    var body: some View {
        NavigationStack {
            Form {
                actionSection
                AutomationTriggerForm(draft: $draft, options: filterOptions)
                LaunchOptionsSection(
                    variant: .automation,
                    devices: devices,
                    deviceId: deviceBinding,
                    noDeviceNote: AutomationCopy.noAutomationDevice,
                    availableAgents: availableAgents,
                    agent: agent,
                    onAgentChange: selectAgent,
                    model: $model,
                    effort: $effort
                )
            }
            // EXP-603: the app background instead of the system grouped-list
            // gray; rows carry the glass fill.
            .scrollContentBackground(.hidden)
            .background(AppBackground())
            .navigationTitle(editing == nil ? "New automation" : "Edit automation")
            .navigationBarTitleDisplayMode(.inline)
            .listSectionSpacing(12)
            // EXP-594: white control tint — system blue is retired.
            .tint(DesignTokens.Palette.primary)
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
        .task {
            filterOptions = await AutomationFilterOptions.load(
                db: deps.db, accountId: accountId, teamId: teamId
            )
        }
    }

    // MARK: - Sections

    @ViewBuilder
    private var actionSection: some View {
        Section {
            if eligibleActions.isEmpty {
                Text(
                    customActions.isEmpty
                        ? AutomationCopy.noAutomatableActions
                        : AutomationCopy.requiredInputsHint
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            } else {
                GlassPickerRow(
                    "Action",
                    selection: $actionId,
                    options: eligibleActions.map(\.id),
                    label: { id in
                        eligibleActions.first { $0.id == id }?.name ?? id
                    }
                )
            }
        } header: {
            Text("Action")
        }
        .listRowBackground(glassFormRowFill)
    }

    // MARK: - Bindings

    /// Both writes clear DOWNSTREAM picks the way an explicit user switch
    /// must (and `seed()`'s prefill deliberately must not — hence a binding
    /// rather than onChange): a machine that doesn't advertise the pinned
    /// agent would be rejected server-side, and model/effort vocabularies are
    /// per-agent, so a stale pick can never ride along.
    private var deviceBinding: Binding<String> {
        Binding(
            get: { deviceId },
            set: { value in
                guard value != deviceId else { return }
                deviceId = value
                if agent != LaunchVocabulary.deviceDefault, !availableAgents.contains(agent) {
                    agent = LaunchVocabulary.deviceDefault
                    model = LaunchVocabulary.deviceDefault
                    effort = LaunchVocabulary.deviceDefault
                }
            }
        )
    }

    private func selectAgent(_ value: String) {
        guard value != agent else { return }
        agent = value
        model = LaunchVocabulary.deviceDefault
        effort = LaunchVocabulary.deviceDefault
    }

    // MARK: - Seed / submit

    private func seed() {
        guard !seeded else { return }
        seeded = true
        if let editing {
            actionId = editing.actionId
            deviceId = editing.deviceId
            agent = editing.agent ?? LaunchVocabulary.deviceDefault
            model = editing.model ?? LaunchVocabulary.deviceDefault
            effort = editing.effort ?? LaunchVocabulary.deviceDefault
            draft = AutomationDraft(trigger: editing.parsedTrigger)
        }
        if actionId.isEmpty { actionId = eligibleActions.first?.id ?? "" }
        if deviceId.isEmpty { deviceId = devices.first?.deviceId ?? "" }
    }

    private func submit() {
        guard canSave else { return }
        // Snapshot before dismissing — the payload must not depend on what
        // the teardown does to the sheet's state.
        let launch = AutomationLaunchPatch(
            agent: pinnedAgent,
            model: pinnedAgent == nil || model == LaunchVocabulary.deviceDefault ? nil : model,
            effort: pinnedAgent == nil || effort == LaunchVocabulary.deviceDefault ? nil : effort
        )
        let payload = (actionId, deviceId, draft.trigger, launch)
        dismiss()
        onSubmit(payload.0, payload.1, payload.2, payload.3)
    }
}
