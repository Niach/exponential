import ExpCore
import ExpUI
import GRDB
import SwiftUI

// The "Add trigger" / "Edit trigger" form (EXP-583; SLOP-2: a trigger belongs
// to the action whose Triggers tab opens this, so there is no action picker).
// A trigger is a when-part — schedule or event — plus its runner: the device
// that fires it and that device's account/model/effort. EXP-995: the pin is
// picked off THE account picker (brand mark + email over the bound machine's
// logins, its default first) and the agent rides the pick; a blank
// model/effort stores nothing. Owner-only: the tab hides the entry points for
// everyone else, and the server refuses anyway.
//
// EXP-615: the when-part rows are the shared `TriggerForm` and the
// machine/account/model/effort rows the shared `LaunchOptionsSection` in its
// trigger variant.
struct TriggerFormSheet: View {
    let teamId: String
    /// Trigger-capable machines (cap `automations`), OFFLINE INCLUDED: a
    /// sleeping machine still owns the trigger and fires the missed schedule
    /// when it comes back.
    let devices: [SteerDevice]
    /// nil = add a new trigger.
    let editing: ActionTrigger?
    /// The configured trigger: `editing`'s id and enabled flag kept, a new
    /// one without an id (the server mints it) and enabled.
    let onSubmit: (ActionTriggerInput) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    @State private var deviceId = ""
    @State private var draft = TriggerDraft()
    @State private var agent = ""
    /// EXP-995: the agent profile id the run spends; "" = the bound
    /// machine's last used login for `agent` (stores NULL).
    @State private var account = ""
    @State private var model = LaunchVocabulary.cliDefault
    @State private var effort = LaunchVocabulary.cliDefault
    @State private var filterOptions = TriggerFilterOptions()
    @State private var seeded = false

    private var selectedDevice: SteerDevice? {
        devices.first { $0.deviceId == deviceId }
    }

    /// The chosen machine's runnable agents, in contract order.
    private var availableAgents: [String] {
        LaunchVocabulary.agents(of: selectedDevice)
    }

    /// EXP-995: the ONE account list — every signed-in login the bound
    /// machine reports, across agents, its last used login first
    /// (`AccountOptions.flatten`, the composer's list). A machine that reports
    /// no login (or none bound yet) still offers a row per runnable agent,
    /// named by the agent (blank id = unpinned), so the pin can be made
    /// before the heartbeat lands.
    private var accountOptions: [AccountOption] {
        let device = selectedDevice
        let options = AccountOptions.flatten(
            accounts: device?.agentAccounts,
            usage: device?.agentUsage,
            launchDefaults: device?.launchDefaults
        )
        if !options.isEmpty { return options }
        let lastUsedAgent = LaunchVocabulary.lastUsedAgent(of: device)
        return availableAgents.map { value in
            AccountOption(
                id: "",
                agent: value,
                email: LaunchVocabulary.agentLabel(value),
                isLastUsed: value == lastUsedAgent
            )
        }
    }

    /// Which option the row reads back as: the stored (agent, account) pair,
    /// else that agent's first login — its last used one, what an UNPINNED
    /// (NULL) account runs on, or a profile the machine no longer reports —
    /// else the first row.
    private var selectedAccount: AccountOption? {
        let options = accountOptions
        return options.first { $0.agent == agent && $0.id == account }
            ?? options.first { $0.agent == agent }
            ?? options.first
    }

    private var canSave: Bool {
        !deviceId.isEmpty
    }

