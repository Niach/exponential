import ExpUI
import ExpCore
import SwiftUI

/// The Agents tab: "My machines" — the caller's registered devices (EXP-403:
/// desktops AND headless `exponential` daemon servers, online or not, polled
/// from `devices.list`) with a per-machine "Start coding" launcher, rename /
/// remove / self-update row actions — above the caller's OWN running coding
/// sessions in the active account (EXP-312 — teammates' runs are owner-only, so
/// they are not listed at all). Session rows open the live agent session view
/// directly when the relay is configured (the same viewer AgentPrCard presents
/// from an issue), else fall back to the issue detail; the trailing info
/// affordance always goes to the issue detail. When the relay is off the
/// machines section is absent (web parity — nothing here can be started) and
/// the tab shows the full-screen empty state until a session appears.
struct AgentsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @State private var viewModel: AgentsViewModel?
    @State private var steerEnabled = false
    @State private var devices: [SteerDevice]?
    /// EXP-420: `devices.list`'s advertised latest versions — gates the
    /// server rows' Update action on an actually-newer CLI build.
    @State private var latestVersions: LatestVersions?
    /// Re-polls `devices.list` while the tab is visible, so a machine coming
    /// online (or a daemon finishing its self-update) appears on its own.
    @State private var devicePollTask: Task<Void, Never>?
    @State private var startSheetDevice: SteerDevice?
    // Machine row actions (EXP-403): the rename/remove alert targets, the
    // optimistic "Updating…" ids (the flag itself only lands on the next
    // poll), and the shared failure caption.
    @State private var renameTarget: SteerDevice?
    @State private var renameText = ""
    @State private var removeTarget: SteerDevice?
    @State private var updatingIds: Set<String> = []
    @State private var deviceError: String?
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
                    // Machines section present — no full-screen empty state.
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
            startDevicePolling()
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
            // Re-arm the poll on every appear (the .task doesn't re-run on
            // pop-back). A no-op until steering resolves enabled.
            startDevicePolling()
        }
        .onChange(of: teamState.activeTeam?.id) { _, teamId in
            viewModel?.activeTeamId = teamId
        }
        .onDisappear {
            viewModel?.stopObserving()
            stopDevicePolling()
        }
        .sheet(item: $startSheetDevice) { device in
            // EXP-257: wiring teamId + onRunAction gives the sheet its
            // Issues | Actions segmented control — actions launch from the
            // same unified dialog.
            StartCodingSheet(
                // Offline machines are listed but can't be started on — the
                // picker's pool (and its caps gating) stays online-only.
                devices: onlineDevices,
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

    // MARK: - My machines

    /// The machines a run can be sent to (EXP-403: the list itself includes
    /// offline rows). The sheet narrows this further — EXP-409 drops machines
    /// whose every installed agent is signed out.
    private var onlineDevices: [SteerDevice] {
        (devices ?? []).filter(\.isOnline)
    }

    /// Poll `devices.list` while the tab is visible — restartable, so the
    /// appear/disappear cycle owns exactly one loop. The first pass runs
    /// immediately; the relay-off case keeps the section absent (web parity:
    /// nothing listed here could be started).
    private func startDevicePolling() {
        stopDevicePolling()
        guard steerEnabled else {
            devices = nil
            return
        }
        devicePollTask = Task {
            while !Task.isCancelled {
                await refreshDevices()
                try? await Task.sleep(for: .seconds(15))
            }
        }
    }

    private func stopDevicePolling() {
        devicePollTask?.cancel()
        devicePollTask = nil
    }

    private func refreshDevices() async {
        guard steerEnabled else {
            devices = nil
            return
        }
        let result = try? await deps.devicesApi.list(accountId: accountId)
        devices = result?.devices ?? devices ?? []
        latestVersions = result?.latestVersions ?? latestVersions
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
                sectionHeader("My machines")
                if let devices {
                    if devices.isEmpty {
                        deviceHintRow
                    } else {
                        ForEach(devices) { deviceRow($0) }
                    }
                } else {
                    deviceLoadingRow
                }
                if let deviceError {
                    Text(deviceError)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .padding(.horizontal, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
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
        // The machine alerts hang off THIS view, not the body's ZStack: that
        // one already owns the merge-and-close alert, and stacking three
        // `.alert`s on one node is where SwiftUI starts dropping presentations.
        .alert(
            "Rename machine",
            isPresented: Binding(
                get: { renameTarget != nil },
                set: { if !$0 { renameTarget = nil } }
            )
        ) {
            TextField("Name", text: $renameText)
            Button("Cancel", role: .cancel) { renameTarget = nil }
            Button("Save") { rename() }
        } message: {
            Text("The name shows on every client, whether the machine is online or not.")
        }
        .alert(
            "Remove machine?",
            isPresented: Binding(
                get: { removeTarget != nil },
                set: { if !$0 { removeTarget = nil } }
            ),
            presenting: removeTarget
        ) { device in
            Button("Cancel", role: .cancel) { removeTarget = nil }
            Button("Remove", role: .destructive) { remove(device) }
        } message: { device in
            Text("Remove “\(deviceName(device))” from your machines? A machine with the daemon still running re-registers itself on its next heartbeat.")
        }
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

    /// One machine: kind glyph, label + version, live/last-seen state, the
    /// launcher for online ones, and an overflow menu. The menu is an explicit
    /// trailing control rather than a long-press context menu (EXP-331: the
    /// same reason the label rows grew one) and it only appears on registered
    /// rows — a desktop build predating the registry shows up from relay
    /// presence alone and has nothing to rename or remove.
    private func deviceRow(_ device: SteerDevice) -> some View {
        HStack(spacing: 12) {
            AppIcon(
                device.isServer ? AppIcons.uiServer : AppIcons.uiDevice,
                size: AppIcon.Size.medium
            )
            .foregroundStyle(.white.opacity(TextOpacity.secondary))

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(deviceName(device))
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    if let version = device.version, !version.isEmpty {
                        Text("v\(version)")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                }
                deviceStatusLine(device)
            }

            Spacer(minLength: 0)

            // Offline machines keep their row (rename/remove still apply) but
            // offer no launcher — a start would be rejected server-side. Same
            // for a machine with nothing runnable (EXP-409: every installed
            // agent signed out); its status line carries the reason.
            if device.isOnline, device.hasRunnableAgent {
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

            if device.isRegistered {
                Menu {
                    deviceMenu(device)
                } label: {
                    AppIcon(AppIcons.uiMore, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(6)
                }
                .accessibilityLabel("Machine actions")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
        // EXP-409: a machine that can run nothing reads like an offline one.
        .opacity(device.needsAgentSignIn ? 0.6 : 1)
    }

    /// A requested self-update REPLACES the live state: the daemon is about to
    /// restart, so "Online" would only read as a lie — unless the update is
    /// parked behind live coding sessions (EXP-411): then the row says
    /// "Update queued" without a spinner instead of "Updating…" forever.
    /// EXP-409: signed-out agents replace "Online" when nothing is runnable
    /// (amber dot, web + desktop parity) and annotate it when a runnable
    /// sibling exists.
    @ViewBuilder
    private func deviceStatusLine(_ device: SteerDevice) -> some View {
        let signedOut = device.unauthedAgentIds.joined(separator: ", ")
        let signInNeeded = device.needsAgentSignIn
        HStack(spacing: 5) {
            if isUpdateQueued(device) {
                Text("Update queued")
            } else if isUpdating(device) {
                ProgressView().controlSize(.mini).tint(.white)
                Text("Updating…")
            } else if device.isOnline {
                Circle()
                    .fill(signInNeeded ? DesignTokens.Semantic.yellow : DesignTokens.Semantic.green)
                    .frame(width: 6, height: 6)
                if signInNeeded {
                    Text("\(signedOut) not signed in")
                } else {
                    Text("Online")
                    if !signedOut.isEmpty {
                        Text("· \(signedOut) not signed in")
                            .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                            .lineLimit(1)
                    }
                }
            } else {
                Text(lastSeenCaption(device))
            }
        }
        .font(.caption)
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }

    /// Row actions for a registered machine. Self-update is a daemon-server
    /// affordance only (the desktop app updates itself), and it needs the
    /// machine online to pick the request up.
    @ViewBuilder
    private func deviceMenu(_ device: SteerDevice) -> some View {
        Button {
            renameText = device.deviceLabel
            renameTarget = device
        } label: {
            Label("Rename", appIcon: AppIcons.uiEdit)
        }
        // EXP-420: offered only when a newer CLI version really exists.
        if device.isServer, device.isOnline, !isUpdating(device),
            device.updateAvailable(latest: latestVersions?.cli)
        {
            Button {
                requestUpdate(device)
            } label: {
                Label("Update", appIcon: AppIcons.uiUpdate)
            }
        }
        Button(role: .destructive) {
            removeTarget = device
        } label: {
            Label("Remove", appIcon: AppIcons.uiDelete)
        }
    }

    private func deviceName(_ device: SteerDevice) -> String {
        device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
    }

    /// The pending flag rides the server row until the daemon re-registers;
    /// the local set covers the gap until the next poll returns it.
    private func isUpdating(_ device: SteerDevice) -> Bool {
        device.updateRequested == true || updatingIds.contains(device.deviceId)
    }

    /// EXP-411: the pending update is parked behind live coding sessions on
    /// the machine — the daemon applies it once they close.
    private func isUpdateQueued(_ device: SteerDevice) -> Bool {
        device.updateRequested == true && device.updateBlocked == true
    }

    private func lastSeenCaption(_ device: SteerDevice) -> String {
        guard let lastSeenAt = device.lastSeenAt else { return "Offline" }
        let relative = relativeDate(lastSeenAt)
        return relative.isEmpty ? "Offline" : "Last seen \(relative)"
    }

    private var deviceHintRow: some View {
        HStack(spacing: 8) {
            // EXP-317: the same glyph the web draws on its empty machines row
            // (`ui-device-offline`); `ui-offline` stays the network indicator.
            AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.small)
            // Web puts its install one-liner behind this row; a phone can't
            // run it, so mobile points at the surface that can (Android says
            // the same thing, word for word).
            Text("No machines yet. Open the Exponential desktop app, or add a server on the web.")
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
            Text("Checking for machines…")
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

    // MARK: - Machine actions

    /// Rename through the registry (the label is authoritative there, so it
    /// shows on every client at once). Refreshes immediately instead of
    /// waiting out the poll tick.
    private func rename() {
        guard let device = renameTarget else { return }
        renameTarget = nil
        let label = renameText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !label.isEmpty, label != device.deviceLabel else { return }
        deviceError = nil
        Task {
            do {
                try await deps.devicesApi.rename(
                    accountId: accountId, deviceId: device.deviceId, label: label
                )
            } catch {
                deviceError = error.localizedDescription
            }
            await refreshDevices()
        }
    }

    private func remove(_ device: SteerDevice) {
        removeTarget = nil
        deviceError = nil
        Task {
            do {
                try await deps.devicesApi.remove(accountId: accountId, deviceId: device.deviceId)
            } catch {
                deviceError = error.localizedDescription
            }
            await refreshDevices()
        }
    }

    /// Ask a daemon server to self-update. The row keeps its "Updating…" state
    /// until the daemon re-registers (which clears the flag server-side),
    /// which the poll picks up.
    private func requestUpdate(_ device: SteerDevice) {
        deviceError = nil
        updatingIds.insert(device.deviceId)
        Task {
            do {
                try await deps.devicesApi.requestUpdate(
                    accountId: accountId, deviceId: device.deviceId
                )
            } catch {
                deviceError = error.localizedDescription
            }
            await refreshDevices()
            updatingIds.remove(device.deviceId)
        }
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
