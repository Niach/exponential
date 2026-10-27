import ExpUI
import ExpCore
import GRDB
import SwiftUI

/// The Devices tab (EXP-686, formerly Agents — the type names stay):
/// "My devices" — the caller's registered devices (EXP-403: desktops AND
/// headless `exponential` daemon servers, online or not — since EXP-481 read
/// from the synced `devices` shape, online-ness derived from last_seen_at
/// freshness) — then "Team devices" (EXP-432: teammates' servers shared with
/// ANY of the caller's teams — EXP-1186: cross-team like the Inbox, each
/// server once — readable but never manageable here).
///
/// EXP-909: a device row carries exactly ONE control ×4, the settings GEAR
/// that opens `DeviceSettingsSheet`, and only on the caller's own registered
/// rows. The play glyph is gone (starting a run is the Agent page composer's
/// device picker, one launcher and one place to pick a machine) and so is the
/// "…" menu: Update and Remove are the settings sheet's last two sections, so
/// a row never has to explain which of three controls does what.
/// FEED-149: a team OWNER's Team devices rows shared into the CURRENT team
/// carry one control in the gear's slot — Remove, which drops that teammate's
/// share of the machine from this team (`devices.dropShare`).
///
/// EXP-825: machines ONLY (web parity, EXP-818) — sessions live in the
/// Threads list. EXP-1295: a start is a `sessions` row the machine picks up
/// from its shapes, so the list never waits on the steer relay.
///
/// EXP-909: every device row LISTS ITS OWN LOGINS (`DeviceLogins`) — since
/// A-16 (`devices-list.json`) as one wrapping row of chips on the device's
/// line, each opening (own machines only) the menu that signs it in, signs it
/// out, or removes it, under its limit bars; a row's trouble (dead logins, an
/// update) is ONE attention icon, never inline text. The separate cross-device "Accounts" section is gone: it
/// email-merged logins that are per MACHINE, so one account lived in two
/// places with two orderings and two menus. A login belongs to the machine
/// that holds it, and that is where it now reads.
struct AgentsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @State private var viewModel: AgentsViewModel?
    /// EXP-909: the row's ONE action — the settings sheet it opens (name,
    /// default device, sharing, launch defaults, and now update + remove,
    /// which used to be row-level handlers here).
    @State private var settingsTarget: DeviceSettingsTarget?
    /// EXP-862: the sign-in a chip (or the Accounts section) asked for, and
    /// the removal a chip is confirming.
    @State private var loginTarget: AgentLoginTarget?
    @State private var removeAccountTarget: AccountRemoveTarget?
    /// EXP-1137: the sign-out a chip is confirming.
    @State private var signOutAccountTarget: AccountRemoveTarget?
    /// EXP-1169: the band's "Add device" sheet.
    @State private var showAddDevice = false
    /// A-16: the instance's latest client builds — the rows' version compare
    /// (outdated tone, the attention card's update line). One read per visit.
    @State private var latestVersions: LatestVersions?
    /// FEED-149: the caller owns the current team — its Team devices rows
    /// offer Remove. Resolved off the synced `team_members` row.
    @State private var isTeamOwner = false
    /// FEED-149: the teammate's shared row a team owner is removing.
    @State private var dropShareTarget: DropShareTarget?
    @Environment(\.toaster) private var toaster

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

    /// FEED-149: a pending "Remove from this team": the synced devices ROW id
    /// (a legacy duplicate shares the machine id), the team it leaves and the
    /// name the confirm says — captured so the sentence stays put as the rows
    /// re-sync.
    private struct DropShareTarget: Identifiable {
        let rowId: String
        let teamId: String
        let name: String
        var id: String { rowId }
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                machinesContent(vm)
            }
        }
        // #EXP-1384: pushed from the Team page's Devices row, under the
        // page header (the back control before the title).
        .pageHeader(Chats.Copy.Headers.devices, pushed: true)
        // A-16: the version compare's other side — instance config, read
        // once per visit, never polled.
        .task {
            guard latestVersions == nil else { return }
            latestVersions = try? await deps.devicesApi.latestVersions(accountId: accountId)
        }
        .onAppear {
            if viewModel == nil {
                viewModel = AgentsViewModel(
                    accountId: accountId, userId: deps.auth.userId, db: deps.db
                )
            }
            // EXP-1186: the list spans every member team — the VM observes
            // the account's rows, the view hands it the team set.
            viewModel?.activeTeamId = teamState.activeTeam?.id
            viewModel?.deviceTeamIds = memberTeamIds
            // EXP-829: the Accounts section queues its usage refreshes from
            // this page only.
            viewModel?.devicesApi = deps.devicesApi
            // Re-arm on every appear: pushing a detail stops the observation
            // (onDisappear), popping back must resume it.
            viewModel?.startObserving()
        }
        .onChange(of: memberTeamIds) { _, teamIds in
            // EXP-432/EXP-1186: the shared rows belong to the member teams —
            // the VM recomposes when one is joined or left.
            viewModel?.deviceTeamIds = teamIds
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
        // FEED-149: re-resolved whenever the current team changes.
        .task(id: teamState.activeTeamId) {
            await resolveTeamOwner()
        }
    }

    /// FEED-149: owners own the destructive surface (`resolveTeamAccess`) —
    /// only they drop a teammate's share; the server refuses everyone else.
    private func resolveTeamOwner() async {
        guard let teamId = teamState.activeTeamId,
              let userId = deps.auth.userId,
              let pool = try? deps.db.pool(forAccountId: accountId) else {
            isTeamOwner = false
            return
        }
        isTeamOwner = (try? await pool.read { db in
            try TeamMemberEntity
                .filter(Column("team_id") == teamId && Column("user_id") == userId)
                .fetchOne(db)?.role == DomainContract.teamRoleOwner
        }) ?? false
    }

    /// FEED-149: the team a teammate's row can be removed from — the current
    /// one, for its owner, when the row is shared into it and is a synced
    /// registry row (`rowId`). nil = the row carries no control.
    private func dropShareTeamId(_ device: SteerDevice) -> String? {
        guard isTeamOwner, !device.isMine, device.rowId != nil,
              let teamId = teamState.activeTeamId,
              device.sharedTeamIds.contains(teamId) else { return nil }
        return teamId
    }

    private func dropShare(_ target: DropShareTarget) {
        let api = deps.devicesApi
        let account = accountId
        // The synced shape removes the row; a refusal is the failed-action
        // toast, the same one the settings sheet's Remove device raises.
        Task {
            do {
                try await api.dropShare(accountId: account, id: target.rowId, teamId: target.teamId)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "remove", thing: "the device"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    /// EXP-1186: every team the caller is a member of (the synced teams).
    private var memberTeamIds: Set<String> {
        Set(teamState.teams.map(\.id))
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

    /// EXP-432: teammates' servers shared with any member team — the VM
    /// composes them after the own rows.
    private var teamDevices: [SteerDevice] {
        devices?.filter { !$0.isMine } ?? []
    }

    // MARK: - Machines content

    @ViewBuilder
    private func machinesContent(_ vm: AgentsViewModel) -> some View {
        // EXP-818: the page is a TABLE now — a filled group band per group with
        // its flat rows hanging straight off it (`GlassSectionBand` +
        // `.flatRow()`), so the stack of bordered cards this used to be reads
        // as one list.
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 0) {
                    // EXP-1169 (web + desktop parity): "Add device" rides
                    // the band and opens the ONE device setup block.
                    GlassSectionBand(DevicesList.Copy.mine) {
                        GlassPill("Add device", icon: AppIcons.uiAdd, mode: .action {
                            showAddDevice = true
                        })
                    }
                    if let myDevices {
                        if myDevices.isEmpty {
                            deviceHintRow
                        } else {
                            deviceRows(vm, myDevices)
                        }
                    } else {
                        deviceLoadingRow
                    }
                }

                // EXP-432: teammates' shared servers, grouped below the
                // caller's own. Absent entirely when nothing is shared.
                if !teamDevices.isEmpty {
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(DevicesList.Copy.team)
                        deviceRows(vm, teamDevices)
                    }
                }
            }
            .padding()
        }
        // Clearance for the floating tab bar (EXP-36) — on the SCROLLER
        // itself, so its content inset is the one that grows.
        .tabBarBottomInset()
        // EXP-481: the gear opens the sheet (name, default device, sharing,
        // launch defaults) — the row menu's rename alert retired
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
        // One presentation per node is the rule, so the login sheet, the
        // account confirm and the Add device sheet hang off zero-size nodes
        // of their own.
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .sheet(isPresented: $showAddDevice) {
                    AddDeviceSheet(accountId: accountId)
                }
        )
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
                // EXP-1215: the app's own alert card (`GlassAlert`), ×4.
                .glassAlert(item: $removeAccountTarget) { target in
                    // The pinned sentence ×4: it names the login and the
                    // machine, and says the account itself survives.
                    GlassAlert(
                        title: "Remove account?",
                        message: AgentAccountsRows.removeAccountConfirmCopy(
                            account: AgentAccountsRows.loginLabel(target.row),
                            device: confirmDeviceLabel(target.row)
                        ),
                        actions: [
                            GlassAlertAction("Cancel", role: .outline, isDefault: true, isCancel: true, id: "cancel") {},
                            GlassAlertAction("Remove", role: .destructive, id: "remove") {
                                viewModel?.removeAccount(target.row)
                            },
                        ]
                    )
                }
        )
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $signOutAccountTarget) { target in
                    // EXP-1137: the pinned sentence ×4.
                    GlassAlert(
                        title: "Sign out?",
                        message: AgentAccountsRows.signOutConfirmCopy(
                            account: AgentAccountsRows.loginLabel(target.row),
                            device: confirmDeviceLabel(target.row)
                        ),
                        actions: [
                            GlassAlertAction("Cancel", role: .outline, isDefault: true, isCancel: true, id: "cancel") {},
                            GlassAlertAction("Sign out", role: .destructive, id: "sign-out") {
                                viewModel?.signOutAccount(target.row)
                            },
                        ]
                    )
                }
        )
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                // FEED-149: the team owner's Remove, confirmed (prompts.json).
                .glassAlert(item: $dropShareTarget) { target in
                    GlassAlert(
                        prompt: Prompts.RemoveDeviceShare.copy(name: target.name),
                        handlers: ["remove": { dropShare(target) }]
                    )
                }
        )
    }

    /// The machine a confirm names: its label, or its id for a row that has
    /// none.
    private func confirmDeviceLabel(_ row: AgentProfileUsageRow) -> String {
        row.deviceLabel.isEmpty ? row.deviceId : row.deviceLabel
    }

    /// A-16 (`devices-list.json`): every row's derivation — its group, its
    /// empty-slot wording and its attention items — off the ×4 pure rule.
    private func listRow(_ vm: AgentsViewModel, _ device: SteerDevice) -> DevicesList.Row {
        DevicesList.row(DevicesList.Input(
            online: device.isOnline,
            mine: device.isMine,
            server: device.isServer,
            registered: device.isRegistered,
            outdated: isOutdated(device),
            latest: latestVersion(device),
            updateQueued: isUpdateQueued(device),
            accountsReported: device.agentAccounts != nil,
            logins: vm.deviceLoginRows(device.deviceId).map(DevicesList.Login.init)
        ))
    }

    /// The instance's latest build for this machine's channel: the CLI for a
    /// daemon server, the desktop app otherwise (web `my-machines.tsx`).
    private func latestVersion(_ device: SteerDevice) -> String? {
        device.isServer ? latestVersions?.cli : latestVersions?.desktop
    }

    /// EXP-420: the client's own version compare.
    private func isOutdated(_ device: SteerDevice) -> Bool {
        device.updateAvailable(latest: latestVersion(device))
    }

    /// A-16: one group of rows — online first in the list's own order, then
    /// under the muted `Offline` label every offline row, dimmed.
    @ViewBuilder
    private func deviceRows(_ vm: AgentsViewModel, _ rows: [SteerDevice]) -> some View {
        let online = rows.filter(\.isOnline)
        let offline = rows.filter { !$0.isOnline }
        ForEach(online) { deviceRow(vm, $0) }
        if !offline.isEmpty {
            Text(DevicesList.Copy.offline)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 12)
                .padding(.top, 10)
                .padding(.bottom, 2)
            ForEach(offline) { deviceRow(vm, $0) }
        }
    }

    /// A-16: one machine, one line — kind glyph, name, version (warning tone
    /// when MY machine is outdated), the default star, the Shared tag, the status line under
    /// the name; then its login chips; then the attention icon when there is
    /// anything to say; then the gear (own registered rows only, EXP-909).
    /// Nothing folds and no warning is written inline.
    private func deviceRow(_ vm: AgentsViewModel, _ device: SteerDevice) -> some View {
        let row = listRow(vm, device)
        // Only my own machine's update is mine to act on (web, Android).
        let outdated = device.isMine && isOutdated(device)
        // TOP-aligned (EXP-944): the glyph, the attention icon and the gear
        // stay level with the device NAME.
        return HStack(alignment: .top, spacing: 12) {
            // EXP-924: the owner's pick, else the kind default.
            AppIcon(
                DeviceIconDisplay.iconName(for: device),
                size: AppIcon.Size.medium
            )
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .frame(height: GlassTokens.controlSize)

            VStack(alignment: .leading, spacing: 8) {
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text(deviceName(device))
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        if let version = device.version, !version.isEmpty {
                            Text("v\(version)")
                                .font(.caption2)
                                .foregroundStyle(
                                    outdated
                                        ? DesignTokens.Semantic.yellow
                                        : .white.opacity(TextOpacity.tertiary)
                                )
                                .lineLimit(1)
                        }
                        // EXP-622: the machine every device picker prefills.
                        if device.isDefaultDevice {
                            AppIcon(AppIcons.uiDeviceDefault, size: AppIcon.Size.small)
                                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                                .accessibilityLabel("Default device")
                        }
                        // EXP-432: one of the caller's own machines shared with
                        // any team says so (a teammate's is attributed in the
                        // accessibility label only). A phone is never a listed
                        // device, so the `This device` tag never applies here.
                        if device.owner == nil, !device.sharedTeamIds.isEmpty {
                            Text("Shared")
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                                .lineLimit(1)
                        }
                    }
                    deviceStatusLine(device)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel(
                    device.owner.map { "\(device.deviceLabel), shared by \($0.name)" } ?? device.deviceLabel
                )
                .accessibilityIdentifier("device-row-\(device.deviceId)")

                // EXP-849/EXP-862/EXP-909: the machine's logins with the
                // repairs on them; a teammate's shared server renders
                // read-only.
                DeviceLogins(
                    viewModel: vm,
                    device: device,
                    noAccount: row.noAccount,
                    readOnly: !device.isMine,
                    onSignIn: { signIn(device, agent: $0.agent, profileId: $0.profileId) },
                    onRemove: { removeAccountTarget = AccountRemoveTarget(row: $0) },
                    onSignOut: { signOutAccountTarget = AccountRemoveTarget(row: $0) }
                )
            }

            Spacer(minLength: 0)

            // A-16: the warning-tone attention icon, only when its card has
            // something to list.
            if !row.attention.isEmpty {
                DeviceAttentionMenu(
                    deviceId: device.deviceId,
                    items: row.attention,
                    onSignIn: { item in
                        guard let agent = item.agent else { return }
                        signIn(device, agent: agent, profileId: item.profileId)
                    },
                    onUpdate: { requestUpdate(device) }
                )
            }

            // EXP-909: the settings gear, and nothing else. EXP-432: own
            // machines only (FEED-149: bar a team owner's Remove, below).
            if device.isMine, device.isRegistered {
                GhostIconButton(
                    AppIcons.navSettings,
                    accessibilityLabel: "Device settings"
                ) {
                    settingsTarget = DeviceSettingsTarget(id: device.deviceId)
                }
                .accessibilityIdentifier("machine-settings")
            } else if let teamId = dropShareTeamId(device), let rowId = device.rowId {
                // FEED-149: a team owner's Remove on a teammate's row shared
                // into the current team — the gear's slot and style.
                GhostIconButton(
                    AppIcons.uiDelete,
                    accessibilityLabel: "Remove \(deviceName(device)) from this team"
                ) {
                    dropShareTarget = DropShareTarget(
                        rowId: rowId, teamId: teamId, name: deviceName(device)
                    )
                }
                .accessibilityIdentifier("device-drop-share")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .opacity(row.group == .offline ? 0.5 : 1)
        .flatRow()
    }

    /// The sign-in sheet on ONE login (a chip's menu or an attention line).
    private func signIn(_ device: SteerDevice, agent: String, profileId: String?) {
        loginTarget = AgentLoginTarget(
            deviceId: device.deviceId,
            deviceLabel: deviceName(device),
            agent: agent,
            profileId: profileId
        )
    }

    /// The attention card's Update — the settings sheet's update-now, the
    /// outcome landing via sync (the row's status line says "Updating…").
    private func requestUpdate(_ device: SteerDevice) {
        let api = deps.devicesApi
        let account = accountId
        let id = device.deviceId
        Task {
            do {
                try await api.requestUpdate(accountId: account, deviceId: id)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "update", thing: "the device"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    /// A requested self-update REPLACES the live state: the daemon is about to
    /// restart, so "Online" would only read as a lie — unless the update is
    /// parked behind live coding sessions (EXP-411): then the row says
    /// "Update queued" without a spinner instead of "Updating…" forever.
    ///
    /// EXP-862: nothing here says anything about SIGN-INS any more. A login's
    /// state is said once, in the row's attention card (A-16) — the line used
    /// to repeat it as "codex not signed in".
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

    /// EXP-1274 `devices`: the band's "Add device" pill is the add
    /// (EXP-1169), the same device setup block every client opens.
    private var deviceHintRow: some View {
        EmptyState(.devices)
    }

    /// EXP-1275 (D8): the first load is a skeleton row in the device row's
    /// geometry, not "Checking for devices…".
    private var deviceLoadingRow: some View {
        SkeletonRows(rows: 1, rowHeight: 56, dividers: false)
    }

    private func relativeDate(_ s: String) -> String {
        // Electric syncs timestamps as Postgres text (space separator,
        // hour-only offset), which ISO8601DateFormatter alone rejects —
        // WireTimestamps handles both wire forms (EXP-169).
        // Polish pin: list captions are COMPACT ×4 ("Last seen 5h").
        RelativeTime.compact(wire: s)
    }
}
