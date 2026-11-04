import ExpCore
import ExpUI
import SwiftUI

/// EXP-1560 (`devices-list.json`): a device CARD's bottom row — its logins as
/// ICONS ONLY, in `sortDeviceLogins` order: each a ghost icon button
/// (`Card.iconSize`) holding the agent's brand mark (`Card.markSize`), the
/// run-mark's amber `needs_input` badge on every login with a `login`
/// attention item. Past `Card.maxAccountIcons` logins the first
/// `maxAccountIcons - 1` show, then a ghost `⋯` (More accounts) reveals the
/// rest in place.
///
/// #EXP-1643 THE ACCOUNT PANEL: a tap on an icon expands THE card with
/// `DeviceAccountPanel` under a hairline; the open icon wears the glass-active
/// wash and a tap on it closes. One account open per device, remembered on this
/// client (`OpenDeviceAccounts`, `DevicesList.openDeviceAccount`), never synced.
/// A team device or an offline one of mine expands the same panel with no
/// actions. Own, online cards with an agent to add end with a ghost `+` (Add
/// account). A machine that reported accounts but holds no login says
/// `No account`; one that never reported says `Checking…`.
struct DeviceLogins: View {
    let viewModel: AgentsViewModel
    let device: SteerDevice
    /// The card's own derivation (`DevicesList.row`) — `noAccount` decides the
    /// empty slot's wording, `attention` which icons wear the badge.
    let noAccount: Bool
    let attention: [DevicesList.AttentionItem]
    /// A team device: the panel never acts — no account actions and no
    /// "Add account".
    var readOnly = false
    /// Sign this login in — the host opens `AgentLoginSheet` on it (the
    /// PROFILE rides along, so a re-login repairs the login that was tapped).
    let onSignIn: (AgentProfileUsageRow) -> Void
    /// EXP-1626: "Persist login" / "Renew persisted login" on an existing
    /// signed-in claude login — the host opens the same `AgentLoginSheet`,
    /// long-lived on THIS profile.
    let onPersistLogin: (AgentProfileUsageRow) -> Void
    /// EXP-862: ask to remove this login from the machine (the host confirms).
    let onRemove: (AgentProfileUsageRow) -> Void
    /// EXP-1137: ask to sign this login out on the machine (the host confirms).
    let onSignOut: (AgentProfileUsageRow) -> Void

    /// EXP-862: the device-bound "Add account" sheet (agent, then the login).
    @State private var addingAccount = false
    /// EXP-1560: the `⋯` More accounts reveal (in place, per card).
    @State private var revealed = false
    /// #EXP-1643: the open account per device, on this client only.
    @AppStorage(OpenDeviceAccounts.storageKey) private var openAccounts = OpenDeviceAccounts()

    private var rows: [AgentProfileUsageRow] {
        viewModel.deviceLoginRows(device.deviceId)
    }

