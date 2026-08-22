import ExpCore
import ExpUI
import SwiftUI

/// The Actions surface (EXP-253, view + run only — no manual edit on mobile):
/// the active team's action prompts, each with a Run affordance that
/// remote-starts the action on one of the caller's actions-capable desktops.
/// The "New action" button (EXP-431, in the web-parity "Actions" section
/// header since EXP-574) opens the same sheet in its dedicated create mode —
/// the "Create action" builtin left the list.
/// After a successful send the screen waits for the desktop's synced
/// coding_sessions row and jumps into the existing live steer screen once.
struct ActionsListView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @State private var viewModel: ActionsViewModel?
    @State private var devices: [SteerDevice]?
    @State private var steerEnabled = false
    /// The action the run sheet was opened for (non-nil = sheet up).
    @State private var runTarget: ActionDto?
    /// EXP-431: the toolbar's "New action" entry opens the SAME sheet locked
    /// to the create builtin, presented as a creation flow.
    @State private var createMode = false
    /// Consumed-once navigation target (the SettingsView pendingTeam idiom).
    @State private var sessionTarget: StartedRunWatcher.StartedSession?
    /// EXP-530: "Use suggestion" prefill for the create sheet (description +
    /// icon input values); cleared on dismiss.
    @State private var suggestionPrefill: [String: String]?
    /// EXP-583: the suggested trigger of an "Action + automation" seed — the
    /// ONLY thing that turns on the create sheet's Automation block.
    @State private var suggestionAutomation: AutomationTrigger?
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

            if let vm = viewModel {
                content(vm)
            }
        }
        .navigationTitle("Actions")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
            await refreshDevices()
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
            // Refresh presence on every appear (the .task doesn't re-run on
            // pop-back). A no-op until steering resolves enabled.
            Task { await refreshDevices() }
        }
        .onDisappear {
            viewModel?.stopWatching()
        }
        // EXP-257: the unified Start-coding sheet, opened in Actions mode
        // preselected on the tapped row (it filters device candidates by
        // capability itself). The Issues tab carries the team's real
        // candidate pool (Android parity) so flipping over never dead-ends.
        .sheet(item: $runTarget, onDismiss: {
            createMode = false
            suggestionPrefill = nil
            suggestionAutomation = nil
        }) { action in
            StartCodingSheet(
                devices: devices ?? [],
                issues: viewModel?.startCandidates ?? [],
                preselectedIds: [],
                teamId: teamState.activeTeam?.id,
                initialTab: .actions,
                preselectedActionId: action.id,
                createActionMode: createMode,
                prefilledInputs: suggestionPrefill,
                suggestedAutomation: suggestionAutomation,
                automationDevices: viewModel?.allDevices.filter(\.canRunAutomations) ?? [],
                onStart: { device, issueIds, options in
                    viewModel?.startCoding(
                        device: device,
                        issueIds: issueIds,
                        options: options,
                        userId: deps.auth.userId
                    )
                },
                onRunAction: { device, chosen, options, inputs in
                    viewModel?.run(
                        action: chosen,
                        device: device,
                        options: options,
                        inputs: inputs,
                        userId: deps.auth.userId
                    )
                }
            )
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
        } message: { _ in
            Text("The action stays; only this trigger goes away.")
        }
        // The desktop picked the start up — jump into the live steer screen
        // ONCE (the same destination the .agentSession route arm builds).
        .onChange(of: viewModel?.startWatcher.startedSession) { _, started in
            if let started {
                viewModel?.startWatcher.startedSession = nil
                sessionTarget = started
            }
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
                steerApi: deps.steerApi,
                automationsApi: deps.automationsApi,
                auth: deps.auth
            )
        }
    }

    private func refreshDevices() async {
        guard steerEnabled else {
            devices = nil
            return
        }
        // EXP-432: team-scoped, so a teammate's shared server can host the
        // run. EXP-481: read off the synced devices shape, not the network.
        devices = await DeviceQueries.onlineStartTargets(
            db: deps.db, accountId: accountId,
            teamId: teamState.activeTeam?.id, userId: deps.auth.userId
        )
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
                LazyVStack(alignment: .leading, spacing: 8) {
                    // EXP-574 (web parity): "Actions · count" header with the
                    // "New action" entry (EXP-431) as its trailing control.
                    HStack(spacing: 6) {
                        sectionLabel("Actions", count: vm.actions.count)
                        Spacer(minLength: 0)
                        newActionButton
                    }
                    if let sentCaption = vm.startWatcher.sentCaption {
                        HStack(spacing: 6) {
                            ProgressView().controlSize(.small).tint(.white)
                            Text(sentCaption)
                                .font(.caption2)
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.horizontal, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if let startError = vm.startWatcher.failure {
                        Text(startError)
                            .font(.caption2)
                            .foregroundStyle(DesignTokens.Semantic.red)
                            .padding(.horizontal, 4)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    ForEach(vm.actions) { actionRow($0) }
                }
                .padding()
            }
        }
    }

    /// The web `SectionLabel` pair — "Actions 6", "Automations 2" — heading
    /// each segment's list (EXP-574 layout parity).
    private func sectionLabel(_ title: String, count: Int) -> some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
            Text("\(count)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .padding(.horizontal, 4)
    }

    /// EXP-431: creation left the list ("Create action" no longer poses as a
    /// row) — this button opens the sheet in its create mode.
    private var newActionButton: some View {
        Button {
            guard let teamId = teamState.activeTeam?.id else { return }
            createMode = true
            runTarget = ActionDto.builtinCreateAction(teamId: teamId)
            Task { await viewModel?.refreshStartCandidates() }
        } label: {
            HStack(spacing: 4) {
                AppIcon(AppIcons.actionCreate, size: 12)
                Text("New action")
                    .font(.caption.weight(.medium))
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .glassButton()
        }
        .buttonStyle(.plain)
        .disabled(teamState.activeTeam == nil)
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
                LazyVStack(alignment: .leading, spacing: 8) {
                    // EXP-574 (web parity): counted section headers.
                    HStack(spacing: 6) {
                        sectionLabel("Automations", count: vm.automations.count)
                        Spacer(minLength: 0)
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
                        sectionLabel("Recent automated runs", count: vm.automationRuns.count)
                            .padding(.top, 12)
                        ForEach(vm.automationRuns) { automatedRunRow($0) }
                    }
                }
                .padding()
            }
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
            Button {
                formTarget = AutomationFormTarget(id: "new", automation: nil)
            } label: {
                HStack(spacing: 4) {
                    AppIcon(AppIcons.uiAdd, size: 12)
                    Text("New automation")
                        .font(.caption.weight(.medium))
                }
                .foregroundStyle(.white)
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .glassButton()
            }
            .buttonStyle(.plain)
            .disabled(teamState.activeTeam == nil)
            .accessibilityLabel("New automation")
        }
    }

    /// One automation: the target action's glyph + name, the trigger
    /// sentence, the bound machine (label + online dot off the synced devices
    /// rows; raw id when the row isn't visible to us), the pinned agent/model
    /// when it overrides the machine's defaults, the next schedule run in the
    /// VIEWER's timezone (hence "(device time)" — the machine fires on its
    /// own clock), the last run, and the owner-only enabled toggle.
    private func automationRow(_ automation: AutomationDto, vm: ActionsViewModel) -> some View {
        let action = vm.actions.first { $0.id == automation.actionId }
        let trigger = automation.parsedTrigger
        let boundDevice = vm.allDevices.first { $0.deviceId == automation.deviceId }
        let busy = vm.automationBusyId == automation.id
        return HStack(alignment: .top, spacing: 12) {
            AppIcon(action?.icon ?? AppIcons.actionDefault, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            VStack(alignment: .leading, spacing: 3) {
                Text(action?.name ?? "Deleted action")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                if let trigger {
                    Text(AutomationTriggerDisplay.summary(trigger))
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
                if case let .schedule(schedule)? = trigger, automation.enabled,
                   let next = AutomationTriggerDisplay.nextScheduleRun(schedule, after: Date()) {
                    Text("Next run \(next.formatted(date: .abbreviated, time: .shortened)) (device time)")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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

            // Owner-only (the automations router is owner-gated server-side).
            Toggle("", isOn: Binding(
                get: { automation.enabled },
                set: { vm.setAutomationEnabled(automation, enabled: $0) }
            ))
            .labelsHidden()
            .tint(DesignTokens.Palette.primary)
            .disabled(!vm.permissions.isOwner || busy)
            .accessibilityLabel("Automation enabled")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
        .contextMenu {
            if vm.permissions.isOwner {
                Button {
                    formTarget = AutomationFormTarget(id: automation.id, automation: automation)
                } label: {
                    Label("Edit", systemImage: "pencil")
                }
                Button(role: .destructive) {
                    pendingDelete = automation
                } label: {
                    Label("Delete", systemImage: "trash")
                }
            }
        }
        .accessibilityIdentifier("automation-row")
    }

    /// "Claude Code · Opus · High" — only what the automation PINS; an unset
    /// field means the machine's own launch default, which is not ours to
    /// name here.
    private func launchCaption(_ automation: AutomationDto) -> String? {
        var parts: [String] = []
        if let agent = automation.agent, !agent.isEmpty {
            parts.append(StartCodingSheet.agentLabel(agent))
        }
        if let model = automation.model, !model.isEmpty {
            parts.append(StartCodingSheet.modelLabel(model))
        }
        if let effort = automation.effort, !effort.isEmpty {
            parts.append(StartCodingSheet.effortLabel(effort))
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
    /// "Automated" badge, action-name snapshot, status, relative start time.
    private func automatedRunRow(_ session: CodingSessionEntity) -> some View {
        HStack(spacing: 10) {
            HStack(spacing: 4) {
                AppIcon(AppIcons.actionAutomation, size: 10)
                Text("Automated")
                    .font(.caption2.weight(.semibold))
            }
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(Color.white.opacity(0.08), in: Capsule())

            Text(session.actionName ?? "Action run")
                .font(.caption)
                .foregroundStyle(.white)
                .lineLimit(1)

            Spacer(minLength: 0)

            Text(sessionStatusLabel(session.status))
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            let time = relativeDate(session.startedAt)
            if !time.isEmpty {
                Text(time)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
        .accessibilityIdentifier("automated-run-row")
    }

    private func sessionStatusLabel(_ status: String) -> String {
        switch status {
        case DomainContract.codingSessionStatusRunning: return "Running"
        case DomainContract.codingSessionStatusInReview: return "In review"
        case DomainContract.codingSessionStatusMerged: return "Merged"
        case DomainContract.codingSessionStatusEnded: return "Ended"
        default: return status.replacingOccurrences(of: "_", with: " ").capitalized
        }
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
    }

    private func suggestionCard(_ suggestion: ActionSuggestion) -> some View {
        HStack(spacing: 12) {
            AppIcon(suggestion.icon, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            VStack(alignment: .leading, spacing: 3) {
                Text(suggestion.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                // EXP-583: what "Use suggestion" will set up.
                Text(suggestion.automation == nil ? "Action" : "Action + automation")
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Color.white.opacity(0.08), in: Capsule())
                Text(suggestion.description)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(3)
            }

            Spacer(minLength: 0)

            Button {
                useSuggestion(suggestion)
            } label: {
                Text("Use suggestion")
                    .font(.caption.weight(.medium))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .glassButton()
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
        .accessibilityIdentifier("suggestion-row")
    }

    /// "Use suggestion": the create sheet, prefilled with the suggestion's
    /// description + icon input values (the same keys the create builtin's
    /// input defs declare) and, for an "Action + automation" seed, its
    /// suggested trigger.
    private func useSuggestion(_ suggestion: ActionSuggestion) {
        guard let teamId = teamState.activeTeam?.id else { return }
        suggestionPrefill = [
            "description": suggestion.description,
            "icon": suggestion.icon,
        ]
        // EXP-583: only an "Action + automation" seed opens the create
        // sheet's Automation block — the plain "New action" path never does.
        suggestionAutomation = suggestion.automation
        createMode = true
        runTarget = ActionDto.builtinCreateAction(teamId: teamId)
        Task { await viewModel?.refreshStartCandidates() }
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
                HStack(spacing: 6) {
                    Text(action.name)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    if action.repositoryId != nil {
                        // Small repo indicator: this action clones its repo.
                        AppIcon(AppIcons.actionRepository, size: 11)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .accessibilityLabel("Runs in a repository")
                    }
                }
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

            Button {
                runTarget = action
                // Rebuild the Issues-tab pool at open time — the sheet
                // self-heals if the read lands after presentation.
                Task { await viewModel?.refreshStartCandidates() }
            } label: {
                Text("Run")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .glassButton()
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
        .accessibilityIdentifier("action-row")
    }
}
