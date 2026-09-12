import ExpCore
import ExpUI
import SwiftUI

/// The Actions surface (EXP-253): the active team's action prompts, each with
/// a Run affordance. EXP-825: Run, "New action" (EXP-431, in the web-parity
/// "Actions" section header since EXP-574) and a suggestion's tap are all
/// NAVIGATION into the Agent page composer, seeded with the action (or the
/// Create action builtin plus the suggestion's text and icon) — the
/// dedicated run and create sheets are gone. EXP-694 added the row menu's
/// "Edit" (the `EditActionSheet`, read-only for non-owners) — editing is no
/// longer web/desktop-only.
struct ActionsListView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(TeamState.self) private var teamState
    @State private var viewModel: ActionsViewModel?
    @State private var steerEnabled = false
    /// EXP-694: the action being edited (nil = closed). Owners edit, everyone
    /// else reads.
    @State private var editTarget: ActionDto?
    /// The automated-run rows' tap target (the SettingsView pendingTeam idiom).
    @State private var sessionTarget: StartedRunWatcher.StartedSession?
    /// EXP-583: the automation form sheet's target (nil = closed; a nil
    /// `automation` inside = create).
    @State private var formTarget: AutomationFormTarget?
    /// Owner-only delete, confirmed first (destructive native actions do).
    @State private var pendingDelete: AutomationDto?
    /// EXP-530: Actions · Automations · Suggestions (the MyWorkView segment
    /// pattern — the choice survives relaunch via AppStorage).
    @AppStorage("actionsSegment") private var segmentRaw = Segment.actions.rawValue

    /// Sheet item for the automation form: `id` is the automation's id, or
    /// "new" for a creation.
    private struct AutomationFormTarget: Identifiable {
        let id: String
        let automation: AutomationDto?
    }

    private enum Segment: String, CaseIterable {
        case actions
        case automations
        case suggestions

        var label: String {
            switch self {
            case .actions: return "Actions"
            case .automations: return "Automations"
            case .suggestions: return "Suggestions"
            }
        }
    }

    private var segment: Segment {
        Segment(rawValue: segmentRaw) ?? .actions
    }

    var body: some View {
        ZStack {
            AppBackground()

            // The action editor hangs off its own zero-size node: this ZStack
            // owns the automation sheet, and stacking presentations on one
            // node is where SwiftUI starts dropping them.
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                // EXP-694: the action editor. Non-owners open it read-only —
                // the server refuses their write anyway.
                .sheet(item: $editTarget) { action in
                    EditActionSheet(
                        action: action,
                        canEdit: viewModel?.permissions.isOwner == true
                    )
                    .environment(\.accountId, accountId)
                }

            if let vm = viewModel {
                content(vm)
            }
        }
        .navigationTitle("Actions")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        // Reload when the active team changes (and on first mount).
        .task(id: teamState.activeTeam?.id) {
            ensureViewModel()
            if let teamId = teamState.activeTeam?.id {
                await viewModel?.load(teamId: teamId)
            }
        }
        .onAppear {
            ensureViewModel()
        }
        // EXP-583: the owner-only automation form, in create or edit mode.
        .sheet(item: $formTarget) { target in
            if let teamId = teamState.activeTeam?.id, let vm = viewModel {
                AutomationFormSheet(
                    teamId: teamId,
                    actions: vm.actions,
                    devices: vm.allDevices.filter(\.canRunAutomations),
                    editing: target.automation,
                    onSubmit: { actionId, deviceId, trigger, launch in
                        vm.saveAutomation(
                            editing: target.automation,
                            teamId: teamId,
                            actionId: actionId,
                            deviceId: deviceId,
                            trigger: trigger,
                            launch: launch
                        )
                    }
                )
                .environment(\.accountId, accountId)
            }
        }
        .alert(
            "Delete automation?",
            isPresented: Binding(
                get: { pendingDelete != nil },
                set: { if !$0 { pendingDelete = nil } }
            ),
            presenting: pendingDelete
        ) { automation in
            Button("Delete", role: .destructive) {
                viewModel?.deleteAutomation(automation)
                pendingDelete = nil
            }
            Button("Cancel", role: .cancel) { pendingDelete = nil }
        } message: { automation in
            Text(AutomationCopy.deleteBody(
                actionName: viewModel?.actions.first { $0.id == automation.actionId }?.name
            ))
        }
        .navigationDestination(item: $sessionTarget) { target in
            AgentSessionRouteView(sessionId: target.sessionId)
                .environment(\.accountId, accountId)
        }
    }

    private func ensureViewModel() {
        if viewModel == nil {
            viewModel = ActionsViewModel(
                accountId: accountId,
                db: deps.db,
                automationsApi: deps.automationsApi,
                auth: deps.auth
            )
        }
    }

    /// EXP-825: every launch here is a push into the Agent page composer.
    private func openComposer(_ seed: AgentComposerSeed) {
        guard teamState.activeTeam != nil else { return }
        pushRoute(.agent(accountId: accountId, seed: seed))
    }

    // MARK: - Content

    /// EXP-530: the segmented triptych — Actions (run list), Automations
    /// (triggered actions + recent automated runs), Suggestions (seed ideas).
    @ViewBuilder
    private func content(_ vm: ActionsViewModel) -> some View {
        VStack(spacing: 0) {
            GlassSegmentedControl(
                options: Segment.allCases,
                selection: segment,
                label: { $0.label },
                identifier: { "actions-segment-\($0.rawValue)" },
                onSelect: { segmentRaw = $0.rawValue }
            )
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            switch segment {
            case .actions:
                actionsContent(vm)
            case .automations:
                automationsContent(vm)
            case .suggestions:
                suggestionsContent
            }
        }
    }

    @ViewBuilder
    private func actionsContent(_ vm: ActionsViewModel) -> some View {
        if vm.actions.isEmpty {
            Spacer()
            if vm.isLoading {
                ProgressView().tint(.white)
            } else if let error = vm.loadError {
                errorState(error)
            } else {
                emptyState
            }
            Spacer()
        } else {
            ScrollView {
                // EXP-818: a filled group BAND over flat rows — the list reads
                // as a table instead of a stack of cards (web `ListRow`,
                // desktop `surface::flat_row`).
                LazyVStack(alignment: .leading, spacing: 0) {
                    // EXP-574 (web parity): the "Actions" band with the
                    // "New action" entry (EXP-431) as its trailing control.
                    GlassSectionBand("Actions") {
                        newActionButton
                    }
                    ForEach(vm.actions) { actionRow($0) }
                }
                .padding()
            }
            // Actions is a tab of its own since EXP-686 — reserve the
            // floating bar's clearance (EXP-36).
            .tabBarBottomInset()
        }
    }

    /// EXP-431: creation left the list ("Create action" no longer poses as a
    /// row); EXP-825: it is the composer with the Create action builtin
    /// picked — describe it, and the creator run writes it.
    private var newActionButton: some View {
        GlassPill(
            "New action",
            icon: AppIcons.actionCreate,
            mode: .action {
                openComposer(AgentComposerSeed(actionId: DomainContract.builtinCreateActionId))
            },
            enabled: teamState.activeTeam != nil
        )
        .accessibilityLabel("New action")
    }

    // MARK: - Automations (EXP-583)

    @ViewBuilder
    private func automationsContent(_ vm: ActionsViewModel) -> some View {
        if vm.automations.isEmpty, vm.automationRuns.isEmpty {
            Spacer()
            emptyAutomationsState(vm)
            Spacer()
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    // EXP-574 (web parity): section bands (EXP-818).
                    GlassSectionBand("Automations") {
                        if vm.permissions.isOwner {
                            newAutomationButton(vm)
                        }
                    }
                    if let error = vm.automationError {
                        Text(error)
                            .font(.caption2)
                            .foregroundStyle(DesignTokens.Semantic.red)
                            .padding(.horizontal, 4)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if vm.automations.isEmpty {
                        emptyAutomationsState(vm)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 16)
                    } else {
                        ForEach(vm.automations) { automationRow($0, vm: vm) }
                    }
                    if !vm.automationRuns.isEmpty {
                        recentAutomatedRuns(vm)
                    }
                }
                .padding()
            }
            .tabBarBottomInset()
        }
    }

    /// EXP-637: the automated runs list, its own node so the Resume confirm
    /// doesn't stack onto a node that already presents something.
    @ViewBuilder
    private func recentAutomatedRuns(_ vm: ActionsViewModel) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand("Recent automated runs")
                .padding(.top, 12)
            ForEach(vm.automationRuns) { automatedRunRow($0, vm: vm) }
        }
    }

    private func emptyAutomationsState(_ vm: ActionsViewModel) -> some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.actionAutomation, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No automations yet.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
            if vm.permissions.isOwner {
                newAutomationButton(vm)
            }
        }
        .padding(.horizontal, 40)
    }

    /// Owner-only entry to the automation form (EXP-583). Steering off means
    /// no machine can ever run one, so the button stays hidden then.
    @ViewBuilder
    private func newAutomationButton(_ vm: ActionsViewModel) -> some View {
        if steerEnabled {
            GlassPill(
                "New automation",
                icon: AppIcons.uiAdd,
                mode: .action { formTarget = AutomationFormTarget(id: "new", automation: nil) },
                enabled: teamState.activeTeam != nil
            )
            .accessibilityLabel("New automation")
        }
    }

    /// The trigger sentence as the row prints it. A schedule fires on the
    /// BOUND MACHINE's wall clock, so the recurrence carries the caveat the
    /// row used to hang off an absolute next-run date (EXP-812).
    private func triggerCaption(_ trigger: AutomationTrigger) -> String {
        if case .schedule = trigger {
            return "\(AutomationTriggerDisplay.summary(trigger)) (device time)"
        }
        return AutomationTriggerDisplay.summary(trigger)
    }

    /// One automation: the target action's glyph + name, the trigger
    /// sentence, the bound machine (label + online dot off the synced devices
    /// rows; raw id when the row isn't visible to us), the pinned agent/model
    /// when it overrides the machine's defaults, the last run, and the
    /// owner-only enabled toggle. A schedule's sentence carries "(device
    /// time)" because the machine fires on its own clock; the row prints no
    /// absolute next-run date (EXP-812 — the calendar moved it under every
    /// screenshot, and the recurrence says the same thing).
    private func automationRow(_ automation: AutomationDto, vm: ActionsViewModel) -> some View {
        let action = vm.actions.first { $0.id == automation.actionId }
        let trigger = automation.parsedTrigger
        let boundDevice = vm.allDevices.first { $0.deviceId == automation.deviceId }
        let busy = vm.automationBusyId == automation.id
        // EXP-698: the row's trailing cluster (toggle + menu) is CENTRED —
        // an automation body runs to five lines, and a top-pinned toggle left
        // it floating beside the first one instead of lining up with the play
        // and "…" controls every other row in this list wears. The glyph and
        // the body keep their own top alignment inside the leading group.
        return HStack(spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                AppIcon(action?.icon ?? AppIcons.actionDefault, size: AppIcon.Size.medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                VStack(alignment: .leading, spacing: 3) {
                    Text(action?.name ?? "Deleted action")
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    if let trigger {
                        Text(triggerCaption(trigger))
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                    HStack(spacing: 5) {
                        Circle()
                            .fill(boundDevice?.isOnline == true
                                ? DesignTokens.Semantic.green
                                : Color.white.opacity(0.25))
                            .frame(width: 6, height: 6)
                        Text(deviceLabel(boundDevice, deviceId: automation.deviceId))
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                    if let launch = launchCaption(automation) {
                        Text(launch)
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                    if let last = vm.lastRunByAutomation[automation.id] {
                        let time = relativeDate(last.startedAt)
                        if !time.isEmpty {
                            Text("Last run \(time)")
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                }

                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            // Owner-only (the automations router is owner-gated server-side).
            Toggle("", isOn: Binding(
                get: { automation.enabled },
                set: { vm.setAutomationEnabled(automation, enabled: $0) }
            ))
            .labelsHidden()
            .fixedSize()
            .disabled(!vm.permissions.isOwner || busy)
            .accessibilityLabel("Automation enabled")

            // EXP-603: edit/delete used to hide behind a long press. Same
            // owner gate, now a visible affordance.
            if vm.permissions.isOwner {
                GlassMenu {
                    GlassMenuItem("Edit", icon: AppIcons.uiEdit) {
                        formTarget = AutomationFormTarget(id: automation.id, automation: automation)
                    }
                    GlassMenuItem("Delete", icon: AppIcons.uiDelete, destructive: true) {
                        pendingDelete = automation
                    }
                } label: {
                    CircleIconLabel(AppIcons.uiMore)
                }
                .accessibilityLabel("Automation actions")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
        .accessibilityIdentifier("automation-row")
    }

    /// "Claude Code · Opus · High" — only what the automation PINS; an unset
    /// field means the machine's own launch default, which is not ours to
    /// name here.
    private func launchCaption(_ automation: AutomationDto) -> String? {
        var parts: [String] = []
        if let agent = automation.agent, !agent.isEmpty {
            parts.append(LaunchVocabulary.agentLabel(agent))
        }
        if let model = automation.model, !model.isEmpty {
            parts.append(LaunchVocabulary.modelLabel(model))
        }
        if let effort = automation.effort, !effort.isEmpty {
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

    /// One automation-started coding_sessions row (started_reason non-null):
    /// action-name snapshot, state, relative time. No "Automated" badge
    /// (EXP-643) — the section header already says so.
    ///
    /// EXP-773: a plain link. The close-out summary and Resume moved to the
    /// top of the run's own session view, so live and finished rows behave
    /// identically: a tap opens that session.
    private func automatedRunRow(_ session: CodingSessionEntity, vm: ActionsViewModel) -> some View {
        let ended = session.status == DomainContract.codingSessionStatusEnded
        return EndedRunRow(
            title: session.actionName ?? "Action run",
            byline: runByline(session, ended: ended),
            isLive: !ended,
            onOpen: { sessionTarget = .init(sessionId: session.id) }
        )
        .accessibilityIdentifier("automated-run-row")
    }

    /// "ended 5m ago" once the run finished, "started 5m ago" while it is
    /// still going — the automated runs list has no machine column to add.
    private func runByline(_ session: CodingSessionEntity, ended: Bool) -> String {
        if ended {
            let time = relativeDate(session.endedAt ?? session.startedAt)
            return time.isEmpty ? "" : "ended \(time)"
        }
        let time = relativeDate(session.startedAt)
        return time.isEmpty ? "" : "started \(time)"
    }

    private func relativeDate(_ s: String) -> String {
        guard let date = WireTimestamps.parse(s) else { return "" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }

    // MARK: - Suggestions (EXP-530)

    private var suggestionsContent: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 8) {
                ForEach(ActionSuggestion.seeds) { suggestionCard($0) }
            }
            .padding()
        }
        .tabBarBottomInset()
    }

    /// EXP-694: the whole card IS the affordance — the "Use" pill was the only
    /// text button sitting where every sibling list puts a glyph, and a
    /// suggestion row has nothing else to tap.
    private func suggestionCard(_ suggestion: ActionSuggestion) -> some View {
        Button {
            useSuggestion(suggestion)
        } label: {
            HStack(spacing: 12) {
                AppIcon(suggestion.icon, size: AppIcon.Size.medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                VStack(alignment: .leading, spacing: 3) {
                    Text(suggestion.title)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    // EXP-583: what the tap will set up.
                    GlassPill(suggestion.automation == nil ? "Action" : "Automation")
                    Text(suggestion.description)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(3)
                }

                Spacer(minLength: 0)

                AppIcon(AppIcons.uiChevronRight, size: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 12)
            .flatRow()
            .contentShape(Rectangle())
        }
        // `.plain` is what every tappable glass row in the app wears (the
        // ended-run rows, the session rows) — the tap feedback is the row's
        // own highlight, never a blue tint.
        .buttonStyle(.plain)
        .accessibilityIdentifier("suggestion-row")
    }

    /// Tapping a suggestion opens the composer on the Create action builtin
    /// with the suggestion's description as the request and its icon picked.
    /// EXP-583: an "Action + automation" seed appends the machine-readable
    /// trigger block the creator agent copies into
    /// `exponential_automations_create` (byte-identical across the four
    /// clients — `AutomationNote.format`), bound to the caller's default
    /// automation-capable machine, else the first one (offline included: a
    /// sleeping box still owns the binding). No such machine = no block.
    private func useSuggestion(_ suggestion: ActionSuggestion) {
        var text = suggestion.description
        if let trigger = suggestion.automation, let device = automationDevice {
            text += AutomationNote.format(AutomationSpec(trigger: trigger, deviceId: device.deviceId))
        }
        openComposer(AgentComposerSeed(
            actionId: DomainContract.builtinCreateActionId,
            text: text,
            icon: suggestion.icon
        ))
    }

    /// The automation's runner (web `automationDevices` + `defaultDeviceId`).
    private var automationDevice: SteerDevice? {
        let candidates = (viewModel?.allDevices ?? []).filter(\.canRunAutomations)
        return candidates.first(where: \.isDefaultDevice) ?? candidates.first
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.actionDefault, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No actions yet")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Actions are reusable prompts your team runs on a desktop. Team owners create them on the web or desktop app.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
    }

    private func errorState(_ message: String) -> some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.uiWarning, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text(message)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
    }

    private func automationCount(_ action: ActionDto) -> Int {
        viewModel?.automations.filter { $0.actionId == action.id }.count ?? 0
    }

    private func actionRow(_ action: ActionDto) -> some View {
        HStack(spacing: 12) {
            // The builtin "Create action" row (EXP-257) wears the create
            // affordance; real actions keep the bolt.
            // EXP-273: the action's own curated glyph (the builtins set one too),
            // falling back to the generic action mark.
            AppIcon(action.icon ?? AppIcons.actionDefault, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            VStack(alignment: .leading, spacing: 3) {
                // EXP-697: no repo glyph beside the name — the row says what
                // the action is, not where it runs.
                Text(action.name)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                if let description = action.description, !description.isEmpty {
                    Text(description)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(2)
                }
                // EXP-583: automations are their own rows on their own tab —
                // an action only says HOW MANY point at it.
                let count = automationCount(action)
                if count > 0 {
                    HStack(spacing: 4) {
                        AppIcon(AppIcons.actionAutomation, size: 10)
                        Text("\(count) \(count == 1 ? "automation" : "automations")")
                            .lineLimit(1)
                    }
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }

            Spacer(minLength: 0)

            // EXP-615: the play glyph, not a "Run" pill — the same affordance
            // web and desktop wear on their action cards. EXP-825: it pushes
            // the composer with this action picked.
            CircleIconButton(AppIcons.actionRun, accessibilityLabel: "Run") {
                openComposer(AgentComposerSeed(actionId: action.id))
            }

            // EXP-694: editing reached mobile. The builtins have no row to
            // edit (the server refuses their id), so they wear no menu;
            // non-owners get the sheet read-only rather than no entry at all.
            if !action.isBuiltin {
                GlassMenu {
                    // EXP-858: no Pin row — the phone has no sidebar.
                    GlassMenuItem("Edit", icon: AppIcons.uiEdit) {
                        editTarget = action
                    }
                } label: {
                    CircleIconLabel(AppIcons.uiMore)
                }
                .accessibilityLabel("Action actions")
                .accessibilityIdentifier("action-menu")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
        .accessibilityIdentifier("action-row")
    }
}