    var body: some View {
        let rows = rows
        let open = DevicesList.openDeviceAccount(
            loginKeys: rows.map(\.key),
            remembered: openAccounts.accounts[device.deviceId]
        )
        // A login folded behind the `⋯` that is open reveals the fold.
        let openIndex = rows.firstIndex { $0.key == open }
        let folded = DevicesList.accountIcons(count: rows.count, expanded: false).visible
        let icons = DevicesList.accountIcons(
            count: rows.count,
            expanded: revealed || (openIndex.map { $0 >= folded } ?? false)
        )
        VStack(alignment: .leading, spacing: DevicesList.Card.panelGap) {
            FlowLayout(spacing: 4) {
                if rows.isEmpty {
                    emptyLine
                } else {
                    ForEach(rows.prefix(icons.visible)) { row in
                        loginIcon(row, open: row.key == open)
                    }
                    if icons.more {
                        GhostIconButton(
                            AppIcons.uiMore,
                            accessibilityLabel: DevicesList.Copy.moreAccounts,
                            size: DevicesList.Card.iconSize,
                            glyphSize: DevicesList.Card.markSize
                        ) {
                            revealed = true
                        }
                        .accessibilityIdentifier("device-more-accounts-\(device.deviceId)")
                    }
                }
                if showsAddAccount {
                    addAccountButton
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let openIndex {
                panel(rows[openIndex])
            }
        }
    }

    // MARK: - The empty states

    /// Two different silences ×4: a machine that has not reported its accounts
    /// YET is still working, one that reported NONE has nothing to run with.
    private var emptyLine: some View {
        Text(noAccount ? DevicesList.Copy.noAccount : DevicesList.Copy.checking)
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(height: DevicesList.Card.iconSize)
            .accessibilityIdentifier("device-logins-empty-\(device.deviceId)")
    }

    // MARK: - One login

    /// The login's attention item (at most one per login), if any.
    private func item(_ row: AgentProfileUsageRow) -> DevicesList.AttentionItem? {
        attention.first {
            $0.kind == .login && $0.agent == row.agent && $0.profileId == row.profileId
        }
    }

    private func loginIcon(_ row: AgentProfileUsageRow, open: Bool) -> some View {
        let item = item(row)
        return DeviceLoginIcon(
            agent: row.agent,
            badgeId: "device-login-badge-\(row.key)",
            badged: item != nil,
            open: open,
            pending: viewModel.isAccountActionPending(row)
        ) {
            openAccounts.accounts[device.deviceId] = .some(open ? nil : row.key)
        }
        .accessibilityLabel(iconAccessibilityLabel(row, item: item))
        .accessibilityIdentifier("device-login-\(row.key)")
    }

    private func iconAccessibilityLabel(
        _ row: AgentProfileUsageRow,
        item: DevicesList.AttentionItem?
    ) -> String {
        var label = AgentAccountsRows.loginLabel(row)
        if row.email != nil, let plan = row.plan { label += ", \(plan)" }
        if let status = item?.status { label += ", \(DevicesList.Copy.status(status))" }
        return label
    }

    /// #EXP-1643: the open login's panel. The actions are one rule,
    /// byte-identical with web's `useAccountActions`, the desktop rows and
    /// Android: Sign in, else the persist option (EXP-1626, claude only);
    /// Sign out (EXP-1137); Remove account.
    private func panel(_ row: AgentProfileUsageRow) -> some View {
        let actionable = isActionable(row)
        var pill: DeviceAccountPanel.Pill?
        if actionable {
            if AgentAccountsRows.chipSignsIn(row) {
                pill = DeviceAccountPanel.Pill(label: DevicesList.Copy.signIn, icon: AppIcons.uiSignIn) {
                    onSignIn(row)
                }
            } else if let label = AgentAccountsRows.chipLongLivedLabel(row) {
                pill = DeviceAccountPanel.Pill(label: label, icon: AppIcons.uiClock) {
                    onPersistLogin(row)
                }
            }
        }
        return DeviceAccountPanel(
            name: row.email ?? AgentAccountsRows.loginLabel(row),
            plan: row.email != nil ? row.plan : nil,
            persisted: AgentAccountsRows.persistedLine(row),
            status: item(row)?.status.map(DevicesList.Copy.status),
            usage: row.usage,
            pill: pill,
            onSignOut: actionable && AgentAccountsRows.chipSignsOut(row) ? { onSignOut(row) } : nil,
            onRemove: actionable ? { onRemove(row) } : nil,
            busy: viewModel.isAccountActionPending(row)
        )
    }

    /// Actionable only on one of MY machines that is listening and advertises
    /// the `agent-login` capability: every action rides the owner→device queue
    /// the server gates on that cap, and an offline machine would hold the
    /// command until it wakes, which reads as a dead tap.
    private func isActionable(_ row: AgentProfileUsageRow) -> Bool {
        !readOnly && row.mine && device.isOnline && device.canAgentLogin
    }

    // MARK: - Adding one

    /// EXP-909: ABSENT unless a sign-in could actually be queued here — my
    /// machine, listening, advertising `agent-login`, with at least one agent
    /// installed — and never on a team device.
    private var showsAddAccount: Bool {
        !readOnly && device.isMine && device.isOnline && device.canAgentLogin
            && !installedAgents.isEmpty
    }

    /// Every agent INSTALLED on the machine — runnable or signed out; either
    /// can take a login.
    private var installedAgents: [String] {
        let set = Set(device.agentIds).union(device.unauthedAgentIds)
        return DomainContract.codingAgentValues.filter { set.contains($0) }
    }

    /// The ghost `+` closing the icon row.
    private var addAccountButton: some View {
        GhostIconButton(
            AppIcons.uiAdd,
            accessibilityLabel: DevicesList.Copy.addAccount,
            size: DevicesList.Card.iconSize,
            glyphSize: DevicesList.Card.markSize
        ) {
            addingAccount = true
        }
        .accessibilityIdentifier("add-account-\(device.deviceId)")
        .sheet(isPresented: $addingAccount) {
            AddAccountSheet(viewModel: viewModel, device: device)
        }
    }
}

/// #EXP-1643: which account's panel is open per device, on this client only
/// (never synced): device id → the login key, `null` = closed by hand, absent =
/// never touched (`DevicesList.openDeviceAccount`).
struct OpenDeviceAccounts: RawRepresentable {
    static let storageKey = "exp:device-account-open"

    var accounts: [String: String?] = [:]

    init() {}

    init?(rawValue: String) {
        guard let accounts = try? JSONDecoder().decode([String: String?].self, from: Data(rawValue.utf8))
        else { return nil }
        self.accounts = accounts
    }

    var rawValue: String {
        (try? JSONEncoder().encode(accounts)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
    }
}

/// EXP-1560: one login's ghost icon button: the brand mark (EXP-849: resolved
/// through the brand-mark helper, never a blank), the run-mark's amber
/// `needs_input` badge top-right ringed in the card's ground, a spinner while a
/// command is in flight. #EXP-1643: the open icon wears the glass-active wash.
struct DeviceLoginIcon: View {
    let agent: String
    let badgeId: String
    let badged: Bool
    let open: Bool
    var pending = false
    let onToggle: () -> Void

    var body: some View {
        Button(action: onToggle) {
            ZStack {
                if pending {
                    ProgressView()
                        .controlSize(.mini)
                        .tint(.white)
                } else {
                    BeatingAgentMark(agent: agent, beating: false)
                        .frame(width: DevicesList.Card.markSize, height: DevicesList.Card.markSize)
                        .overlay(alignment: .topTrailing) {
                            if badged {
                                Circle()
                                    .fill(DesignTokens.Semantic.yellow)
                                    .frame(width: 6, height: 6)
                                    .overlay(
                                        Circle().stroke(GlassTokens.opaqueCardFill, lineWidth: 2)
                                    )
                                    .offset(x: 2, y: -2)
                                    .accessibilityIdentifier(badgeId)
                            }
                        }
                }
            }
            .frame(width: DevicesList.Card.iconSize, height: DevicesList.Card.iconSize)
            .background {
                if open {
                    RoundedRectangle(cornerRadius: GlassMenuTokens.activeFillRadius)
                        .fill(GlassTokens.fillActive)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(open ? .isSelected : [])
    }
}

/// #EXP-1643 THE ACCOUNT PANEL (web `DeviceAccountPanel`): what a device card
/// grows under a hairline. Line 1 = the email (else the label), then the
/// icon-only Sign out and Remove account (their confirms stay the host's). Line
/// 2 = the plan muted, then the one action pill. Then the persisted line, the
/// status line in warning tone, the usage windows. A team or offline device
/// passes no action: the same panel, no buttons, no pill.
struct DeviceAccountPanel: View {
    struct Pill {
        let label: String
        let icon: String
        let action: () -> Void
    }

    /// The email, else the label.
    let name: String
    var plan: String?
    var persisted: String?
    /// The attention item's status line.
    var status: String?
    var usage: AgentUsage?
    var pill: Pill?
    var onSignOut: (() -> Void)?
    var onRemove: (() -> Void)?
    var busy = false

    var body: some View {
        VStack(alignment: .leading, spacing: DevicesList.Card.panelGap) {
            // Web's `border-t border-border`, not the fainter row divider.
            Rectangle()
                .fill(DesignTokens.Palette.border)
                .frame(height: GlassTokens.hairline)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 2) {
                    // EXP-1627: the foreground colour at medium weight.
                    Text(name)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(DesignTokens.Palette.foreground)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if let onSignOut {
                        action(AppIcons.uiSignOut, "Sign out", id: "device-account-sign-out", onSignOut)
                    }
                    if let onRemove {
                        action(AppIcons.uiDelete, "Remove account", id: "device-account-remove", onRemove)
                            .padding(.trailing, -4)
                    }
                }
                .frame(minHeight: DevicesList.Card.iconSize)
                if plan != nil || pill != nil {
                    HStack(spacing: 8) {
                        Text(plan ?? "")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        if let pill {
                            GlassPill(
                                pill.label,
                                icon: pill.icon,
                                mode: .action(pill.action),
                                enabled: !busy
                            )
                            .fixedSize()
                            .accessibilityIdentifier("device-account-action")
                        }
                    }
                    .frame(minHeight: DevicesList.Card.iconSize)
                }
                if let persisted {
                    Text(persisted)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("device-login-persisted")
                }
                if let status {
                    Text(status)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.yellow)
                        .lineLimit(1)
                        .accessibilityIdentifier("device-login-status")
                }
            }
            if let usage, !(usage.windows ?? []).isEmpty {
                UsageWindows(usage: usage)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-account-panel")
    }

    private func action(
        _ icon: String, _ label: String, id: String, _ run: @escaping () -> Void
    ) -> some View {
        GhostIconButton(
            icon,
            accessibilityLabel: label,
            size: DevicesList.Card.iconSize,
            glyphSize: DevicesList.Card.markSize,
            enabled: !busy,
            action: run
        )
        .accessibilityIdentifier(id)
    }
}

/// EXP-1560 + #EXP-1643: a device card's TOP row: the device's own icon (the
/// owner's pick, else the kind default) at `Card.deviceIconSize`, muted, beside
/// [name + version + the default star] over the status slot, the `⋯` slot
/// trailing.
struct DeviceCardHeader<Status: View, Trailing: View>: View {
    let icon: String
    let name: String
    var version: String?
    var outdated = false
    var isDefault = false
    let accessibilityName: String
    var accessibilityId: String?
    @ViewBuilder let status: () -> Status
    @ViewBuilder let trailing: () -> Trailing

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(icon, size: DevicesList.Card.deviceIconSize)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.top, 1)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(name)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .layoutPriority(-1)
                    if let version, !version.isEmpty {
                        Text("v\(version)")
                            .font(.caption2)
                            .foregroundStyle(
                                outdated
                                    ? DesignTokens.Semantic.yellow
                                    : .white.opacity(TextOpacity.tertiary)
                            )
                            .lineLimit(1)
                            .fixedSize()
                    }
                    // EXP-622: the machine every device picker prefills.
                    if isDefault {
                        AppIcon(AppIcons.uiDeviceDefault, size: AppIcon.Size.small)
                            .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                            .accessibilityLabel("Default device")
                    }
                }
                status()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .combine)
            .accessibilityLabel(accessibilityName)
            .accessibilityIdentifier(accessibilityId ?? "")
            trailing()
                .padding(.top, -4)
                .padding(.trailing, -4)
        }
    }
}

