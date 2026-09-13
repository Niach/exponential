import ExpUI
import ExpCore
import SwiftUI

/// The Devices tab (EXP-686, formerly Agents — the type names stay):
/// "My devices" — the caller's registered devices (EXP-403: desktops AND
/// headless `exponential` daemon servers, online or not — since EXP-481 read
/// from the synced `devices` shape, online-ness derived from last_seen_at
/// freshness) with a per-device play glyph and a "…" menu (Device settings /
/// self-update / remove) — then "Team devices" (EXP-432: teammates' servers
/// shared with the active team, startable but never manageable here).
///
/// EXP-825: machines ONLY (web parity, EXP-818). The Running/Past sessions
/// moved to the Agent page, which is also the ONE launcher: a device's play
/// glyph pushes it with that device preselected, and the tab bar's Chat FAB
/// pushes it with an empty seed. When the relay is off nothing here can be
/// started, so the tab says so instead of listing devices.
///
/// EXP-829: below the devices, "Accounts" (`AgentAccountsSection`) — the Usage
/// page web and desktop folded into Devices in EXP-818: one row per agent
/// account across the same devices, the freshest report's usage bars, per-agent
/// tabs, and a chip per device holding the login.
///
/// EXP-849 splits the two jobs this page used to mix. Accounts (below) is the
/// DECISION surface — which login, how much is left, is it still good. The
/// device rows are the SETUP/REPAIR surface: each one badges the worst health
/// of its logins ("Needs re-login" is not "Signed out") and carries its account
/// chips, whose menu signs a login in, makes one the device's default, or
/// removes it from that device (EXP-862).
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
    /// EXP-862: the sign-in a chip (or the Accounts section) asked for, and
    /// the removal a chip is confirming.
    @State private var loginTarget: AgentLoginTarget?
    @State private var removeAccountTarget: AccountRemoveTarget?

    /// A pending "Remove account": the login the confirm names. Captured as a
    /// value so the sentence stays put even as the rows re-sync underneath.
    private struct AccountRemoveTarget: Identifiable {
        let row: AgentProfileUsageRow
        var id: String { row.key }
    }

    /// The machine a settings sheet is open for. EXP-490: the ID only — the
    /// sheet reads the LIVE devices-shape row itself, so a value captured here
    /// would only go stale under it. EXP-862 took the agent/profile hop back
    /// out: a sign-in is its own sheet (`AgentLoginSheet`), not a tab of the
    /// machine's settings.
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
        // EXP-818: the page is a TABLE now — a filled group band per group with
        // its flat rows hanging straight off it (`GlassSectionBand` +
        // `.flatRow()`), so the stack of bordered cards this used to be reads
        // as one list.
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 0) {
                    GlassSectionBand("My devices")
                    if let myDevices {
                        if myDevices.isEmpty {
                            deviceHintRow
                        } else {
                            ForEach(myDevices) { deviceRow(vm, $0) }
                        }
                    } else {
                        deviceLoadingRow
                    }
                }

                // EXP-432: teammates' shared servers, grouped below the
                // caller's own. Absent entirely when nothing is shared.
                if !teamDevices.isEmpty {
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand("Team devices")
                        ForEach(teamDevices) { deviceRow(vm, $0) }
                    }
                }

                // EXP-829: the agent accounts across those devices (web /
                // desktop EXP-818 parity). A login is REPAIRED on the device
                // row that holds it; what this section owns is adding one —
                // "+ Add account" and the per-account "+" (EXP-862).
                AgentAccountsSection(
                    viewModel: vm,
                    onSignIn: { loginTarget = $0 },
                    onRemove: { removeAccountTarget = AccountRemoveTarget(row: $0) }
                )
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
        // Clearance for the floating tab bar (EXP-36) — on the SCROLLER
        // itself, so its content inset is the one that grows.
        .tabBarBottomInset()
        // EXP-481: "Device settings" opens the sheet (name, default device,
        // sharing, launch defaults, worktrees) — the row menu's rename alert
        // retired into it. EXP-490: it takes the view model and the device id,
        // not a snapshot — the sheet renders the live row and auto-saves.
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
            "Remove device",
            isPresented: Binding(
                get: { removeTarget != nil },
                set: { if !$0 { removeTarget = nil } }
            ),
            presenting: removeTarget
        ) { device in
            Button("Cancel", role: .cancel) { removeTarget = nil }
            Button("Remove", role: .destructive) { remove(device) }
        } message: { device in
            Text("Remove “\(deviceName(device))” from your devices? A device with the daemon still running will re-register itself on its next heartbeat.")
        }
        // One presentation per node is the rule, so the login sheet and the
        // account confirm hang off a zero-size node of their own.
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .sheet(item: $loginTarget) { target in
                    if let viewModel {
                        AgentLoginSheet(viewModel: viewModel, target: target)
                    }
                }
        )
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .alert(
                    "Remove account?",
                    isPresented: Binding(
                        get: { removeAccountTarget != nil },
                        set: { if !$0 { removeAccountTarget = nil } }
                    ),
                    presenting: removeAccountTarget
                ) { target in
                    Button("Cancel", role: .cancel) { removeAccountTarget = nil }
                    Button("Remove", role: .destructive) {
                        removeAccountTarget = nil
                        viewModel?.removeAccount(target.row)
                    }
                } message: { target in
                    // The pinned sentence ×4: it names the login and the
                    // machine, and says the account itself survives.
                    Text(AgentAccountsRows.removeAccountConfirmCopy(
                        account: target.row.email ?? target.row.profileLabel,
                        device: target.row.deviceLabel.isEmpty
                            ? target.row.deviceId
                            : target.row.deviceLabel
                    ))
                }
        )
    }

    /// One machine: kind glyph, label + version, live/last-seen state, the
    /// launcher for online ones, and an overflow menu. The menu is an explicit
    /// trailing control rather than a long-press context menu (EXP-331: the
    /// same reason the label rows grew one) and it only appears on registered
    /// rows — a desktop build predating the registry shows up from relay
    /// presence alone and has nothing to rename or remove.
    private func deviceRow(_ vm: AgentsViewModel, _ device: SteerDevice) -> some View {
        VStack(alignment: .leading, spacing: 8) {
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
                                .accessibilityLabel("Default device")
                        }
                        // EXP-432: a teammate's machine is attributed to its owner;
                        // one of the caller's own that is shared with any team just
                        // says so (the per-team toggles live in the settings sheet).
                        if let owner = device.owner {
                            Text("shared by \(owner.name)")
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                                .lineLimit(1)
                        } else if !device.sharedTeamIds.isEmpty {
                            Text("Shared")
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                                .lineLimit(1)
                        }
                        // EXP-849: the worst health among this machine's logins —
                        // a refused credential is its own state, not "signed out".
                        healthBadge(vm, device)
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
                        GhostIconLabel(AppIcons.uiMore)
                    }
                    .accessibilityLabel("Device menu")
                    .accessibilityIdentifier("machine-menu")
                }
            }
            // EXP-849/EXP-862: the machine's logins, with the repairs on
            // them. A sign-in opens the login sheet on the chip's OWN login
            // (a machine with two claude profiles would otherwise re-login the
            // active one, not the expired one that was tapped); a removal
            // confirms first.
            DeviceAccountChips(
                viewModel: vm,
                device: device,
                onSignIn: { row in
                    loginTarget = AgentLoginTarget(
                        deviceId: device.deviceId,
                        deviceLabel: deviceName(device),
                        agent: row.agent,
                        profileId: row.profileId
                    )
                },
                onRemove: { removeAccountTarget = AccountRemoveTarget(row: $0) }
            )
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
    }

    /// EXP-849: the machine's health badge — the worst of its reported logins,
    /// and nothing at all when they are fine or were never probed.
    @ViewBuilder
    private func healthBadge(_ vm: AgentsViewModel, _ device: SteerDevice) -> some View {
        if let label = vm.deviceHealth(device.deviceId).badgeLabel {
            Text(label)
                .font(.caption2)
                .foregroundStyle(DesignTokens.Semantic.yellow)
                .lineLimit(1)
                .accessibilityIdentifier("device-health-\(device.deviceId)")
        }
    }

    /// A requested self-update REPLACES the live state: the daemon is about to
    /// restart, so "Online" would only read as a lie — unless the update is
    /// parked behind live coding sessions (EXP-411): then the row says
    /// "Update queued" without a spinner instead of "Updating…" forever.
    ///
    /// EXP-862: nothing here says anything about SIGN-INS any more. A login's
    /// state is said once, on the chip that owns it (and summarised by the
    /// row's health badge) — the line used to repeat it as "codex not signed
    /// in" beside a chip already wearing the badge.
    @ViewBuilder
    private func deviceStatusLine(_ device: SteerDevice) -> some View {
        HStack(spacing: 5) {
            if isUpdateQueued(device) {
                Text("Update queued")
            } else if isUpdating(device) {
                ProgressView().controlSize(.mini).tint(.white)
                Text("Updating…")
            } else if device.isOnline {
                Circle()
                    .fill(DesignTokens.Semantic.green)
                    .frame(width: 6, height: 6)
                Text("Online")
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
        // EXP-862: "Device settings" + the settings gear ×4 — "Edit" with a
        // pencil promised an inline rename, not the sheet it opens.
        GlassMenuItem("Device settings", icon: AppIcons.navSettings) {
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
            // EXP-317: the same glyph the web draws on its empty devices row
            // (`ui-device-offline`); `ui-offline` stays the network indicator.
            AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.small)
            // Web puts its install one-liner behind this row; a phone can't
            // run it, so mobile points at the surface that can (Android says
            // the same thing, word for word).
            Text("No devices yet. Open the Exponential desktop app, or add a device on the web.")
                .font(.caption)
            Spacer(minLength: 0)
        }
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
    }

    private var deviceLoadingRow: some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small).tint(.white)
            Text("Checking for devices…")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
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