    var body: some View {
        GlassSheetChrome(
            title: editing == nil ? "New trigger" : "Edit trigger",
            height: .full,
            content: {
                Form {
                    TriggerForm(draft: $draft, options: filterOptions)
                    LaunchOptionsSection(
                        variant: .trigger,
                        devices: devices,
                        deviceId: deviceBinding,
                        noDeviceNote: TriggerCopy.noTriggerDevice,
                        availableAgents: availableAgents,
                        agent: agent,
                        onAgentChange: selectAgent,
                        accountOptions: accountOptions,
                        selectedAccount: selectedAccount,
                        onAccountSelect: selectAccount,
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
                GlassSubmitButton(
                    editing == nil ? "Add trigger" : "Save changes",
                    enabled: canSave
                ) {
                    submit()
                }
            }
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("trigger-form-sheet")
        .onAppear { seed() }
        .onChange(of: devices.count) {
            // The machine pool can arrive after the sheet does — bind and seed
            // the agent as soon as it lands.
            if deviceId.isEmpty { deviceId = defaultDeviceId }
            seedAgentFromDevice()
        }
        .task {
            filterOptions = await TriggerFilterOptions.load(
                db: deps.db, accountId: accountId, teamId: teamId
            )
        }
    }

    // MARK: - Bindings

    /// Switching machines re-seeds the pin (EXP-615) rather than dropping
    /// it — a trigger always names a concrete agent now.
    private var deviceBinding: Binding<String> {
        Binding(
            get: { deviceId },
            set: { value in
                guard value != deviceId else { return }
                deviceId = value
                seedAgentFromDevice(rebound: true)
            }
        )
    }

    /// Seed (and re-seed) the pin off the bound machine: an unset pin — a
    /// trigger saved before EXP-615 carries no agent — or one the bound
    /// machine cannot run falls back to that machine's LAST USED login, which
    /// names the agent (EXP-995: the composer's seed), clamped to what it
    /// advertises. Model/effort vocabularies are per-agent, so an agent
    /// change clears them to the "CLI default" blank.
    ///
    /// A pin the machine CAN run is left alone on the first bind, so a manual
    /// pick sticks — but a profile id (`agent_profiles`) is DEVICE-LOCAL, so
    /// `rebound` (the machine row just switched) re-seeds it from the NEW
    /// machine: the same agent's login there (its active one first, exactly
    /// the row `selectedAccount` would read back), so what the sheet shows is
    /// what Save stores rather than a stale id from the previous machine.
    private func seedAgentFromDevice(rebound: Bool = false) {
        guard selectedDevice != nil else { return }
        let runsPinnedAgent = !agent.isEmpty && availableAgents.contains(agent)
        guard rebound || !runsPinnedAgent else { return }
        let options = accountOptions
        let fallback: AccountOption? = runsPinnedAgent
            ? options.first { $0.agent == agent && $0.id == account }
                ?? options.first { $0.agent == agent }
            : AccountOptions.lastUsed(options)
        let previousAgent = agent
        if let fallback {
            agent = fallback.agent
            account = fallback.id
        } else {
            agent = LaunchVocabulary.lastUsedAgent(of: selectedDevice)
            account = ""
        }
        guard agent != previousAgent else { return }
        model = LaunchVocabulary.cliDefault
        effort = LaunchVocabulary.cliDefault
    }

    private func selectAgent(_ value: String) {
        guard value != agent else { return }
        agent = value
        // A profile belongs to ONE agent, and the model/effort vocabularies
        // are per agent — a switch has to reset them, or a stale value hits a
        // server refusal.
        account = ""
        model = LaunchVocabulary.cliDefault
        effort = LaunchVocabulary.cliDefault
    }

    /// EXP-995: take BOTH halves of a picked option — the agent first (which
    /// clears the per-agent pins), then the login on top, VERBATIM; a blank
    /// (NULL) account is unpinned and runs on the machine's last used login
    /// (EXP-1158).
    private func selectAccount(_ option: AccountOption) {
        selectAgent(option.agent)
        account = option.id
    }

    // MARK: - Seed / submit

    private func seed() {
        guard !seeded else { return }
        seeded = true
        if let editing {
            deviceId = editing.deviceId
            agent = editing.agent ?? ""
            // A legacy `system` pin (the retired ambient login) = unpinned.
            account = AccountOptions.wireAccount(editing.account) ?? ""
            model = editing.model ?? LaunchVocabulary.cliDefault
            effort = editing.effort ?? LaunchVocabulary.cliDefault
            draft = TriggerDraft(trigger: editing.when)
        }
        if deviceId.isEmpty { deviceId = defaultDeviceId }
        seedAgentFromDevice()
    }

    /// EXP-622: seed the runner to the caller's default machine when it is
    /// one of the trigger-capable candidates, else the first of them.
    private var defaultDeviceId: String {
        (devices.first(where: \.isDefaultDevice) ?? devices.first)?.deviceId ?? ""
    }

    private func submit() {
        guard canSave else { return }
        // Snapshot before dismissing — the payload must not depend on what
        // the teardown does to the sheet's state. A blank model/effort is the
        // "CLI default" that stores nothing.
        let input = ActionTriggerInput(
            id: editing?.id,
            enabled: editing?.enabled ?? true,
            deviceId: deviceId,
            agent: agent.isEmpty ? nil : agent,
            // EXP-1158: the pick VERBATIM; NULL = unpinned = the machine's
            // last used login.
            account: agent.isEmpty ? nil : AccountOptions.wireAccount(account),
            model: model.isEmpty || model == LaunchVocabulary.cliDefault ? nil : model,
            effort: effort.isEmpty || effort == LaunchVocabulary.cliDefault ? nil : effort,
            when: draft.trigger
        )
        dismiss()
        onSubmit(input)
    }
}
