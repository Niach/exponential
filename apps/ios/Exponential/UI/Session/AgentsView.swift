import ExpUI
import ExpCore
import SwiftUI

/// The Agents tab: "My machines" — the caller's registered devices (EXP-403:
/// desktops AND headless `exponential` daemon servers, online or not — since
/// EXP-481 read from the synced `devices` shape, online-ness derived from
/// last_seen_at freshness) with a per-machine "Start coding" launcher and an
/// Edit (device settings sheet: name, sharing, agent defaults, worktrees) /
/// self-update / remove row menu — then "Team machines" (EXP-432: teammates'
/// servers shared with the active team, startable but never manageable here)
/// — above the caller's OWN running coding
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
    /// EXP-420: `devices.list`'s advertised latest versions — gates the
    /// server rows' Update action on an actually-newer CLI build. The one
    /// remaining tRPC read here (instance config, not a shape column):
    /// fetched once per account instead of polled.
    @State private var latestVersions: LatestVersions?
    @State private var startSheetDevice: SteerDevice?
    // Machine row actions (EXP-403/EXP-481): the settings-sheet target (Edit
    // — rename/sharing/defaults/worktrees live there now), the remove alert
    // target, the optimistic "Updating…" ids (the flag itself lands via
    // sync), and the shared failure caption.
    @State private var settingsTarget: DeviceSettingsTarget?
    @State private var removeTarget: SteerDevice?
    @State private var updatingIds: Set<String> = []
    @State private var deviceError: String?
    // Success feedback (informational, tertiary) vs. failure (red) are kept
    // separate: a start error must read as an error and not persist forever.
    // EXP-536: a remote start pushes the live session once the desktop's row
    // syncs in, instead of saying it'll show up in the list below. The watcher
    // owns the "waiting for the desktop" caption, the failure and the one-shot
    // navigation target.
    @State private var startWatcher = StartedRunWatcher()
    @State private var sessionTarget: StartedRunWatcher.StartedSession?
    // Merge (EXP-498: merging always closes the session), keyed by row id:
    // the confirm target, the in-flight rows, and the per-row failure caption
    // (inline like the Reviews rows, EXP-323 — never a modal the tab bar can
    // cover).
    @State private var mergeTarget: MergeTarget?
    @State private var merging: Set<String> = []
    @State private var mergeErrors: [String: String] = [:]
    // "Fix conflicts" (EXP-486, Reviews parity EXP-323): a refused merge is
    // usually a conflict, so the failing row's caption offers the builtin
    // recovery run on any reachable machine.
    @State private var fixTarget: FixConflictsTarget?

    /// The row a merge confirm is pending for. Only the ids are captured —
    /// the alert copy is fixed, and the row itself may re-sync underneath
    /// the alert.
    private struct MergeTarget: Identifiable {
        let rowId: String
        let issueId: String
        var id: String { rowId }
    }

    /// The row a "Fix conflicts" launch is pending for — the sheet preselects
    /// the builtin action with this row's PR already picked.
    private struct FixConflictsTarget: Identifiable {
        let rowId: String
        let issueId: String
        var id: String { rowId }
    }

    /// The machine a settings sheet is open for. EXP-490: the ID only — the
    /// sheet reads the LIVE devices-shape row itself, so a value captured here
    /// would only go stale under it.
    private struct DeviceSettingsTarget: Identifiable {
        let id: String
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
            await refreshLatestVersions()
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
        }
        .onChange(of: teamState.activeTeam?.id) { _, teamId in
            // EXP-432/EXP-481: the shared rows belong to the ACTIVE team —
            // the VM recomposes on the team switch.
            viewModel?.activeTeamId = teamId
        }
        .onDisappear {
            viewModel?.stopObserving()
            startWatcher.stop()
        }
        // The desktop picked the start up — push the live steer screen ONCE
        // (the same destination the .agentSession route arm builds).
        .onChange(of: startWatcher.startedSession) { _, started in
            if let started {
                startWatcher.startedSession = nil
                sessionTarget = started
            }
        }
        .navigationDestination(item: $sessionTarget) { target in
            AgentSessionRouteView(sessionId: target.sessionId)
                .environment(\.accountId, accountId)
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
                // EXP-481: the live inventory backs the resume offer.
                worktrees: viewModel?.worktrees,
                onStart: { chosenDevice, issueIds, options in
                    start(on: chosenDevice, issueIds: issueIds, options: options)
                },
                onRunAction: { chosenDevice, action, options, inputs in
                    runAction(on: chosenDevice, action: action, options: options, inputs: inputs)
                }
            )
        }
        .alert(
            "Merge pull request?",
            isPresented: Binding(
                get: { mergeTarget != nil },
                set: { if !$0 { mergeTarget = nil } }
            ),
            presenting: mergeTarget
        ) { target in
            Button("Merge", role: .destructive) { merge(target) }
            Button("Cancel", role: .cancel) { mergeTarget = nil }
        } message: { _ in
            Text("Merges the pull request, completes every linked issue, and closes the coding session.")
        }
    }

    // MARK: - My machines

    /// The machines a run can be sent to (EXP-403: the list itself includes
    /// offline rows; EXP-432: and teammates' shared servers). The sheet
    /// narrows this further — EXP-409 drops machines whose every installed
    /// agent is signed out.
    private var devices: [SteerDevice]? {
        viewModel?.devices
    }

    private var onlineDevices: [SteerDevice] {
        (devices ?? []).filter(\.isOnline)
    }

    /// The caller's own machines — the only ones with row actions.
    private var myDevices: [SteerDevice]? {
        devices?.filter(\.isMine)
    }

    /// EXP-432: teammates' servers shared with the active team — the VM
    /// composes them after the own rows.
    private var teamDevices: [SteerDevice] {
        devices?.filter { !$0.isMine } ?? []
    }

    /// EXP-481: the machines themselves stream off the devices shape — the
    /// only network read left is the latest-version hint (instance config),
    /// once per account.
    private func refreshLatestVersions() async {
        guard steerEnabled else { return }
        let result = try? await deps.devicesApi.list(
            accountId: accountId, teamId: teamState.activeTeam?.id
        )
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
                if let myDevices {
                    if myDevices.isEmpty {
                        deviceHintRow
                    } else {
                        ForEach(myDevices) { deviceRow($0) }
                    }
                } else {
                    deviceLoadingRow
                }

                // EXP-432: teammates' shared servers, grouped below the
                // caller's own. Absent entirely when nothing is shared.
                if !teamDevices.isEmpty {
                    sectionHeader("Team machines")
                    ForEach(teamDevices) { deviceRow($0) }
                }
                if let deviceError {
                    Text(deviceError)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .padding(.horizontal, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                if let sentCaption = startWatcher.sentCaption {
                    Text(sentCaption)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.horizontal, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                if let startError = startWatcher.failure {
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
            // Its own node, NOT the ScrollView (that one owns the settings
            // sheet + remove alert) and NOT the body ZStack (the start sheet
            // + merge alert) — same one-presentation-per-node rule as below.
            .sheet(item: $fixTarget) { target in
                StartCodingSheet(
                    devices: onlineDevices,
                    issues: viewModel?.startCandidates(teamId: teamState.activeTeam?.id) ?? [],
                    preselectedIds: [],
                    teamId: teamState.activeTeam?.id,
                    initialTab: .actions,
                    preselectedActionId: DomainContract.builtinFixConflictsId,
                    preselectedPrIssueId: target.issueId,
                    worktrees: viewModel?.worktrees,
                    onStart: { chosenDevice, issueIds, options in
                        start(on: chosenDevice, issueIds: issueIds, options: options)
                    },
                    onRunAction: { chosenDevice, action, options, inputs in
                        runAction(on: chosenDevice, action: action, options: options, inputs: inputs)
                    }
                )
            }
        }
        // Clearance for the floating tab bar (EXP-36).
        .tabBarBottomInset()
        // EXP-481: Edit opens the device settings sheet (name, sharing, agent
        // defaults, worktrees) — the row menu's rename alert retired into it.
        // EXP-490: it takes the view model and the device id, not a snapshot —
        // the sheet renders the live row and auto-saves.
        .sheet(item: $settingsTarget) { target in
            if let viewModel {
                DeviceSettingsSheet(
                    viewModel: viewModel,
                    deviceId: target.id,
                    teams: teamState.teams
                )
            }
        }
        // The machine alert hangs off THIS view, not the body's ZStack: that
        // one already owns the merge-and-close alert, and stacking several
        // `.alert`s on one node is where SwiftUI starts dropping presentations.
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
                    // EXP-622: the machine every device picker prefills.
                    if device.isDefaultDevice {
                        AppIcon(AppIcons.uiDeviceDefault, size: AppIcon.Size.small)
                            .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                            .accessibilityLabel("Default machine")
                    }
                    // EXP-432: a teammate's machine is attributed to its owner;
                    // one of the caller's own that is shared just says so (the
                    // share toggle itself is web-only).
                    if let owner = device.owner {
                        Text("shared by \(owner.name)")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                            .lineLimit(1)
                    } else if device.sharedTeamId != nil {
                        Text("Shared")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.quaternary))
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
                // EXP-615: the play glyph, not a "Start coding" pill — the
                // same affordance web and desktop wear on their machine rows.
                CircleIconButton(AppIcons.actionRun, accessibilityLabel: "Start coding") {
                    startSheetDevice = device
                }
            }

            // EXP-432: rename / remove / update are OWN-machine actions —
            // a teammate's shared server is startable but not manageable.
            if device.isMine, device.isRegistered {
                GlassMenu {
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

    /// Row actions for a registered machine (EXP-481: Edit opens the device
    /// settings sheet — name, sharing, agent defaults, worktrees). Self-update
    /// is a daemon-server affordance only (the desktop app updates itself),
    /// and it needs the machine online to pick the request up.
    @ViewBuilder
    private func deviceMenu(_ device: SteerDevice) -> some View {
        GlassMenuItem("Edit", icon: AppIcons.uiEdit) {
            settingsTarget = DeviceSettingsTarget(id: device.deviceId)
        }
        // EXP-420: offered only when a newer CLI version really exists.
        if device.isServer, device.isOnline, !isUpdating(device),
            device.updateAvailable(latest: latestVersions?.cli)
        {
            GlassMenuItem("Update", icon: AppIcons.uiUpdate) {
                requestUpdate(device)
            }
        }
        GlassMenuItem("Remove", icon: AppIcons.uiDelete, destructive: true) {
            removeTarget = device
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

    /// EXP-481: outcomes land via sync (the devices shape), so the handlers
    /// only report failures — no poll refresh to force.
    private func remove(_ device: SteerDevice) {
        removeTarget = nil
        deviceError = nil
        Task {
            do {
                try await deps.devicesApi.remove(accountId: accountId, deviceId: device.deviceId)
            } catch {
                deviceError = error.localizedDescription
            }
        }
    }

    /// Ask a daemon server to self-update. The row keeps its "Updating…"
    /// state until the daemon re-registers (which clears the flag
    /// server-side), which sync delivers.
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
                mergeErrorCaption(row, message: message)
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

            // Merge shortcut — merging always closes the run too (EXP-498),
            // so it only shows while there IS an open PR to merge. EXP-535:
            // batch rows merge through their resolved PR's representative
            // issue — same button, same server call (the server resolves a
            // batch PR to EVERY linked issue by exact pr_url).
            if let prIssue = row.issue ?? row.batchPrIssue,
                prIssue.prState == DomainContract.prStateOpen
            {
                Button {
                    mergeTarget = MergeTarget(rowId: row.id, issueId: prIssue.id)
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
                .accessibilityLabel("Merge")
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

    /// A refused merge (conflicts, branch protection, GitHub App errors)
    /// captions THIS row — and a conflict is the common case, so the builtin
    /// recovery run sits right next to the reason (EXP-486, the same shape as
    /// the Reviews rows, EXP-323).
    @ViewBuilder
    private func mergeErrorCaption(_ row: AgentsViewModel.Row, message: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(message)
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
                .fixedSize(horizontal: false, vertical: true)

            // EXP-535: a batch row's refused merge recovers through the same
            // representative issue its Merge button used — the sheet's PR
            // picker normalizes any linked issue id to its option.
            if let issue = row.issue ?? row.batchPrIssue, canFixConflicts(issue) {
                GlassPillButton("Fix conflicts", icon: AppIcons.uiBranch) {
                    fixTarget = FixConflictsTarget(rowId: row.id, issueId: issue.id)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The recovery run rebases the PR's branch, so it needs one recorded —
    /// the same gate the Reviews rows apply. The caption already implies a
    /// failed merge, so only steer + branch remain to check.
    private func canFixConflicts(_ issue: IssueEntity) -> Bool {
        steerEnabled && !(issue.branch ?? "").isEmpty
    }

    /// Merge the row's PR — the server always ends its session too (EXP-498;
    /// `closeSessions: true` stays on the wire for old-server compat). No
    /// local list surgery: the server flips the row to `ended`, which drops
    /// it out of the live query through sync. A refusal (conflicts, branch
    /// protection) captions THIS row.
    private func merge(_ target: MergeTarget) {
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
        // EXP-550: the host machine stopped heartbeating (lid closed) — the
        // run is PAUSED, not ended, and resumes when the machine returns. A
        // pulsing "coding now" dot would be a lie, so it goes neutral.
        let paused = row.device.isPaused(state)
        HStack(spacing: 12) {
            if paused || state != .running {
                Circle()
                    .fill(paused ? DesignTokens.Semantic.neutral : stateColor(state))
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
                    if paused {
                        Text("Paused")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                    } else if let label = stateLabel(state) {
                        Text(label)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(stateColor(state))
                            .lineLimit(1)
                    }
                    Text(byline(row, paused: paused))
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

    /// EXP-549: the machine name comes from the LIVE devices row (a rename
    /// never rewrites the session's start-time snapshot). EXP-550: a paused
    /// row says WHY instead of how long ago it started.
    private func byline(_ row: AgentsViewModel.Row, paused: Bool) -> String {
        let device = row.device.displayLabel
        if paused { return "\(device) · offline" }
        let started = relativeDate(row.session.startedAt)
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
        guard let key = StartedRunKey.forIssues(issueIds) else { return }
        startWatcher.sending()
        Task {
            do {
                if issueIds.count > 1 {
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
                startWatcher.begin(
                    key: key,
                    userId: deps.auth.userId,
                    device: device,
                    db: deps.db,
                    accountId: accountId
                )
            } catch {
                startWatcher.failed(error.localizedDescription)
            }
        }
    }

    /// Actions-mode launch from the unified sheet (EXP-257): full option set,
    /// typed input values, `teamId` only for the builtin "Create action" (its
    /// virtual row has no server-resolvable id). EXP-536: like an issue start,
    /// the run is pushed as soon as its synced row lands.
    private func runAction(
        on device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: [String: String]
    ) {
        startWatcher.sending()
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
                startWatcher.begin(
                    key: .action(name: action.name),
                    userId: deps.auth.userId,
                    device: device,
                    db: deps.db,
                    accountId: accountId
                )
            } catch {
                startWatcher.failed(error.localizedDescription)
            }
        }
    }
}
