import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-1291 / #EXP-1846: THE AGENT PAGE, ONE for the Assistant and every
// team agent (fixture `agent-settings.json` `page`, top to bottom): Identity
// (the board form's icon picker + colour swatch + name; the Assistant: its
// brand mark + name), Runs on (Device, Account, Model, Skills, MCP servers),
// Triggers (`AgentTriggersSection`: Check in, then the trigger list + Add),
// Voice (the Assistant only), Instructions (team agents only; markdown,
// server-only: `agents.get`) and Delete (team agents, owners, confirmed).
// A team agent's members read; the Assistant is its member's.
//
// NEW-2: an agent is a DEFINITION row (`agents`), never a session. A new one
// is ONE `agents.create`; after that the fields save through `agents.update`
// — the triggers at once (a WHOLE-ARRAY write per add/edit/toggle/delete),
// everything else INLINE (EXP-1554, like every platform: a pick saves at
// once, the name after a short pause, the instructions on blur).

/// The page's editable state, hoisted out of the views (`AgentSettingsForm`
/// draws it).
@MainActor
@Observable
final class AgentSettingsDraft {
    var name = ""
    var icon = SessionTree.ListCopy.agentDefaultIcon
    var color = SessionTree.ListCopy.agentDefaultColor
    var deviceId: String?
    /// The picked login's agent (`sessions.cli`); nil = the device's last
    /// used one.
    var cli: String?
    /// The picked profile id; nil = unpinned (the device's last used login).
    var account: String?
    /// S-3: the pinned model (the Assistant page's Runs on); nil = the
    /// device's default for the agent.
    var model: String?
    /// An EXISTING agent's CLI: the page never changes it, so
    /// its Account row only offers that CLI's logins. nil = a new agent (any
    /// login, its agent becomes the CLI) or one created unpinned.
    private(set) var lockedCli: String?
    /// The instructions' markdown editor.
    let instructions = IssueEditorModel()
    /// False while an existing agent's `agents.get` is in flight.
    var instructionsLoaded = false
    var instructionsFailed = false
    /// The Triggers group's rows (`AgentTriggersSection`); a new agent
    /// keeps them here until Create.
    let triggerList = AgentTriggerList()
    var saving = false
    /// Field errors show once a submit was tried.
    var submitted = false

    /// What the server last said, for the dirty check and the patch.
    private var baseline = Baseline()

    struct Baseline: Equatable {
        var name = ""
        var icon = SessionTree.ListCopy.agentDefaultIcon
        var color = SessionTree.ListCopy.agentDefaultColor
        var deviceId: String?
        var cli: String?
        var account: String?
        var model: String?
        /// nil until loaded.
        var instructions: String?
    }

    /// A new agent: nothing to load.
    func seedNew() {
        instructionsLoaded = true
        snapshotBaseline()
    }

    /// An existing agent's synced row (once, on first resolve).
    func seed(from agent: AgentDefinitionEntity) {
        name = agent.name
        icon = agent.icon ?? SessionTree.ListCopy.agentDefaultIcon
        color = agent.color ?? SessionTree.ListCopy.agentDefaultColor
        deviceId = agent.deviceId
        cli = agent.cli
        lockedCli = agent.cli
        account = AccountOptions.wireAccount(agent.account)
        model = agent.model
        triggerList.triggers = AgentTrigger.parseList(agent.triggers)
        let instructionsBase = baseline.instructions
        snapshotBaseline()
        baseline.instructions = instructionsBase
    }

    func loadInstructions(_ markdown: String) {
        instructions.load(markdown: markdown, baseURL: nil)
        instructionsLoaded = true
        instructionsFailed = false
        baseline.instructions = instructions.currentMarkdown()
    }

    /// After a successful save: what was sent is the new baseline. An
    /// inline save passes the snapshot it took BEFORE the request, so an
    /// edit made while it flew stays dirty and saves next.
    func snapshotBaseline(_ sent: Baseline? = nil) {
        baseline = sent ?? currentSnapshot()
    }

    /// The fields as they stand, as a baseline.
    func currentSnapshot() -> Baseline {
        Baseline(
            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
            icon: icon,
            color: color,
            deviceId: deviceId,
            cli: cli,
            account: account,
            model: model,
            instructions: instructionsLoaded ? instructions.currentMarkdown() : nil
        )
    }

