import ExpCore
import ExpUI
import SwiftUI

/// The Actions surface (EXP-253, view + run only — no manual edit on mobile):
/// the active team's action prompts, each with a Run affordance that
/// remote-starts the action on one of the caller's actions-capable desktops.
/// The toolbar's "New action" button (EXP-431) opens the same sheet in its
/// dedicated create mode — the "Create action" builtin left the list.
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
    /// EXP-530: Actions · Automations · Suggestions (the MyWorkView segment
    /// pattern — the choice survives relaunch via AppStorage).
    @AppStorage("actionsSegment") private var segmentRaw = Segment.actions.rawValue

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
        // EXP-431: creation left the list ("Create action" no longer poses as
        // a row) — the toolbar button opens the sheet in its create mode.
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    guard let teamId = teamState.activeTeam?.id else { return }
                    createMode = true
                    runTarget = ActionDto.builtinCreateAction(teamId: teamId)
                    Task { await viewModel?.refreshStartCandidates() }
                } label: {
                    AppIcon(AppIcons.actionCreate, size: AppIcon.Size.medium)
                }
                .disabled(teamState.activeTeam == nil)
                .accessibilityLabel("New action")
            }
        }
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
                actionsApi: deps.actionsApi,
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

    // MARK: - Automations (EXP-530)

    @ViewBuilder
    private func automationsContent(_ vm: ActionsViewModel) -> some View {
        let automated = vm.actions.filter { !$0.isBuiltin && $0.parsedTrigger != nil }
        if automated.isEmpty, vm.automationRuns.isEmpty {
            Spacer()
            emptyAutomationsState
            Spacer()
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    if let error = vm.triggerError {
                        Text(error)
                            .font(.caption2)
                            .foregroundStyle(DesignTokens.Semantic.red)
                            .padding(.horizontal, 4)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if automated.isEmpty {
                        emptyAutomationsState
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 16)
                    } else {
                        ForEach(automated) { automationRow($0, vm: vm) }
                    }
                    if !vm.automationRuns.isEmpty {
                        Text("Recent automated runs")
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .padding(.top, 12)
                            .padding(.horizontal, 4)
                        ForEach(vm.automationRuns) { automatedRunRow($0) }
                    }
                }
                .padding()
            }
        }
    }

    private var emptyAutomationsState: some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.actionAutomation, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No automations yet. Add a schedule or event trigger to an action.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
    }

    /// One triggered action: glyph + name, the trigger sentence, the bound
    /// device (label + online dot off the synced devices rows; raw id when
    /// the row isn't visible to us), the next schedule run in the VIEWER's
    /// timezone (hence the "(device time)" label — the device fires on its
    /// own clock), and the owner-only enabled toggle.
    private func automationRow(_ action: ActionDto, vm: ActionsViewModel) -> some View {
        let trigger = action.parsedTrigger
        let boundDevice = trigger.flatMap { t in
            vm.allDevices.first { $0.deviceId == t.deviceId }
        }
        // An automated run has nobody to type required inputs, so the server
        // refuses to ENABLE such a trigger — but a legacy row that is already
        // on must stay switchable OFF.
        let hasRequired = action.inputs?.contains(where: \.isRequired) == true
        let locked = hasRequired && !(trigger?.enabled ?? false)
        return HStack(alignment: .top, spacing: 12) {
            AppIcon(action.icon ?? AppIcons.actionDefault, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            VStack(alignment: .leading, spacing: 3) {
                Text(action.name)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                if let trigger {
                    Text(ActionTriggerDisplay.summary(trigger))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    HStack(spacing: 5) {
                        Circle()
                            .fill(boundDevice?.isOnline == true
                                ? DesignTokens.Semantic.green
                                : Color.white.opacity(0.25))
                            .frame(width: 6, height: 6)
                        Text(deviceLabel(boundDevice, deviceId: trigger.deviceId))
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                    if case let .schedule(schedule) = trigger, trigger.enabled,
                       let next = ActionTriggerDisplay.nextScheduleRun(schedule, after: Date()) {
                        Text("Next run \(next.formatted(date: .abbreviated, time: .shortened)) (device time)")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                if locked {
                    Text("This action has required inputs, and an automated run has none to fill them with. Make the inputs optional to enable it.")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            Spacer(minLength: 0)

            // Owner-only (actions.update is owner-gated server-side).
            Toggle("", isOn: Binding(
                get: { trigger?.enabled ?? false },
                set: { vm.setTriggerEnabled(action: action, enabled: $0) }
            ))
            .labelsHidden()
            .disabled(!vm.permissions.isOwner || vm.triggerBusyActionId != nil || locked)
            .accessibilityLabel("Automation enabled")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
        .accessibilityIdentifier("automation-row")
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
                Text("Ideas your agent can build as team actions.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 4)
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
                Text(suggestion.description)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(3)
            }

            Spacer(minLength: 0)

            Button {
                useSuggestion(suggestion)
            } label: {
                Text("Use")
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
        .accessibilityIdentifier("suggestion-row")
    }

    /// "Use suggestion": the create sheet, prefilled with the suggestion's
    /// description + icon input values (the same keys the create builtin's
    /// input defs declare).
    private func useSuggestion(_ suggestion: ActionSuggestion) {
        guard let teamId = teamState.activeTeam?.id else { return }
        suggestionPrefill = [
            "description": suggestion.description,
            "icon": suggestion.icon,
        ]
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
                // EXP-530: a configured automation shows its one-line summary.
                if let trigger = action.parsedTrigger {
                    HStack(spacing: 4) {
                        AppIcon(AppIcons.actionAutomation, size: 10)
                        Text(ActionTriggerDisplay.summary(trigger))
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
