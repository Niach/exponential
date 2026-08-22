import ExpUI
import ExpCore
import SwiftUI

// The device settings sheet (EXP-481) — Edit on a machines row opens it, the
// iOS twin of the web/IDE device-settings dialog. Four sections, no Save
// buttons (EXP-490):
//   Name     — devices.rename (registry-authoritative, works offline),
//              debounced while typing and flushed on blur/submit/close.
//   Sharing  — devices.setShared, SERVER machines only (nil clears).
//   Defaults — the machine's SERVER-AUTHORITATIVE launch defaults
//              (devices.setLaunchDefaults), debounced per edit. Editable while
//              the machine is OFFLINE too: the row is the truth and the
//              machine's settings.json converges on its next heartbeat, so the
//              only offline concession is a footer saying so.
//   Worktrees — the synced inventory (shape 18) with per-row Remove and a
//              Prune button, queued as devices.createCommand rows the device
//              runs on its next heartbeat (immediately when online). Progress
//              polls devices.getCommand ~2s; the material outcome (a row
//              disappearing) arrives via sync when the device re-reports.
// EXP-490: the sheet renders the LIVE devices-shape row (looked up by id
// through the view model) rather than a value latched at open, so a rename or
// a defaults edit made on another client lands here while it is open. Every
// field auto-saves; the live row is echoed back into the drafts only while
// nothing is pending, in flight, or focused — a remote update must never stomp
// an edit in progress. Owner-only: the machines list offers Edit on `isMine`
// rows exclusively, and the sheet closes itself if the row goes away.
struct DeviceSettingsSheet: View {
    let viewModel: AgentsViewModel
    let deviceId: String
    let teams: [TeamEntity]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    /// Sentinel for the blank "CLI default" choice (the StartCodingSheet
    /// convention — omit the flag; for codex/pi also the omit-model default).
    private static let cliDefault = "cli-default"
    /// Sentinel tag for "Not shared" in the team picker.
    private static let notShared = "not-shared"
    /// Typing/tapping settles before a save goes out — the issue-detail
    /// autosave window.
    private static let autosaveDelay = Duration.seconds(1.2)