    /// Every inline-saved scalar field, for the page's debounced autosave
    /// (the instructions save on blur instead).
    var autosaveKey: String {
        [name, icon, color, deviceId ?? "", cli ?? "", account ?? "", model ?? ""]
            .joined(separator: "\u{1}")
    }

    // MARK: - Validation

    var form: AgentSettings.Form {
        AgentSettings.Form(
            name: name,
            deviceId: deviceId,
            instructions: instructionsLoaded ? instructions.currentMarkdown() : ""
        )
    }

    var errors: [AgentSettings.FormError] {
        submitted ? AgentSettings.validate(form) : []
    }

    func error(_ field: String) -> String? {
        errors.first { $0.field == field }?.message
    }

    // MARK: - Patch

    /// Only what changed rides `agents.update`; the triggers write on
    /// their own (`AgentTriggersSection`).
    var patch: AgentPatch {
        var patch = AgentPatch()
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed != baseline.name { patch.name = trimmed }
        if icon != baseline.icon { patch.icon = icon }
        if color != baseline.color { patch.color = color }
        if let deviceId, deviceId != baseline.deviceId { patch.deviceId = deviceId }
        // An explicit null = the device's last used login.
        if cli != baseline.cli || account != baseline.account { patch.account = .some(account) }
        if model != baseline.model { patch.model = .some(model) }
        if instructionsLoaded, let base = baseline.instructions {
            let markdown = instructions.currentMarkdown()
            if markdown != base { patch.instructions = markdown }
        }
        return patch
    }

    // MARK: - Device + account

    /// Every signed-in login the machine reports, its last used one first
    /// (`AccountOptions.flatten`, the composer's list) — only `cli`'s when
    /// one is given (an existing agent's, which no update can change).
    static func accountOptions(on device: SteerDevice?, cli: String? = nil) -> [AccountOption] {
        guard let device else { return [] }
        let options = AccountOptions.flatten(
            accounts: device.agentAccounts,
            usage: device.agentUsage,
            launchDefaults: device.launchDefaults
        )
        guard let cli else { return options }
        return options.filter { $0.agent == cli }
    }

    /// What the Account row names (`AgentSettings.agentAccountValue`,
    /// fixture-locked ×4): the pinned login (of the agent's CLI), else that
    /// CLI's first login, else the machine's last used one, else the first.
    /// Only REPORTED logins count (a blank-id fallback row is none), so a
    /// machine that reports none reads `Last used`.
    func selectedAccount(in options: [AccountOption]) -> AccountOption? {
        AgentSettings.agentAccountValue(options: options.filter { !$0.id.isEmpty }, account: account, cli: cli)
    }

    func selectAccount(_ option: AccountOption) {
        cli = option.agent
        account = AccountOptions.wireAccount(option.id)
    }

    /// Profile ids are per machine: a device switch carries the SAME login
    /// (agent + email) when the new machine has it, else unpins.
    func selectDevice(_ id: String, devices: [SteerDevice]) {
        guard id != deviceId else { return }
        let previous = selectedAccount(
            in: Self.accountOptions(on: devices.first { $0.deviceId == deviceId }, cli: lockedCli)
        )
        deviceId = id
        let options = Self.accountOptions(on: devices.first { $0.deviceId == id }, cli: lockedCli)
        if let carried = AccountOptions.carried(options, previous: previous) {
            selectAccount(carried)
        } else {
            cli = lockedCli
            account = nil
        }
    }

    /// The Device row's pool: online with a runnable agent, plus the bound
    /// one whatever its state; NEW-99 (`runsOnCases`): an Assistant's lists
    /// desktops only (a bound non-desktop stays, naming itself), a team
    /// agent's every team device.
    static func runsOnDevices(_ all: [SteerDevice], boundId: String?, assistant: Bool) -> [SteerDevice] {
        let candidates = all.filter { $0.isOnline && $0.hasRunnableAgent }
        var pool = candidates
        if let boundId,
           !candidates.contains(where: { $0.deviceId == boundId }),
           let row = all.first(where: { $0.deviceId == boundId }) {
            pool = [row] + candidates
        }
        let ids = Set(AgentSettings.runsOnDeviceIds(
            pool.map { (deviceId: $0.deviceId, kind: $0.kind) },
            boundId: boundId,
            assistant: assistant
        ))
        return pool.filter { ids.contains($0.deviceId) }
    }
}

