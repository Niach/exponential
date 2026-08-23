import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-615: "New action" is its own sheet. It used to be a MODE of the
// Start-coding sheet (EXP-431's `createActionMode`), which meant the creation
// flow inherited a subject switch it had to hide, an action picker it had to
// suppress, and — once suggestions could seed an automation (EXP-583) — a
// trigger block that only appeared for one entry point. Now creation looks
// like every other create form on mobile: icon + name, description,
// repository, an ALWAYS-visible automation row that pushes a detail form, and
// the shared launch options for the run that authors it.
//
// Creation itself is unchanged: the builtin "Create action" run does the
// writing (there is no manual action form on any client since EXP-257), so
// submitting starts that action on a machine with the typed inputs, and a
// configured automation rides the description as the byte-identical
// `AutomationNote` block the creator agent copies into
// `exponential_automations_create`.
struct CreateActionSheet: View {
    let teamId: String
    /// Machines a builtin action run can be sent to (online, runnable,
    /// `actions` + `action-inputs`).
    let devices: [SteerDevice]
    /// Automation-capable machines (cap `automations`), OFFLINE INCLUDED —
    /// the automation binds independently of whatever machine runs the
    /// creator session, and a sleeping box still owns the binding.
    let automationDevices: [SteerDevice]
    /// EXP-530 "Use suggestion": the seeded description + icon.
    var prefillDescription: String = ""
    var prefillIcon: String = ""
    /// EXP-583: an "Action + automation" seed's trigger. Unlike before, an
    /// automation can also be configured from scratch — this only PRE-fills.
    var prefillAutomation: AutomationTrigger?
    /// The builtin create run: device, action, options, resolved inputs.
    let onSubmit: (SteerDevice, ActionDto, SteerStartOptions, [String: String]) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var descriptionText = ""
    @State private var icon = ""
    @State private var repoId = ""
    @State private var repos: [TeamRepo] = []

    // Automation (optional): `hasAutomation` is the row's on/off state, the
    // draft its when-part, and the device/agent pins its how-part.
    @State private var hasAutomation = false
    @State private var draft = AutomationDraft()
    @State private var filterOptions = AutomationFilterOptions()
    @State private var automationDeviceId = ""
    @State private var automationAgent = LaunchVocabulary.deviceDefault
    @State private var automationModel = LaunchVocabulary.deviceDefault
    @State private var automationEffort = LaunchVocabulary.deviceDefault

    // The creator RUN's own options, seeded from the resolved machine's
    // advertised defaults (EXP-437) exactly like the Start-coding sheet.
    @State private var deviceId: String?
    @State private var agent = "claude"
    @State private var model = ""
    @State private var effort = LaunchVocabulary.cliDefault
    @State private var ultracode = false
    @State private var planMode = false
    @State private var skipPermissions = false
    @State private var seeded = false
    @State private var lastSeededDeviceId: String?

    /// The action being run: the "Create action" builtin, whose input defs
    /// are the contract for what this form collects.
    private var builtin: ActionDto {
        ActionDto.builtinCreateAction(teamId: teamId)
    }

    private var device: SteerDevice? {
        if let deviceId, let match = devices.first(where: { $0.deviceId == deviceId }) {
            return match
        }
        return devices.first
    }

    private var availableAgents: [String] {
        let supported = device?.agentIds ?? ["claude"]
        let ordered = DomainContract.codingAgentValues.filter { supported.contains($0) }
        return ordered.isEmpty ? ["claude"] : ordered
    }

    private var canSubmit: Bool {
        device != nil
            && !descriptionText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && descriptionText.count <= DomainContract.actionInputTextMax
            && name.count <= DomainContract.actionInputTextMax
    }

