import ExpCore
import ExpUI
import GRDB
import SwiftUI

// S-3 (agent-settings.json `assistant`): THE ASSISTANT PAGE, the agent
// page's layout and controls in `AgentSettings.assistantSections` order:
// identity (the name only, the Exponential brand mark instead of the icon
// picker), Runs on (the agent page's Device + Account rows and THE model
// picker; the Assistant's device IS the member's voice device, the server
// mirrors it), Check in (THE Picker over `assistantSchedules`, written at
// once as `assistantScheduleTriggers`), Voice (the Voice settings block
// moved here whole, minus its Voice device + Agent pickers: the server
// derives both from this Assistant). No Instructions (#EXP-1439: the
// prompt is built in; the instructions are never loaded, so Save never
// sends them) and no Delete: nobody deletes an Assistant. Only its member ever reaches it (`visibleAgent`).

/// The viewer's Assistant avatar: the Exponential brand mark in the
/// foreground colour on bg muted, never a stored icon or colour.
struct AssistantAvatar: View {
    var size: CGFloat = 24

    var body: some View {
        Circle()
            .fill(DesignTokens.Palette.muted)
            .frame(width: size, height: size)
            .overlay {
                ExpLogoMark(size: size * 0.75, color: DesignTokens.Palette.foreground)
            }
            .accessibilityHidden(true)
    }
}

/// `agents.ensureAssistant` once per team per app run (the Settings Agents
/// group mounting is one of its callers).
@MainActor
enum AssistantEnsure {
    private static var ensured: Set<String> = []

    static func run(_ deps: AppDependencies, accountId: String, teamId: String) async {
        let key = "\(accountId)/\(teamId)"
        guard !accountId.isEmpty, !ensured.contains(key) else { return }
        ensured.insert(key)
        do {
            try await deps.agentsApi.ensureAssistant(accountId: accountId, teamId: teamId)
        } catch {
            // Quiet: the next mount in a later run retries; the page reads
            // `noDevice` until the row syncs.
            ensured.remove(key)
        }
    }
}

struct AssistantSettingsPage: View {
    let teamId: String
    /// nil = not created yet (the member has no device): `copy.noDevice`.
    let agentId: String?

    @Environment(\.accountId) private var accountId

    var body: some View {
        AssistantSettingsContent(teamId: teamId, agentId: agentId, accountId: accountId)
            .id("\(accountId)/\(agentId ?? "")")
    }
}

