import ExpUI
import ExpCore
import SwiftUI

// The device settings sheet (EXP-481) — Edit on a machines row opens it, the
// iOS twin of the web/IDE device-settings dialog. Four sections, per-section
// commits:
//   Name     — devices.rename (registry-authoritative, works offline).
//   Sharing  — devices.setShared, SERVER machines only (nil clears).
//   Defaults — the machine's SERVER-AUTHORITATIVE launch defaults
//              (devices.setLaunchDefaults). Editable while the machine is
//              OFFLINE too: the row is the truth and the machine's
//              settings.json converges on its next heartbeat, so the only
//              offline concession is a footer saying so.
//   Worktrees — the synced inventory (shape 18) with per-row Remove and a
//              Prune button, queued as devices.createCommand rows the device
//              runs on its next heartbeat (immediately when online). Progress
//              polls devices.getCommand ~2s; the material outcome (a row
//              disappearing) arrives via sync when the device re-reports.
// Drafts are latched at open (the seeding runs once) — live shape updates
// must never stomp in-flight edits. Owner-only: the machines list offers Edit
// on `isMine` rows exclusively.
struct DeviceSettingsSheet: View {
    let device: SteerDevice
    let worktrees: [DeviceWorktreeEntity]
    let teams: [TeamEntity]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    /// Sentinel for the blank "CLI default" choice (the StartCodingSheet
    /// convention — omit the flag; for codex/pi also the omit-model default).
    private static let cliDefault = "cli-default"
    /// Sentinel tag for "Not shared" in the team picker.
    private static let notShared = "not-shared"

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
    @State private var sharedTeamTag = DeviceSettingsSheet.notShared
    @State private var savingShare = false
    @State private var defaultAgent = "claude"
    @State private var selectedAgent = "claude"
    @State private var drafts: [String: AgentDraft] = [:]
    @State private var savingDefaults = false
    @State private var defaultsSaved = false
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

    var body: some View {
        NavigationStack {
            Form {
                nameSection
                if device.isServer {
                    sharingSection
                }
                defaultsSection
                worktreesSection
                if let errorMessage {
                    Section {
                        Text(errorMessage)
                            .font(.caption)
                            .foregroundStyle(DesignTokens.Semantic.red)
                    }
                }
            }
            .navigationTitle(device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel)
            .navigationBarTitleDisplayMode(.inline)
            .listSectionSpacing(12)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.large])
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-settings-sheet")
        .onAppear { seed() }
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

