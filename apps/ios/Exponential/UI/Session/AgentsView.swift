import ExpUI
import ExpCore
import SwiftUI

/// The Devices tab (EXP-686, formerly Agents — the type names stay):
/// "My devices" — the caller's registered devices (EXP-403: desktops AND
/// headless `exponential` daemon servers, online or not — since EXP-481 read
/// from the synced `devices` shape, online-ness derived from last_seen_at
/// freshness) — then "Team devices" (EXP-432: teammates' servers shared with
/// the active team, readable but never manageable here).
///
/// EXP-909: a device row carries exactly ONE control ×4, the settings GEAR
/// that opens `DeviceSettingsSheet`, and only on the caller's own registered
/// rows. The play glyph is gone (starting a run is the Agent page composer's
/// device picker, one launcher and one place to pick a machine) and so is the
/// "…" menu: Update and Remove are the settings sheet's last two sections, so
/// a row never has to explain which of three controls does what.
///
/// EXP-825: machines ONLY (web parity, EXP-818). The Running/Recent sessions
/// moved to the Agent page, which is also the ONE launcher; the tab bar's Chat
/// FAB pushes it with an empty seed. When the relay is off nothing here can be
/// started, so the tab says so instead of listing devices.
///
/// EXP-909: every device row LISTS ITS OWN LOGINS beneath it (`DeviceLogins`)
/// — who each login is, its health, its compact usage line, and (own machines
/// only) the menu that signs it in, makes it that machine's default, or
/// removes it. The separate cross-device "Accounts" section is gone: it
/// email-merged logins that are per MACHINE, so one account lived in two
/// places with two orderings and two menus. A login belongs to the machine
/// that holds it, and that is where it now reads.
struct AgentsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @State private var viewModel: AgentsViewModel?
    /// nil until the relay config resolves.
    @State private var steerEnabled: Bool?
    /// EXP-909: the row's ONE action — the settings sheet it opens (name,
    /// default device, sharing, launch defaults, worktrees, and now update +
    /// remove, which used to be row-level handlers here).
    @State private var settingsTarget: DeviceSettingsTarget?
    /// EXP-862: the sign-in a chip (or the Accounts section) asked for, and
    /// the removal a chip is confirming.
    @State private var loginTarget: AgentLoginTarget?
    @State private var removeAccountTarget: AccountRemoveTarget?
    /// EXP-944: devices COLLAPSE. The list answers "which machines do I have
    /// and are they up" first; a machine's logins, their usage bars and its
    /// "Add account" are the second question, and three machines' worth of
    /// them made the first one unreadable. VIEW state: a fold is not a setting.
    @State private var expandedDeviceIds: Set<String> = []
    /// EXP-944: the clock the login rows' reset countdowns age on — the same
    /// 30s beat the "as of …" captions already use.
    @State private var now = Date()

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
        }
        // EXP-944: the 30s beat the login rows' "as of …" captions and their
        // reset countdowns age on — a countdown that never moves is a stamp.
        .task {
            while !Task.isCancelled {
                now = Date()
                try? await Task.sleep(for: .seconds(30))
            }
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
            }
            .padding()
        }
        // Clearance for the floating tab bar (EXP-36) — on the SCROLLER
        // itself, so its content inset is the one that grows.
        .tabBarBottomInset()
        // EXP-481: the gear opens the sheet (name, default device, sharing,
        // launch defaults, worktrees) — the row menu's rename alert retired
        // into it, and EXP-909 its update and remove actions too. EXP-490: it
        // takes the view model and the device id, not a snapshot — the sheet
        // renders the live row and auto-saves.
        .sheet(item: $settingsTarget) { target in
            if let viewModel {
                DeviceSettingsSheet(
                    viewModel: viewModel,
                    deviceId: target.id,
                    teams: teamState.teams
                )
            }
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

    /// One machine: kind glyph, label + version, live/last-seen state, its
    /// logins — and, on the caller's own registered rows, the settings gear.
    /// EXP-909: that gear is the row's ONLY control. It is an explicit trailing
    /// button rather than a long-press context menu (EXP-331: the same reason
    /// the label rows grew one), and it skips unregistered rows — a desktop
    /// build predating the registry shows up from relay presence alone and has
    /// nothing to rename, update or remove.
    private func deviceRow(_ vm: AgentsViewModel, _ device: SteerDevice) -> some View {
        let expanded = expandedDeviceIds.contains(device.deviceId)
        // EXP-944: TOP-aligned — the device glyph and the gear stay level with
        // the device NAME instead of centring themselves against the block
        // under it.
        return VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 12) {
                // Everything but the gear folds the row.
                HStack(alignment: .top, spacing: 12) {
                    // The fold chevron — the same affordance the run tree uses.
                    // The whole leading block is the target, so this is an
                    // affordance rather than a second button.
                    AppIcon(
                        expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight,
                        size: AppIcon.Size.small
                    )
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                    // EXP-924: the owner's pick, else the kind default.
                    AppIcon(
                        DeviceIconDisplay.iconName(for: device),
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
                }
                .contentShape(Rectangle())
                .onTapGesture { toggleDeviceFold(device.deviceId) }
                // One button named after the machine — the fold is a tap
                // target, so VoiceOver reads it as one and says what it opens.
                .accessibilityElement(children: .combine)
                .accessibilityAddTraits(.isButton)
                .accessibilityLabel(device.deviceLabel)
                .accessibilityIdentifier("device-row-\(device.deviceId)")

                // EXP-909: the settings gear, and nothing else. Starting a run
                // is the Agent page composer's device picker (the ONE launcher),
                // so no play glyph; rename / defaults / update / remove all live
                // one tap away in the sheet. EXP-432: own machines only — a
                // teammate's shared server is startable from the composer but
                // never manageable here, so its row carries no control at all.
                if device.isMine, device.isRegistered {
                    GhostIconButton(
                        AppIcons.navSettings,
                        accessibilityLabel: "Device settings"
                    ) {
                        settingsTarget = DeviceSettingsTarget(id: device.deviceId)
                    }
                    .accessibilityIdentifier("machine-settings")
                }
            }
            // EXP-849/EXP-862/EXP-909: the machine's logins, with the repairs
            // on them. A sign-in opens the login sheet on the row's OWN login
            // (a machine with two claude profiles would otherwise re-login the
            // active one, not the expired one that was tapped); a removal
            // confirms first. A teammate's shared server renders read-only.
            // EXP-944: they live in the FOLD now, inset under the device name,
            // and "Add account" with them.
            if expanded {
                DeviceLogins(
                    viewModel: vm,
                    device: device,
                    now: now,
                    readOnly: !device.isMine,
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
                .padding(.leading, 28)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
    }

    /// EXP-944: fold one machine open or shut. A set, not a single id: two
    /// machines being compared stay open together.
    private func toggleDeviceFold(_ deviceId: String) {
        if expandedDeviceIds.remove(deviceId) == nil {
            expandedDeviceIds.insert(deviceId)
        }
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

    private func deviceName(_ device: SteerDevice) -> String {
        device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
    }

    /// The pending flag rides the server row until the daemon re-registers
    /// (EXP-909: the optimistic gap is the settings sheet's business now — the
    /// list only ever renders what sync says).
    private func isUpdating(_ device: SteerDevice) -> Bool {
        device.updateRequested == true
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