/// The status line's live state: the green dot + `Online`.
struct DeviceOnlineStatus: View {
    var body: some View {
        HStack(spacing: 5) {
            Circle()
                .fill(DesignTokens.Semantic.green)
                .frame(width: 6, height: 6)
            Text("Online")
        }
        .font(.caption)
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }
}

/// EXP-1560: a device card's `⋯` (Device actions). #EXP-1643: THE Menu in its
/// SHEET presentation, titled with the device, never a popover. Rows in order:
/// the update line (an `update` / `update_queued` item's text, a disabled muted
/// row), `Update` (only when that item's action is `update`; busy while
/// requested), `Device settings` (own registered devices). A team device's
/// menu holds only a team owner's `Remove from this team`. The host draws no
/// `⋯` when there is no item (`DeviceCardMenu.hasItems`). A pick runs once the
/// sheet has dismissed (a second sheet or a confirm from under a dismissing one
/// is dropped).
struct DeviceCardMenu: View {
    let deviceId: String
    let deviceName: String
    /// The card's update item (`update` / `update_queued`), if any.
    let updateItem: DevicesList.AttentionItem?
    /// A requested update is in flight: `Update` stays, disabled.
    let updating: Bool
    let onUpdate: () -> Void
    /// Own registered devices: open the Device settings sheet.
    let onSettings: (() -> Void)?
    /// A team owner on a teammate's device shared into the current team.
    let onRemoveFromTeam: (() -> Void)?
    /// Capture only: the specimen that photographs the sheet.
    var startsOpen = false