// MARK: - Page (host)

struct AgentSettingsPage: View {
    let teamId: String
    /// nil = a new agent.
    let agentId: String?
    /// Owners write; members read.
    let isOwner: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @Environment(\.toaster) private var toaster

    @State private var draft = AgentSettingsDraft()
    @State private var agent: AgentDefinitionEntity?
    @State private var allDevices: [SteerDevice] = []
    @State private var seeded = false
    @State private var observation: Task<Void, Never>?
    @State private var showDelete = false
    @State private var deleting = false

    /// The machines an agent can live on: online with a runnable agent (the
    /// composer's pool), plus the one it is bound to whatever its state;
    /// every kind for a team agent (`runsOnCases`).
    private var devices: [SteerDevice] {
        AgentSettingsDraft.runsOnDevices(allDevices, boundId: draft.deviceId, assistant: false)
    }

    private var title: String {
        agentId == nil ? AgentSettings.Copy.newAgent : (agent?.name ?? "")
    }

    private var canSubmit: Bool {
        guard isOwner, !draft.saving else { return false }
        return agentId == nil || !draft.patch.isEmpty
    }

    var body: some View {
        // S-3: an Assistant (only ever the viewer's own) opens THE Assistant
        // page wherever an agent page is pushed (settings, chats, chat).
        if let agentId, agent?.isAssistant == true {
            AssistantSettingsPage(teamId: teamId, agentId: agentId)
        } else {
            agentPage
        }
    }

