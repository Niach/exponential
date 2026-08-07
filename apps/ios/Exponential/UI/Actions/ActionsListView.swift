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
    @State private var sessionTarget: ActionsViewModel.StartedSession?

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
        .sheet(item: $runTarget, onDismiss: { createMode = false }) { action in
            StartCodingSheet(
                devices: devices ?? [],
                issues: viewModel?.startCandidates ?? [],
                preselectedIds: [],
                teamId: teamState.activeTeam?.id,
                initialTab: .actions,
                preselectedActionId: action.id,
                createActionMode: createMode,
                onStart: { device, issueIds, options in
                    viewModel?.startCoding(device: device, issueIds: issueIds, options: options)
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
        .onChange(of: viewModel?.startedSession) { _, started in
            if let started {
                viewModel?.startedSession = nil
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
                steerApi: deps.steerApi
            )
        }
    }

    private func refreshDevices() async {
        guard steerEnabled else {
            devices = nil
            return
        }
        // EXP-432: team-scoped, so a teammate's shared server can host the run.
        devices = (try? await deps.devicesApi.onlineStartTargets(
            accountId: accountId, teamId: teamState.activeTeam?.id
        )) ?? []
    }

    // MARK: - Content

    @ViewBuilder
    private func content(_ vm: ActionsViewModel) -> some View {
        if vm.actions.isEmpty {
            if vm.isLoading {
                ProgressView().tint(.white)
            } else if let error = vm.loadError {
                errorState(error)
            } else {
                emptyState
            }
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    if let sentCaption = vm.sentCaption {
                        HStack(spacing: 6) {
                            ProgressView().controlSize(.small).tint(.white)
                            Text(sentCaption)
                                .font(.caption2)
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.horizontal, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if let startError = vm.startError {
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
