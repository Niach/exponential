import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1849 (apps.json `agentSlot`): App settings' AGENT group, right after
/// the identity: a section header over THE agent page's Runs on rows (the
/// same rows): Device = `Each member's own device` + the team-shared devices
/// (offline ones listed, disabled), Account and Model only while a device is
/// set, then Skills and MCP servers. Writes = `agents.update {id, deviceId,
/// …}` (`""` = each member's own device), by the app's creator and the
/// team's owners; members read.
struct AppAgentSlotGroup: View {
    let agent: AgentDefinitionEntity
    let canEdit: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var draft = AgentSettingsDraft()
    @State private var seeded = false
    @State private var allDevices: [SteerDevice] = []
    @State private var mcpServers: [McpServerRow] = []
    /// The slot as last seeded or saved (nil = empty).
    @State private var savedSlot: String?

    /// The team-shared devices (+ the bound slot whatever its sharing).
    private var slotDevices: [SteerDevice] {
        let ids = Set(Apps.AgentSlot.deviceIds(
            allDevices.map { (deviceId: $0.deviceId, sharedTeamIds: $0.sharedTeamIds) },
            teamId: agent.teamId
        ))
        return allDevices.filter { ids.contains($0.deviceId) || $0.deviceId == draft.deviceId }
    }

    private var device: SteerDevice? {
        allDevices.first { $0.deviceId == draft.deviceId }
    }

    private func label(_ key: String) -> String {
        AgentSettings.fields.first { $0.key == key }?.label ?? ""
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionHeader(Apps.AgentSlot.Copy.title)
            VStack(spacing: 0) {
                deviceRow
                    .rowPadding()
                if draft.deviceId != nil {
                    GlassDivider()
                    AgentAccountRow(draft: draft, device: device, canEdit: canEdit, title: label("account"))
                        .rowPadding()
                    GlassDivider()
                    modelRow
                        .rowPadding()
                }
                GlassDivider()
                AgentSkillsRow(teamId: agent.teamId, agentId: agent.id, skillIds: agent.skillIds, canEdit: canEdit)
                    .rowPadding()
                if !mcpServers.isEmpty || !agent.mcpServerIds.isEmpty {
                    GlassDivider()
                    AgentMcpServersRow(
                        agentId: agent.id, servers: mcpServers, mcpServerIds: agent.mcpServerIds, canEdit: canEdit
                    )
                    .rowPadding()
                }
            }
            .glassSection()
            .padding(.top, 4)
        }
        .tint(DesignTokens.Palette.primary)
        .modifier(AgentMcpServersLoader(teamId: agent.teamId, servers: $mcpServers))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.AgentSlot.testId)
        .task(id: agent.id) { await start() }
        .task(id: draft.autosaveKey) { await autosave() }
    }

    /// THE device picker: `Each member's own device` first.
    private var deviceRow: some View {
        let rows = [DevicePickerDevice(id: Apps.AgentSlot.eachMemberId, name: Apps.AgentSlot.Copy.eachMember)]
            + slotDevices.map { device in
                device.isOnline
                    ? DevicePickerDevice(device)
                    : DevicePickerDevice(
                        id: device.deviceId,
                        name: LaunchVocabulary.deviceName(device),
                        icon: DeviceIconDisplay.iconName(for: device),
                        description: DevicesList.Copy.offline,
                        disabled: true
                    )
            }
        return DevicePicker(
            devices: rows,
            value: draft.deviceId ?? Apps.AgentSlot.eachMemberId,
            onChange: { pick($0) },
            title: Apps.AgentSlot.Copy.runsOn,
            trigger: {
                GlassPickerRowLabel(
                    label("device"),
                    value: device.map(LaunchVocabulary.deviceName) ?? Apps.AgentSlot.Copy.eachMember
                )
            }
        )
        .disabled(!canEdit)
        .accessibilityIdentifier("app-agent-slot-device")
    }

    /// THE model picker over the login's agent (the agent page's row).
    private var modelRow: some View {
        let cli = draft.cli ?? "claude"
        let value = AgentSettings.assistantModelValue(model: draft.model, cli: cli)
        return ModelPickerRow(
            "Model",
            selection: Binding(get: { value }, set: { draft.model = $0 }),
            agent: cli
        )
        .disabled(!canEdit)
        .accessibilityIdentifier("agent-model-row")
    }

    private func start() async {
        allDevices = await DeviceQueries.devices(
            db: deps.db, accountId: accountId, teamId: agent.teamId, userId: deps.auth.userId
        )
        guard !seeded else { return }
        seeded = true
        draft.seed(from: agent)
        // "" = the empty slot: no device, no Account and Model rows.
        draft.deviceId = Apps.AgentSlot.slot(agent.deviceId)
        savedSlot = draft.deviceId
        draft.snapshotBaseline()
    }

    private func pick(_ picked: String) {
        let next = Apps.AgentSlot.slot(Apps.AgentSlot.write(picked))
        guard next != draft.deviceId else { return }
        if let next {
            draft.selectDevice(next, devices: allDevices)
        } else {
            draft.deviceId = nil
        }
    }

    /// Inline, like the agent page: a pick saves at once.
    private func autosave() async {
        guard canEdit, seeded else { return }
        while draft.saving {
            try? await Task.sleep(for: .milliseconds(100))
            guard !Task.isCancelled else { return }
        }
        var patch = draft.patch
        if draft.deviceId == nil {
            // Back to each member's own device: only the slot writes (the
            // server keeps every member's own runner).
            guard savedSlot != nil else { return }
            patch = AgentPatch(deviceId: Apps.AgentSlot.write(Apps.AgentSlot.eachMemberId))
        }
        guard !patch.isEmpty else { return }
        let sent = draft.currentSnapshot()
        draft.saving = true
        defer { draft.saving = false }
        do {
            try await deps.agentsApi.update(accountId: accountId, id: agent.id, patch: patch)
            draft.snapshotBaseline(sent)
            savedSlot = sent.deviceId
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "save", thing: "the agent"),
                description: error.trpcUserMessage
            )
        }
    }
}

private extension View {
    func rowPadding() -> some View {
        padding(.horizontal, 16).padding(.vertical, 12)
    }
}
