import ExpUI
import ExpCore
import SwiftUI

/// The Devices tab (EXP-686, formerly Agents — the type names stay):
/// "My machines" — the caller's registered devices (EXP-403:
/// desktops AND headless `exponential` daemon servers, online or not — since
/// EXP-481 read from the synced `devices` shape, online-ness derived from
/// last_seen_at freshness) with a per-machine play glyph and an Edit (device
/// settings sheet: name, sharing, agent defaults, worktrees) / self-update /
/// remove row menu — then "Team machines" (EXP-432: teammates' servers shared
/// with the active team, startable but never manageable here).
///
/// EXP-825: machines ONLY (web parity, EXP-818). The Running/Past sessions
/// moved to the Agent page, which is also the ONE launcher: a machine's play
/// glyph pushes it with that machine preselected, and the tab bar's Chat FAB
/// pushes it with an empty seed. When the relay is off nothing here can be
/// started, so the tab says so instead of listing machines.
///
/// EXP-829: below the machines, "Accounts" (`AgentAccountsSection`) — the
/// Usage page web and desktop folded into Devices in EXP-818: one row per
/// agent account across the same machines, the freshest report's usage
/// bars, a chip per machine that opens the device settings sheet.
struct AgentsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(TeamState.self) private var teamState
    @State private var viewModel: AgentsViewModel?
    /// nil until the relay config resolves.
    @State private var steerEnabled: Bool?
    /// EXP-420: the instance's advertised latest versions — gates the
    /// server rows' Update action on an actually-newer CLI build. The one
    /// remaining tRPC read here (instance config, not a shape column):
    /// fetched once per account instead of polled.
    @State private var latestVersions: LatestVersions?
    // Machine row actions (EXP-403/EXP-481): the settings-sheet target (Edit
    // — rename/sharing/defaults/worktrees live there now), the remove alert
    // target, the optimistic "Updating…" ids (the flag itself lands via
    // sync), and the shared failure caption.
    @State private var settingsTarget: DeviceSettingsTarget?
    @State private var removeTarget: SteerDevice?
    @State private var updatingIds: Set<String> = []
    @State private var deviceError: String?

    /// The machine a settings sheet is open for. EXP-490: the ID only — the
    /// sheet reads the LIVE devices-shape row itself, so a value captured here
    /// would only go stale under it.
    private struct DeviceSettingsTarget: Identifiable {
        let id: String
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel, let steerEnabled {
                if steerEnabled {
                    machinesContent(vm)
                } else {
                    relayOffState
                }
            }
        }
        .navigationTitle("Devices")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
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
            // The list is scoped to the active team — the VM observes the
            // account's rows, the view owns the team.
            viewModel?.activeTeamId = teamState.activeTeam?.id
            // EXP-829: the Accounts section queues its usage refreshes from
            // this page only.
            viewModel?.devicesApi = deps.devicesApi
            // Re-arm on every appear: pushing a detail stops the observation
            // (onDisappear), popping back must resume it.
            viewModel?.startObserving()
        }
        .onChange(of: teamState.activeTeam?.id) { _, teamId in
            // EXP-432/EXP-481: the shared rows belong to the ACTIVE team —
            // the VM recomposes on the team switch.
            viewModel?.activeTeamId = teamId
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
    }

    // MARK: - My machines

    /// The machines list (EXP-403: offline rows included; EXP-432: and
    /// teammates' shared servers).
    private var devices: [SteerDevice]? {
        viewModel?.devices
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
        guard steerEnabled == true else { return }
        let result = try? await deps.devicesApi.latestVersions(accountId: accountId)
        latestVersions = result ?? latestVersions
    }

    /// Web parity: without the relay there is nothing to start on, and the
    /// machines list would only ever be decorative.
    private var relayOffState: some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.navDevices, size: 28)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("Remote start isn't available on this server")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Machines and remote starts need the steer relay. Live sessions still show up on the Agent page.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
    }

    // MARK: - Machines content (relay on)

    @ViewBuilder
    private func machinesContent(_ vm: AgentsViewModel) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 8) {
                GlassSectionHeader("My machines")
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
                    GlassSectionHeader("Team machines")
                    ForEach(teamDevices) { deviceRow($0) }
                }

                // EXP-829: the agent accounts across those machines (web /
                // desktop EXP-818 parity). A chip opens the machine's
                // settings sheet — the same target as the row menu's Edit.
                AgentAccountsSection(viewModel: vm) { deviceId in
                    settingsTarget = DeviceSettingsTarget(id: deviceId)
                }
                if let deviceError {
                    Text(deviceError)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .padding(.horizontal, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding()
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
        // The machine alert hangs off the ScrollView's own node — one
        // presentation per node, or SwiftUI starts dropping them.
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
                // EXP-825: it pushes the Agent page with THIS machine picked.
                CircleIconButton(AppIcons.actionRun, accessibilityLabel: "Start coding") {
                    pushRoute(.agent(
                        accountId: accountId,
                        seed: AgentComposerSeed(deviceId: device.deviceId)
                    ))
                }
            }

            // EXP-432: rename / remove / update are OWN-machine actions —
            // a teammate's shared server is startable but not manageable.
            if device.isMine, device.isRegistered {
                GlassMenu {
                    deviceMenu(device)
                } label: {
                    CircleIconLabel(AppIcons.uiMore)
                }
                .accessibilityLabel("Machine actions")
                .accessibilityIdentifier("machine-menu")
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
            Text("No machines yet. Open the Exponential desktop app, or add a device on the web.")
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
                deviceError = error.userFacingMessage
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
                deviceError = error.userFacingMessage
            }
            updatingIds.remove(device.deviceId)
        }
    }

    private func relativeDate(_ s: String) -> String {
        // Electric syncs timestamps as Postgres text (space separator,
        // hour-only offset), which ISO8601DateFormatter alone rejects —
        // WireTimestamps handles both wire forms (EXP-169).
        guard let date = WireTimestamps.parse(s) else { return "" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}
