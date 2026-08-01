import ExpUI
import ExpCore
import SwiftUI

/// The Agents tab: the caller's online desktops (with a per-device "Start
/// coding" launcher) above the caller's OWN running coding sessions in the
/// active account (EXP-312 — teammates' runs are owner-only, so they are not
/// listed at all). Session rows open the live agent session view directly when
/// the relay is configured (the same viewer AgentPrCard presents from an issue),
/// else fall back to the issue detail; the trailing info affordance always goes
/// to the issue detail. When the relay is off the desktops section is absent and
/// the tab shows the full-screen empty state until a session appears.
struct AgentsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @State private var viewModel: AgentsViewModel?
    @State private var steerEnabled = false
    @State private var devices: [SteerDevice]?
    @State private var startSheetDevice: SteerDevice?
    // Success feedback (informational, tertiary) vs. failure (red) are kept
    // separate: a start error must read as an error and not persist forever.
    @State private var sentCaption: String?
    @State private var startError: String?
    // "Merge and close" (EXP-358), keyed by row id: the confirm target, the
    // in-flight rows, and the per-row failure caption (inline like the Reviews
    // rows, EXP-323 — never a modal the tab bar can cover).
    @State private var mergeTarget: MergeAndCloseTarget?
    @State private var merging: Set<String> = []
    @State private var mergeErrors: [String: String] = [:]

    /// The row a "Merge and close" confirm is pending for. Only the ids are
    /// captured — the alert copy is fixed, and the row itself may re-sync
    /// underneath the alert.
    private struct MergeAndCloseTarget: Identifiable {
        let rowId: String
        let issueId: String
        var id: String { rowId }
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                if steerEnabled {
                    // Desktops section present — no full-screen empty state.
                    agentsContent(vm)
                } else if vm.rows.isEmpty {
                    emptyState
                } else {
                    sessionList(vm)
                }
            }
        }
        .navigationTitle("Agents")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        // Team actions (EXP-253) live behind the Agents surface — the tab bar
        // is already at capacity (six tabs + compose on a 375pt screen), so
        // the entry rides the Agents toolbar instead of a seventh tab. NOT
        // helpdesk-gated.
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink(value: AppRoute.actions) {
                    HStack(spacing: 4) {
                        AppIcon(AppIcons.actionDefault, size: AppIcon.Size.small)
                        Text("Actions")
                            .font(.subheadline)
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .contentShape(Rectangle())
                }
                .accessibilityLabel("Actions")
            }
        }
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
            await refreshDevices()
        }
        .onAppear {
            if viewModel == nil {
                viewModel = AgentsViewModel(
                    accountId: accountId, userId: deps.auth.userId, db: deps.db
                )
            }
            // The list is scoped to the active team like web's Agents page —
            // the VM observes the account's sessions, the view owns the team.
            viewModel?.activeTeamId = teamState.activeTeam?.id
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            viewModel?.startObserving()
            // Refresh presence on every appear (the .task doesn't re-run on
            // pop-back). A no-op until steering resolves enabled.
            Task { await refreshDevices() }
        }
        .onChange(of: teamState.activeTeam?.id) { _, teamId in
            viewModel?.activeTeamId = teamId
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
        .sheet(item: $startSheetDevice) { device in
            // EXP-257: wiring teamId + onRunAction gives the sheet its
            // Issues | Actions segmented control — actions launch from the
            // same unified dialog.
            StartCodingSheet(
                devices: devices ?? [],
                issues: viewModel?.startCandidates(teamId: teamState.activeTeam?.id) ?? [],
                preselectedIds: [],
                preferredDeviceId: device.deviceId,
                teamId: teamState.activeTeam?.id,
                onStart: { chosenDevice, issueIds, options in
                    start(on: chosenDevice, issueIds: issueIds, options: options)
                },
                onRunAction: { chosenDevice, action, options, inputs in
                    runAction(on: chosenDevice, action: action, options: options, inputs: inputs)
                }
            )
        }
        .alert(
            "Merge and close?",
            isPresented: Binding(
                get: { mergeTarget != nil },
                set: { if !$0 { mergeTarget = nil } }
            ),
            presenting: mergeTarget
        ) { target in
            Button("Merge and close", role: .destructive) { mergeAndClose(target) }
            Button("Cancel", role: .cancel) { mergeTarget = nil }
        } message: { _ in
            Text("Merges the pull request, completes every linked issue, and closes the coding session.")
        }
    }

    private func refreshDevices() async {
        guard steerEnabled else {
            devices = nil
            return
        }
        devices = (try? await deps.steerApi.myDevices(accountId: accountId)) ?? []
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.navAgents, size: 28)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No agents running")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Start coding on an issue from the desktop IDE. Live sessions show up here.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
    }

    // MARK: - Combined content (relay on)

    @ViewBuilder
    private func agentsContent(_ vm: AgentsViewModel) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 8) {
                sectionHeader("My desktops")
                if let devices {
                    if devices.isEmpty {
                        deviceHintRow
                    } else {
                        ForEach(devices) { deviceRow($0) }
                    }
                } else {
                    deviceLoadingRow
                }
                if let sentCaption {
                    Text(sentCaption)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.horizontal, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                if let startError {
                    Text(startError)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .padding(.horizontal, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }

                sectionHeader("Running")
                if vm.rows.isEmpty {
                    noAgentsRow
                } else {
                    ForEach(vm.rows) { sessionRow($0) }
                }
            }
            .padding()
        }
        // Clearance for the floating tab bar (EXP-36).
        .tabBarBottomInset()
    }

    private func sectionHeader(_ title: String) -> some View {
        HStack {
            Text(title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .padding(.top, 4)
    }

    private func deviceRow(_ device: SteerDevice) -> some View {
        HStack(spacing: 12) {
            AppIcon(AppIcons.uiDevice, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
            Spacer(minLength: 0)
            Button {
                startSheetDevice = device
            } label: {
                Text("Start coding")
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
    }

    private var deviceHintRow: some View {
        HStack(spacing: 8) {
            // EXP-317: the same glyph the web draws next to this exact copy
            // (`ui-device-offline`); `ui-offline` stays the network indicator.
            AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.small)
            Text("No desktop online. Open the Exponential desktop app to run here.")
                .font(.caption)
            Spacer(minLength: 0)
        }
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
    }

    private var deviceLoadingRow: some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small).tint(.white)
            Text("Checking for desktops…")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
    }

    private var noAgentsRow: some View {
        HStack(spacing: 8) {
            Text("No agents running right now.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
    }

    // MARK: - Session list (relay off, sessions present)

    @ViewBuilder
    private func sessionList(_ vm: AgentsViewModel) -> some View {
        ScrollView {
            LazyVStack(spacing: 8) {
                ForEach(vm.rows) { row in
                    sessionRow(row)
                }
            }
            .padding()
        }
        // Clearance for the floating tab bar (EXP-36).
        .tabBarBottomInset()
    }

    // The primary tap target and the trailing affordances (merge-and-close,
    // info) are siblings (not nested controls) so every hit area stays
    // reliable.
    @ViewBuilder
    private func sessionRow(_ row: AgentsViewModel.Row) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            sessionRowBody(row)
            if let message = mergeErrors[row.id] {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
        .accessibilityIdentifier("agent-session-row")
    }

    @ViewBuilder
    private func sessionRowBody(_ row: AgentsViewModel.Row) -> some View {
        HStack(spacing: 12) {
            // Every listed row is the caller's own (EXP-312: live sessions are
            // owner-only), so with the relay configured the row jumps straight
            // into the live agent session; without it, into the issue detail,
            // where the card shows whatever is available.
            Group {
                if steerEnabled {
                    NavigationLink(value: AppRoute.agentSession(
                        accountId: accountId, sessionId: row.session.id
                    )) {
                        sessionRowContent(row)
                    }
                    .buttonStyle(.plain)
                } else if let issue = row.issue {
                    NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                        sessionRowContent(row)
                    }
                    .buttonStyle(.plain)
                } else {
                    sessionRowContent(row)
                }
            }

            // Merging the PR no longer ends the run (EXP-358) — this is the one
            // affordance that does both, so it only shows while there IS an
            // open PR to merge. Every other merge button stays merge-only.
            if let issue = row.issue, issue.prState == DomainContract.prStateOpen {
                Button {
                    mergeTarget = MergeAndCloseTarget(rowId: row.id, issueId: issue.id)
                } label: {
                    Group {
                        if merging.contains(row.id) {
                            ProgressView().controlSize(.mini).tint(.white)
                        } else {
                            AppIcon(AppIcons.prMerged, size: AppIcon.Size.medium)
                        }
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 32, height: 32)
                    .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .disabled(merging.contains(row.id))
                .accessibilityLabel("Merge and close")
            }

            if let issue = row.issue {
                NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                    AppIcon(AppIcons.uiInfo, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Open issue")
            }
        }
    }

    /// Merge the row's PR AND end its session (EXP-358). No local list surgery:
    /// the server flips the row to `ended`, which drops it out of the live
    /// query through sync. A refusal (conflicts, branch protection) captions
    /// THIS row.
    private func mergeAndClose(_ target: MergeAndCloseTarget) {
        mergeTarget = nil
        mergeErrors[target.rowId] = nil
        merging.insert(target.rowId)
        Task {
            do {
                try await deps.issuesApi.mergePr(
                    accountId: accountId,
                    issueId: target.issueId,
                    closeSessions: true
                )
            } catch {
                mergeErrors[target.rowId] = error.localizedDescription
            }
            merging.remove(target.rowId)
        }
    }

    /// Static-dot/label tint per parked display state (EXP-194/EXP-214):
    /// review green, done/merged blue (the issue-status palette), needs-input
    /// amber.
    private func stateColor(_ state: CodingSessionDisplayState) -> Color {
        switch state {
        case .needsInput: DesignTokens.Semantic.yellow
        case .review: DesignTokens.Semantic.green
        case .done: DesignTokens.Semantic.blue
        case .merged: DesignTokens.Semantic.blue
        case .running: DesignTokens.Semantic.green
        }
    }

    private func stateLabel(_ state: CodingSessionDisplayState) -> String? {
        switch state {
        case .needsInput: "Needs input"
        case .review: "Ready for review"
        case .done: "Done"
        case .merged: "Merged"
        case .running: nil
        }
    }

    @ViewBuilder
    private func sessionRowContent(_ row: AgentsViewModel.Row) -> some View {
        // The parked states render a static dot/label instead of the
        // pulsing-green "Coding now": review green, done blue (once the PR
        // merges), needs-input amber while the agent waits on a picker
        // (EXP-194/EXP-214).
        let state = CodingSessionDisplayState.of(
            session: row.session, prState: row.issue?.prState
        )
        HStack(spacing: 12) {
            if state != .running {
                Circle()
                    .fill(stateColor(state))
                    .frame(width: 9, height: 9)
            } else {
                PulsingLiveDot()
            }

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    if let identifier = row.issue?.identifier {
                        Text(identifier)
                            .font(.caption.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                    Text(title(row))
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                }
                HStack(spacing: 6) {
                    if let label = stateLabel(state) {
                        Text(label)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(stateColor(state))
                            .lineLimit(1)
                    }
                    Text(byline(row.session))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
            }

            Spacer(minLength: 0)
        }
        .contentShape(Rectangle())
    }

    /// An issueless (nil session issueId) run is an action run when the row
    /// carries its action_name snapshot (EXP-253), else a batch run — never
    /// "Untitled issue". A single-issue session whose issue row simply hasn't
    /// synced yet still reads "Untitled issue".
    private func title(_ row: AgentsViewModel.Row) -> String {
        if row.issue == nil, row.session.issueId == nil {
            return row.session.actionName ?? "Batch run"
        }
        return row.issue?.title ?? "Untitled issue"
    }

    private func byline(_ session: CodingSessionEntity) -> String {
        let device: String
        if let label = session.deviceLabel, !label.isEmpty {
            device = label
        } else {
            device = "Desktop"
        }
        let started = relativeDate(session.startedAt)
        return started.isEmpty ? device : "\(device) · started \(started)"
    }

    private func relativeDate(_ s: String) -> String {
        // Electric syncs started_at as Postgres text (space separator, hour-only
        // offset), which ISO8601DateFormatter alone rejects — WireTimestamps
        // handles both wire forms (EXP-169).
        guard let date = WireTimestamps.parse(s) else { return "" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }

    // MARK: - Remote start

    private func start(on device: SteerDevice, issueIds: [String], options: SteerStartOptions) {
        guard !issueIds.isEmpty else { return }
        // A fresh attempt supersedes the previous outcome (success or error).
        sentCaption = nil
        startError = nil
        let isBatch = issueIds.count > 1
        let label = device.deviceLabel
        Task {
            do {
                if isBatch {
                    try await deps.steerApi.startSession(
                        accountId: accountId,
                        issueIds: issueIds,
                        deviceId: device.deviceId,
                        options: options
                    )
                } else {
                    try await deps.steerApi.startSession(
                        accountId: accountId,
                        issueId: issueIds[0],
                        deviceId: device.deviceId,
                        options: options
                    )
                }
                sentCaption = isBatch
                    ? "Batch start sent to \(label). It'll appear here when it spins up."
                    : "Start sent to \(label). It'll appear here when it spins up."
                // The desktop inserts the coding_sessions row when the launcher
                // spins up, which surfaces in the Running list via sync. Clear
                // the informational caption after a grace window (errors persist
                // until the next attempt so they can't be missed).
                Task {
                    try? await Task.sleep(for: .seconds(30))
                    sentCaption = nil
                }
            } catch {
                startError = error.localizedDescription
            }
        }
    }

    /// Actions-mode launch from the unified sheet (EXP-257): full option set,
    /// typed input values, `teamId` only for the builtin "Create action" (its
    /// virtual row has no server-resolvable id). The run surfaces in the
    /// Running list via sync like any other session.
    private func runAction(
        on device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: [String: String]
    ) {
        // A fresh attempt supersedes the previous outcome (success or error).
        sentCaption = nil
        startError = nil
        let label = device.deviceLabel
        Task {
            do {
                try await deps.steerApi.startSession(
                    accountId: accountId,
                    actionId: action.id,
                    deviceId: device.deviceId,
                    teamId: action.isBuiltin ? action.teamId : nil,
                    options: options,
                    inputs: inputs.isEmpty ? nil : inputs
                )
                sentCaption = "Run sent to \(label). It'll appear here when it spins up."
                Task {
                    try? await Task.sleep(for: .seconds(30))
                    sentCaption = nil
                }
            } catch {
                startError = error.localizedDescription
            }
        }
    }
}
