import ExpCore
import ExpUI
import SwiftUI

/// The Actions surface (EXP-253, view + run only — no create/edit on mobile):
/// the active team's action prompts, each with a Run affordance that
/// remote-starts the action on one of the caller's actions-capable desktops.
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
        .sheet(item: $runTarget) { action in
            RunActionSheet(
                action: action,
                devices: (devices ?? []).filter(\.canRunActions)
            ) { device, model, effort in
                viewModel?.run(
                    action: action,
                    device: device,
                    model: model,
                    effort: effort,
                    userId: deps.auth.userId
                )
            }
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
                actionsApi: deps.actionsApi,
                steerApi: deps.steerApi
            )
        }
    }

    private func refreshDevices() async {
        guard steerEnabled else {
            devices = nil
            return
        }
        devices = (try? await deps.steerApi.myDevices(accountId: accountId)) ?? []
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
            Image(systemName: "bolt")
                .font(.title2)
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
            Image(systemName: "exclamationmark.triangle")
                .font(.title2)
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
            Image(systemName: "bolt")
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(action.name)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    if action.repositoryId != nil {
                        // Small repo indicator: this action clones its repo.
                        Image(systemName: "arrow.triangle.branch")
                            .font(.caption2)
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

// MARK: - Run sheet

/// The run sheet: an actions-capable desktop picker plus optional Claude
/// Model/Effort pickers (the StartCodingSheet's claude contract lists; the
/// "Desktop default" entry omits the field so the desktop's per-agent
/// settings default applies). Action runs are Claude-only v1 — no agent
/// strip, no ultracode/plan/skip toggles.
private struct RunActionSheet: View {
    let action: ActionDto
    /// Already filtered to devices advertising the `actions` capability.
    let devices: [SteerDevice]
    let onStart: (SteerDevice, String?, String?) -> Void

    @Environment(\.dismiss) private var dismiss

    /// Sentinel for the omit-the-field "Desktop default" choice.
    private static let desktopDefault = "desktop-default"

    @State private var deviceId: String?
    @State private var model = Self.desktopDefault
    @State private var effort = Self.desktopDefault

    private var device: SteerDevice? {
        if let deviceId, let match = devices.first(where: { $0.deviceId == deviceId }) {
            return match
        }
        return devices.first
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(action.name)
                            .font(.subheadline.weight(.medium))
                        if let description = action.description, !description.isEmpty {
                            Text(description)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                } header: {
                    Text("Action")
                }

                Section {
                    if devices.isEmpty {
                        Text("No actions-capable desktop online — open or update the Exponential desktop app.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else if devices.count > 1 {
                        Picker("Desktop", selection: deviceBinding) {
                            ForEach(devices) { device in
                                Text(device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel)
                                    .tag(device.deviceId)
                            }
                        }
                    } else if let device {
                        HStack(spacing: 8) {
                            Image(systemName: "display")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Text(device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel)
                                .font(.subheadline)
                        }
                    }
                } header: {
                    Text("Desktop")
                }

                Section {
                    Picker("Model", selection: $model) {
                        Text("Desktop default").tag(Self.desktopDefault)
                        ForEach(DomainContract.codingModelValues, id: \.self) { value in
                            Text(Self.modelLabel(value)).tag(value)
                        }
                    }
                    Picker("Effort", selection: $effort) {
                        Text("Desktop default").tag(Self.desktopDefault)
                        ForEach(DomainContract.codingEffortValues, id: \.self) { value in
                            Text(Self.effortLabel(value)).tag(value)
                        }
                    }
                }
            }
            .listSectionSpacing(12)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Run action") { submit() }
                        .disabled(device == nil)
                }
            }
        }
        .presentationDetents([.medium, .large])
    }

    private var deviceBinding: Binding<String> {
        Binding(
            get: { device?.deviceId ?? "" },
            set: { deviceId = $0 }
        )
    }

    private func submit() {
        guard let device else { return }
        dismiss()
        onStart(
            device,
            model == Self.desktopDefault ? nil : model,
            effort == Self.desktopDefault ? nil : effort
        )
    }

    private static func modelLabel(_ value: String) -> String {
        value.prefix(1).uppercased() + value.dropFirst()
    }

    private static func effortLabel(_ value: String) -> String {
        value == "xhigh" ? "XHigh" : value.prefix(1).uppercased() + value.dropFirst()
    }
}
