import ExpCore
import ExpUI
import SwiftUI

/// A-16 (`devices-list.json`): a machine's logins as ONE wrapping row of
/// chips on its Devices row — brand mark + `loginLabel` (+ ` · plan` when the
/// label is the email), in `sortDeviceLogins` order. Nothing folds and no
/// warning is written on a chip: a login's trouble is the row's attention
/// icon's business.
///
/// Touch: tapping a chip opens the account menu (Sign in / Sign out / Remove
/// account — the existing chip rules, EXP-862/EXP-1137) with that login's
/// limit bars at its top (the account picker's inline bars, EXP-992). A team
/// device's chip previews (the bars alone) and never offers an action. Own,
/// online rows with an agent to add end with a ghost `+` (Add account). A
/// machine that reported accounts but holds no login says `No account`; one
/// that never reported says `Checking…`.
struct DeviceLogins: View {
    let viewModel: AgentsViewModel
    let device: SteerDevice
    /// The row's own derivation (`DevicesList.row`) — `noAccount` decides the
    /// empty slot's wording.
    let noAccount: Bool
    /// A team device: the chips are a statement, never a control — no menus
    /// and no "Add account".
    var readOnly = false
    /// Sign this login in — the host opens `AgentLoginSheet` on it (the
    /// PROFILE rides along, so a re-login repairs the login that was tapped).
    let onSignIn: (AgentProfileUsageRow) -> Void
    /// EXP-862: ask to remove this login from the machine (the host confirms).
    let onRemove: (AgentProfileUsageRow) -> Void
    /// EXP-1137: ask to sign this login out on the machine (the host confirms).
    let onSignOut: (AgentProfileUsageRow) -> Void

    /// EXP-862: the device-bound "Add account" sheet (agent, then the login).
    @State private var addingAccount = false

    private var rows: [AgentProfileUsageRow] {
        viewModel.deviceLoginRows(device.deviceId)
    }

    var body: some View {
        FlowLayout(spacing: 6) {
            if rows.isEmpty {
                emptyLine
            } else {
                ForEach(rows) { row in
                    chip(row)
                }
            }
            if showsAddAccount {
                addAccountChip
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
            .frame(height: 28)
            .accessibilityIdentifier("device-logins-empty-\(device.deviceId)")
    }

    // MARK: - One login

    @ViewBuilder
    private func chip(_ row: AgentProfileUsageRow) -> some View {
        let limits = AccountOptions.limits(row)
        if isActionable(row) || limits != nil {
            GlassMenu {
                menuHeader(row, limits: limits)
                if isActionable(row) {
                    menuItems(row)
                }
            } label: {
                chipLabel(row)
            }
            .accessibilityLabel(chipAccessibilityLabel(row))
            .accessibilityIdentifier("device-login-chip-\(row.key)")
        } else {
            // Nothing to preview and nothing to do: the chip is a statement.
            chipLabel(row)
                .accessibilityElement(children: .combine)
                .accessibilityLabel(chipAccessibilityLabel(row))
                .accessibilityIdentifier("device-login-chip-\(row.key)")
        }
    }

    /// The account picker trigger's capsule (`AccountPickerTriggerLabel`'s
    /// chrome): brand mark, label, the muted plan tail when the label is the
    /// email, and a spinner while a menu command is in flight.
    private func chipLabel(_ row: AgentProfileUsageRow) -> some View {
        HStack(spacing: 5) {
            // EXP-849: resolved through the brand-mark helper — an agent id
            // with no asset draws the neutral glyph, never a blank.
            if let mark = AgentBrandMark.image(row.agent) {
                mark
                    .resizable()
                    .scaledToFit()
                    .frame(width: 14, height: 14)
            }
            Text(AgentAccountsRows.loginLabel(row))
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
                .truncationMode(.middle)
            if row.email != nil, let plan = row.plan {
                Text("· \(plan)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            if viewModel.isAccountActionPending(row) {
                ProgressView()
                    .controlSize(.mini)
                    .tint(.white)
                    .accessibilityLabel("Working…")
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 28)
        .background(GlassTokens.fillRow, in: Capsule())
        .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
        .contentShape(Capsule())
    }

    private func chipAccessibilityLabel(_ row: AgentProfileUsageRow) -> String {
        var label = AgentAccountsRows.loginLabel(row)
        if row.email != nil, let plan = row.plan { label += ", \(plan)" }
        if let badge = AgentAccountsRows.healthBadge(row) { label += ", \(badge)" }
        return label
    }

    /// The top of the chip's menu: who the login is and its limit bars — the
    /// account picker's touch preview (EXP-992), not a second bar style.
    @ViewBuilder
    private func menuHeader(_ row: AgentProfileUsageRow, limits: AccountLimits?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(AgentAccountsRows.loginLabel(row))
                .font(.subheadline)
                .foregroundStyle(.white.opacity(GlassMenuTokens.textOpacity))
                .lineLimit(1)
                .truncationMode(.middle)
            if let limits {
                AccountLimitBars(limits: limits)
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
        // desktop rows and Android, in this fixed order: Sign in, Sign out
        // (EXP-1137), Remove account.
        if AgentAccountsRows.chipSignsIn(row) {
            GlassMenuItem("Sign in", icon: AppIcons.uiSignIn) {
                onSignIn(row)
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

    /// The ghost `+` closing the chip row.
    private var addAccountChip: some View {
        GhostIconButton(
            AppIcons.uiAdd,
            accessibilityLabel: DevicesList.Copy.addAccount,
            size: 28,
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

/// A-16: the row's attention card — one line per `DevicesList` item, its
/// text left and its one action pill right (none on a teammate's machine, an
/// offline one, or a queued update). Floats from the warning icon.
struct DeviceAttentionMenu: View {
    let deviceId: String
    let items: [DevicesList.AttentionItem]
    let onSignIn: (DevicesList.AttentionItem) -> Void
    let onUpdate: () -> Void

    var body: some View {
        GlassMenu {
            ForEach(items) { item in
                DeviceAttentionLine(item: item, onSignIn: onSignIn, onUpdate: onUpdate)
            }
        } label: {
            GhostIconLabel(AppIcons.uiWarning, tint: DesignTokens.Semantic.yellow)
        }
        .accessibilityLabel(DevicesList.Copy.attention)
        .accessibilityIdentifier("device-attention-\(deviceId)")
    }
}

private struct DeviceAttentionLine: View {
    let item: DevicesList.AttentionItem
    let onSignIn: (DevicesList.AttentionItem) -> Void
    let onUpdate: () -> Void

    @Environment(\.glassMenuDismiss) private var dismissMenu

    var body: some View {
        HStack(spacing: 10) {
            Text(item.text)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(GlassMenuTokens.textOpacity))
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            switch item.action {
            case .signIn:
                GlassPill(DevicesList.Copy.signIn, icon: AppIcons.uiSignIn, mode: .action {
                    dismissMenu()
                    onSignIn(item)
                })
            case .update:
                GlassPill(DevicesList.Copy.update, icon: AppIcons.uiUpdate, mode: .action {
                    dismissMenu()
                    onUpdate()
                })
            case nil:
                EmptyView()
            }
        }
        .padding(.horizontal, GlassMenuTokens.itemHPadding)
        .frame(minHeight: GlassMenuTokens.itemMinHeight)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-attention-item-\(item.id)")
    }
}