    /// Latch the drafts once per presentation: the sheet's item identity is
    /// the device, so a re-present reseeds, while live shape updates during
    /// an edit never stomp it.
    private func seed() {
        guard !seeded else { return }
        seeded = true
        name = device.deviceLabel
        sharedTeamTag = device.sharedTeamId ?? Self.notShared
        let advertisedDefault = device.launchDefaults?.defaultAgent
        defaultAgent = editableAgents.contains(advertisedDefault ?? "") ? advertisedDefault! : (editableAgents.first ?? "claude")
        selectedAgent = defaultAgent
        for agent in editableAgents {
            drafts[agent] = Self.draft(from: device.agentDefaults(for: agent), agent: agent)
        }
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

    private var nameSection: some View {
        Section("Name") {
            TextField("Name", text: $name)
            Button {
                saveName()
            } label: {
                if savingName {
                    ProgressView().controlSize(.small)
                } else {
                    Text("Save name")
                }
            }
            .disabled(savingName || trimmedName.isEmpty || trimmedName == device.deviceLabel)
        }
    }

    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func saveName() {
        let label = trimmedName
        guard !label.isEmpty else { return }
        savingName = true
        errorMessage = nil
        Task {
            do {
                try await deps.devicesApi.rename(
                    accountId: accountId, deviceId: device.deviceId, label: label
                )
            } catch {
                errorMessage = error.localizedDescription
            }
            savingName = false
        }
    }

    // MARK: - Sharing (server machines only)

    private var sharingSection: some View {
        Section {
            Picker("Shared with", selection: $sharedTeamTag) {
                Text("Not shared").tag(Self.notShared)
                ForEach(teams) { team in
                    Text(team.name).tag(team.id)
                }
            }
            .disabled(savingShare)
            .onChange(of: sharedTeamTag) { oldValue, newValue in
                guard seeded, oldValue != newValue else { return }
                saveShare(teamId: newValue == Self.notShared ? nil : newValue)
            }
        } header: {
            Text("Sharing")
        } footer: {
            Text("Teammates of the shared team can start coding sessions on this server. Moving or clearing the share ends their running sessions.")
        }
    }

    private func saveShare(teamId: String?) {
        savingShare = true
        errorMessage = nil
        Task {
            do {
                try await deps.devicesApi.setShared(
                    accountId: accountId, deviceId: device.deviceId, teamId: teamId
                )
            } catch {
                errorMessage = error.localizedDescription
                // Roll the picker back to what the row says.
                sharedTeamTag = device.sharedTeamId ?? Self.notShared
            }
            savingShare = false
        }
    }

    // MARK: - Agent defaults

    /// Every agent worth a tab: runnable ∪ signed-out installs ∪ agents the
    /// stored defaults already carry — an OFFLINE machine's defaults stay
    /// editable even though nothing is advertised as runnable right now.
    private var editableAgents: [String] {
        var set = Set(device.agentIds)
        set.formUnion(device.unauthedAgentIds)
        if let stored = device.launchDefaults?.agents?.keys {
            set.formUnion(stored)
        }
        if set.isEmpty { set.insert("claude") }
        return DomainContract.codingAgentValues.filter { set.contains($0) }
    }

    private var defaultsSection: some View {
        Section {
            if editableAgents.count > 1 {
                Picker("Default agent", selection: $defaultAgent) {
                    ForEach(editableAgents, id: \.self) { value in
                        Text(Self.agentLabel(value)).tag(value)
                    }
                }
                Picker("Agent", selection: $selectedAgent) {
                    ForEach(editableAgents, id: \.self) { value in
                        Text(Self.agentLabel(value)).tag(value)
                    }
                }
                .pickerStyle(.segmented)
            }
            if let draft = drafts[selectedAgent] {
                Picker("Model", selection: draftBinding(\.model)) {
                    ForEach(Self.modelValues(for: selectedAgent), id: \.self) { value in
                        Text(Self.modelLabel(value)).tag(value)
                    }
                }
                Picker(Self.effortTitle(for: selectedAgent), selection: draftBinding(\.effort)) {
                    Text("CLI default").tag(Self.cliDefault)
                    ForEach(Self.effortValues(for: selectedAgent), id: \.self) { value in
                        Text(Self.effortLabel(value)).tag(value)
                    }
                }
                .disabled(selectedAgent == "claude" && draft.ultracode)
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
            Button {
                saveDefaults()
            } label: {
                if savingDefaults {
                    ProgressView().controlSize(.small)
                } else {
                    Text(defaultsSaved ? "Saved" : "Save defaults")
                }
            }
            .disabled(savingDefaults)
        } header: {
            Text("Agent defaults")
        } footer: {
            if !device.isOnline {
                Text("Applies when the device comes online.")
            }
        }
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
                defaultsSaved = false
            }
        )
    }

    /// Whole-object replace: every drafted agent rides, sentinels resolved to
    /// the wire's blank-string "CLI default" form. The server clamps
    /// vocabulary field-wise, so version skew degrades a field, never the save.
    private func saveDefaults() {
        savingDefaults = true
        errorMessage = nil
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
        let payload = DeviceLaunchDefaultsInput(defaultAgent: defaultAgent, agents: agents)
        Task {
            do {
                try await deps.devicesApi.setLaunchDefaults(
                    accountId: accountId, deviceId: device.deviceId, launchDefaults: payload
                )
                defaultsSaved = true
            } catch {
                errorMessage = error.localizedDescription
            }
            savingDefaults = false
        }
    }

    // MARK: - Worktrees

    private var deviceWorktrees: [DeviceWorktreeEntity] {
        guard let rowId = device.rowId else { return [] }
        return worktrees
            .filter { $0.deviceRowId == rowId }
            .sorted { ($0.repoFullName, $0.branch) < ($1.repoFullName, $1.branch) }
    }

    private var worktreesSection: some View {
        Section {
            if deviceWorktrees.isEmpty {
                Text("No worktrees reported.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(deviceWorktrees) { worktree in
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
            if !device.isOnline, !deviceWorktrees.isEmpty {
                Text("Runs when the device comes online.")
            }
        }
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
                    deviceId: device.deviceId,
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
