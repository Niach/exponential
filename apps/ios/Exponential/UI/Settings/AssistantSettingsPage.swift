import ExpCore
import ExpUI
import GRDB
import SwiftUI

// S-3 / #EXP-1846 (agent-settings.json `page`): THE ASSISTANT'S host of the
// ONE agent page (`AgentSettingsForm assistant:`): identity (the name beside
// the Exponential brand mark), Runs on (its device IS the member's voice
// device, the server mirrors it; desktops only, NEW-99), Triggers (Check in
// + the trigger list, like every agent), Voice (#EXP-1847: the agent Voice
// rows every agent has, then the Voice settings block's member rows, minus
// its Voice device + Agent pickers (the server derives both from this
// Assistant) and its Language / Voice file / Transcript / Wake phrase). No Instructions (#EXP-1439: the prompt is built in; never
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

/// #EXP-1987 (agent-settings.json `assistant`): `agents.ensureAssistant`
/// once per app run per (account, team) where the screen has a current team
/// (the composer home, the chats list, the Settings Agents group: switch off,
/// that team gets its Assistant on first use), and once per ACCOUNT WITHOUT a
/// team where it has none (Settings › Assistant, account-level links). The
/// account-level answer is kept, so that page finds the agent even before
/// its row syncs; the synced row stays the truth.
@MainActor
enum AssistantEnsure {
    private static var ensured: Set<String> = []
    private static var answers: [String: String] = [:]

    /// The Assistant the team-less `ensureAssistant` answered for this
    /// account (nil = not asked yet, or no device to run it on).
    static func agentId(accountId: String) -> String? { answers[accountId] }

    @discardableResult
    static func run(_ deps: AppDependencies, accountId: String, teamId: String?) async -> String? {
        guard !accountId.isEmpty else { return nil }
        let key = teamId.map { "\(accountId)/\($0)" } ?? accountId
        guard !ensured.contains(key) else { return teamId == nil ? answers[accountId] : nil }
        ensured.insert(key)
        do {
            let agent = try await deps.agentsApi.ensureAssistant(accountId: accountId, teamId: teamId)
            if teamId == nil { answers[accountId] = agent?.id }
            // No device yet: ask again on the next mount.
            if agent == nil { ensured.remove(key) }
            return agent?.id
        } catch {
            // Quiet: the next mount retries; the page reads `noDevice`
            // until the row syncs.
            ensured.remove(key)
            return nil
        }
    }
}

extension SessionQueries {
    /// #EXP-1987: the viewer's ONE Assistant for Settings › Assistant,
    /// whichever team is current: theirs in that team (before the move),
    /// else the one in a team they do not sync (their personal team after
    /// it, `agentsInTeam`), else the one `ensureAssistant` answered, else
    /// their oldest.
    static func myAssistant(db: Database, teamId: String?, me: String?, answered: String?) throws -> AgentDefinitionEntity? {
        guard let me else { return nil }
        if let teamId,
           let row = try teamAgents(db: db, teamId: teamId, me: me).first(where: { $0.isAssistant && $0.ownerId == me }) {
            return row
        }
        let mine = try agents(db: db, teamId: nil).filter { $0.isAssistant && $0.ownerId == me }
        let teamIds = Set(try String.fetchAll(db, sql: "SELECT id FROM teams"))
        return mine.first { !teamIds.contains($0.teamId) }
            ?? mine.first { $0.id == answered }
            ?? mine.min { ($0.createdAt, $0.id) < ($1.createdAt, $1.id) }
    }
}

/// #EXP-1987 (settings-nav.json `nav`): Settings › Assistant is an ACCOUNT
/// page: no team of its own. It finds the viewer's ONE Assistant through
/// `ensureAssistant` (no team: the page has none) and the synced agents, so it reads the same
/// before the move (theirs in the current team) and after it (the one in
/// their personal team). The first team only feeds the per-team pickers
/// (#EXP-2079: the device list is the member's own machines).
struct AssistantSettingsPage: View {
    /// An agent page pushed for an Assistant (settings, chats, chat);
    /// nil = the viewer's own, resolved here.
    var agentId: String? = nil

    @Environment(\.accountId) private var accountId
    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @State private var resolved: String?
    @State private var loaded = false

    private var teamId: String { teamState.orderedTeams.first?.id ?? "" }

    var body: some View {
        Group {
            if let agentId {
                content(agentId)
            } else if loaded {
                // nil = not created yet (no device): `copy.noDevice`.
                content(resolved)
            } else {
                AppBackground()
            }
        }
        .task(id: "\(accountId)/\(teamId)/\(agentId ?? "")") { await resolve() }
    }

    private func content(_ id: String?) -> some View {
        AssistantSettingsContent(teamId: teamId, agentId: id, accountId: accountId)
            .id("\(accountId)/\(id ?? "")")
    }

    /// The synced row of the viewer's Assistant, re-read as the agents
    /// shape changes (its first creation, the move into the personal team).
    private func resolve() async {
        guard agentId == nil else { return }
        let ensure = Task { await AssistantEnsure.run(deps, accountId: accountId, teamId: nil) }
        guard let pool = try? deps.db.pool(forAccountId: accountId) else {
            loaded = true
            return
        }
        let teamId = teamState.orderedTeams.first?.id
        let me = deps.auth.userId
        let answered = AssistantEnsure.agentId(accountId: accountId)
        let observation = ValueObservation.tracking { db in
            try SessionQueries.myAssistant(db: db, teamId: teamId, me: me, answered: answered)?.id
        }.removeDuplicates()
        do {
            for try await next in observation.values(in: pool) {
                resolved = next
                // A first read with nothing synced waits for the answer.
                if next == nil, !loaded { _ = await ensure.value }
                loaded = true
            }
        } catch {
            loaded = true
        }
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

    /// The machines it can live on, online or not (the agent page's pool;
    /// an offline one is listed disabled unless bound, `runsOnRows`).
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
                    mcpServerIds: agent?.mcpServerIds ?? [],
                    agentVoice: agent?.voice ?? AgentVoice()
                )
            }
        }
        .pageHeader(AgentSettings.AssistantCopy.title, pushed: true)
        .voiceSettingsHost(voice)
        .task(id: agentId) { await start() }
        .task(id: draft.autosaveKey) { await autosave() }
        .onDisappear { observation?.cancel() }
        // The synced row is the truth for the triggers once no write of
        // ours is in flight.
        .onChange(of: agent?.triggers) { _, raw in
            guard seeded, !draft.triggerList.busy, let raw else { return }
            draft.triggerList.load(raw)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("assistant-settings-page")
    }

    // MARK: - Load

    private func start() async {
        // #EXP-2079 (agent-settings.json `assistant.chatTeams` DEVICES): an
        // Assistant's page lists the member's OWN machines, whatever teams
        // they are shared with, and no teammate's server (never the current
        // team's rule: the Assistant runs in the personal team).
        allDevices = await DeviceQueries.devices(
            db: deps.db, accountId: accountId, teamId: nil, userId: deps.auth.userId
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
