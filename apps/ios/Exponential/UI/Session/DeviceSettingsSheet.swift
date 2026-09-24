import ExpUI
import ExpCore
import SwiftUI

// The device settings sheet (EXP-481) — the settings gear on a device row
// opens it, the iOS twin of the web/IDE device-settings dialog. Six sections,
// no Save buttons above the last two (EXP-490):
//   Name     — devices.rename (registry-authoritative, works offline),
//              debounced while typing and flushed on blur/submit/close.
//              EXP-924: the icon picker shares the row (the board form's
//              identity layout ×4) — devices.setIcon over the DEVICE glyph
//              set, written straight through on the pick and reverted onto
//              this section's error line if the server refuses.
//   Default  — devices.setDefault (EXP-622), the device every device picker
//              prefills; a single toggle, written straight through.
//   Sharing  — devices.setShared, SERVER devices only: one toggle per team
//              (FEED-33), each written straight through off the live row.
//   Defaults — the device's SERVER-AUTHORITATIVE launch defaults
//              (devices.setLaunchDefaults), debounced per edit: the default
//              agent (the shared picker) and, per agent, Model / Effort /
//              Ultracode / Plan. Editable while the device is OFFLINE too: the
//              row is the truth and the device's settings.json converges on its
//              next heartbeat, so the only offline concession is a footer
//              saying so.
//              EXP-862 took the ACCOUNT and USAGE rows back out (×4). A login
//              is a flow, not a setting: signing in lives on the account chips
//              (`AgentLoginSheet`) and the numbers live on ONE surface, Devices
//              → Accounts.
//              EXP-1042: and its last row is the "Workflow settings" SUB-SHELL
//              (`SubShell`, ×4) — the machine's workflow model pair, which
//              belongs to the DEFAULT agent rather than to the agent tab, so
//              it sits outside the tabs and slides its own page in.
//   Update   — EXP-909, SERVER devices only (a desktop app updates itself):
//              the version, an amber "Update available" caption, and the
//              Update / Queued / Updating… control the device ROW used to
//              carry. The row now carries the gear alone ×4.
//   Remove   — EXP-909: devices.remove behind the confirm the row's menu used
//              to raise. The sheet closes itself when the row goes away.
//              EXP-1042 took its header away: it is one plain destructive row
//              in the same shell, not a section of its own.
// EXP-1042 also retired the WORKTREES section: the inventory (shape 18) and
// its Remove/Prune commands are an IDE surface now, and a phone had no use
// for a list of branches it cannot open.
// EXP-490: the sheet renders the LIVE devices-shape row (looked up by id
// through the view model) rather than a value latched at open, so a rename or
// a defaults edit made on another client lands here while it is open. Every
// field auto-saves; the live row is echoed back into the drafts only while
// nothing is pending, in flight, or focused — a remote update must never stomp
// an edit in progress. Owner-only: the devices list offers the sheet on
// `isMine` rows exclusively, and the sheet closes itself if the row goes away.
struct DeviceSettingsSheet: View {
    let viewModel: AgentsViewModel
    let deviceId: String
    let teams: [TeamEntity]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    /// Typing/tapping settles before a save goes out — the issue-detail
    /// autosave window.
    private static let autosaveDelay = Duration.seconds(1.2)

    /// One agent's editable defaults (the launchDefaults wire shape with the
    /// picker sentinels resolved).
    private struct AgentDraft: Equatable {
        var model: String
        /// EXP-981: claude's subagent model; "" (the "Default" row) is a real
        /// stored choice — the CLI picks its own.
        var subagentModel: String
        var effort: String
        var ultracode: Bool
        var planMode: Bool
    }