    private var agentPage: some View {
        ZStack {
            AppBackground()

            AgentSettingsForm(
                draft: draft,
                devices: devices,
                canEdit: isOwner,
                baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                accountId: accountId,
                httpClient: deps.httpClient,
                onRetryInstructions: { Task { await loadInstructions(retried: true) } },
                onDelete: isOwner && agentId != nil ? { showDelete = true } : nil,
                teamId: teamId,
                agentId: agentId,
                skillIds: agent?.skillIds ?? [],
                mcpServerIds: agent?.mcpServerIds ?? []
            )
        }
        // EXP-1554: only a NEW agent has a submit; an existing one saves
        // inline (no Save changes bar on any platform).
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if isOwner, agentId == nil {
                GlassSubmitButton("Create agent", enabled: canSubmit, loading: draft.saving) {
                    Task { await save() }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
            }
        }
        .task(id: draft.autosaveKey) { await autosave(after: .milliseconds(600)) }
        .onChange(of: draft.instructions.focusedBlockId) { _, focused in
            if focused == nil { Task { await autosave(after: nil) } }
        }
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task(id: agentId) { await start() }
        .onDisappear { observation?.cancel() }
        // The synced row is the truth for the triggers once no write of ours
        // is in flight (the echo of the last one included).
        .onChange(of: agent?.triggers) { _, raw in
            guard seeded, !draft.triggerList.busy, let raw else { return }
            draft.triggerList.triggers = AgentTrigger.parseList(raw)
        }
        // EXP-1215: the app's own alert card, on a zero-size node of its own.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showDelete) {
                    GlassAlert(
                        title: AgentSettings.Copy.deleteConfirm,
                        actions: [
                            GlassAlertAction("Cancel", isCancel: true) {},
                            GlassAlertAction("Delete", role: .destructive, enabled: !deleting) {
                                Task { await deleteAgent() }
                            },
                        ]
                    )
                }
        }
    }

    // MARK: - Load

    private func start() async {
        allDevices = await DeviceQueries.devices(
            db: deps.db, accountId: accountId, teamId: teamId, userId: deps.auth.userId
        )
        guard let agentId else {
            guard !seeded else { return }
            seeded = true
            draft.seedNew()
            // EXP-622: the caller's default machine, else the first one.
            let pool = devices
            draft.deviceId = (pool.first(where: \.isDefaultDevice) ?? pool.first)?.deviceId
            draft.snapshotBaseline()
            return
        }
        observe(agentId: agentId)
        await loadInstructions(retried: false)
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

    /// The server-only instructions. A failed first load is quiet (the
    /// Retry row); a failed Retry tap toasts.
    private func loadInstructions(retried: Bool) async {
        guard let agentId else { return }
        draft.instructionsFailed = false
        do {
            let details = try await deps.agentsApi.get(accountId: accountId, id: agentId)
            draft.loadInstructions(details.instructions)
        } catch {
            draft.instructionsFailed = true
            if retried {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "load", thing: "the instructions"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    // MARK: - Writes

    /// The inline save of an existing agent: whatever changed, once the
    /// edit settles (`after`; a `.task(id:)` restart cancels the pause).
    private func autosave(after pause: Duration?) async {
        guard isOwner, agentId != nil, seeded else { return }
        if let pause {
            try? await Task.sleep(for: pause)
            guard !Task.isCancelled else { return }
        }
        // A write already in flight: wait it out, then send what is left.
        while draft.saving {
            try? await Task.sleep(for: .milliseconds(100))
            guard !Task.isCancelled else { return }
        }
        guard !draft.patch.isEmpty else { return }
        // Its own task: the next keystroke restarting `.task(id:)` must not
        // cancel a write already on the wire.
        Task { await save() }
    }

    private func save() async {
        draft.submitted = true
        guard draft.errors.isEmpty, let deviceId = draft.deviceId else { return }
        draft.saving = true
        defer { draft.saving = false }
        do {
            if let agentId {
                let patch = draft.patch
                guard !patch.isEmpty else { return }
                let sent = draft.currentSnapshot()
                try await deps.agentsApi.update(accountId: accountId, id: agentId, patch: patch)
                draft.snapshotBaseline(sent)
            } else {
                try await deps.agentsApi.create(
                    accountId: accountId,
                    AgentCreateInput(
                        teamId: teamId,
                        name: draft.name.trimmingCharacters(in: .whitespacesAndNewlines),
                        icon: draft.icon,
                        color: draft.color,
                        instructions: draft.instructions.currentMarkdown(),
                        triggers: draft.triggerList.inputs(draft.triggerList.triggers),
                        deviceId: deviceId,
                        cli: draft.cli,
                        account: draft.account,
                        model: draft.model
                    )
                )
                dismiss()
            }
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(
                    verb: agentId == nil ? "create" : "save", thing: "the agent"
                ),
                description: error.trpcUserMessage
            )
        }
    }

    private func deleteAgent() async {
        guard let agentId else { return }
        deleting = true
        defer { deleting = false }
        do {
            try await deps.agentsApi.delete(accountId: accountId, id: agentId)
            dismiss()
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "delete", thing: "the agent"),
                description: error.trpcUserMessage
            )
        }
    }
}

// MARK: - Form

/// THE agent page's groups, top to bottom (fixture `page.sections`), for
/// the Assistant (`assistant`) and every team agent alike. Pure over the
/// draft + the device pool, so the capture seam draws it too.
struct AgentSettingsForm: View {
    @Bindable var draft: AgentSettingsDraft
    let devices: [SteerDevice]
    let canEdit: Bool
    /// The viewer's Assistant: brand mark identity, Voice, no Instructions
    /// and no Delete.
    var assistant = false
    /// The Assistant's Voice group (nil = none).
    var voice: VoiceSettingsModel? = nil
    var baseURL: URL? = nil
    var accountId: String = ""
    var httpClient: HTTPClient? = nil
    var onRetryInstructions: () -> Void = {}
    /// nil hides the Delete row (members, an Assistant, or an agent not
    /// created yet).
    var onDelete: (() -> Void)? = nil
    /// #EXP-1661: the Skills row (an existing agent only; nil hides it).
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

            // runs-on: device, account, model, skills, mcp-servers
            Section {
                AgentDeviceRow(draft: draft, devices: devices, canEdit: canEdit, title: label("device"))
                errorLine("device")
                AgentAccountRow(draft: draft, device: device, canEdit: canEdit, title: label("account"))
                modelRow
                if let teamId, let agentId {
                    AgentSkillsRow(teamId: teamId, agentId: agentId, skillIds: skillIds, canEdit: canEdit)
                    // #EXP-1793 (`mcpRow`): while the team has a server or
                    // the agent names one.
                    if !mcpServers.isEmpty || !mcpServerIds.isEmpty {
                        AgentMcpServersRow(
                            agentId: agentId, servers: mcpServers, mcpServerIds: mcpServerIds, canEdit: canEdit
                        )
                    }
                }
            } header: {
                GlassSectionHeader(AgentSettings.Copy.runsOn)
            }
            .listRowBackground(glassFormRowFill)