    /// One agent's editable defaults (the launchDefaults wire shape with the
    /// picker sentinels resolved).
    private struct AgentDraft: Equatable {
        var model: String
        var effort: String
        var ultracode: Bool
        var planMode: Bool
        var skipPermissions: Bool
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
    @State private var sharedTeamTag = DeviceSettingsSheet.notShared
    @State private var savingShare = false
    @State private var defaultAgent = "claude"
    @State private var selectedAgent = "claude"
    @State private var drafts: [String: AgentDraft] = [:]
    @State private var savingDefaults = false
    @State private var defaultsSaveTask: Task<Void, Never>?
    @State private var defaultsPending = false
    @State private var errorMessage: String?
    /// In-flight command per target key (a worktree row id, or "prune") — the
    /// poll loop clears it on a terminal status.
    @State private var pendingCommands: [String: String] = [:]
    /// Device-reported failure message per target key (EXP-323: inline, next
    /// to the triggering control).
    @State private var commandErrors: [String: String] = [:]
    @State private var removeTarget: DeviceWorktreeEntity?
    /// The device-reported prune summary ("Pruned 2 worktrees"), shown once.
    @State private var commandSummary: String?

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
        NavigationStack {
            Form {
                nameSection(device)
                if device.isServer {
                    sharingSection(device)
                }
                defaultsSection(device)
                worktreesSection(device)
                if let errorMessage {
                    Section {
                        Text(errorMessage)
                            .font(.caption)
                            .foregroundStyle(DesignTokens.Semantic.red)
                    }
                    .listRowBackground(glassFormRowFill)
                }
            }
            // EXP-603: the app background instead of the system grouped-list
            // gray; rows carry the glass fill.
            .scrollContentBackground(.hidden)
            .background(AppBackground())
            .navigationTitle(device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel)
            .navigationBarTitleDisplayMode(.inline)
            .listSectionSpacing(12)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") {
                        flushAll()
                        dismiss()
                    }
                }
            }
        }
        .presentationDetents([.large])
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-settings-sheet")
        .onAppear { seed(device) }
        // Live echo: a rename/share/defaults change from another client (or
        // this one's own accepted save) lands in the drafts — but only while
        // the field is idle, so it can never stomp an edit in progress.
        .onChange(of: device.deviceLabel) { _, newValue in
            guard !nameFocused, !namePending, !savingName else { return }
            name = newValue
        }
        .onChange(of: device.sharedTeamId) { _, newValue in
            guard !savingShare else { return }
            sharedTeamTag = newValue ?? Self.notShared
        }
        .onChange(of: device.launchDefaults) { _, _ in
            guard seeded, !defaultsPending, !savingDefaults else { return }
            applyDefaults(device, keepTab: true)
        }
        // Closing (Done, swipe-down, or the row vanishing) must not drop a
        // debounced edit.
        .onDisappear { flushAll() }
        // EXP-594: white control tint — system blue is retired (toggles,
        // menu pickers).
        .tint(DesignTokens.Palette.primary)
        .alert(
            "Remove worktree?",
            isPresented: Binding(
                get: { removeTarget != nil },
                set: { if !$0 { removeTarget = nil } }
            ),
            presenting: removeTarget
        ) { worktree in
            Button("Cancel", role: .cancel) { removeTarget = nil }
            Button("Remove", role: .destructive) { removeWorktree(worktree) }
        } message: { worktree in
            Text("Remove \(worktree.branch) on \(device.deviceLabel)? Uncommitted tracked changes make the machine refuse.")
        }
    }

    // MARK: - Seeding

    /// Fill the drafts once per presentation; from there on the live echo above
    /// keeps them current whenever nothing is pending.
    private func seed(_ device: SteerDevice) {
        guard !seeded else { return }
        seeded = true
        name = device.deviceLabel
        sharedTeamTag = device.sharedTeamId ?? Self.notShared
        applyDefaults(device, keepTab: false)
    }

    /// (Re)build the defaults drafts from the row. Callers own the guards —
    /// seeding runs once at open, the live echo only while nothing is pending.
    /// `keepTab` holds the agent tab the user is looking at (a re-seed must not
    /// yank it), as long as the row still offers that agent.
    private func applyDefaults(_ device: SteerDevice, keepTab: Bool) {
        let agents = editableAgents(device)
        let advertisedDefault = device.launchDefaults?.defaultAgent
        defaultAgent = agents.contains(advertisedDefault ?? "") ? advertisedDefault! : (agents.first ?? "claude")
        selectedAgent = keepTab && agents.contains(selectedAgent) ? selectedAgent : defaultAgent
        var next: [String: AgentDraft] = [:]
        for agent in agents {
            next[agent] = Self.draft(from: device.agentDefaults(for: agent), agent: agent)
        }
        drafts = next
    }

    /// The advertised per-agent defaults as a draft, contract-validated with
    /// static fallbacks (the StartCodingSheet seeding semantics).
    private static func draft(from advertised: AgentLaunchDefaults?, agent: String) -> AgentDraft {
        let models = modelValues(for: agent)
        let model: String
        if let value = advertised?.model, !value.isEmpty, models.contains(value) {
            model = value
        } else if let value = advertised?.model, value.isEmpty, agent != "claude" {
            model = cliDefault
        } else {
            model = defaultModel(for: agent)
        }
        let effort: String
        if let value = advertised?.effort, effortValues(for: agent).contains(value) {
            effort = value
        } else {
            effort = cliDefault
        }
        return AgentDraft(
            model: model,
            effort: effort,
            ultracode: agent == "claude" && (advertised?.ultracode ?? false),
            planMode: supportsPlanMode(agent) && (advertised?.planMode ?? false),
            skipPermissions: agent != "pi" && (advertised?.skipPermissions ?? false)
        )
    }

    // MARK: - Name

    private func nameSection(_ device: SteerDevice) -> some View {
        Section("Name") {
            HStack(spacing: 8) {
                TextField("Name", text: $name)
                    .focused($nameFocused)
                    .onSubmit { flushName() }
                if savingName {
                    ProgressView().controlSize(.small)
                }
            }
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
        .listRowBackground(glassFormRowFill)
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

    private func sharingSection(_ device: SteerDevice) -> some View {
        Section {
            GlassPickerRow(
                "Shared with",
                selection: $sharedTeamTag,
                options: [Self.notShared] + teams.map(\.id),
                label: { id in
                    guard id != Self.notShared else { return "Not shared" }
                    return teams.first { $0.id == id }?.name ?? id
                },
                enabled: !savingShare
            )
            .onChange(of: sharedTeamTag) { oldValue, newValue in
                guard seeded, oldValue != newValue else { return }
                // The live echo writes this too — a re-seed to what the row
                // already says is not an edit.
                guard newValue != (device.sharedTeamId ?? Self.notShared) else { return }
                saveShare(teamId: newValue == Self.notShared ? nil : newValue)
            }
        } header: {
            Text("Sharing")
        } footer: {
            Text("Teammates of the shared team can start coding sessions on this server. Moving or clearing the share ends their running sessions.")
        }
        .listRowBackground(glassFormRowFill)
    }

    private func saveShare(teamId: String?) {
        savingShare = true
        errorMessage = nil
        Task {
            do {
                try await deps.devicesApi.setShared(
                    accountId: accountId, deviceId: deviceId, teamId: teamId
                )
            } catch {
                errorMessage = error.localizedDescription
                // Roll the picker back to what the live row says.
                sharedTeamTag = liveDevice?.sharedTeamId ?? Self.notShared
            }
            savingShare = false
        }
    }

    // MARK: - Agent defaults

    /// Every agent worth a tab: runnable ∪ signed-out installs ∪ agents the
    /// stored defaults already carry — an OFFLINE machine's defaults stay
    /// editable even though nothing is advertised as runnable right now.
    private func editableAgents(_ device: SteerDevice) -> [String] {
        var set = Set(device.agentIds)
        set.formUnion(device.unauthedAgentIds)
        if let stored = device.launchDefaults?.agents?.keys {
            set.formUnion(stored)
        }
        if set.isEmpty { set.insert("claude") }
        return DomainContract.codingAgentValues.filter { set.contains($0) }
    }

    private func defaultsSection(_ device: SteerDevice) -> some View {
        let agents = editableAgents(device)
        return Section {
            if agents.count > 1 {
                GlassPickerRow(
                    "Default agent",
                    selection: defaultAgentBinding,
                    options: agents,
                    label: { Self.agentLabel($0) }
                )
                // Which agent's options are on screen — a view choice, never
                // an edit, so it deliberately bypasses the autosave.
                GlassSegmentedControl(
                    options: agents,
                    selection: selectedAgent,
                    label: { Self.agentLabel($0) },
                    onSelect: { selectedAgent = $0 }
                )
                .accessibilityLabel("Agent")
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
            }
            if let draft = drafts[selectedAgent] {
                GlassPickerRow(
                    "Model",
                    selection: draftBinding(\.model),
                    options: Self.modelValues(for: selectedAgent),
                    label: { Self.modelLabel($0) }
                )
                GlassPickerRow(
                    Self.effortTitle(for: selectedAgent),
                    selection: draftBinding(\.effort),
                    options: [Self.cliDefault] + Self.effortValues(for: selectedAgent),
                    label: { $0 == Self.cliDefault ? "CLI default" : Self.effortLabel($0) },
                    enabled: !(selectedAgent == "claude" && draft.ultracode)
                )
                if selectedAgent == "claude" {
                    Toggle("Ultracode", isOn: draftBinding(\.ultracode))
                }
                if Self.supportsPlanMode(selectedAgent) {
                    Toggle("Plan mode", isOn: draftBinding(\.planMode))
                }
                if selectedAgent != "pi" {
                    Toggle("Skip permissions", isOn: draftBinding(\.skipPermissions))
                }
            }
        } footer: {
            if !device.isOnline {
                Text("Applies when the device comes online.")
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    /// Like `draftBinding`, a choke point that only a USER pick runs through —
    /// a picker never writes its binding for a programmatic re-seed, which is
    /// exactly why the live echo can't trigger a save loop.
    private var defaultAgentBinding: Binding<String> {
        Binding(
            get: { defaultAgent },
            set: { newValue in
                guard newValue != defaultAgent else { return }
                defaultAgent = newValue
                defaultsPending = true
                scheduleDefaultsAutosave()
            }
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
                model: draft.model == Self.cliDefault ? "" : draft.model,
                effort: draft.effort == Self.cliDefault ? "" : draft.effort,
                ultracode: agent == "claude" ? draft.ultracode : nil,
                planMode: Self.supportsPlanMode(agent) ? draft.planMode : nil,
                skipPermissions: agent == "pi" ? nil : draft.skipPermissions
            )
        }
        // Built synchronously: the payload is what the drafts say NOW, and a
        // later edit re-arms the debounce on its own.
        let payload = DeviceLaunchDefaultsInput(defaultAgent: defaultAgent, agents: agents)
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

    // MARK: - Worktrees

    private func deviceWorktrees(_ device: SteerDevice) -> [DeviceWorktreeEntity] {
        guard let rowId = device.rowId else { return [] }
        return viewModel.worktrees
            .filter { $0.deviceRowId == rowId }
            .sorted { ($0.repoFullName, $0.branch) < ($1.repoFullName, $1.branch) }
    }

    private func worktreesSection(_ device: SteerDevice) -> some View {
        let worktrees = deviceWorktrees(device)
        return Section {
            if worktrees.isEmpty {
                Text("No worktrees reported.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(worktrees) { worktree in
                    worktreeRow(worktree)
                }
                Button {
                    prune()
                } label: {
                    HStack(spacing: 6) {
                        if pendingCommands["prune"] != nil {
                            ProgressView().controlSize(.small)
                            Text(device.isOnline ? "Pruning…" : "Prune queued")
                        } else {
                            Label("Prune merged worktrees", appIcon: AppIcons.uiClean)
                        }
                    }
                }
                .disabled(pendingCommands["prune"] != nil)
                if let message = commandErrors["prune"] {
                    Text(message)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.red)
                }
                if let commandSummary {
                    Text(commandSummary)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } header: {
            Text("Worktrees")
        } footer: {
            if !device.isOnline, !worktrees.isEmpty {
                Text("Runs when the device comes online.")
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    @ViewBuilder
    private func worktreeRow(_ worktree: DeviceWorktreeEntity) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiBranch, size: AppIcon.Size.small)
                    .foregroundStyle(.secondary)
                VStack(alignment: .leading, spacing: 2) {
                    Text(worktree.branch)
                        .font(.subheadline.weight(.medium))
                        .lineLimit(1)
                    HStack(spacing: 6) {
                        Text(worktree.repoFullName)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        if let identifier = worktree.issueIdentifier {
                            Text(identifier)
                                .font(.caption.monospaced())
                                .foregroundStyle(.secondary)
                        }
                        if worktree.dirty == "tracked" || worktree.dirty == "untracked" {
                            Text(worktree.dirty == "tracked" ? "uncommitted changes" : "untracked files")
                                .font(.caption2)
                                .foregroundStyle(DesignTokens.Semantic.yellow)
                        }
                        if worktree.busy {
                            Text("session live")
                                .font(.caption2)
                                .foregroundStyle(DesignTokens.Semantic.green)
                        }
                    }
                }
                Spacer(minLength: 0)
                if pendingCommands[worktree.id] != nil {
                    ProgressView().controlSize(.small)
                } else {
                    Button {
                        removeTarget = worktree
                    } label: {
                        AppIcon(AppIcons.uiDelete, size: AppIcon.Size.small)
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                    // A live session holds the branch — the machine would
                    // refuse anyway; don't offer it.
                    .disabled(worktree.busy)
                    .accessibilityLabel("Remove worktree")
                }
            }
            if let message = commandErrors[worktree.id] {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func removeWorktree(_ worktree: DeviceWorktreeEntity) {
        removeTarget = nil
        runCommand(
            targetKey: worktree.id,
            kind: "worktree_remove",
            repoFullName: worktree.repoFullName,
            branch: worktree.branch
        )
    }

    private func prune() {
        runCommand(targetKey: "prune", kind: "worktree_prune")
    }

    /// Queue the command, then poll its row ~2s until terminal (bounded — an
    /// offline machine keeps the command queued server-side, so the poll
    /// gives up quietly and the outcome lands via sync whenever it runs).
    private func runCommand(
        targetKey: String,
        kind: String,
        repoFullName: String? = nil,
        branch: String? = nil
    ) {
        commandErrors[targetKey] = nil
        pendingCommands[targetKey] = ""
        Task {
            do {
                let created = try await deps.devicesApi.createCommand(
                    accountId: accountId,
                    deviceId: deviceId,
                    kind: kind,
                    repoFullName: repoFullName,
                    branch: branch
                )
                pendingCommands[targetKey] = created.id
                // ~2 minutes of 2s polls; a queued-behind-offline command
                // just stops being watched (sync still delivers the result).
                for _ in 0..<60 {
                    try? await Task.sleep(for: .seconds(2))
                    guard pendingCommands[targetKey] == created.id else { return }
                    guard let command = try? await deps.devicesApi.getCommand(
                        accountId: accountId, commandId: created.id
                    ) else { continue }
                    if !command.isPending {
                        pendingCommands[targetKey] = nil
                        if command.isFailed {
                            commandErrors[targetKey] = command.result ?? "The machine refused the command."
                        } else if targetKey == "prune" {
                            // The prune summary is worth showing on success
                            // ("Pruned 2 worktrees").
                            commandSummary = command.result
                        }
                        return
                    }
                }
                pendingCommands[targetKey] = nil
            } catch {
                pendingCommands[targetKey] = nil
                commandErrors[targetKey] = error.localizedDescription
            }
        }
    }

    // MARK: - Vocabulary (mirrors StartCodingSheet)

    private static func modelValues(for agent: String) -> [String] {
        switch agent {
        case "codex": [cliDefault] + DomainContract.codexModelValues
        case "pi": [cliDefault] + DomainContract.piModelValues
        default: DomainContract.codingModelValues
        }
    }

    private static func effortValues(for agent: String) -> [String] {
        switch agent {
        case "codex": DomainContract.codexEffortValues
        case "pi": DomainContract.piThinkingValues
        default: DomainContract.codingEffortValues
        }
    }

    private static func defaultModel(for agent: String) -> String {
        agent == "claude" ? (DomainContract.codingModelValues.first ?? "") : cliDefault
    }

    private static func effortTitle(for agent: String) -> String {
        switch agent {
        case "codex": "Reasoning"
        case "pi": "Thinking"
        default: "Effort"
        }
    }

    private static func supportsPlanMode(_ agent: String) -> Bool {
        agent == "claude" || agent == "pi"
    }

    private static func agentLabel(_ value: String) -> String {
        switch value {
        case "claude": "Claude Code"
        case "codex": "Codex"
        case "pi": "pi"
        default: value
        }
    }

    private static func modelLabel(_ value: String) -> String {
        switch value {
        case cliDefault: "CLI default"
        case "gpt-5.6-sol": "GPT-5.6 Sol"
        case "gpt-5.6-terra": "GPT-5.6 Terra"
        case "gpt-5.6-luna": "GPT-5.6 Luna"
        case "grok-4.5": "Grok 4.5"
        default: value.prefix(1).uppercased() + value.dropFirst()
        }
    }

    private static func effortLabel(_ value: String) -> String {
        value == "xhigh" ? "XHigh" : value.prefix(1).uppercased() + value.dropFirst()
    }
}
