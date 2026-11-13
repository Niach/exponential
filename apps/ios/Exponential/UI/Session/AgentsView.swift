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
/// EXP-1560 (`devices-list.json`): each section is a GRID of device CARDS.
/// A card's one control is its `⋯` (Device actions: the update line,
/// `Update`, `Device settings` on own registered devices); a team OWNER's
/// Team devices cards shared into the CURRENT team hold Remove from this team
/// there (FEED-149, `devices.dropShare`). The play glyph stays gone (starting
/// a run is the Agent page composer's device picker, EXP-909).
///
/// EXP-825: machines ONLY (web parity, EXP-818) — sessions live in the
/// Chats list. EXP-1295: a start is a `sessions` row the machine picks up
/// from its shapes, so the list never waits on the steer relay.
///
/// EXP-909: every device card LISTS ITS OWN LOGINS (`DeviceLogins`) — since
/// EXP-1560 as brand-mark icons on the card's bottom row; #EXP-1643 a tap
/// expands the card with that login's panel: its usage windows and (own, online
/// machines only) what signs it in, signs it out, or removes it. A login's
/// trouble is the amber badge on its icon. A login belongs to the machine that
/// holds it, and that is where it reads.
struct AgentsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @State private var viewModel: AgentsViewModel?
    /// The `⋯` menu's Device settings — the sheet it opens (name, default
    /// device, sharing, launch defaults, update + remove).
    @State private var settingsTarget: DeviceSettingsTarget?
    /// EXP-862: the sign-in a chip (or the Accounts section) asked for, and
    /// the removal a chip is confirming.
    @State private var loginTarget: AgentLoginTarget?
    @State private var removeAccountTarget: AccountRemoveTarget?
    /// EXP-1137: the sign-out a chip is confirming.
    @State private var signOutAccountTarget: AccountRemoveTarget?
    /// EXP-1169: the band's "Add device" sheet.
    @State private var showAddDevice = false
    /// A-16: the instance's latest client builds — the cards' version compare
    /// (outdated tone, the `⋯` menu's update line). One read per visit.
    @State private var latestVersions: LatestVersions?
    /// FEED-149: the caller owns the current team — its Team devices rows
    /// offer Remove. Resolved off the synced `team_members` row.
    @State private var isTeamOwner = false
    /// FEED-149: the teammate's shared row a team owner is removing.
    @State private var dropShareTarget: DropShareTarget?
    /// EXP-1560: the sections' content width — the card grid's columns
    /// (`DevicesList.columns`). 358 = a 390pt phone until measured.
    @State private var sectionWidth: CGFloat = 358
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
        // #EXP-1534 (inbox-route.json): a signed-out login's inbox row or
        // push lands here with its devices ROW id; the settings sheet opens
        // once the list holds that machine.
        .onChange(of: deviceRowIds, initial: true) { _, _ in openPendingDeviceSettings() }
        .onChange(of: deps.deepLinkBus.pendingDeviceSettingsRowId) { _, _ in openPendingDeviceSettings() }
    }

    /// The listed machines' synced devices ROW ids (nil = not loaded yet).
    private var deviceRowIds: [String]? {
        viewModel?.devices?.compactMap(\.rowId)
    }

    /// #EXP-1534: open the pending device's settings — the caller's own
    /// registered machine whose devices ROW id the tap named. Cleared as soon
    /// as the list has loaded, found or not (a removed machine opens nothing).
    private func openPendingDeviceSettings() {
        guard let rowId = deps.deepLinkBus.pendingDeviceSettingsRowId,
              let devices = viewModel?.devices else { return }
        deps.deepLinkBus.pendingDeviceSettingsRowId = nil
        guard let device = devices.first(where: { $0.rowId == rowId && $0.isMine && $0.isRegistered })
        else { return }
        settingsTarget = DeviceSettingsTarget(id: device.deviceId)
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
        // EXP-1560: a filled group band per section with its GRID of device
        // cards under it (`devices-list.json`). The section's content width
        // decides the grid's columns.
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
                            deviceGrid(vm, myDevices)
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
                        deviceGrid(vm, teamDevices)
                    }
                }
            }
            .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width in
                sectionWidth = width
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
        // #EXP-1815: an open device's settings = the screen's thread.
        .onChange(of: settingsTarget?.id) { _, deviceId in
            let rowId = deviceId.flatMap { id in viewModel?.devices?.first { $0.deviceId == id && $0.isMine }?.rowId }
            ViewingThread.shared.device = rowId.map { "device:\($0)" }
        }
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
                            device: confirmDeviceLabel(target.row),
                            agent: target.row.agent
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
        let now = Date()
        return DevicesList.row(DevicesList.Input(
            online: device.isOnline,
            mine: device.isMine,
            server: device.isServer,
            registered: device.isRegistered,
            outdated: isOutdated(device),
            latest: latestVersion(device),
            updateQueued: isUpdateQueued(device),
            accountsReported: device.agentAccounts != nil,
            logins: vm.deviceLoginRows(device.deviceId).map { DevicesList.Login($0, now: now) },
            now: now
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

    /// EXP-1560: one section's GRID of device cards — online first in the
    /// list's own order, then the muted `Offline` label spanning the grid,
    /// then every offline card in its own grid, dimmed. Columns come off the
    /// section's content width (`DevicesList.columns`: 1 on a 390pt phone in portrait).
    @ViewBuilder
    private func deviceGrid(_ vm: AgentsViewModel, _ rows: [SteerDevice]) -> some View {
        let online = rows.filter(\.isOnline)
        let offline = rows.filter { !$0.isOnline }
        let columns = Array(
            repeating: GridItem(.flexible(), spacing: DevicesList.Card.gap, alignment: .top),
            count: DevicesList.columns(width: sectionWidth)
        )
        VStack(alignment: .leading, spacing: DevicesList.Card.gap) {
            if !online.isEmpty {
                LazyVGrid(columns: columns, alignment: .leading, spacing: DevicesList.Card.gap) {
                    ForEach(online) { deviceCard(vm, $0) }
                }
            }
            if !offline.isEmpty {
                Text(DevicesList.Copy.offline)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 4)
                LazyVGrid(columns: columns, alignment: .leading, spacing: DevicesList.Card.gap) {
                    ForEach(offline) { deviceCard(vm, $0) }
                }
            }
        }
        .padding(.top, DevicesList.Card.gap)
    }

    /// EXP-1560: one machine, one CARD (THE GlassCard surface, padding
    /// `Card.padding`), two rows. TOP (`DeviceCardHeader`): #EXP-1643 the
    /// device's own icon, then [name + version (warning tone when MY machine
    /// is outdated) + the default star] over [the status line + the Shared
    /// tag], the `⋯` (Device actions) top-right. BOTTOM: the logins as icons
    /// (`DeviceLogins`), badged when they need attention, and the open
    /// account's panel under them; the card grows, its neighbours stay.
    private func deviceCard(_ vm: AgentsViewModel, _ device: SteerDevice) -> some View {
        let row = listRow(vm, device)
        // Only my own machine's update is mine to act on (web, Android).
        let outdated = device.isMine && isOutdated(device)
        let updateItem = row.attention.first { $0.kind != .login }
        let settings = device.isMine && device.isRegistered
        let dropShareTeamId = dropShareTeamId(device)
        return VStack(alignment: .leading, spacing: 12) {
            DeviceCardHeader(
                icon: DeviceIconDisplay.iconName(for: device),
                name: deviceName(device),
                version: device.version,
                outdated: outdated,
                isDefault: device.isDefaultDevice,
                accessibilityName: device.owner.map { "\(device.deviceLabel), shared by \($0.name)" } ?? device.deviceLabel,
                accessibilityId: "device-row-\(device.deviceId)"
            ) {
                HStack(spacing: 6) {
                    deviceStatusLine(device)
                    // EXP-432: one of the caller's own machines shared
                    // with any team says so (a teammate's is attributed
                    // in the accessibility label only). A phone is never
                    // a listed device, so the `This device` tag never
                    // applies here.
                    if device.owner == nil, !device.sharedTeamIds.isEmpty {
                        Text("Shared")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                            .lineLimit(1)
                            .fixedSize()
                    }
                }
            } trailing: {
                // EXP-1560: the `⋯`; a card with no item keeps the slot empty.
                if DeviceCardMenu.hasItems(
                    updateItem: updateItem,
                    settings: settings,
                    removeFromTeam: dropShareTeamId != nil
                ) {
                    DeviceCardMenu(
                        deviceId: device.deviceId,
                        deviceName: deviceName(device),
                        updateItem: updateItem,
                        updating: isUpdating(device),
                        onUpdate: { requestUpdate(device) },
                        onSettings: settings
                            ? { settingsTarget = DeviceSettingsTarget(id: device.deviceId) }
                            : nil,
                        onRemoveFromTeam: dropShareTeamId.flatMap { teamId in
                            device.rowId.map { rowId in
                                {
                                    dropShareTarget = DropShareTarget(
                                        rowId: rowId, teamId: teamId, name: deviceName(device)
                                    )
                                }
                            }
                        }
                    )
                } else {
                    Color.clear
                        .frame(width: DevicesList.Card.iconSize, height: DevicesList.Card.iconSize)
                }
            }

            // EXP-849/EXP-862/EXP-909: the machine's logins with the repairs
            // on them; a teammate's shared server expands read-only.
            DeviceLogins(
                viewModel: vm,
                device: device,
                noAccount: row.noAccount,
                attention: row.attention,
                readOnly: !device.isMine,
                onSignIn: { signIn(device, agent: $0.agent, profileId: $0.profileId) },
                onPersistLogin: { persistLogin(device, row: $0) },
                onRemove: { removeAccountTarget = AccountRemoveTarget(row: $0) },
                onSignOut: { signOutAccountTarget = AccountRemoveTarget(row: $0) }
            )
        }
        .padding(DevicesList.Card.padding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
        .opacity(row.group == .offline ? 0.6 : 1)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-card-\(device.deviceId)")
    }

    /// The sign-in sheet on ONE login (its account panel's Sign in) —
    /// the agent's normal browser login (EXP-1626).
    private func signIn(_ device: SteerDevice, agent: String, profileId: String?) {
        loginTarget = AgentLoginTarget(
            deviceId: device.deviceId,
            deviceLabel: deviceName(device),
            agent: agent,
            profileId: profileId
        )
    }

    /// EXP-1626 PERSIST LOGIN: "Persist login" / "Renew persisted login"
    /// (account panel only) — the same sign-in sheet, long-lived on THIS
    /// existing profile: the machine attaches a one-year token to it and
    /// never touches the browser login it holds.
    private func persistLogin(_ device: SteerDevice, row: AgentProfileUsageRow) {
        loginTarget = AgentLoginTarget(
            deviceId: device.deviceId,
            deviceLabel: deviceName(device),
            agent: row.agent,
            profileId: row.profileId,
            longLived: true
        )
    }

    /// The `⋯` menu's Update — the settings sheet's update-now, the outcome
    /// landing via sync (the card's status line says "Updating…").
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
    /// state is its icon's badge + its panel's status line (EXP-1560).
    @ViewBuilder
    private func deviceStatusLine(_ device: SteerDevice) -> some View {
        HStack(spacing: 5) {
            if isUpdateQueued(device) {
                Text("Update queued")
            } else if isUpdating(device) {
                ProgressView().controlSize(.mini).tint(.white)
                Text("Updating…")
            } else if device.isOnline {
                DeviceOnlineStatus()
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
