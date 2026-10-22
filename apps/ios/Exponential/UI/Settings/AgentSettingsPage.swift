import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-1291 Threads v2: the AGENT PAGE — Team settings → Agents, laid out
// exactly like a board (fixture `agent-settings.json` `fields`, top to
// bottom): the board form's ONE identity row (icon picker + colour swatch +
// name), Device (required), Account (the device's logins), Triggers (one row
// per trigger: `AgentSettings.triggerSummary` + its switch, Add opens the
// trigger editor), Instructions (markdown, server-only: `agents.get`) and
// Delete (owner-only, confirmed). Members read; every owner-only control is
// hidden or disabled for them.
//
// NEW-2: an agent is a DEFINITION row (`agents`), never a session. A new one
// is ONE `agents.create`; after that the fields save through `agents.update`
// — the triggers at once (a WHOLE-ARRAY write per add/edit/toggle/delete),
// everything else on Save changes.

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
    /// An EXISTING agent's CLI: the page never changes it, so
    /// its Account row only offers that CLI's logins. nil = a new agent (any
    /// login, its agent becomes the CLI) or one created unpinned.
    private(set) var lockedCli: String?
    /// The instructions' markdown editor.
    let instructions = IssueEditorModel()
    /// False while an existing agent's `agents.get` is in flight.
    var instructionsLoaded = false
    var instructionsFailed = false
    /// Stored order. A trigger added before the agent exists (or before the
    /// echo lands) carries a `local-` id and is sent WITHOUT one.
    var triggers: [AgentTrigger] = []
    /// One trigger write at a time: each replaces the whole array.
    var triggerBusy = false
    var saving = false
    /// Field errors show once a submit was tried.
    var submitted = false

    /// What the server last said, for the dirty check and the patch.
    private var baseline = Baseline()

    private struct Baseline: Equatable {
        var name = ""
        var icon = SessionTree.ListCopy.agentDefaultIcon
        var color = SessionTree.ListCopy.agentDefaultColor
        var deviceId: String?
        var cli: String?
        var account: String?
        /// nil until loaded.
        var instructions: String?
    }

    static let localTriggerPrefix = "local-"

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
        triggers = AgentTrigger.parseList(agent.triggers)
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

    /// After a successful save: what was sent is the new baseline.
    func snapshotBaseline() {
        baseline = Baseline(
            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
            icon: icon,
            color: color,
            deviceId: deviceId,
            cli: cli,
            account: account,
            instructions: instructionsLoaded ? instructions.currentMarkdown() : nil
        )
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
    /// their own.
    var patch: AgentPatch {
        var patch = AgentPatch()
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed != baseline.name { patch.name = trimmed }
        if icon != baseline.icon { patch.icon = icon }
        if color != baseline.color { patch.color = color }
        if let deviceId, deviceId != baseline.deviceId { patch.deviceId = deviceId }
        // An explicit null = the device's last used login.
        if cli != baseline.cli || account != baseline.account { patch.account = .some(account) }
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

    // MARK: - Triggers

    /// The whole-array write of `list`: a local (unsaved) trigger travels
    /// without an id, so the server mints one.
    func inputs(_ list: [AgentTrigger]) -> [AgentTriggerInput] {
        list.map { trigger in
            var input = AgentTriggerInput(trigger)
            if trigger.id.hasPrefix(Self.localTriggerPrefix) { input.id = nil }
            return input
        }
    }

    /// The list with `input` added (`editing` nil) or swapped in.
    func applying(_ input: AgentTriggerInput, editing: AgentTrigger?) -> [AgentTrigger] {
        guard let editing else {
            let added = AgentTrigger(
                id: Self.localTriggerPrefix + UUID().uuidString,
                enabled: input.enabled,
                when: input.when
            )
            return triggers + [added]
        }
        return triggers.map { trigger in
            trigger.id == editing.id
                ? AgentTrigger(id: trigger.id, enabled: input.enabled, when: input.when)
                : trigger
        }
    }

    func settingEnabled(_ trigger: AgentTrigger, _ enabled: Bool) -> [AgentTrigger] {
        triggers.map { $0.id == trigger.id ? AgentTrigger(id: $0.id, enabled: enabled, when: $0.when) : $0 }
    }

    func removing(_ trigger: AgentTrigger) -> [AgentTrigger] {
        triggers.filter { $0.id != trigger.id }
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
    @State private var triggerTarget: TriggerFormTarget?
    @State private var pendingTriggerDelete: AgentTrigger?
    @State private var showDelete = false
    @State private var deleting = false

    /// Sheet item for the trigger form: `id` is the trigger's id, or "new".
    private struct TriggerFormTarget: Identifiable {
        let id: String
        let trigger: AgentTrigger?
    }

    /// The machines an agent can live on: online with a runnable agent (the
    /// composer's pool), plus the one it is bound to whatever its state.
    private var devices: [SteerDevice] {
        let candidates = allDevices.filter { $0.isOnline && $0.hasRunnableAgent }
        guard let bound = draft.deviceId,
              !candidates.contains(where: { $0.deviceId == bound }),
              let row = allDevices.first(where: { $0.deviceId == bound })
        else { return candidates }
        return [row] + candidates
    }

    private var title: String {
        agentId == nil ? AgentSettings.Copy.newAgent : (agent?.name ?? "")
    }

    private var canSubmit: Bool {
        guard isOwner, !draft.saving else { return false }
        return agentId == nil || !draft.patch.isEmpty
    }

    var body: some View {
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
                onOpenTrigger: { trigger in
                    triggerTarget = TriggerFormTarget(id: trigger?.id ?? "new", trigger: trigger)
                },
                onToggleTrigger: { trigger, enabled in
                    writeTriggers(draft.settingEnabled(trigger, enabled), verb: "save")
                },
                onDeleteTrigger: { pendingTriggerDelete = $0 },
                onDelete: isOwner && agentId != nil ? { showDelete = true } : nil
            )
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if isOwner {
                GlassSubmitButton(
                    agentId == nil ? "Create agent" : "Save changes",
                    enabled: canSubmit,
                    loading: draft.saving
                ) {
                    Task { await save() }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
            }
        }
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task(id: agentId) { await start() }
        .onDisappear { observation?.cancel() }
        // The synced row is the truth for the triggers once no write of ours
        // is in flight (the echo of the last one included).
        .onChange(of: agent?.triggers) { _, raw in
            guard seeded, !draft.triggerBusy, let raw else { return }
            draft.triggers = AgentTrigger.parseList(raw)
        }
        .sheet(item: $triggerTarget) { target in
            TriggerFormSheet(teamId: teamId, editing: target.trigger) { input in
                writeTriggers(draft.applying(input, editing: target.trigger), verb: "save")
            }
            .environment(\.accountId, accountId)
        }
        // EXP-1215: the app's own alert card, each on a zero-size node of its
        // own (two presentations on one node and SwiftUI drops the second).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $pendingTriggerDelete) { trigger in
                    GlassAlert(
                        prompt: Prompts.DeleteTrigger.copy(),
                        handlers: ["delete": { writeTriggers(draft.removing(trigger), verb: "delete") }]
                    )
                }
        }
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

    /// A new agent keeps its triggers locally until Create; an existing one
    /// writes the WHOLE array at once (optimistic, rolled back on refusal).
    private func writeTriggers(_ next: [AgentTrigger], verb: String) {
        guard let agentId else {
            draft.triggers = next
            return
        }
        guard !draft.triggerBusy else { return }
        let previous = draft.triggers
        draft.triggers = next
        draft.triggerBusy = true
        let inputs = draft.inputs(next)
        Task {
            do {
                try await deps.agentsApi.update(
                    accountId: accountId, id: agentId, patch: AgentPatch(triggers: inputs)
                )
            } catch {
                draft.triggers = previous
                toaster.error(
                    FeedbackStates.Errors.title(verb: verb, thing: "the trigger"),
                    description: error.trpcUserMessage
                )
            }
            draft.triggerBusy = false
        }
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
                try await deps.agentsApi.update(accountId: accountId, id: agentId, patch: patch)
                draft.snapshotBaseline()
            } else {
                try await deps.agentsApi.create(
                    accountId: accountId,
                    AgentCreateInput(
                        teamId: teamId,
                        name: draft.name.trimmingCharacters(in: .whitespacesAndNewlines),
                        icon: draft.icon,
                        color: draft.color,
                        instructions: draft.instructions.currentMarkdown(),
                        triggers: draft.inputs(draft.triggers),
                        deviceId: deviceId,
                        cli: draft.cli,
                        account: draft.account
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

/// The agent page's rows, top to bottom (fixture `fields`). Pure over the
/// draft + the device pool, so the capture seam draws it too.
struct AgentSettingsForm: View {
    @Bindable var draft: AgentSettingsDraft
    let devices: [SteerDevice]
    let canEdit: Bool
    var baseURL: URL? = nil
    var accountId: String = ""
    var httpClient: HTTPClient? = nil
    var onRetryInstructions: () -> Void = {}
    /// nil = add.
    var onOpenTrigger: (AgentTrigger?) -> Void = { _ in }
    var onToggleTrigger: (AgentTrigger, Bool) -> Void = { _, _ in }
    var onDeleteTrigger: (AgentTrigger) -> Void = { _ in }
    /// nil hides the Delete row (members, or an agent not created yet).
    var onDelete: (() -> Void)? = nil

    private var device: SteerDevice? {
        devices.first { $0.deviceId == draft.deviceId }
    }

    private func label(_ key: String) -> String {
        AgentSettings.fields.first { $0.key == key }?.label ?? ""
    }

    var body: some View {
        Form {
            Section {
                identityRow
                errorLine("identity")
                deviceRow
                errorLine("device")
            }
            .listRowBackground(glassFormRowFill)

            Section {
                accountRow
                scheduleRows
            }
            .listRowBackground(glassFormRowFill)

            Section {
                instructionsEditor
                errorLine("instructions")
            }
            .listRowBackground(glassFormRowFill)

            if let onDelete {
                Section {
                    Button(action: onDelete) {
                        Text(label("delete"))
                            .foregroundStyle(DesignTokens.Palette.destructive)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("agent-delete")
                }
                .listRowBackground(glassFormRowFill)
            }
        }
        // EXP-603: the page's own background shows through the grouped list;
        // rows carry the glass fill.
        .scrollContentBackground(.hidden)
        .listSectionSpacing(8)
        // EXP-594: white control tint — system blue is retired.
        .tint(DesignTokens.Palette.primary)
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

    /// The board form's ONE row: icon, colour, name (EXP-862, ×4).
    private var identityRow: some View {
        HStack(spacing: 8) {
            IconPicker(selection: $draft.icon, tint: Color(hex: draft.color))
            ColorSwatchPicker(selection: $draft.color)
            TextField(AgentSettings.Copy.namePlaceholder, text: $draft.name)
                .font(.subheadline)
                .accessibilityIdentifier("agent-name")
        }
        .disabled(!canEdit)
    }

    /// The shared `DevicePicker` — the composer's rows (glyph, name, owner).
    private var deviceRow: some View {
        DevicePicker(
            devices: devices.map(DevicePickerDevice.init),
            value: draft.deviceId,
            onChange: { draft.selectDevice($0, devices: devices) },
            trigger: {
                GlassPickerRowLabel(
                    label("device"),
                    value: device.map(LaunchVocabulary.deviceName) ?? "",
                    icon: device.map { DeviceIconDisplay.iconName(for: $0) }
                )
            }
        )
        .disabled(!canEdit)
        .accessibilityIdentifier("agent-device-row")
    }

    /// The shared `AccountPicker` over the picked machine's logins; a pick
    /// names the agent (`cli`) too. EXP-1306: the value reads like the
    /// composer chip (the agent's brand mark + the login's email) and the row
    /// like the Device row beside it. A lone login is nothing to pick: the
    /// same line outside the picker (which disables a lone login, and a
    /// disabled Form row dims). No login reported = `Last used`, muted.
    /// EXP-1307: every form ends in the chevron, like the rows beside it (web).
    @ViewBuilder
    private var accountRow: some View {
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
                trigger: { accountRowLabel(current) }
            )
            .disabled(!canEdit)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("agent-account-row")
        } else {
            accountRowLabel(current)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("agent-account-row")
        }
    }

    private func accountRowLabel(_ current: AccountOption?) -> some View {
        HStack(spacing: 8) {
            Text(label("account"))
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

    /// One row per trigger (its summary + switch); none = the agent only
    /// waits for messages.
    @ViewBuilder
    private var scheduleRows: some View {
        if draft.triggers.isEmpty {
            Button { onOpenTrigger(nil) } label: {
                GlassPickerRowLabel(label("triggers"), value: AgentSettings.Copy.noSchedule)
            }
            .buttonStyle(.plain)
            .disabled(!canEdit)
            .accessibilityIdentifier("agent-schedule-row")
        } else {
            ForEach(draft.triggers) { trigger in
                triggerRow(trigger)
            }
            if canEdit, draft.triggers.count < DomainContract.agentTriggerMaxPerAgent {
                Button { onOpenTrigger(nil) } label: {
                    HStack(spacing: 10) {
                        AppIcon(AppIcons.uiAdd, size: AppIcon.Size.small)
                        Text(AgentSettings.Copy.addTrigger)
                        Spacer(minLength: 0)
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(draft.triggerBusy)
                .accessibilityIdentifier("add-trigger")
            }
        }
    }

    /// The row's body opens the editor; the switch sits BESIDE it (a control
    /// nested in a button's label has its tap swallowed). Swipe deletes.
    private func triggerRow(_ trigger: AgentTrigger) -> some View {
        HStack(spacing: 12) {
            Button { onOpenTrigger(trigger) } label: {
                HStack(spacing: 10) {
                    AppIcon(TriggerGlyph.icon(for: trigger.when), size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Text(AgentSettings.triggerSummary(trigger))
                        .foregroundStyle(.white.opacity(TextOpacity.primary))
                        .lineLimit(1)
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!canEdit)

            GlassToggleRow(nil, isOn: Binding(
                get: { trigger.enabled },
                set: { onToggleTrigger(trigger, $0) }
            ))
            .disabled(!canEdit || draft.triggerBusy)
            .accessibilityLabel(AgentSettings.triggerSummary(trigger))
            .accessibilityIdentifier("trigger-enabled")
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            if canEdit {
                Button("Delete", role: .destructive) { onDeleteTrigger(trigger) }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("trigger-row")
    }

    @ViewBuilder
    private var instructionsEditor: some View {
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
                    onRetryInstructions()
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