            // triggers: Check in, then the list + Add
            if let teamId {
                AgentTriggersSection(agentId: agentId, editable: canEdit, teamId: teamId, list: draft.triggerList)
            }

            // voice: the Assistant only, THE block without its Voice device +
            // Agent pickers.
            if assistant, let voice {
                VoiceSettingsBlock(model: voice, pickers: false, header: AgentSettings.AssistantCopy.voice)
            }

            // instructions: team agents only (the Assistant's is built in).
            if !assistant {
                Section {
                    instructionsEditor
                    errorLine("instructions")
                } footer: {
                    // Web's caption under the field.
                    Text(label("instructions"))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .listRowBackground(glassFormRowFill)
            }

            if !assistant, let onDelete {
                // EXP-1554: THE destructive button, under the form,
                // left-aligned, on no card (web's placement).
                Section {
                    DestructiveButton(label("delete"), action: onDelete)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("agent-delete")
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0))
            }
        }
        // EXP-603: the page's own background shows through the grouped list;
        // rows carry the glass fill.
        .scrollContentBackground(.hidden)
        .listSectionSpacing(8)
        // EXP-594: white control tint — system blue is retired.
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

    /// The board form's ONE row: icon, colour, name (EXP-862, ×4); the
    /// Assistant's brand mark in place of the icon + colour pickers.
    @ViewBuilder
    private var identityRow: some View {
        if assistant {
            HStack(spacing: 8) {
                // Web's identity lead (`AssistantMark` size-9).
                AssistantAvatar(size: 36)
                TextField(AgentSettings.AssistantCopy.name, text: $draft.name)
                    .font(.subheadline)
                    .accessibilityIdentifier("agent-name")
            }
            .disabled(!canEdit)
        } else {
            HStack(spacing: 8) {
                IconPicker(selection: $draft.icon, tint: Color(hex: draft.color))
                ColorSwatchPicker(selection: $draft.color)
                TextField(AgentSettings.Copy.namePlaceholder, text: $draft.name)
                    .font(.subheadline)
                    .accessibilityIdentifier("agent-name")
            }
            .disabled(!canEdit)
        }
    }

    /// THE model picker over the login's agent, every agent's; unpinned
    /// reads the device default for that agent (`modelValueCases`).
    private var modelRow: some View {
        let agent = draft.cli ?? "claude"
        let value = AgentSettings.assistantModelValue(model: draft.model, cli: agent)
        return ModelPickerRow(
            "Model",
            selection: Binding(get: { value }, set: { draft.model = $0 }),
            agent: agent
        )
        .disabled(!canEdit)
        .accessibilityIdentifier("agent-model-row")
    }

    private var instructionsEditor: some View {
        AgentInstructionsEditor(
            draft: draft,
            canEdit: canEdit,
            baseURL: baseURL,
            accountId: accountId,
            httpClient: httpClient,
            onRetry: onRetryInstructions
        )
    }
}

// MARK: - Shared rows (the agent page + the Assistant page)

/// The shared `DevicePicker` — the composer's rows (glyph, name, owner).
struct AgentDeviceRow: View {
    @Bindable var draft: AgentSettingsDraft
    let devices: [SteerDevice]
    let canEdit: Bool
    let title: String

    private var device: SteerDevice? {
        devices.first { $0.deviceId == draft.deviceId }
    }

    var body: some View {
        DevicePicker(
            devices: devices.map(DevicePickerDevice.init),
            value: draft.deviceId,
            onChange: { draft.selectDevice($0, devices: devices) },
            trigger: {
                // Web's row: the device's name as the value, no glyph.
                GlassPickerRowLabel(
                    title,
                    value: device.map(LaunchVocabulary.deviceName) ?? ""
                )
            }
        )
        .disabled(!canEdit)
        .accessibilityIdentifier("agent-device-row")
    }
}

