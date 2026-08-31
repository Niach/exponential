import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-583: the "+ New automation" / "Edit automation" form — the mobile twin
// of the web's automation dialog. An automation binds ONE action to ONE
// device with a schedule/event trigger and its own agent/model/effort — the
// agent seeds from the bound machine's own default (EXP-615: there is no
// "Device default" pill any more), a blank model/effort stores NULL. Owner-
// only: the list hides the entry points for everyone else, and the server
// refuses anyway.
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
    @State private var agent = ""
    @State private var model = LaunchVocabulary.cliDefault
    @State private var effort = LaunchVocabulary.cliDefault
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

    private var canSave: Bool {
        !actionId.isEmpty && !deviceId.isEmpty
    }

    var body: some View {
        GlassSheetChrome(
            title: editing == nil ? "New automation" : "Edit automation",
            height: .full,
            content: {
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
                // EXP-603: the sheet's own background shows through the
                // grouped list; rows carry the glass fill.
                .scrollContentBackground(.hidden)
                .listSectionSpacing(8)
                // EXP-594: white control tint — system blue is retired.
                .tint(DesignTokens.Palette.primary)
            },
            primaryAction: {
                // Web-parity wording (EXP-615).
                GlassSubmitButton(
                    editing == nil ? "Create automation" : "Save changes",
                    enabled: canSave
                ) {
                    submit()
                }
            }
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("automation-form-sheet")
        .onAppear { seed() }
        .onChange(of: devices.count) {
            // The machine pool can arrive after the sheet does — bind and seed
            // the agent as soon as it lands.
            if deviceId.isEmpty { deviceId = defaultDeviceId }
            seedAgentFromDevice()
        }
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
        }
        // No "Action" section header — the row below already says it
        // (EXP-615 dedupe).
        .listRowBackground(glassFormRowFill)
    }

    // MARK: - Bindings

    /// Switching machines re-seeds the agent (EXP-615) rather than dropping
    /// the pin — a binding always names a concrete agent now.
    private var deviceBinding: Binding<String> {
        Binding(
            get: { deviceId },
            set: { value in
                guard value != deviceId else { return }
                deviceId = value
                seedAgentFromDevice()
            }
        )
    }

    /// Seed (and re-seed) the agent off the bound machine: an unset pin — a
    /// row saved before EXP-615 carries a NULL agent — or one the newly picked
    /// machine cannot run falls back to that machine's default launch agent,
    /// clamped to what it advertises. A pin it CAN run is left alone, so a
    /// manual pick sticks. Model/effort vocabularies are per-agent, so a
    /// re-seed clears them to the "CLI default" blank.
    private func seedAgentFromDevice() {
        guard selectedDevice != nil else { return }
        guard agent.isEmpty || !availableAgents.contains(agent) else { return }
        agent = LaunchVocabulary.defaultAgent(of: selectedDevice)
        model = LaunchVocabulary.cliDefault
        effort = LaunchVocabulary.cliDefault
    }

    private func selectAgent(_ value: String) {
        guard value != agent else { return }
        agent = value
        // The model/effort vocabularies are per agent — a switch has to reset
        // them, or a stale value hits a server refusal.
        model = LaunchVocabulary.cliDefault
        effort = LaunchVocabulary.cliDefault
    }

    // MARK: - Seed / submit

    private func seed() {
        guard !seeded else { return }
        seeded = true
        if let editing {
            actionId = editing.actionId
            deviceId = editing.deviceId
            agent = editing.agent ?? ""
            model = editing.model ?? LaunchVocabulary.cliDefault
            effort = editing.effort ?? LaunchVocabulary.cliDefault
            draft = AutomationDraft(trigger: editing.parsedTrigger)
        }
        if actionId.isEmpty { actionId = eligibleActions.first?.id ?? "" }
        if deviceId.isEmpty { deviceId = defaultDeviceId }
        seedAgentFromDevice()
    }

    /// EXP-622: seed the binding to the caller's default machine when it is
    /// one of the automation-capable candidates, else the first of them.
    private var defaultDeviceId: String {
        (devices.first(where: \.isDefaultDevice) ?? devices.first)?.deviceId ?? ""
    }

    private func submit() {
        guard canSave else { return }
        // Snapshot before dismissing — the payload must not depend on what
        // the teardown does to the sheet's state. A blank model/effort is the
        // "CLI default" that stores NULL.
        let launch = AutomationLaunchPatch(
            agent: agent.isEmpty ? nil : agent,
            model: model.isEmpty || model == LaunchVocabulary.cliDefault ? nil : model,
            effort: effort.isEmpty || effort == LaunchVocabulary.cliDefault ? nil : effort
        )
        let payload = (actionId, deviceId, draft.trigger, launch)
        dismiss()
        onSubmit(payload.0, payload.1, payload.2, payload.3)
    }
}
