import ExpCore
import ExpUI
import SwiftUI

/// EXP-849: a machine's agent logins, as the chips on its row in "My devices"
/// — the SETUP/REPAIR half of the accounts split.
///
/// The Accounts section below decides WHICH account to use (email, plan, live
/// usage bars, health); a machine row is where a broken or missing login gets
/// fixed. So each chip here names the agent and the login, badges that
/// machine's own health for it, and carries the repairs — three states, three
/// menus (EXP-862, ×4):
///
///   - **signed out or expired** — "Sign in", and nothing else: a dead
///     credential cannot be switched to, and there is nothing to remove that
///     would help.
///   - **healthy, not this machine's login** — "Set as default"
///     (`agent_profile_use`: no login flow, no logout, no credential touched)
///     plus "Remove account".
///   - **healthy, the machine's current login** — "Remove account" alone.
///
/// "Remove account" deletes THIS machine's copy of the login (its config dir
/// and index row); the account itself is untouched, which is what the confirm
/// says. The chip's badge is the only signed-out notice on the row — the
/// status line that used to spell it out is gone.
///
/// A teammate's shared server renders the same chips READ-ONLY: seeing that a
/// shared machine's codex login expired explains a refused start, but only its
/// owner can fix it.
struct DeviceAccountChips: View {
    let viewModel: AgentsViewModel
    let device: SteerDevice
    /// Sign this chip's login in — the host opens `AgentLoginSheet` on it. The
    /// PROFILE rides along, not just the agent: a machine holding two claude
    /// logins would otherwise re-login whichever one it is currently using,
    /// which on an expired sibling repairs the wrong one.
    let onSignIn: (AgentProfileUsageRow) -> Void
    /// EXP-862: ask to remove this login from the machine. The host owns the
    /// confirm (`AgentAccountsRows.removeAccountConfirmCopy`) — a destructive
    /// action never fires straight off a menu row.
    let onRemove: (AgentProfileUsageRow) -> Void

    private var rows: [AgentProfileUsageRow] {
        viewModel.deviceAccountRows(device.deviceId)
    }

    // Explicitly builder-annotated: the whole view is ONE conditional, and a
    // bare `if` reads as a statement without it.
    @ViewBuilder
    var body: some View {
        if !rows.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                FlowLayout(spacing: 6) {
                    ForEach(rows) { row in
                        chip(row)
                    }
                }
                // The MATERIAL outcome of a chip action lands by SYNC (the
                // machine re-reports its accounts), but a refusal would
                // otherwise be silent — including the honest one the server
                // answers for a machine too old to know the command. It
                // belongs HERE, under the chips that fired it: a caption at
                // the bottom of the page is off-screen from this row.
                if let error = viewModel.accountActionError(deviceId: device.deviceId) {
                    Text(error)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("device-account-error-\(device.deviceId)")
                }
            }
        }
    }

    /// Actionable only on one of MY machines that is listening and advertises
    /// the `agent-login` capability: every action rides the owner→device queue
    /// the server gates on that cap, and an offline machine would hold the
    /// command until it wakes, which reads as a dead tap. A chip whose state
    /// leaves no entry at all (a healthy login on a build with neither
    /// `account-switch` nor `account-remove`) carries no menu rather than an
    /// empty one.
    private func isActionable(_ row: AgentProfileUsageRow) -> Bool {
        guard row.mine, device.isOnline, device.canAgentLogin else { return false }
        if AgentAccountsRows.chipSignsIn(row) { return true }
        if AgentAccountsRows.chipSetsDefault(row, canSwitchAccount: device.canSwitchAccount) {
            return true
        }
        return AgentAccountsRows.canRemoveAccount(
            row,
            canAgentLogin: device.canAgentLogin,
            canRemoveAccount: device.canRemoveAccount
        )
    }

    @ViewBuilder
    private func chip(_ row: AgentProfileUsageRow) -> some View {
        if isActionable(row) {
            GlassMenu {
                menuItems(row)
            } label: {
                pill(row)
            }
            .accessibilityLabel("\(caption(row)) actions")
            .accessibilityIdentifier("device-account-chip-\(row.key)")
        } else {
            pill(row)
                .accessibilityIdentifier("device-account-chip-\(row.key)")
        }
    }

    @ViewBuilder
    private func menuItems(_ row: AgentProfileUsageRow) -> some View {
        // ONE menu per state, byte-identical with web `MachineAccountChip`,
        // the desktop chips and Android. A signed-out or refused login can
        // only be signed in; a healthy one is picked as the machine's default
        // or removed from it.
        if AgentAccountsRows.chipSignsIn(row) {
            GlassMenuItem("Sign in", icon: AppIcons.uiSignIn) {
                onSignIn(row)
            }
        } else {
            if AgentAccountsRows.chipSetsDefault(row, canSwitchAccount: device.canSwitchAccount) {
                GlassMenuItem("Set as default", icon: AppIcons.uiSwap) {
                    viewModel.useAccountHere(row)
                }
            }
            // EXP-862: gated on the machine's `account-remove` cap — the server
            // refuses the command below it, and an older build would leave the
            // queued row pending forever.
            if AgentAccountsRows.canRemoveAccount(
                row,
                canAgentLogin: device.canAgentLogin,
                canRemoveAccount: device.canRemoveAccount
            ) {
                GlassMenuItem("Remove account", icon: AppIcons.uiDelete, destructive: true) {
                    onRemove(row)
                }
            }
        }
    }

    /// The chip itself — a READONLY pill, because its host (`GlassMenu`) owns
    /// the tap. The brand mark names the agent, the label names the login, and
    /// the trailing slot says what state it is in.
    private func pill(_ row: AgentProfileUsageRow) -> some View {
        GlassPill(
            caption(row),
            mode: .readonly,
            tint: row.health.needsAttention ? DesignTokens.Semantic.yellow : nil
        ) {
            // EXP-849: resolved through the brand-mark helper — an agent id
            // with no asset draws the neutral glyph, never a blank.
            if let mark = AgentBrandMark.image(row.agent) {
                mark
                    .resizable()
                    .scaledToFit()
                    .frame(width: GlassPillSize.sm.glyphSize, height: GlassPillSize.sm.glyphSize)
            }
        } trailing: {
            if viewModel.isAccountActionPending(row) {
                ProgressView()
                    .controlSize(.mini)
                    .tint(.white)
                    .accessibilityLabel("Working…")
            } else if let badge = row.health.badgeLabel {
                AppIcon(AppIcons.uiWarning, size: GlassPillSize.sm.glyphSize)
                    .foregroundStyle(DesignTokens.Semantic.yellow)
                    .accessibilityLabel(badge)
            } else if row.active {
                AppIcon(AppIcons.uiCheck, size: GlassPillSize.sm.glyphSize)
                    .foregroundStyle(DesignTokens.Semantic.green)
                    .accessibilityLabel("Active login")
            }
        }
    }

    /// `Claude · dev@acme.test` — the agent, then the login: its email, else
    /// the bare plan (an agent that names a provider instead of an address),
    /// else the profile's label.
    private func caption(_ row: AgentProfileUsageRow) -> String {
        let account = row.email ?? row.plan ?? row.profileLabel
        return "\(LaunchVocabulary.agentLabel(row.agent)) · \(account)"
    }
}
