import ExpCore
import ExpUI
import GRDB
import SwiftUI

// S-3 / #EXP-1846 (agent-settings.json `page`): THE ASSISTANT'S host of the
// ONE agent page (`AgentSettingsForm assistant:`): identity (the name beside
// the Exponential brand mark), Runs on (its device IS the member's voice
// device, the server mirrors it; desktops only, NEW-99), Triggers (Check in
// + the trigger list, like every agent), Voice (the Voice settings block,
// minus its Voice device + Agent pickers: the server derives both from this
// Assistant). No Instructions (#EXP-1439: the prompt is built in; never
// loaded, so Save never sends them) and no Delete. Only its member ever
// reaches it (`visibleAgent`) and edits it.

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
        AgentSettingsDraft.runsOnDevices(allDevices, boundId: draft.deviceId, assistant: true)
    }

    var body: some View {
        ZStack {
            AppBackground()
            if agentId == nil {
                EmptyState(title: AgentSettings.AssistantCopy.noDevice, icon: AppIcons.navDevices, size: .page)
                    .accessibilityIdentifier("assistant-no-device")
            } else {
                // EXP-1554: no Save changes bar; the fields save inline.
                AgentSettingsForm(
                    draft: draft,
                    devices: devices,
                    canEdit: true,
                    assistant: true,
                    voice: voice,
                    accountId: accountId,
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
        // The synced row is the truth for the triggers once no write of
        // ours is in flight.
        .onChange(of: agent?.triggers) { _, raw in
            guard seeded, !draft.triggerList.busy, let raw else { return }
            draft.triggerList.triggers = AgentTrigger.parseList(raw)
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