private struct AssistantSettingsContent: View {
    let teamId: String
    let agentId: String?
    let accountId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.toaster) private var toaster

    @State private var draft = AgentSettingsDraft()
    @State private var voice: VoiceSettingsModel
    @State private var agent: AgentDefinitionEntity?
    @State private var allDevices: [SteerDevice] = []
    @State private var seeded = false
    @State private var observation: Task<Void, Never>?

    init(teamId: String, agentId: String?, accountId: String) {
        self.teamId = teamId
        self.agentId = agentId
        self.accountId = accountId
        _voice = State(initialValue: VoiceSettingsModel(accountId: accountId))
    }

    /// The machines it can live on: online with a runnable agent, plus the
    /// one it is bound to whatever its state (the agent page's pool).
    /// NEW-99 (`runsOnCases`): its device is the voice device, so desktops
    /// only; a bound non-desktop stays, naming itself.
    private var devices: [SteerDevice] {
        let candidates = allDevices.filter { $0.isOnline && $0.hasRunnableAgent }
        let pool: [SteerDevice]
        if let bound = draft.deviceId,
           !candidates.contains(where: { $0.deviceId == bound }),
           let row = allDevices.first(where: { $0.deviceId == bound }) {
            pool = [row] + candidates
        } else {
            pool = candidates
        }
        let ids = Set(AgentSettings.runsOnDeviceIds(
            pool.map { (deviceId: $0.deviceId, kind: $0.kind) },
            boundId: draft.deviceId,
            assistant: true
        ))
        return pool.filter { ids.contains($0.deviceId) }
    }

    var body: some View {
        ZStack {
            AppBackground()
            if agentId == nil {
                EmptyState(title: AgentSettings.AssistantCopy.noDevice, icon: AppIcons.navDevices, size: .page)
                    .accessibilityIdentifier("assistant-no-device")
            } else {
                // EXP-1554: no Save changes bar; the fields save inline.
                AssistantSettingsForm(
                    draft: draft,
                    voice: voice,
                    devices: devices,
                    onCheckIn: { setCheckIn($0) },
                    teamId: teamId,
                    agentId: agentId,
                    skillIds: agent?.skillIds ?? [],
                    mcpServerIds: agent?.mcpServerIds ?? []
                )
            }
        }
        .navigationTitle(AgentSettings.AssistantCopy.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .voiceSettingsHost(voice)
        .task(id: agentId) { await start() }
        .task(id: draft.autosaveKey) { await autosave() }
        .onDisappear { observation?.cancel() }
        // The synced row is the truth for the check-in once no write of
        // ours is in flight.
        .onChange(of: agent?.triggers) { _, raw in
            guard seeded, !draft.triggerBusy, let raw else { return }
            draft.triggers = AgentTrigger.parseList(raw)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("assistant-settings-page")
    }

    // MARK: - Load

    private func start() async {
        allDevices = await DeviceQueries.devices(
            db: deps.db, accountId: accountId, teamId: teamId, userId: deps.auth.userId
        )
        guard let agentId else { return }
        observe(agentId: agentId)
    }

    private func observe(agentId: String) {
        observation?.cancel()
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let obs = ValueObservation.tracking { db in
            try AgentDefinitionEntity.fetchOne(db, key: agentId)
        }
        observation = Task {
            do {
                for try await row in obs.values(in: pool) {
                    await MainActor.run {
                        agent = row
                        if let row, !seeded {
                            seeded = true
                            draft.seed(from: row)
                        }
                    }
                }
            } catch {}
        }
    }

    // MARK: - Writes

    /// Check in writes at once: the preset's ONE schedule trigger (none for
    /// Off), keeping the stored zone, else the phone's. Optimistic, rolled
    /// back on a refusal.
    private func setCheckIn(_ id: AgentSettings.AssistantScheduleId) {
        guard let agentId, !draft.triggerBusy,
              id != AgentSettings.assistantScheduleOf(draft.triggers) else { return }
        let storedZone = draft.triggers.lazy.compactMap { trigger -> String? in
            if case let .schedule(schedule) = trigger.when { return schedule.tz }
            return nil
        }.first
        let inputs = AgentSettings.assistantScheduleTriggers(id, tz: storedZone ?? TimeZone.current.identifier)
        let previous = draft.triggers
        draft.triggers = inputs.map {
            AgentTrigger(id: AgentSettingsDraft.localTriggerPrefix + UUID().uuidString, enabled: $0.enabled, when: $0.when)
        }
        draft.triggerBusy = true
        Task {
            do {
                try await deps.agentsApi.update(
                    accountId: accountId, id: agentId, patch: AgentPatch(triggers: inputs)
                )
            } catch {
                draft.triggers = previous
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the check-in"),
                    description: error.trpcUserMessage
                )
            }
            draft.triggerBusy = false
        }
    }

    /// The inline save: whatever changed, once the edit settles (a
    /// `.task(id:)` restart cancels the pause, never a write on the wire).
    private func autosave() async {
        guard agentId != nil, seeded else { return }
        try? await Task.sleep(for: .milliseconds(600))
        // A write already in flight: wait it out, then send what is left.
        while draft.saving {
            try? await Task.sleep(for: .milliseconds(100))
            guard !Task.isCancelled else { return }
        }
        guard !Task.isCancelled, !draft.patch.isEmpty else { return }
        Task { await save() }
    }

    /// Name and Runs on ride `agents.update` inline (the
    /// device change = `{id, deviceId}`, the server moves the voice device).
    private func save() async {
        guard let agentId else { return }
        draft.submitted = true
        guard draft.errors.isEmpty else { return }
        let patch = draft.patch
        guard !patch.isEmpty else { return }
        draft.saving = true
        defer { draft.saving = false }
        let sent = draft.currentSnapshot()
        do {
            try await deps.agentsApi.update(accountId: accountId, id: agentId, patch: patch)
            draft.snapshotBaseline(sent)
            // The voice tray names the device: re-read the derived settings.
            if patch.deviceId != nil { await voice.load() }
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "save", thing: "the Assistant"),
                description: error.trpcUserMessage
            )
        }
    }
}

// MARK: - Form

