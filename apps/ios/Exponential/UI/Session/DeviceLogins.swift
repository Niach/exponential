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
/// Touch: tapping an icon opens THE account menu (Sign in / Keep signed in
/// (claude, EXP-1533) / Sign out / Remove account — the existing chip rules,
/// EXP-862/EXP-1137) headed by the login: the email (else the label), the plan
/// muted, the attention item's status line in warning tone, then the usage
/// windows. A team device's icon opens the header alone (a preview, never an
/// action). Own, online cards with an agent to add end with a ghost `+` (Add
/// account). A machine that reported accounts but holds no login says
/// `No account`; one that never reported says `Checking…`.
struct DeviceLogins: View {
    let viewModel: AgentsViewModel
    let device: SteerDevice
    /// The card's own derivation (`DevicesList.row`) — `noAccount` decides the
    /// empty slot's wording, `attention` which icons wear the badge.
    let noAccount: Bool
    let attention: [DevicesList.AttentionItem]
    /// A team device: the icons preview, never act — no account actions and
    /// no "Add account".
    var readOnly = false
    /// Sign this login in — the host opens `AgentLoginSheet` on it (the
    /// PROFILE rides along, so a re-login repairs the login that was tapped).
    let onSignIn: (AgentProfileUsageRow) -> Void
    /// EXP-1533: "Keep signed in" / "Renew long-lived login" on a claude
    /// login — the host opens the same `AgentLoginSheet`, long-lived.
    let onKeepSignedIn: (AgentProfileUsageRow) -> Void
    /// EXP-862: ask to remove this login from the machine (the host confirms).
    let onRemove: (AgentProfileUsageRow) -> Void
    /// EXP-1137: ask to sign this login out on the machine (the host confirms).
    let onSignOut: (AgentProfileUsageRow) -> Void

    /// EXP-862: the device-bound "Add account" sheet (agent, then the login).
    @State private var addingAccount = false
    /// EXP-1560: the `⋯` More accounts reveal (in place, per card).
    @State private var expanded = false

    private var rows: [AgentProfileUsageRow] {
        viewModel.deviceLoginRows(device.deviceId)
    }

    var body: some View {
        let rows = rows
        let icons = DevicesList.accountIcons(count: rows.count, expanded: expanded)
        FlowLayout(spacing: 4) {
            if rows.isEmpty {
                emptyLine
            } else {
                ForEach(rows.prefix(icons.visible)) { row in
                    loginIcon(row)
                }
                if icons.more {
                    GhostIconButton(
                        AppIcons.uiMore,
                        accessibilityLabel: DevicesList.Copy.moreAccounts,
                        size: DevicesList.Card.iconSize,
                        glyphSize: AppIcon.Size.small
                    ) {
                        expanded = true
                    }
                    .accessibilityIdentifier("device-more-accounts-\(device.deviceId)")
                }
            }
            if showsAddAccount {
                addAccountButton
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
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

    private func loginIcon(_ row: AgentProfileUsageRow) -> some View {
        let item = item(row)
        return GlassMenu {
            menuHeader(row, item: item)
            if isActionable(row) {
                menuItems(row)
            }
        } label: {
            iconLabel(row, badged: item != nil)
        }
        .accessibilityLabel(iconAccessibilityLabel(row, item: item))
        .accessibilityIdentifier("device-login-\(row.key)")
    }

    /// The ghost icon button's face: the brand mark (EXP-849: resolved
    /// through the brand-mark helper, never a blank), the run-mark's amber
    /// `needs_input` badge top-right ringed in the card's ground, a spinner
    /// while a menu command is in flight.
    private func iconLabel(_ row: AgentProfileUsageRow, badged: Bool) -> some View {
        ZStack {
            if viewModel.isAccountActionPending(row) {
                ProgressView()
                    .controlSize(.mini)
                    .tint(.white)
            } else {
                BeatingAgentMark(agent: row.agent, beating: false)
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
                                .accessibilityIdentifier("device-login-badge-\(row.key)")
                        }
                    }
            }
        }
        .frame(width: DevicesList.Card.iconSize, height: DevicesList.Card.iconSize)
        .contentShape(Rectangle())
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

    /// The account preview's header ×4: the email (else the label), the plan
    /// muted, the attention item's status line in warning tone, then the
    /// usage windows with their resets.
    @ViewBuilder
    private func menuHeader(_ row: AgentProfileUsageRow, item: DevicesList.AttentionItem?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            VStack(alignment: .leading, spacing: 2) {
                Text(row.email ?? AgentAccountsRows.loginLabel(row))
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(GlassMenuTokens.textOpacity))
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let plan = row.plan, row.email != nil {
                    Text(plan)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
                if let status = item?.status {
                    Text(DevicesList.Copy.status(status))
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.yellow)
                        .lineLimit(1)
                        .accessibilityIdentifier("device-login-status")
                }
            }
            if let usage = row.usage, !(usage.windows ?? []).isEmpty {
                UsageWindows(usage: usage)
                    .padding(.top, 4)
            }
        }
        .padding(.horizontal, GlassMenuTokens.itemHPadding)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Actionable only on one of MY machines that is listening and advertises
    /// the `agent-login` capability: every action rides the owner→device queue
    /// the server gates on that cap, and an offline machine would hold the
    /// command until it wakes, which reads as a dead tap.
    private func isActionable(_ row: AgentProfileUsageRow) -> Bool {
        !readOnly && row.mine && device.isOnline && device.canAgentLogin
            && !viewModel.isAccountActionPending(row)
    }

    @ViewBuilder
    private func menuItems(_ row: AgentProfileUsageRow) -> some View {
        // ONE rule, byte-identical with web's `accountChipActions`, the
        // desktop rows and Android, in this fixed order: Sign in, the
        // long-lived login (EXP-1533, claude only), Sign out (EXP-1137),
        // Remove account.
        if AgentAccountsRows.chipSignsIn(row) {
            GlassMenuItem("Sign in", icon: AppIcons.uiSignIn) {
                onSignIn(row)
            }
        }
        if let longLived = AgentAccountsRows.chipLongLivedLabel(row) {
            GlassMenuItem(longLived, icon: AppIcons.uiClock) {
                onKeepSignedIn(row)
            }
        }
        if AgentAccountsRows.chipSignsOut(row) {
            GlassMenuItem("Sign out", icon: AppIcons.uiSignOut, destructive: true) {
                onSignOut(row)
            }
        }
        GlassMenuItem("Remove account", icon: AppIcons.uiDelete, destructive: true) {
            onRemove(row)
        }
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
            glyphSize: AppIcon.Size.small
        ) {
            addingAccount = true
        }
        .accessibilityIdentifier("add-account-\(device.deviceId)")
        .sheet(isPresented: $addingAccount) {
            AddAccountSheet(viewModel: viewModel, device: device)
        }
    }
}