    var body: some View {
        NavigationStack {
            Form {
                identitySection
                descriptionSection
                repositorySection
                automationSection
                LaunchOptionsSection(
                    variant: .launch,
                    devices: devices,
                    deviceId: deviceBinding,
                    noDeviceNote: noDeviceNote,
                    availableAgents: availableAgents,
                    agent: agent,
                    onAgentChange: selectAgent,
                    model: $model,
                    effort: $effort,
                    ultracode: $ultracode,
                    planMode: $planMode,
                    skipPermissions: $skipPermissions
                )
            }
            // EXP-603: the app background instead of the system grouped-list
            // gray; rows carry the glass fill.
            .scrollContentBackground(.hidden)
            .background(AppBackground())
            .navigationTitle("New action")
            .navigationBarTitleDisplayMode(.inline)
            .listSectionSpacing(12)
            // EXP-594: white control tint — system blue is retired.
            .tint(DesignTokens.Palette.primary)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        submit()
                    } label: {
                        HStack(spacing: 6) {
                            AppIcon(AppIcons.actionCreate, size: 14)
                            Text("Create")
                        }
                    }
                    .disabled(!canSubmit)
                }
            }
        }
        .presentationDetents([.large])
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("create-action-sheet")
        .onAppear { seed() }
        .task { await load() }
    }

    // MARK: - Form sections

    /// Icon + name on ONE row (web/desktop parity): the glyph the action will
    /// wear, and an optional name — blank lets the agent pick one.
    private var identitySection: some View {
        Section {
            HStack(spacing: 12) {
                IconPicker(selection: $icon, allowsNone: true)
                TextField("Leave blank to let the agent name it", text: $name)
                    .accessibilityIdentifier("create-action-name")
            }
        } header: {
            Text("Name")
        }
        .listRowBackground(glassFormRowFill)
    }

    private var descriptionSection: some View {
        Section {
            TextField("What should this action do?", text: $descriptionText, axis: .vertical)
                .lineLimit(4...10)
                .accessibilityIdentifier("create-action-description")
        } header: {
            Text("Description")
        }
        .listRowBackground(glassFormRowFill)
    }

    @ViewBuilder
    private var repositorySection: some View {
        Section {
            GlassPickerRow(
                "Repository",
                selection: $repoId,
                options: [""] + repos.map(\.id),
                label: { id in
                    guard !id.isEmpty else { return "None" }
                    return repos.first { $0.id == id }?.fullName ?? id
                }
            )
        }
        .listRowBackground(glassFormRowFill)
    }

    /// The always-visible automation row — an issue-row shaped summary that
    /// pushes the detail form. "No automation" is the empty state; a
    /// configured one reads as its trigger sentence plus the bound machine.
    private var automationSection: some View {
        Section {
            NavigationLink {
                automationDetail
            } label: {
                HStack(spacing: 12) {
                    AppIcon(AppIcons.actionAutomation, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    VStack(alignment: .leading, spacing: 3) {
                        Text(hasAutomation
                            ? AutomationTriggerDisplay.summary(draft.trigger)
                            : "No automation")
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        if hasAutomation, let bound = automationDevice {
                            Text(LaunchVocabulary.deviceCaption(bound))
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                        }
                    }
                    Spacer(minLength: 0)
                }
            }
        } header: {
            Text("Automation")
        } footer: {
            if hasAutomation {
                Text("The agent sets this up after writing the action.")
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    /// The pushed detail: the same trigger rows and machine/agent options the
    /// automation form sheet renders, plus the on/off switch.
    private var automationDetail: some View {
        Form {
            Section {
                Toggle("Automate this action", isOn: $hasAutomation)
            } footer: {
                if automationDevices.isEmpty {
                    Text(AutomationCopy.noAutomationDevice)
                }
            }
            .listRowBackground(glassFormRowFill)

            if hasAutomation {
                AutomationTriggerForm(draft: $draft, options: filterOptions)
                LaunchOptionsSection(
                    variant: .automation,
                    devices: automationDevices,
                    deviceId: automationDeviceBinding,
                    noDeviceNote: AutomationCopy.noAutomationDevice,
                    availableAgents: LaunchVocabulary.agents(of: automationDevice),
                    agent: automationAgent,
                    onAgentChange: selectAutomationAgent,
                    model: $automationModel,
                    effort: $automationEffort
                )
            }
        }
        .scrollContentBackground(.hidden)
        .background(AppBackground())
        .navigationTitle("Automation")
        .navigationBarTitleDisplayMode(.inline)
        .listSectionSpacing(12)
        .tint(DesignTokens.Palette.primary)
    }

    // MARK: - Devices

    private var noDeviceNote: String {
        "No actions-capable desktop online. Open (or update) the Exponential desktop app."
    }

    private var deviceBinding: Binding<String> {
        Binding(
            get: { device?.deviceId ?? "" },
            set: { value in
                let switched = value != lastSeededDeviceId
                deviceId = value
                clampAgentToDevice()
                if switched { applyDeviceDefaults() }
            }
        )
    }

    /// The machine that will RUN the automation, resolved off its own pool.
    private var automationDevice: SteerDevice? {
        if !automationDeviceId.isEmpty,
           let match = automationDevices.first(where: { $0.deviceId == automationDeviceId }) {
            return match
        }
        return automationDevices.first
    }

    /// Both automation writes clear DOWNSTREAM picks the way an explicit user
    /// switch must: a machine that doesn't advertise the pinned agent would be
    /// rejected, and model/effort vocabularies are per-agent.
    private var automationDeviceBinding: Binding<String> {
        Binding(
            get: { automationDevice?.deviceId ?? "" },
            set: { value in
                guard value != automationDevice?.deviceId else { return }
                automationDeviceId = value
                let agents = LaunchVocabulary.agents(of: automationDevice)
                if automationAgent != LaunchVocabulary.deviceDefault,
                   !agents.contains(automationAgent) {
                    automationAgent = LaunchVocabulary.deviceDefault
                    automationModel = LaunchVocabulary.deviceDefault
                    automationEffort = LaunchVocabulary.deviceDefault
                }
            }
        )
    }

    private func selectAutomationAgent(_ value: String) {
        guard value != automationAgent else { return }
        automationAgent = value
        automationModel = LaunchVocabulary.deviceDefault
        automationEffort = LaunchVocabulary.deviceDefault
    }

    // MARK: - Launch options (EXP-437, the Start-coding sheet's rules)

    private func selectAgent(_ value: String) {
        guard value != agent else { return }
        agent = value
        applyAgentDefaults(for: value)
    }

    private func clampAgentToDevice() {
        if !availableAgents.contains(agent) {
            selectAgent(availableAgents.first ?? "claude")
        }
    }

    private func seed() {
        guard !seeded else { return }
        seeded = true
        descriptionText = prefillDescription
        icon = prefillIcon
        if let prefillAutomation {
            hasAutomation = true
            draft = AutomationDraft(trigger: prefillAutomation)
        }
        applyDeviceDefaults()
    }

    private func applyDeviceDefaults() {
        if let advertised = device?.defaultLaunchAgent {
            agent = advertised
        } else if !availableAgents.contains(agent) {
            agent = availableAgents.first ?? "claude"
        }
        applyAgentDefaults(for: agent)
        lastSeededDeviceId = device?.deviceId
    }

    private func applyAgentDefaults(for value: String) {
        let advertised = device?.agentDefaults(for: value)
        model = seedModel(advertised?.model, for: value)
        effort = seedEffort(advertised?.effort, for: value)
        ultracode = advertised?.ultracode ?? false
        planMode = advertised?.planMode ?? false
        skipPermissions = advertised?.skipPermissions ?? false
        if agent != "claude" { ultracode = false }
        if !LaunchVocabulary.supportsPlanMode(agent) { planMode = false }
        if agent == "pi" { skipPermissions = false }
    }

    private func seedModel(_ value: String?, for agent: String) -> String {
        guard let value, !value.isEmpty,
              LaunchVocabulary.modelValues(for: agent).contains(value)
        else {
            return LaunchVocabulary.defaultModel(for: agent)
        }
        return value
    }

    private func seedEffort(_ value: String?, for agent: String) -> String {
        guard let value, LaunchVocabulary.effortValues(for: agent).contains(value) else {
            return LaunchVocabulary.cliDefault
        }
        return value
    }

    // MARK: - Load / submit

    @MainActor
    private func load() async {
        repos = (try? await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)) ?? []
        filterOptions = await AutomationFilterOptions.load(
            db: deps.db, accountId: accountId, teamId: teamId
        )
    }

    /// The automation the creator agent should set up, or nil when the row is
    /// off or no machine can run one.
    private var configuredAutomation: AutomationSpec? {
        guard hasAutomation, let bound = automationDevice else { return nil }
        let pinned = automationAgent == LaunchVocabulary.deviceDefault ? nil : automationAgent
        return AutomationSpec(
            trigger: draft.trigger,
            deviceId: bound.deviceId,
            agent: pinned,
            model: pinned == nil ? nil : automationModel,
            effort: pinned == nil ? nil : automationEffort
        )
    }

    private func submit() {
        guard let device, canSubmit else { return }
        let action = builtin
        let options = SteerStartOptions(
            agent: agent,
            model: model == LaunchVocabulary.cliDefault ? "" : model,
            effort: effort == LaunchVocabulary.cliDefault ? "" : effort,
            ultracode: agent == "claude" ? ultracode : nil,
            planMode: LaunchVocabulary.supportsPlanMode(agent) ? planMode : nil,
            skipPermissions: agent == "pi" ? nil : skipPermissions
        )
        // Values in wire form: text trimmed, blank optionals dropped — the
        // very same helper the action inputs go through.
        var values = ActionInputValues.wireValues(
            action.inputs ?? [],
            values: [
                "description": descriptionText,
                "name": name,
                "repo": repoId,
                "icon": icon,
            ]
        )
        // EXP-583: a configured automation rides the description as a
        // machine-readable trailing block the creator agent copies verbatim
        // into `exponential_automations_create` (byte-identical across the
        // four clients — see AutomationNote.format).
        if let spec = configuredAutomation {
            values["description"] = (values["description"] ?? "") + AutomationNote.format(spec)
        }
        dismiss()
        onSubmit(device, action, options, values)
    }
}