/// The Assistant page's sections, top to bottom (`assistantSections`).
struct AssistantSettingsForm: View {
    @Bindable var draft: AgentSettingsDraft
    @Bindable var voice: VoiceSettingsModel
    let devices: [SteerDevice]
    var onCheckIn: (AgentSettings.AssistantScheduleId) -> Void = { _ in }
    /// #EXP-1661: the Skills row (nil hides it: the capture seam).
    var teamId: String? = nil
    var agentId: String? = nil
    var skillIds: [String] = []
    /// #EXP-1793: the MCP servers row (with the Skills row's ids).
    var mcpServerIds: [String] = []
    /// The team's MCP servers (`mcpServers.list`).
    @State private var mcpServers: [McpServerRow] = []

    private var device: SteerDevice? {
        devices.first { $0.deviceId == draft.deviceId }
    }

    private func label(_ key: String) -> String {
        AgentSettings.fields.first { $0.key == key }?.label ?? ""
    }

    var body: some View {
        Form {
            // identity
            Section {
                identityRow
                errorLine("identity")
            }
            .listRowBackground(glassFormRowFill)

            // runs-on
            Section {
                AgentDeviceRow(draft: draft, devices: devices, canEdit: true, title: label("device"))
                errorLine("device")
                AgentAccountRow(draft: draft, device: device, canEdit: true, title: label("account"))
                modelRow
                // #EXP-1661 (skills.json `agentRow`): no Instructions here, so
                // the last row of Runs on; the member owns their Assistant.
                if let teamId, let agentId {
                    AgentSkillsRow(teamId: teamId, agentId: agentId, skillIds: skillIds, canEdit: true)
                    // #EXP-1793 (agent-settings.json `mcpRow`): after Skills,
                    // while the team has a server or the Assistant names one.
                    if !mcpServers.isEmpty || !mcpServerIds.isEmpty {
                        AgentMcpServersRow(agentId: agentId, servers: mcpServers, mcpServerIds: mcpServerIds, canEdit: true)
                    }
                }
            } header: {
                GlassSectionHeader(AgentSettings.AssistantCopy.runsOn)
            }
            .listRowBackground(glassFormRowFill)

            // check-in
            Section {
                checkInRow
            } header: {
                GlassSectionHeader(AgentSettings.AssistantCopy.checkIn)
            }
            .listRowBackground(glassFormRowFill)

            // voice: THE block, without its Voice device + Agent pickers.
            VoiceSettingsBlock(model: voice, pickers: false, header: AgentSettings.AssistantCopy.voice)
        }
        .scrollContentBackground(.hidden)
        .listSectionSpacing(8)
        .tint(DesignTokens.Palette.primary)
        .modifier(AgentMcpServersLoader(teamId: agentId == nil ? nil : teamId, servers: $mcpServers))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AgentSettings.TestIds.page)
    }

    @ViewBuilder
    private func errorLine(_ field: String) -> some View {
        if let message = draft.error(field) {
            Text(message)
                .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                .foregroundStyle(DesignTokens.Semantic.red)
        }
    }

    /// The agent page's identity row with the brand mark in place of the
    /// icon + colour pickers: the name only.
    private var identityRow: some View {
        HStack(spacing: 8) {
            // Web's identity lead (`AssistantMark` size-9).
            AssistantAvatar(size: 36)
            TextField(AgentSettings.AssistantCopy.name, text: $draft.name)
                .font(.subheadline)
                .accessibilityIdentifier("agent-name")
        }
    }

    /// THE model picker over the login's agent; unpinned reads the
    /// dispatcher root's default for that agent (agent-settings.json
    /// `modelValueCases`).
    private var modelRow: some View {
        let agent = draft.cli ?? "claude"
        let value = AgentSettings.assistantModelValue(model: draft.model, cli: agent)
        return ModelPickerRow(
            "Model",
            selection: Binding(get: { value }, set: { draft.model = $0 }),
            agent: agent
        )
        .accessibilityIdentifier("assistant-model-row")
    }

    /// THE Picker over the presets; a stored custom schedule (never written
    /// by an Assistant) reads as no preset picked.
    private var checkInRow: some View {
        let current = AgentSettings.assistantScheduleOf(draft.triggers)
        return GlassPickerRow(
            AgentSettings.AssistantCopy.checkIn,
            selection: Binding<AgentSettings.AssistantScheduleId?>(
                get: { current },
                set: { picked in if let picked { onCheckIn(picked) } }
            ),
            options: AgentSettings.assistantSchedules.map { Optional($0.id) },
            label: { id in
                AgentSettings.assistantSchedules.first { $0.id == id }?.label ?? ""
            },
            enabled: !draft.triggerBusy
        )
        .accessibilityIdentifier("assistant-check-in-row")
    }
}