/// #EXP-1661 (skills.json `agentRow`): the skills the agent's runs start
/// with, `Skills.agentSkillsValue` (the picked names as the picker summary,
/// None muted at zero) opening THE skill
/// picker; each change saves the whole list (`agents.update {skillIds}`).
/// Owners edit, members read (the Device row's gate).
struct AgentSkillsRow: View {
    let teamId: String
    let agentId: String
    /// The synced row's pick.
    let skillIds: [String]
    let canEdit: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var skills: [SkillEntity] = []
    /// The pick while its write is in flight, until the row echoes it.
    @State private var pending: [String]?

    /// The pick clamped to the skills that still exist, in list order.
    private var picked: [String] {
        Skills.preselectSkillIds(skills, saved: pending ?? skillIds)
    }

    var body: some View {
        SkillPicker(
            skills: skills,
            value: picked,
            onChange: { save($0) },
            trigger: { label }
        )
        .disabled(!canEdit)
        .accessibilityIdentifier("agent-skills-row")
        .task(id: "\(accountId)/\(teamId)") { await observe() }
        .onChange(of: skillIds) { _, synced in
            if synced == pending { pending = nil }
        }
    }

    @ViewBuilder
    private var label: some View {
        let names = picked.compactMap { id in skills.first { $0.id == id }?.name }
        if !names.isEmpty {
            GlassPickerRowLabel(Skills.agentRowLabel, value: Skills.agentSkillsValue(names))
        } else {
            // The Account row's muted `Last used` look.
            HStack(spacing: 8) {
                Text(Skills.agentRowLabel)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                Spacer(minLength: 8)
                Text(Skills.agentSkillsValue([]))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                AppIcon(AppIcons.uiChevronRight, size: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .contentShape(Rectangle())
        }
    }

    private func observe() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let teamId = teamId
        let observation = ValueObservation.tracking { db in
            try SkillEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }.removeDuplicates()
        do {
            for try await rows in observation.values(in: pool) {
                skills = rows
            }
        } catch {}
    }

    private func save(_ next: [String]) {
        guard canEdit else { return }
        let previous = pending
        pending = next
        Task {
            do {
                try await deps.agentsApi.update(accountId: accountId, id: agentId, patch: AgentPatch(skillIds: next))
            } catch {
                if pending == next { pending = previous }
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the skills"),
                    description: error.trpcUserMessage
                )
            }
        }
    }
}

/// #EXP-1793 (agent-settings.json `mcpRow`): the team MCP servers the
/// agent's runs start with, `AgentSettings.agentMcpServersValue` (the picked
/// names in the team's order as the picker summary, None muted at zero)
/// opening THE composer's MCP server picker; each change saves the whole
/// list (`agents.update {mcpServerIds}`). Owners edit, members read. Shown
/// only while the team has a server or the agent names one.
struct AgentMcpServersRow: View {
    let agentId: String
    /// The team's servers (`AgentMcpServersLoader`), in the team's order.
    let servers: [McpServerRow]
    /// The synced row's pick.
    let mcpServerIds: [String]
    let canEdit: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    /// The pick while its write is in flight, until the row echoes it.
    @State private var pending: [String]?

    /// The pick in the team's order (an id no longer listed drops out).
    private var picked: [String] {
        let ids = Set(pending ?? mcpServerIds)
        return servers.map(\.id).filter { ids.contains($0) }
    }

    var body: some View {
        McpServerPicker(
            servers: servers,
            value: picked,
            onChange: { save($0) },
            title: AgentSettings.McpRow.label,
            trigger: { label }
        )
        .disabled(!canEdit)
        .accessibilityIdentifier("agent-mcp-servers-row")
        .onChange(of: mcpServerIds) { _, synced in
            if synced == pending { pending = nil }
        }
    }

