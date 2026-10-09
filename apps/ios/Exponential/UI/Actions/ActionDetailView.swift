import ExpCore
import ExpUI
import SwiftUI

/// The action page's three parts, in order (titled exactly so ×4). On a phone
/// they are TABS on a pager; wide layouts (desktop, web ≥ md) stack them as
/// sections of one page.
enum ActionPageTab: String, CaseIterable, Hashable {
    case prompt
    case triggers
    case runs

    var label: String {
        switch self {
        case .prompt: "Prompt"
        case .triggers: "Triggers"
        case .runs: "Runs"
        }
    }
}

/// SLOP-2: the ACTION PAGE — pushed from an Actions row (and from an `action`
/// entity ref). It replaced the edit sheet and the Automations segment: an
/// action is its prompt, the triggers that start it on their own and the runs
/// it produced.
///
/// The header is the Work screen's: the nav bar (back · the action's icon +
/// name · Run) with the tab strip in the same band directly under it, and the
/// three tabs as PAGES that follow the finger (`WorkFaceTabs` / `FacePager`,
/// EXP-1150/1152).
///
///   Prompt    today's action editor (`ActionPromptTab`), read-only for
///             non-owners
///   Triggers  one row per readable trigger, in stored order; owners add,
///             edit, pause and delete them
///   Runs      every run of this action, newest first, titled by what started
///             it. Opening one and going Back returns here.
///
/// Nothing here RUNS a trigger — the device it is bound to does.
struct ActionDetailView: View {
    let actionId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var tab: ActionPageTab
    @State private var viewModel: ActionDetailViewModel?
    @State private var steerEnabled = false
    /// The trigger form sheet's target (nil = closed; a nil `trigger` inside
    /// = add).
    @State private var formTarget: TriggerFormTarget?
    /// Owner-only delete, confirmed first (destructive native actions do).
    @State private var pendingDelete: ActionTrigger?

    /// Sheet item for the trigger form: `id` is the trigger's id, or
    /// `ActionDetailViewModel.newTriggerKey` for an add.
    private struct TriggerFormTarget: Identifiable {
        let id: String
        let trigger: ActionTrigger?
    }

    init(actionId: String, initialTab: ActionPageTab = .prompt) {
        self.actionId = actionId
        _tab = State(initialValue: initialTab)
    }

    private var action: ActionDto? { viewModel?.action }
    private var isOwner: Bool { viewModel?.permissions.isOwner == true }

    /// The trigger form is the ONE trigger editor — add and edit both open
    /// it — so every path into it shares its gate: an owner, and steering on
    /// (off means no machine can ever run a trigger).
    private var canEditTriggers: Bool { isOwner && steerEnabled }

    var body: some View {
        ZStack {
            AppBackground()

            // The delete confirm hangs off its own zero-size node: this ZStack
            // owns the trigger sheet, and stacking presentations on one node
            // is where SwiftUI starts dropping them.
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $pendingDelete) { trigger in
                    GlassAlert(
                        prompt: Prompts.DeleteTrigger.copy(),
                        handlers: ["delete": { viewModel?.delete(trigger) }]
                    )
                }

            if let vm = viewModel, let action = vm.action {
                pager(action, vm: vm)
                    .workHeaderBand { tabs }
            } else if viewModel?.resolved == true {
                unavailableState
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        // The header band draws the bar's material over the title row AND the
        // tabs, with its own hairline (the Work screen's header, EXP-1150).
        .toolbarBackground(action == nil ? .visible : .hidden, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) { title }
            ToolbarItem(placement: .topBarTrailing) { runButton }
        }
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        .task(id: actionId) {
            ensureViewModel()
            viewModel?.load(actionId: actionId)
        }
        .onAppear { ensureViewModel() }
        .sheet(item: $formTarget) { target in
            if let vm = viewModel, let action = vm.action {
                TriggerFormSheet(
                    teamId: action.teamId,
                    devices: vm.allDevices.filter(\.canRunTriggers),
                    editing: target.trigger,
                    onSubmit: { input in vm.save(input, editing: target.trigger) }
                )
                .environment(\.accountId, accountId)
            }
        }
    }

    private func ensureViewModel() {
        if viewModel == nil {
            viewModel = ActionDetailViewModel(
                accountId: accountId,
                db: deps.db,
                actionsApi: deps.actionsApi,
                auth: deps.auth
            )
        }
    }

    // MARK: - Header