/// EXP-1560: a device card's `⋯` (Device actions), THE Menu. Items in order:
/// the update line (an `update` / `update_queued` item's text, a disabled
/// muted row), `Update` (only when that item's action is `update`; busy while
/// requested), `Device settings` (own registered devices). A team device's
/// menu holds only a team owner's `Remove from this team`. The host draws no
/// `⋯` when there is no item (`DeviceCardMenu.hasItems`).
struct DeviceCardMenu: View {
    let deviceId: String
    /// The card's update item (`update` / `update_queued`), if any.
    let updateItem: DevicesList.AttentionItem?
    /// A requested update is in flight: `Update` stays, disabled.
    let updating: Bool
    let onUpdate: () -> Void
    /// Own registered devices: open the Device settings sheet.
    let onSettings: (() -> Void)?
    /// A team owner on a teammate's device shared into the current team.
    let onRemoveFromTeam: (() -> Void)?

    static func hasItems(
        updateItem: DevicesList.AttentionItem?,
        settings: Bool,
        removeFromTeam: Bool
    ) -> Bool {
        updateItem != nil || settings || removeFromTeam
    }

    var body: some View {
        GlassMenu {
            if let updateItem {
                Text(updateItem.text)
                    .font(.system(size: DesignTokens.Typography.Size.sm))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, GlassMenuTokens.itemHPadding)
                    .frame(maxWidth: .infinity, minHeight: GlassMenuTokens.itemMinHeight, alignment: .leading)
                    .accessibilityIdentifier("device-update-line")
                if updateItem.action == .update {
                    GlassMenuItem(
                        DevicesList.Copy.update,
                        icon: AppIcons.uiUpdate,
                        enabled: !updating,
                        action: onUpdate
                    )
                    .accessibilityIdentifier("device-update")
                }
            }
            if let onSettings {
                GlassMenuItem(DevicesList.Copy.settings, icon: AppIcons.navSettings, action: onSettings)
                    .accessibilityIdentifier("machine-settings")
            }
            if let onRemoveFromTeam {
                GlassMenuItem(
                    DevicesList.Copy.removeFromTeam,
                    icon: AppIcons.uiDelete,
                    destructive: true,
                    action: onRemoveFromTeam
                )
                .accessibilityIdentifier("device-drop-share")
            }
        } label: {
            GhostIconLabel(
                AppIcons.uiMore,
                size: DevicesList.Card.iconSize,
                glyphSize: AppIcon.Size.small
            )
        }
        .accessibilityLabel(DevicesList.Copy.deviceActions)
        .accessibilityIdentifier("device-actions-\(deviceId)")
    }
}