    @ViewBuilder
    private var label: some View {
        let names = picked.compactMap { id in servers.first { $0.id == id }?.name }
        if !names.isEmpty {
            GlassPickerRowLabel(AgentSettings.McpRow.label, value: AgentSettings.agentMcpServersValue(names))
        } else {
            // The Skills row's muted `None` look.
            HStack(spacing: 8) {
                Text(AgentSettings.McpRow.label)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                Spacer(minLength: 8)
                Text(AgentSettings.agentMcpServersValue([]))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                AppIcon(AppIcons.uiChevronRight, size: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .contentShape(Rectangle())
        }
    }

    private func save(_ next: [String]) {
        guard canEdit else { return }
        let previous = pending
        pending = next
        Task {
            do {
                try await deps.agentsApi.update(accountId: accountId, id: agentId, patch: AgentPatch(mcpServerIds: next))
            } catch {
                if pending == next { pending = previous }
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the MCP servers"),
                    description: error.trpcUserMessage
                )
            }
        }
    }
}

/// #EXP-1793: reads the team's MCP servers (`mcpServers.list`) for the agent
/// page's MCP servers row; nil `teamId` (a new agent, a specimen) = none.
struct AgentMcpServersLoader: ViewModifier {
    let teamId: String?
    @Binding var servers: [McpServerRow]

    @Environment(AppDependencies.self) private var deps: AppDependencies?
    @Environment(\.accountId) private var accountId

    func body(content: Content) -> some View {
        content.task(id: "\(accountId)/\(teamId ?? "")") {
            guard let teamId, let deps else { return }
            servers = (try? await McpServersApi(trpc: deps.trpc).list(accountId: accountId, teamId: teamId)) ?? []
        }
    }
}

/// The shared `AccountPicker` over the picked machine's logins; a pick
/// names the agent (`cli`) too. EXP-1306: the value reads like the
/// composer chip (the agent's brand mark + the login's email) and the row
/// like the Device row beside it. A lone login is nothing to pick: the
/// same line outside the picker (which disables a lone login, and a
/// disabled Form row dims). No login reported = `Last used`, muted.
/// EXP-1307: every form ends in the chevron, like the rows beside it (web).
struct AgentAccountRow: View {
    @Bindable var draft: AgentSettingsDraft
    let device: SteerDevice?
    let canEdit: Bool
    let title: String

    var body: some View {
        let options = AgentSettingsDraft.accountOptions(on: device, cli: draft.lockedCli)
        let current = draft.selectedAccount(in: options)
        if let current, options.count > 1 {
            AccountPicker(
                options: options,
                value: current.key,
                onChange: { key in
                    guard let picked = options.first(where: { $0.key == key }) else { return }
                    draft.selectAccount(picked)
                },
                mark: { AgentBrandMark.image($0) },
                trigger: { label(current) }
            )
            .disabled(!canEdit)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("agent-account-row")
        } else {
            label(current)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("agent-account-row")
        }
    }

    private func label(_ current: AccountOption?) -> some View {
        HStack(spacing: 8) {
            Text(title)
                .foregroundStyle(.white.opacity(TextOpacity.primary))
            Spacer(minLength: 8)
            if let current {
                AccountPickerTriggerLabel(
                    option: current,
                    mark: AgentBrandMark.image(current.agent),
                    chevron: true,
                    style: .rowValue
                )
            } else {
                HStack(spacing: 6) {
                    Text(AgentSettings.Copy.accountLastUsed)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                    AppIcon(AppIcons.uiChevronRight, size: 14)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
        }
    }
}

/// The instructions' markdown editor (server-only text: a skeleton until
/// `agents.get` answered, a Retry row when it failed).
struct AgentInstructionsEditor: View {
    @Bindable var draft: AgentSettingsDraft
    let canEdit: Bool
    var baseURL: URL? = nil
    var accountId: String = ""
    var httpClient: HTTPClient? = nil
    var onRetry: () -> Void = {}

    var body: some View {
        if draft.instructionsLoaded {
            MarkdownEditor(
                model: draft.instructions,
                placeholder: AgentSettings.Copy.instructionsPlaceholder,
                baseURL: baseURL,
                accountId: accountId,
                httpClient: httpClient,
                isReadOnly: !canEdit,
                minHeight: 160
            )
            .accessibilityIdentifier("agent-instructions")
        } else if draft.instructionsFailed {
            VStack(spacing: 12) {
                Text("Instructions unavailable")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                GlassPill(FeedbackStates.Errors.retry, icon: AppIcons.uiRefresh, mode: .action {
                    onRetry()
                })
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
        } else {
            // EXP-1275 (D8): a placeholder, not "Loading…".
            SkeletonRows(rows: 3, rowHeight: 22, dot: false, dividers: false)
        }
    }
}