    @State private var open = false
    @State private var pendingPick: (() -> Void)?

    static func hasItems(
        updateItem: DevicesList.AttentionItem?,
        settings: Bool,
        removeFromTeam: Bool
    ) -> Bool {
        updateItem != nil || settings || removeFromTeam
    }

    var body: some View {
        GhostIconButton(
            AppIcons.uiMore,
            accessibilityLabel: DevicesList.Copy.deviceActions,
            size: DevicesList.Card.iconSize,
            glyphSize: DevicesList.Card.markSize
        ) {
            open = true
        }
        .accessibilityIdentifier("device-actions-\(deviceId)")
        .onAppear {
            if startsOpen { open = true }
        }
        .sheet(isPresented: $open, onDismiss: runPendingPick) {
            GlassSheetChrome(title: deviceName) {
                rows
                    .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
                    .padding(.bottom, 16)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("device-menu-sheet")
            }
        }
    }

    private var rows: some View {
        VStack(spacing: 0) {
            if let updateItem {
                Text(updateItem.text)
                    .font(.system(size: DesignTokens.Typography.Size.sm))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, GlassMenuTokens.itemHPadding)
                    .frame(maxWidth: .infinity, minHeight: GlassMenuTokens.itemMinHeight, alignment: .leading)
                    .accessibilityIdentifier("device-update-line")
                if updateItem.action == .update {
                    Button { pick(onUpdate) } label: {
                        GlassMenuItemLabel(DevicesList.Copy.update, icon: AppIcons.uiUpdate, enabled: !updating)
                    }
                    .buttonStyle(.plain)
                    .disabled(updating)
                    .accessibilityIdentifier("device-update")
                }
            }
            if let onSettings {
                Button { pick(onSettings) } label: {
                    GlassMenuItemLabel(DevicesList.Copy.settings, icon: AppIcons.navSettings)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("machine-settings")
            }
            if let onRemoveFromTeam {
                Button { pick(onRemoveFromTeam) } label: {
                    GlassMenuItemLabel(DevicesList.Copy.removeFromTeam, icon: AppIcons.uiDelete, destructive: true)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("device-drop-share")
            }
        }
    }

    private func pick(_ action: @escaping () -> Void) {
        pendingPick = action
        open = false
    }

    private func runPendingPick() {
        pendingPick?()
        pendingPick = nil
    }
}

/// #EXP-1643: the `device-attention` specimen (web
/// `apps/styleguide/src/entries/device-attention.tsx`): two cards from the real
/// pieces, the first with the badged login's panel open. Capture only:
/// `-uiTesting -uiTestingStyleguide device-attention`, or `device-attention-menu`
/// with the first card's `⋯` sheet open.
struct DeviceAttentionSpecimens: View {
    var menuOpen = false