    @State private var seeded = false
    @State private var name = ""
    @State private var savingName = false
    @FocusState private var nameFocused: Bool
    /// The DEBOUNCE timer only — never the request itself (see `saveNameNow`).
    @State private var nameSaveTask: Task<Void, Never>?
    /// An edit the server has not accepted yet: blocks the live echo and keeps
    /// the flush-on-close honest.
    @State private var namePending = false
    /// EXP-924: the optimistic icon pick, held only until the synced row
    /// moves (nil = render the row's own glyph).
    @State private var iconPick: String?
    @State private var savingShare = false
    @State private var savingDefaultDevice = false
    @State private var defaultAgent = "claude"
    /// EXP-872: the machine's default ACCOUNT — a login profile id of
    /// `defaultAgent` (`""` = none stored, which reads as its active login).
    /// EXP-1042: a pick may also park the picker's ambient sentinel
    /// (`system`) here, so the row reads as selected; it never reaches the
    /// server — `DeviceLaunchDefaultsInput` folds it into the clear.
    @State private var defaultAccount = ""
    @State private var selectedAgent = "claude"
    @State private var drafts: [String: AgentDraft] = [:]
    @State private var savingDefaults = false
    @State private var defaultsSaveTask: Task<Void, Never>?
    @State private var defaultsPending = false
    @State private var errorMessage: String?
    /// EXP-1029: the machine's stored WORKFLOW pair, held raw (`""` = nothing
    /// stored). What the page renders and what a save sends is the RESOLVED
    /// pair — `DeviceWorkflowSettings` clamps it to the default agent's
    /// vocabulary, so switching the default account re-seeds it for free.
    @State private var workflowModel = ""
    @State private var workflowStrongModel = ""
    /// EXP-420/EXP-909: the instance's advertised latest versions — the Update
    /// section offers its button only when a newer CLI build really exists.
    /// Instance config, not machine state: one tRPC read when the sheet opens
    /// on a server device, never polled.
    @State private var latestVersions: LatestVersions?
    /// Optimistic "Updating…" until the flag lands on the synced row.
    @State private var updateRequested = false
    /// EXP-909: the device removal this sheet is confirming.
    @State private var confirmingRemove = false

    /// The live row off the devices shape. Own machines only — the sheet is an
    /// owner surface, so a row that stops being ours reads as gone.
    private var liveDevice: SteerDevice? {
        viewModel.devices?.first { $0.deviceId == deviceId && $0.isMine }
    }

    var body: some View {
        if let device = liveDevice {
            content(device)
        } else {
            // Removed here or elsewhere (or un-shared out of reach): there is
            // nothing left to edit, so the sheet closes itself.
            Color.clear.onAppear { dismiss() }
        }
    }