    /// The action's icon + name — the bar's title, identical on every tab.
    @ViewBuilder
    private var title: some View {
        if let action {
            HStack(spacing: 6) {
                AppIcon(ActionIconDisplay.iconName(for: action.icon), size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(action.name)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("action-title")
        }
    }

    /// The existing run flow (EXP-825): the Agent page composer with this
    /// action picked. Bare content — the bar owns the capsule (EXP-698 r4).
    @ViewBuilder
    private var runButton: some View {
        if let action {
            Button {
                pushRoute(.agent(
                    accountId: accountId,
                    seed: AgentComposerSeed(actionId: action.id, teamId: action.teamId)
                ))
            } label: {
                AppIcon(AppIcons.actionRun, size: AppIcon.Size.medium, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: GlassTokens.controlSize, height: GlassTokens.controlSize)
            }
            .accessibilityLabel("Run")
            .accessibilityIdentifier("action-run")
        }
    }

    private var tabs: some View {
        GlassSegmentedControl(
            options: ActionPageTab.allCases,
            selection: tab,
            label: { $0.label },
            identifier: { "action-tab-\($0.rawValue)" },
            style: .capsule,
            onSelect: selectTab
        )
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("action-tabs")
    }

    /// A tab tap slides the pages; a drag onto a page writes `tab` back itself.
    private func selectTab(_ next: ActionPageTab) {
        guard next != tab else { return }
        dismissKeyboard()
        withAnimation { tab = next }
    }

    private func dismissKeyboard() {
        UIApplication.shared.sendAction(
            #selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil
        )
    }

    private func pager(_ action: ActionDto, vm: ActionDetailViewModel) -> some View {
        FacePager(pages: ActionPageTab.allCases, selection: $tab) { page in
            switch page {
            case .prompt:
                ActionPromptTab(action: action, canEdit: isOwner)
                    .environment(\.accountId, accountId)
                    // One editor per action: a different row starts clean.
                    .id(action.id)
            case .triggers:
                triggersPage(action, vm: vm)
            case .runs:
                runsPage(vm)
            }
        }
        .onChange(of: tab) { dismissKeyboard() }
    }

    private var unavailableState: some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.actionDefault, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("This action isn't available")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("It may still be syncing, or it was deleted.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
    }

    // MARK: - Triggers

    private func triggersPage(_ action: ActionDto, vm: ActionDetailViewModel) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                // The tab already says "Triggers", so the page draws no band
                // of its own — only the section's action, at its end.
                if canEditTriggers {
                    HStack {
                        Spacer(minLength: 0)
                        addTriggerButton(action, vm: vm)
                    }
                    .padding(.bottom, 8)
                }
                if let error = vm.triggerError {
                    Text(error)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .padding(.horizontal, 4)
                        .padding(.bottom, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                if vm.triggers.isEmpty {
                    emptyNote(TriggerCopy.empty)
                } else {
                    ForEach(vm.triggers) { triggerRow($0, action: action, vm: vm) }
                }
            }
            .padding()
        }
        .accessibilityIdentifier("action-triggers-tab")
    }

    private func addTriggerButton(_ action: ActionDto, vm: ActionDetailViewModel) -> some View {
        GlassPill(
            "Add trigger",
            icon: AppIcons.uiAdd,
            mode: .action {
                formTarget = TriggerFormTarget(
                    id: ActionDetailViewModel.newTriggerKey, trigger: nil
                )
            },
            // The server caps an action's triggers; one write at a time.
            enabled: vm.triggers.count < DomainContract.actionTriggerMaxPerAction
                && vm.busyTriggerId == nil
        )
        .accessibilityLabel("Add trigger")
        .accessibilityIdentifier("add-trigger")
    }

    /// One trigger: its kind's glyph, the trigger sentence (a schedule's
    /// carries "(device time)" — the machine fires on its own clock, EXP-812),
    /// the bound machine (live dot + label off the synced devices rows; the
    /// raw id when the row isn't visible to us) with the pinned agent/model
    /// when it overrides the machine's defaults, then the enabled switch and,
    /// for owners, the row menu. No "last run" here — Runs shows it.
    private func triggerRow(
        _ trigger: ActionTrigger, action: ActionDto, vm: ActionDetailViewModel
    ) -> some View {
        // A triggered run has nobody to fill a required input, so the server
        // refuses to ENABLE such a trigger — but one that is already on must
        // stay switchable OFF.
        let locked = action.hasRequiredInput && !trigger.enabled
        let busy = vm.busyTriggerId == trigger.id
        // EXP-698: the trailing cluster (toggle + menu) is CENTRED on the
        // row; the glyph and the body keep their own top alignment.
        return HStack(spacing: 12) {
            if canEditTriggers {
                // The row's body opens the form. Its own Button BESIDE the
                // toggle and the menu — a control nested in a button's label
                // has its tap swallowed.
                Button {
                    formTarget = TriggerFormTarget(id: trigger.id, trigger: trigger)
                } label: {
                    triggerBody(trigger, locked: locked, vm: vm)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("trigger-open")
            } else {
                triggerBody(trigger, locked: locked, vm: vm)
            }

            // Shown to everyone, switchable by owners (the write is
            // owner-gated server-side).
            GlassToggleRow(nil, isOn: Binding(
                get: { trigger.enabled },
                set: { vm.setEnabled(trigger, enabled: $0) }
            ))
            .disabled(!isOwner || busy || locked)
            .accessibilityLabel("Enabled: \(AutomationTriggerDisplay.rowSentence(trigger.when))")
            .accessibilityIdentifier("trigger-enabled")

            if isOwner {
                GlassMenu {
                    // No trigger editor to open with steering off.
                    if canEditTriggers {
                        GlassMenuItem("Edit", icon: AppIcons.uiEdit) {
                            formTarget = TriggerFormTarget(id: trigger.id, trigger: trigger)
                        }
                    }
                    GlassMenuItem("Delete", icon: AppIcons.uiDelete, destructive: true) {
                        pendingDelete = trigger
                    }
                } label: {
                    GhostIconLabel(AppIcons.uiMore)
                }
                .accessibilityLabel("Trigger menu")
                .accessibilityIdentifier("trigger-menu")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("trigger-row")
    }

    private func triggerBody(
        _ trigger: ActionTrigger, locked: Bool, vm: ActionDetailViewModel
    ) -> some View {
        let boundDevice = vm.allDevices.first { $0.deviceId == trigger.deviceId }
        return HStack(alignment: .top, spacing: 12) {
            AppIcon(TriggerGlyph.icon(for: trigger.when), size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            VStack(alignment: .leading, spacing: 3) {
                Text(AutomationTriggerDisplay.rowSentence(trigger.when))
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .multilineTextAlignment(.leading)
                HStack(spacing: 8) {
                    HStack(spacing: 5) {
                        Circle()
                            .fill(boundDevice?.isOnline == true
                                ? DesignTokens.Semantic.green
                                : Color.white.opacity(0.25))
                            .frame(width: 6, height: 6)
                        Text(deviceLabel(boundDevice, deviceId: trigger.deviceId))
                            .lineLimit(1)
                    }
                    if let launch = launchCaption(trigger) {
                        Text(launch)
                            .lineLimit(1)
                    }
                }
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                if locked {
                    Text(TriggerCopy.requiredInputsHint)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .multilineTextAlignment(.leading)
                }
            }

            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
    }

    /// "Claude Code · Opus · High" — only what the trigger PINS; an unset
    /// field means the machine's own launch default, which is not ours to
    /// name here.
    private func launchCaption(_ trigger: ActionTrigger) -> String? {
        var parts: [String] = []
        if let agent = trigger.agent, !agent.isEmpty {
            parts.append(LaunchVocabulary.agentLabel(agent))
        }
        if let model = trigger.model, !model.isEmpty {
            parts.append(LaunchVocabulary.modelLabel(model))
        }
        if let effort = trigger.effort, !effort.isEmpty {
            parts.append(LaunchVocabulary.effortLabel(effort))
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func deviceLabel(_ device: SteerDevice?, deviceId: String) -> String {
        guard let device else { return deviceId }
        let name = device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
        guard let owner = device.owner else { return name }
        return "\(name) — \(owner.name)"
    }

    // MARK: - Runs

    private func runsPage(_ vm: ActionDetailViewModel) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if vm.runs.isEmpty {
                    emptyNote("No runs yet.")
                } else {
                    runsTree(vm)
                }
            }
            .padding()
        }
        .accessibilityIdentifier("action-runs-tab")
    }

    /// EXP-1248: every run of this action as THE `SessionRow` (big), nested
    /// like the Agent page's lists (`SessionTree`: a resume succession is one
    /// row, a child run under its parent, with its connector), gapless, no
    /// fold. Every row here ran this action, so the title says what STARTED
    /// the run (`ActionRunTitle`) instead of repeating the action's name. A
    /// tap PUSHES that run's session view (EXP-773), so Back returns here.
    @ViewBuilder
    private func runsTree(_ vm: ActionDetailViewModel) -> some View {
        let rows = SessionTree.visibleRows(SessionTree.sessionTree(vm.runs))
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        ForEach(Array(rows.enumerated()), id: \.element.key) { index, entry in
            runRow(entry.node.session, guide: guides[index], vm: vm)
        }
    }

    private func runRow(
        _ session: CodingSessionEntity, guide: TreeGuide, vm: ActionDetailViewModel
    ) -> some View {
        let route = AppRoute.agentSession(accountId: accountId, sessionId: session.id)
        return Button { pushRoute(route) } label: {
            SessionListRow(
                session: session,
                identifier: nil,
                title: ActionRunTitle.of(startedReason: session.startedReason),
                prState: session.prState,
                device: runDevice(session, vm: vm),
                deviceIcon: SessionListRow.deviceIcon(session, devices: vm.allDevices),
                guide: guide
            )
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("action-run-row")
    }

    /// The run's host as it presents: the registry row's CURRENT label when
    /// one matches (a rename never rewrites the session snapshot), else the
    /// snapshot. Presence stays UNKNOWN — the registry here is a one-shot
    /// read, and a stale snapshot must never claim a live run is paused.
    private func runDevice(
        _ session: CodingSessionEntity, vm: ActionDetailViewModel
    ) -> SessionDevicePresentation {
        let live = session.deviceId.flatMap { id in
            vm.allDevices.first { $0.deviceId == id }?.deviceLabel
        }
        let label = (live?.isEmpty == false) ? live : session.deviceLabel
        return SessionDevicePresentation(label: label, online: nil)
    }

    private func emptyNote(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 4)
            .padding(.vertical, 12)
    }
}