    @State private var width: CGFloat = 358

    private static let logins: [(agent: String, badged: Bool)] = [
        ("claude", false),
        ("claude", true),
        ("codex", false),
    ]

    private static let usage: AgentUsage = {
        let now = Date()
        let iso = ISO8601DateFormatter()
        func resets(_ seconds: TimeInterval) -> String {
            iso.string(from: now.addingTimeInterval(seconds + 30))
        }
        return AgentUsage(fetchedAt: iso.string(from: now), windows: [
            AgentUsageWindow(key: "session", label: "5h", percent: 42, resetsAt: resets(2 * 3600 + 10 * 60)),
            AgentUsageWindow(key: "weekly", label: "Week", percent: 81, resetsAt: resets(3 * 86400 + 14 * 3600)),
            AgentUsageWindow(key: "model:fable", label: "Fable", percent: 64, resetsAt: resets(3 * 86400 + 14 * 3600)),
        ])
    }()

    var body: some View {
        LazyVGrid(
            columns: Array(
                repeating: GridItem(.flexible(), spacing: DevicesList.Card.gap, alignment: .top),
                count: DevicesList.columns(width: width)
            ),
            alignment: .leading,
            spacing: DevicesList.Card.gap
        ) {
            card("macbook", version: "0.14.78", outdated: true, preview: true)
            card("mint", version: "0.14.79", outdated: false, preview: false)
        }
        .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width = $0 }
    }

    private func card(_ name: String, version: String, outdated: Bool, preview: Bool) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            DeviceCardHeader(
                icon: AppIcons.uiDevice,
                name: name,
                version: version,
                outdated: outdated,
                accessibilityName: name
            ) {
                DeviceOnlineStatus()
            } trailing: {
                DeviceCardMenu(
                    deviceId: name,
                    deviceName: name,
                    updateItem: outdated
                        ? DevicesList.AttentionItem(
                            kind: .update,
                            text: DevicesList.Copy.updateAvailable.replacingOccurrences(of: "{version}", with: "0.14.79"),
                            action: .update
                        )
                        : nil,
                    updating: false,
                    onUpdate: {},
                    onSettings: {},
                    onRemoveFromTeam: nil,
                    startsOpen: menuOpen && preview
                )
            }
            VStack(alignment: .leading, spacing: DevicesList.Card.panelGap) {
                HStack(spacing: 4) {
                    ForEach(Array(Self.logins.enumerated()), id: \.offset) { _, login in
                        DeviceLoginIcon(
                            agent: login.agent,
                            badgeId: "device-login-badge-\(name)",
                            badged: login.badged,
                            open: preview && login.badged,
                            onToggle: {}
                        )
                    }
                    GhostIconLabel(
                        AppIcons.uiAdd,
                        size: DevicesList.Card.iconSize,
                        glyphSize: DevicesList.Card.markSize
                    )
                }
                if preview {
                    DeviceAccountPanel(
                        name: "alex@acme.example",
                        plan: "Max",
                        status: DevicesList.Copy.status(.needsRelogin),
                        usage: Self.usage,
                        pill: .init(label: DevicesList.Copy.signIn, icon: AppIcons.uiSignIn) {},
                        onSignOut: {},
                        onRemove: {}
                    )
                }
            }
        }
        .padding(DevicesList.Card.padding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
    }
}