    private func content(_ device: SteerDevice) -> some View {
        // EXP-686: the static sheet title — the machine's name is already the
        // first field below it.
        GlassSheetChrome(
            title: "Device settings",
            height: .full,
            content: {
                // EXP-1042: the sheet IS the card a sub-shell page replaces —
                // "Workflow settings" slides in over the whole form, with a
                // back button on top, rather than pushing a screen.
                SubShellHost {
                    Form {
                        nameSection(device)
                        defaultDeviceSection(device)
                        if device.isServer {
                            sharingSection(device)
                        }
                        defaultsSection(device)
                        if device.isServer {
                            updateSection(device)
                        }
                        removeSection(device)
                        if let errorMessage {
                            Section {
                                Text(errorMessage)
                                    .font(.caption)
                                    .foregroundStyle(DesignTokens.Semantic.red)
                            }
                            .listRowBackground(glassFormRowFill)
                        }
                    }
                    // EXP-603: the sheet's own background shows through the
                    // grouped list; rows carry the glass fill.
                    .scrollContentBackground(.hidden)
                    .listSectionSpacing(8)
                }
            }
        )
        // EXP-694: no Done button — every field autosaves, so the only exits
        // are a swipe down and the scrim, and both have to flush what the
        // debounce still owes.
        .onDisappear { flushAll() }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-settings-sheet")
        .onAppear { seed(device) }
        // EXP-909: only a daemon server has an Update section to gate.
        .task {
            guard device.isServer, latestVersions == nil else { return }
            latestVersions = try? await deps.devicesApi.latestVersions(accountId: accountId)
        }
        // Live echo: a rename/share/defaults change from another client (or
        // this one's own accepted save) lands in the drafts — but only while
        // the field is idle, so it can never stomp an edit in progress.
        .onChange(of: device.deviceLabel) { _, newValue in
            guard !nameFocused, !namePending, !savingName else { return }
            name = newValue
        }
        // EXP-924: the icon's live echo — the synced row moving (our own write
        // landing, or a pick made elsewhere) retires the optimistic value.
        .onChange(of: device.icon) { _, _ in
            iconPick = nil
        }
        .onChange(of: device.launchDefaults) { _, _ in
            guard seeded, !defaultsPending, !savingDefaults else { return }
            applyDefaults(device, keepTab: true)
        }
        // EXP-594: white control tint — system blue is retired (toggles,
        // menu pickers).
        .tint(DesignTokens.Palette.primary)
        // One presentation per node is the rule (SwiftUI drops the second),
        // so the device removal confirms off a zero-size node of its own.
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .alert("Remove device", isPresented: $confirmingRemove) {
                    Button("Cancel", role: .cancel) { confirmingRemove = false }
                    Button("Remove", role: .destructive) { removeDevice() }
                } message: {
                    // The pinned sentence ×4 — unchanged from the row menu
                    // this moved out of (EXP-909).
                    Text("Remove “\(deviceName(device))” from your devices? A device with the daemon still running will re-register itself on its next heartbeat.")
                }
        )
    }

    private func deviceName(_ device: SteerDevice) -> String {
        device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
    }

    // MARK: - Seeding

    /// Fill the drafts once per presentation; from there on the live echo above
    /// keeps them current whenever nothing is pending.
    private func seed(_ device: SteerDevice) {
        guard !seeded else { return }
        seeded = true
        name = device.deviceLabel
        applyDefaults(device, keepTab: false)
    }

    /// (Re)build the defaults drafts from the row. Callers own the guards —
    /// seeding runs once at open, the live echo only while nothing is pending.
    /// `keepTab` holds the agent tab the user is looking at (a re-seed must not
    /// yank it), as long as the row still offers that agent.
    private func applyDefaults(_ device: SteerDevice, keepTab: Bool) {
        let agents = device.editableAgentIds
        let advertisedDefault = device.launchDefaults?.defaultAgent
        defaultAgent = agents.contains(advertisedDefault ?? "") ? advertisedDefault! : (agents.first ?? "claude")
        // EXP-872: the stored account belongs to the stored agent — it only
        // survives a clamp that kept that agent.
        defaultAccount = defaultAgent == advertisedDefault
            ? (device.launchDefaults?.defaultAccount ?? "")
            : ""
        // EXP-1029: the workflow pair belongs to the MACHINE, not to an
        // agent's block — it rides verbatim and the resolver clamps it to the
        // default agent's vocabulary wherever it is read.
        workflowModel = device.launchDefaults?.workflow?.model ?? ""
        workflowStrongModel = device.launchDefaults?.workflow?.strongModel ?? ""
        // A re-seed must not yank the tab the reader is looking at.
        selectedAgent = keepTab && agents.contains(selectedAgent) ? selectedAgent : defaultAgent
        var next: [String: AgentDraft] = [:]
        for agent in agents {
            next[agent] = Self.draft(from: device.agentDefaults(for: agent), agent: agent)
        }
        drafts = next
    }

    /// The advertised per-agent defaults as a draft, contract-validated with
    /// static fallbacks (the Agent page composer's seeding semantics).
    private static func draft(from advertised: AgentLaunchDefaults?, agent: String) -> AgentDraft {
        let models = LaunchVocabulary.modelValues(for: agent)
        let model: String
        if let value = advertised?.model, !value.isEmpty, models.contains(value) {
            model = value
        } else if let value = advertised?.model, value.isEmpty, agent != "claude" {
            model = LaunchVocabulary.cliDefault
        } else {
            model = LaunchVocabulary.defaultModel(for: agent)
        }
        let effort: String
        if let value = advertised?.effort, LaunchVocabulary.effortValues(for: agent).contains(value) {
            effort = value
        } else {
            effort = LaunchVocabulary.cliDefault
        }
        return AgentDraft(
            model: model,
            subagentModel: LaunchOptionsState.seedSubagentModel(
                advertised?.subagentModel, for: agent
            ),
            effort: effort,
            ultracode: agent == "claude" && (advertised?.ultracode ?? false),
            planMode: LaunchVocabulary.supportsPlanMode(agent) && (advertised?.planMode ?? false)
        )
    }

    // MARK: - Name

    private func nameSection(_ device: SteerDevice) -> some View {
        Section {
            // EXP-924: the IDENTITY row every form shares (the board form's
            // icon + name, ×4) — the 36pt picker swatch, then the name field.
            HStack(spacing: 8) {
                IconPicker(
                    selection: iconBinding(device),
                    icons: AppIcons.devicePickable
                )
                // EXP-862: the ONE glass input, borderless inside the already
                // chromed form row — the stock `TextField` was the last system
                // control among the glass rows.
                GlassTextField("Name", text: $name, bordered: false) {
                    EmptyView()
                } trailing: {
                    if savingName {
                        ProgressView().controlSize(.small)
                    }
                }
                .focused($nameFocused)
                .onSubmit { flushName() }
                .onChange(of: name) { _, _ in
                    scheduleNameAutosave(device)
                }
                .onChange(of: nameFocused) { _, focused in
                    guard !focused else { return }
                    let hadPending = namePending
                    flushName()
                    // A rename that arrived while the field was focused was
                    // deliberately skipped — catch up now that no edit is owed.
                    if !hadPending, !savingName {
                        name = device.deviceLabel
                    }
                }
            }
        } header: {
            GlassSectionHeader("Name")
        }
        .listRowBackground(glassFormRowFill)
    }

    // MARK: - Icon (EXP-924)

    /// The picker's value is the RESOLVED glyph, so a machine that never
    /// picked one still shows its kind default as selected. A pick writes
    /// straight through (no debounce: it is one tap, not typing) and shows at
    /// once; the optimistic value is dropped as soon as the synced row moves —
    /// our own write landing, or a pick made on another client.
    private func iconBinding(_ device: SteerDevice) -> Binding<String> {
        Binding(
            get: { iconPick ?? DeviceIconDisplay.iconName(for: device) },
            set: { next in
                guard next != (iconPick ?? DeviceIconDisplay.iconName(for: device)) else { return }
                saveIcon(next)
            }
        )
    }

    private func saveIcon(_ icon: String) {
        iconPick = icon
        errorMessage = nil
        let api = deps.devicesApi
        let account = accountId
        let id = deviceId
        // INDEPENDENT of the sheet (see `saveNameNow`): closing it must not
        // abort a pick already on the wire.
        Task {
            do {
                try await api.setIcon(accountId: account, deviceId: id, icon: icon)
            } catch {
                // Revert to the row's own glyph and say so, on the sheet's ONE
                // error line — the same surface a failed rename uses.
                if iconPick == icon { iconPick = nil }
                errorMessage = error.userFacingMessage
            }
        }
    }

    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func scheduleNameAutosave(_ device: SteerDevice) {
        guard seeded else { return }
        namePending = trimmedName != device.deviceLabel
        nameSaveTask?.cancel()
        guard namePending else {
            nameSaveTask = nil
            return
        }
        nameSaveTask = Task {
            try? await Task.sleep(for: Self.autosaveDelay)
            guard !Task.isCancelled else { return }
            saveNameNow()
        }
    }

    /// Blur/submit: don't wait out the debounce window.
    private func flushName() {
        nameSaveTask?.cancel()
        nameSaveTask = nil
        guard namePending, !savingName else { return }
        saveNameNow()
    }

    private func saveNameNow() {
        nameSaveTask?.cancel()
        nameSaveTask = nil
        let label = trimmedName
        guard let live = liveDevice, !label.isEmpty, label != live.deviceLabel else {
            namePending = false
            return
        }
        savingName = true
        errorMessage = nil
        let api = deps.devicesApi
        let account = accountId
        let id = deviceId
        // INDEPENDENT of `nameSaveTask`: the next keystroke cancels the
        // debounce timer, and the sheet may close outright — neither may abort
        // a rename already on the wire, so nothing here is tied to either.
        Task {
            do {
                try await api.rename(accountId: account, deviceId: id, label: label)
                // A later keystroke already superseded this save — its own
                // debounce owns the pending flag.
                if trimmedName == label { namePending = false }
            } catch {
                errorMessage = error.localizedDescription
            }
            savingName = false
        }
    }

    // MARK: - Sharing (server machines only)

    // MARK: - Default machine (EXP-622)

    /// A single toggle, written straight through: the server clears the flag
    /// on the caller's other machines, and the switch re-renders off the live
    /// row rather than local state.
    private func defaultDeviceSection(_ device: SteerDevice) -> some View {
        Section {
            Toggle(
                "Default device",
                isOn: Binding(
                    get: { device.isDefaultDevice },
                    set: { saveDefaultDevice(isDefault: $0) }
                )
            )
            .disabled(savingDefaultDevice)
        }
        .listRowBackground(glassFormRowFill)
    }

    private func saveDefaultDevice(isDefault: Bool) {
        savingDefaultDevice = true
        errorMessage = nil
        Task {
            do {
                try await deps.devicesApi.setDefault(
                    accountId: accountId, deviceId: deviceId, isDefault: isDefault
                )
            } catch {
                errorMessage = error.localizedDescription
            }
            savingDefaultDevice = false
        }
    }

    /// FEED-33: one toggle per team, each rendered off the live row like the
    /// default-device switch (no draft, so nothing to roll back on error: the
    /// row simply never changed). Every row waits while one write is out.
    private func sharingSection(_ device: SteerDevice) -> some View {
        Section {
            ForEach(teams) { team in
                Toggle(
                    team.name,
                    isOn: Binding(
                        get: { device.sharedTeamIds.contains(team.id) },
                        set: { saveShare(teamId: team.id, shared: $0) }
                    )
                )
                .disabled(savingShare)
                .accessibilityIdentifier("device-share-\(team.id)")
            }
        } header: {
            GlassSectionHeader("Sharing")
        } footer: {
            Text("Teammates of a shared team can start coding sessions on this server. Removing a team ends its running sessions on it.")
        }
        .listRowBackground(glassFormRowFill)
    }

    private func saveShare(teamId: String, shared: Bool) {
        savingShare = true
        errorMessage = nil
        Task {
            do {
                try await deps.devicesApi.setShared(
                    accountId: accountId, deviceId: deviceId, teamId: teamId, shared: shared
                )
            } catch {
                errorMessage = error.localizedDescription
            }
            savingShare = false
        }
    }

    // MARK: - Agent defaults

    /// EXP-872: every login this machine reports, as the ONE list the default
    /// is picked from. An editable agent that reports no login still gets a
    /// row, named by the agent and standing for its AMBIENT login — an
    /// offline box's default stays editable even though it is advertising
    /// nothing right now.
    ///
    /// EXP-1042: per MISSING agent, not all-or-nothing. A machine that
    /// reports one claude login used to offer that single row, which the
    /// picker renders as a plain label (one option is not a choice) — so the
    /// default could not be moved to codex at all.
    private func accountOptions(_ device: SteerDevice) -> [AccountOption] {
        let reported = AccountOptions.flatten(
            accounts: device.agentAccounts,
            usage: device.agentUsage,
            launchDefaults: device.launchDefaults
        )
        let covered = Set(reported.map(\.agent))
        let ambient = device.editableAgentIds
            .filter { !covered.contains($0) }
            .map { agent in
                AccountOption(
                    id: AgentAccountsRows.systemProfileId,
                    agent: agent,
                    email: LaunchVocabulary.agentLabel(agent),
                    // The reported logins carry the device default among
                    // them; an agent that reports nothing never is one.
                    isDeviceDefault: reported.isEmpty && agent == defaultAgent
                )
            }
        return reported + ambient
    }

    /// EXP-694: the agent block is the SHARED `LaunchOptionsSection` — the
    /// sheet used to hand-roll the same tabs/model/effort/toggle rows, which is
    /// how it drifted (bare tabs bleeding to the screen edge, no brand marks).
    /// Only the default stays here, as its own leading card: it is a property
    /// of the MACHINE, not of the agent whose tab is open.
    ///
    /// EXP-872: and it is a default ACCOUNT now, not a default agent — the
    /// pick stores a login profile id and the agent derives from it. Shown
    /// whenever there is anything to pick at all (a lone login is still the
    /// machine's default, and the row is where a person reads which one it is).
    @ViewBuilder
    private func defaultsSection(_ device: SteerDevice) -> some View {
        let agents = device.editableAgentIds
        let options = accountOptions(device)
        if !options.isEmpty {
            Section {
                // EXP-872: the SHARED account picker (brand mark + email over
                // marked menu rows) — the same control the composer's options
                // row and the IDE's settings wear.
                HStack(spacing: 8) {
                    Text("Default account")
                        .foregroundStyle(.white.opacity(TextOpacity.primary))
                    Spacer(minLength: 8)
                    AccountPickerMenu(
                        options: options,
                        selection: selectedDefaultAccount(in: options),
                        mark: { AgentBrandMark.image($0) },
                        onSelect: { pickDefaultAccount($0) }
                    )
                }
            }
            .listRowBackground(glassFormRowFill)
        }
        LaunchOptionsSection(
            variant: .device,
            devices: [],
            deviceId: .constant(deviceId),
            noDeviceNote: "",
            // Which agent's options are on screen — a view choice, never an
            // edit, so it deliberately bypasses the autosave.
            availableAgents: agents,
            agent: selectedAgent,
            onAgentChange: { selectedAgent = $0 },
            model: draftBinding(\.model),
            subagentModel: draftBinding(\.subagentModel),
            effort: draftBinding(\.effort),
            ultracode: draftBinding(\.ultracode),
            planMode: draftBinding(\.planMode),
            footerNote: device.isOnline ? nil : "Applies when the device comes online."
        )
        workflowSection()
    }

    // MARK: - Workflow settings (EXP-1029)

    /// The machine's workflow model pair, one SUB-SHELL row under the agent
    /// block: tapping it slides its page in over the sheet. Shown for both
    /// agents — a workflow runs on the machine's DEFAULT account, so the pair
    /// follows the default agent rather than the tab that happens to be open.
    private func workflowSection() -> some View {
        Section {
            SubShell(label: "Workflow settings", value: workflowSummary) {
                workflowPage()
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    /// The stored pair, clamped to the default agent's vocabulary — what the
    /// row summarises, what the page renders, and what a save sends.
    private var workflowResolved: (model: String, strongModel: String) {
        DeviceWorkflowSettings.resolve(
            agent: defaultAgent,
            stored: DeviceWorkflowDefaults(
                model: workflowModel, strongModel: workflowStrongModel
            )
        )
    }

    /// The trailing summary, `Opus · Fable`.
    private var workflowSummary: String {
        let pair = workflowResolved
        return "\(LaunchVocabulary.modelLabel(pair.model)) · \(LaunchVocabulary.modelLabel(pair.strongModel))"
    }

    /// The page: one card, the two rungs a workflow launches on. Each pick
    /// writes through the same debounce the rest of the defaults use.
    private func workflowPage() -> some View {
        let options = DeviceWorkflowSettings.modelValues(for: defaultAgent)
        return Form {
            Section {
                VStack(alignment: .leading, spacing: 4) {
                    GlassPickerRow(
                        "Model",
                        selection: workflowBinding(strong: false),
                        options: options,
                        label: LaunchVocabulary.modelLabel
                    )
                    Text("Leaf nodes and the subagents inside them.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                VStack(alignment: .leading, spacing: 4) {
                    GlassPickerRow(
                        "Strong model",
                        selection: workflowBinding(strong: true),
                        options: options,
                        label: LaunchVocabulary.modelLabel
                    )
                    Text("Contract, integration and risky nodes, and every review.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .listRowBackground(glassFormRowFill)
        }
        .scrollContentBackground(.hidden)
        .listSectionSpacing(8)
    }

    /// Like `draftBinding`: only a USER pick writes, so a re-seed can never
    /// start a save loop. The stored half is written RAW — the resolver is
    /// what renders it back, clamped.
    private func workflowBinding(strong: Bool) -> Binding<String> {
        Binding(
            get: { strong ? workflowResolved.strongModel : workflowResolved.model },
            set: { newValue in
                let current = strong ? workflowResolved.strongModel : workflowResolved.model
                guard newValue != current else { return }
                if strong {
                    workflowStrongModel = newValue
                } else {
                    workflowModel = newValue
                }
                defaultsPending = true
                scheduleDefaultsAutosave()
            }
        )
    }

    /// Which option the row reads as: the stored pair, else that agent's first
    /// login (a stored profile the machine no longer reports), else the first
    /// row — the same ladder `AccountOptions.flatten` walks for the default.
    private func selectedDefaultAccount(in options: [AccountOption]) -> AccountOption? {
        options.first { $0.agent == defaultAgent && $0.id == defaultAccount }
            ?? options.first { $0.agent == defaultAgent }
            ?? options.first
    }

    /// Like `draftBinding`, a choke point that only a USER pick runs through —
    /// a picker never writes for a programmatic re-seed, which is exactly why
    /// the live echo can't trigger a save loop. EXP-872: one pick writes BOTH
    /// halves, since the agent is derived from the account.
    private func pickDefaultAccount(_ option: AccountOption) {
        guard option.agent != defaultAgent || option.id != defaultAccount else { return }
        defaultAgent = option.agent
        defaultAccount = option.id
        defaultsPending = true
        scheduleDefaultsAutosave()
    }

    private func draftBinding<Value>(_ keyPath: WritableKeyPath<AgentDraft, Value>) -> Binding<Value> {
        Binding(
            get: {
                (drafts[selectedAgent] ?? Self.draft(from: nil, agent: selectedAgent))[keyPath: keyPath]
            },
            set: { newValue in
                var draft = drafts[selectedAgent] ?? Self.draft(from: nil, agent: selectedAgent)
                draft[keyPath: keyPath] = newValue
                drafts[selectedAgent] = draft
                defaultsPending = true
                scheduleDefaultsAutosave()
            }
        )
    }

    private func scheduleDefaultsAutosave() {
        defaultsSaveTask?.cancel()
        defaultsSaveTask = Task {
            try? await Task.sleep(for: Self.autosaveDelay)
            guard !Task.isCancelled else { return }
            saveDefaultsNow()
        }
    }

    /// Whole-object replace: every drafted agent rides, sentinels resolved to
    /// the wire's blank-string "CLI default" form. The server clamps
    /// vocabulary field-wise, so version skew degrades a field, never the save.
    private func saveDefaultsNow() {
        defaultsSaveTask?.cancel()
        defaultsSaveTask = nil
        guard liveDevice != nil else {
            defaultsPending = false
            return
        }
        var agents: [String: AgentLaunchDefaultsInput] = [:]
        for (agent, draft) in drafts {
            agents[agent] = AgentLaunchDefaultsInput(
                model: draft.model == LaunchVocabulary.cliDefault ? "" : draft.model,
                // EXP-981: claude-only, and the blank IS the stored "the CLI
                // decides" value (unlike a start, which omits the field).
                subagentModel: LaunchVocabulary.supportsSubagentModel(agent)
                    ? (draft.subagentModel == LaunchVocabulary.cliDefault
                        ? "" : draft.subagentModel)
                    : nil,
                effort: draft.effort == LaunchVocabulary.cliDefault ? "" : draft.effort,
                ultracode: agent == "claude" ? draft.ultracode : nil,
                planMode: LaunchVocabulary.supportsPlanMode(agent) ? draft.planMode : nil
            )
        }
        // Built synchronously: the payload is what the drafts say NOW, and a
        // later edit re-arms the debounce on its own.
        let payload = DeviceLaunchDefaultsInput(
            defaultAgent: defaultAgent,
            // EXP-872: nil while nothing is picked; the input encodes it as
            // an explicit null (the clear), and the machine then falls back
            // to its active login, exactly as flatten does. EXP-1042: the
            // draft is handed over RAW — the input folds both the blank and
            // the ambient `system` sentinel into that nil itself, so no
            // writer here can leak the sentinel to the server.
            defaultAccount: defaultAccount,
            agents: agents,
            // EXP-1029: a whole-object save REPLACES the stored defaults, so
            // the workflow pair rides every write — leaving it out would
            // clobber it with nothing. The RESOLVED pair goes, which is also
            // how a default-agent switch persists the clamp.
            workflow: DeviceWorkflowDefaultsInput(
                model: workflowResolved.model,
                strongModel: workflowResolved.strongModel
            )
        )
        defaultsPending = false
        savingDefaults = true
        errorMessage = nil
        let api = deps.devicesApi
        let account = accountId
        let id = deviceId
        // INDEPENDENT of the debounce task (see `saveNameNow`).
        Task {
            do {
                try await api.setLaunchDefaults(
                    accountId: account, deviceId: id, launchDefaults: payload
                )
            } catch {
                errorMessage = error.localizedDescription
                defaultsPending = true
            }
            savingDefaults = false
        }
    }

    /// Closing the sheet commits whatever the debounce still owes. Idempotent:
    /// the in-flight flags make a second call (Done → onDisappear) a no-op.
    private func flushAll() {
        nameSaveTask?.cancel()
        nameSaveTask = nil
        defaultsSaveTask?.cancel()
        defaultsSaveTask = nil
        guard liveDevice != nil else {
            namePending = false
            defaultsPending = false
            return
        }
        if namePending, !savingName { saveNameNow() }
        if defaultsPending, !savingDefaults { saveDefaultsNow() }
    }

    // MARK: - Update (server devices only)

    /// EXP-909: the update control the device ROW used to carry, in the sheet
    /// the gear opens. Self-update is a daemon-server affordance only — the
    /// desktop app updates itself — and EXP-420 gates the button on a newer
    /// CLI build actually being advertised, so an up-to-date or offline server
    /// shows its version and nothing else.
    private func updateSection(_ device: SteerDevice) -> some View {
        let latest = latestVersions?.cli
        let outdated = device.updateAvailable(latest: latest)
        return Section {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(device.version.map { "v\($0)" } ?? "Version unknown")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.primary))
                    if outdated, let latest {
                        Text("Update available: v\(latest)")
                            .font(.caption)
                            .foregroundStyle(DesignTokens.Semantic.yellow)
                    }
                }
                Spacer(minLength: 8)
                updateControl(device, outdated: outdated)
            }
        } header: {
            GlassSectionHeader("Update")
        } footer: {
            if isUpdateQueued(device) {
                // EXP-411/FEED-36: parked behind the machine's live coding
                // sessions — the daemon applies it once they close.
                Text("Live coding sessions are holding the update. The device applies it once they end.")
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    @ViewBuilder
    private func updateControl(_ device: SteerDevice, outdated: Bool) -> some View {
        if isUpdateQueued(device) {
            GlassPill("Queued", icon: AppIcons.uiUpdate, enabled: false)
        } else if isUpdating(device) {
            HStack(spacing: 6) {
                ProgressView().controlSize(.mini)
                Text("Updating…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        } else if device.isOnline, outdated {
            GlassPill("Update", icon: AppIcons.uiUpdate, mode: .action {
                requestUpdate()
            })
        }
    }

    /// The pending flag rides the server row until the daemon re-registers
    /// (which clears it server-side); the local flag covers the gap until sync
    /// delivers it.
    private func isUpdating(_ device: SteerDevice) -> Bool {
        device.updateRequested == true || updateRequested
    }

    /// EXP-411: the pending update is parked behind live coding sessions.
    private func isUpdateQueued(_ device: SteerDevice) -> Bool {
        device.updateRequested == true && device.updateBlocked == true
    }

    /// Ask the daemon to self-update. EXP-481: the outcome lands via sync (the
    /// devices shape), so this only has to report a failure.
    private func requestUpdate() {
        errorMessage = nil
        updateRequested = true
        Task {
            do {
                try await deps.devicesApi.requestUpdate(
                    accountId: accountId, deviceId: deviceId
                )
            } catch {
                errorMessage = error.userFacingMessage
            }
            updateRequested = false
        }
    }

    // MARK: - Remove

    /// EXP-909: the row menu's last entry, now the sheet's last section. The
    /// sheet closes itself once the row is gone (`liveDevice` → nil).
    private func removeSection(_ device: SteerDevice) -> some View {
        Section {
            Button(role: .destructive) {
                confirmingRemove = true
            } label: {
                HStack(spacing: 8) {
                    AppIcon(AppIcons.uiDelete, size: AppIcon.Size.medium)
                    Text("Remove device")
                    Spacer(minLength: 0)
                }
                .font(.subheadline.weight(.medium))
                .foregroundStyle(DesignTokens.Palette.destructive)
                // .plain hit-tests opaque pixels only — the whole row taps.
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("device-remove")
        }
        .listRowBackground(glassFormRowFill)
    }

    private func removeDevice() {
        confirmingRemove = false
        errorMessage = nil
        let api = deps.devicesApi
        let account = accountId
        let id = deviceId
        // INDEPENDENT of the sheet: the row vanishing closes it, and the
        // request must not die with the view (see `saveNameNow`).
        Task {
            do {
                try await api.remove(accountId: account, deviceId: id)
            } catch {
                errorMessage = error.userFacingMessage
            }
        }
    }
}
