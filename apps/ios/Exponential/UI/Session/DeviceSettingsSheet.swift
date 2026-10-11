import ExpUI
import ExpCore
import SwiftUI

// The device settings sheet (EXP-481) — the settings gear on a device row
// opens it, the iOS twin of the web/IDE device-settings dialog. Seven sections,
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
//              (devices.setLaunchDefaults), debounced per edit: per agent,
//              Model / Effort / Ultracode / Plan. EXP-1158: no default
//              account — every start uses the login last used there. Editable while the device is OFFLINE too: the
//              row is the truth and the device's settings.json converges on its
//              next heartbeat, so the only offline concession is a footer
//              saying so.
//              EXP-1196: the DEVICE-level "Computer use" switch rides the
//              same debounced whole-object save as a top-level key; it lives
//              in the Readiness block's Computer use group (below).
//              EXP-1236: "Computer use model" (the alias the run's
//              screen-driving subagents run on, default Haiku) is that
//              group's last row, shown only while the switch is on, and rides
//              the same save beside it as `computerUseModel`.
//              EXP-862 took the ACCOUNT and USAGE rows back out (×4). A login
//              is a flow, not a setting: signing in lives on the account chips
//              (`AgentLoginSheet`) and the numbers live on ONE surface, Devices
//              → Accounts.
//   Readiness — EXP-1196/1218/1219: the device's doctor report as THE
//              readiness block (`DeviceReadinessView`, fixture
//              `device-doctor.json`), only when the row carries one; an
//              older build's row (no doctor) gets the bare Computer use switch. A phone
//              is always ANOTHER device: Update queues `agent_update {agent}`,
//              Sign in opens the remote `AgentLoginSheet`, Import (an agent's
//              ambient login, cap `agent-import`) runs `agentImportFlow`;
//              local-only actions render no pill.
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
        /// EXP-1082 §6: the desktop-owned auto-rotate toggle as the row
        /// advertises it. No control here; it only rides back on a save so
        /// the whole-object replace never drops it (nil = absent on the row).
        var autoRotateAccounts: Bool?
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
    /// EXP-1158: the machine's LAST USED agent (`launchDefaults.defaultAgent`),
    /// READ-ONLY here — only the device writes it; the agent tab opens on it.
    @State private var lastUsedAgent = "claude"
    @State private var selectedAgent = "claude"
    @State private var drafts: [String: AgentDraft] = [:]
    /// EXP-1196: the device-level `launchDefaults.computerUse` draft. Off
    /// when the row has no value; sent explicitly on every defaults save.
    @State private var computerUse = false
    /// EXP-1236: the device-level `launchDefaults.computerUseModel` draft,
    /// seeded through `LaunchVocabulary.seedComputerUseModel` (contract
    /// default when the row has none).
    @State private var computerUseModel = DomainContract.deviceComputerUseDefaultsModel
    @State private var savingDefaults = false
    @State private var defaultsSaveTask: Task<Void, Never>?
    @State private var defaultsPending = false
    @Environment(\.toaster) private var toaster
    /// EXP-420/EXP-909: the instance's advertised latest versions — the Update
    /// section offers its button only when a newer CLI build really exists.
    /// Instance config, not machine state: one tRPC read when the sheet opens
    /// on a server device, never polled.
    @State private var latestVersions: LatestVersions?
    /// Optimistic "Updating…" until the flag lands on the synced row.
    @State private var updateRequested = false
    /// EXP-909: the device removal this sheet is confirming.
    @State private var confirmingRemove = false
    /// EXP-1196: the readiness block's Sign in pill → the remote sign-in.
    @State private var loginTarget: AgentLoginTarget?
    /// EXP-1196: agents whose `agent_update` is on the wire.
    @State private var updatingAgents: Set<String> = []
    /// The Import the person is confirming (`agentImportFlow`).
    @State private var importTarget: AgentImportTarget?

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
                Form {
                    nameSection(device)
                    defaultDeviceSection(device)
                    if device.isServer {
                        sharingSection(device)
                    }
                    if let doctor = device.doctor {
                        readinessSection(device, doctor: doctor)
                    } else {
                        computerUseSection
                    }
                    defaultsSection(device)
                    if device.isServer {
                        updateSection(device)
                    }
                    removeSection(device)
                }
                // EXP-603: the sheet's own background shows through the
                // grouped list; rows carry the glass fill.
                .scrollContentBackground(.hidden)
                .listSectionSpacing(8)
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
        // The readiness block's Import pill: confirm, queue, duplicate toast.
        .agentImportFlow($importTarget, devices: viewModel.devices ?? [])
        // EXP-594: white control tint — system blue is retired (toggles,
        // menu pickers).
        .tint(DesignTokens.Palette.primary)
        // One presentation per node is the rule (SwiftUI drops the second),
        // so the device removal confirms off a zero-size node of its own.
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $confirmingRemove) {
                    GlassAlert(
                        prompt: Prompts.RemoveDevice.copy(name: deviceName(device)),
                        handlers: ["remove": { removeDevice() }]
                    )
                }
        )
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .sheet(item: $loginTarget) { target in
                    AgentLoginSheet(viewModel: viewModel, target: target)
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
        lastUsedAgent = agents.contains(advertisedDefault ?? "") ? advertisedDefault! : (agents.first ?? "claude")
        // A re-seed must not yank the tab the reader is looking at.
        selectedAgent = keepTab && agents.contains(selectedAgent) ? selectedAgent : lastUsedAgent
        var next: [String: AgentDraft] = [:]
        for agent in agents {
            next[agent] = Self.draft(from: device.agentDefaults(for: agent), agent: agent)
        }
        drafts = next
        computerUse = device.launchDefaults?.computerUse ?? false
        computerUseModel = LaunchVocabulary.seedComputerUseModel(device.launchDefaults?.computerUseModel)
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
            planMode: LaunchVocabulary.supportsPlanMode(agent) && (advertised?.planMode ?? false),
            autoRotateAccounts: advertised?.autoRotateAccounts
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
        let api = deps.devicesApi
        let account = accountId
        let id = deviceId
        // INDEPENDENT of the sheet (see `saveNameNow`): closing it must not
        // abort a pick already on the wire.
        Task {
            do {
                try await api.setIcon(accountId: account, deviceId: id, icon: icon)
            } catch {
                // Revert to the row's own glyph and say so, as the error toast
                // a failed rename raises too (EXP-1031).
                if iconPick == icon { iconPick = nil }
                toaster.error(error.userFacingMessage)
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
                toaster.error(error.localizedDescription)
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
            GlassToggleRow(
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
        Task {
            do {
                try await deps.devicesApi.setDefault(
                    accountId: accountId, deviceId: deviceId, isDefault: isDefault
                )
            } catch {
                toaster.error(error.localizedDescription)
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
                GlassToggleRow(
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
            Text("Teammates of a shared team can start runs on this server. Removing a team ends its runs on it.")
        }
        .listRowBackground(glassFormRowFill)
    }

    private func saveShare(teamId: String, shared: Bool) {
        savingShare = true
        Task {
            do {
                try await deps.devicesApi.setShared(
                    accountId: accountId, deviceId: deviceId, teamId: teamId, shared: shared
                )
            } catch {
                toaster.error(error.localizedDescription)
            }
            savingShare = false
        }
    }

    // MARK: - Readiness (EXP-1196/1218/1219)

    /// THE readiness block for this device. The Computer use group's switch
    /// row IS the device-level toggle: it shows the draft (the report only
    /// catches up on the next heartbeat) and saves through the debounced
    /// launch-defaults path. No header, no footer: the bands label it.
    private func readinessSection(_ device: SteerDevice, doctor: DeviceDoctor) -> some View {
        Section {
            DeviceReadinessView(
                groups: DeviceReadiness.groups(
                    doctor, remote: true, computerUseOn: computerUse,
                    canImport: device.canImportAgent
                ),
                computerUse: Binding(
                    get: { computerUse },
                    set: { newValue in
                        computerUse = newValue
                        defaultsPending = true
                        scheduleDefaultsAutosave()
                    }
                ),
                computerUseModel: computerUseModelBinding,
                busyActions: updatingAgents,
                onAction: { row in runReadinessAction(row, device: device) },
                onImport: { row in
                    importTarget = AgentImportTarget(row: row, device: device)
                }
            )
            .listRowInsets(EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0))
        }
        .listRowBackground(Color.clear)
    }

    /// EXP-1236: the Computer use model draft's binding — a pick saves
    /// through the same debounced launch-defaults path as the switch.
    private var computerUseModelBinding: Binding<String> {
        Binding(
            get: { computerUseModel },
            set: { newValue in
                computerUseModel = newValue
                defaultsPending = true
                scheduleDefaultsAutosave()
            }
        )
    }

    /// An older build sends no doctor (fixture rule): no block, but the bare
    /// Computer use switch row stays reachable. Label + switch, no footer;
    /// EXP-1236: the model picker row under it while the switch is on.
    private var computerUseSection: some View {
        Section {
            GlassToggleRow(
                "Computer use",
                isOn: Binding(
                    get: { computerUse },
                    set: { newValue in
                        computerUse = newValue
                        defaultsPending = true
                        scheduleDefaultsAutosave()
                    }
                )
            )
            .accessibilityIdentifier("device-computer-use")
            if computerUse {
                GlassPickerRow(
                    "Computer use model",
                    selection: computerUseModelBinding,
                    options: LaunchVocabulary.computerUseModelValues(),
                    label: { LaunchVocabulary.computerUseModelLabel($0) }
                )
                .accessibilityIdentifier("device-computer-use-model")
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    /// A phone only ever sees the REMOTE actions (`DeviceReadiness` drops the
    /// rest): Update = `agent_update {agent}`, Sign in = the remote
    /// `agent_login` flow.
    private func runReadinessAction(_ row: DeviceReadiness.Row, device: SteerDevice) {
        switch row.action {
        case "update":
            requestAgentUpdate(row.key)
        case "sign_in":
            loginTarget = AgentLoginTarget(
                deviceId: device.deviceId,
                deviceLabel: deviceName(device),
                agent: row.key
            )
        default:
            break
        }
    }

    private func requestAgentUpdate(_ agent: String) {
        guard !updatingAgents.contains(agent) else { return }
        updatingAgents.insert(agent)
        let api = deps.devicesApi
        let account = accountId
        let id = deviceId
        Task {
            do {
                try await api.requestAgentUpdate(accountId: account, deviceId: id, agent: agent)
            } catch {
                toaster.error(error.userFacingMessage)
            }
            updatingAgents.remove(agent)
        }
    }

    // MARK: - Agent defaults

    /// EXP-694: the agent block is the SHARED `LaunchOptionsSection` — the
    /// sheet used to hand-roll the same tabs/model/effort/toggle rows, which is
    /// how it drifted (bare tabs bleeding to the screen edge, no brand marks).
    /// EXP-1158: no account card above it — a start runs on the login last
    /// used on the machine, which is no setting.
    @ViewBuilder
    private func defaultsSection(_ device: SteerDevice) -> some View {
        let agents = device.editableAgentIds
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
                planMode: LaunchVocabulary.supportsPlanMode(agent) ? draft.planMode : nil,
                // EXP-1082 §6: echoed verbatim, never invented (see the draft).
                autoRotateAccounts: draft.autoRotateAccounts
            )
        }
        // Built synchronously: the payload is what the drafts say NOW, and a
        // later edit re-arms the debounce on its own.
        // EXP-1158: no `defaultAgent` — the device owns the last used agent
        // and the server carries it forward over this save.
        // EXP-1196: the device-level switch rides as an explicit boolean;
        // EXP-1236: its model alias beside it (always a contract value here).
        let payload = DeviceLaunchDefaultsInput(
            agents: agents,
            computerUse: computerUse,
            computerUseModel: computerUseModel
        )
        defaultsPending = false
        savingDefaults = true
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
                toaster.error(error.localizedDescription)
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
                Text("Live runs are holding the update. The device applies it once they end.")
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
        updateRequested = true
        Task {
            do {
                try await deps.devicesApi.requestUpdate(
                    accountId: accountId, deviceId: deviceId
                )
            } catch {
                toaster.error(error.userFacingMessage)
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
        let api = deps.devicesApi
        let account = accountId
        let id = deviceId
        // INDEPENDENT of the sheet: the row vanishing closes it, and the
        // request must not die with the view (see `saveNameNow`).
        Task {
            do {
                try await api.remove(accountId: account, deviceId: id)
            } catch {
                toaster.error(error.userFacingMessage)
            }
        }
    }
}
