import ExpCore
import ExpUI
import SwiftUI

/// EXP-849: a machine's agent logins, as the chips on its row in "My machines"
/// — the SETUP/REPAIR half of the accounts split.
///
/// The Accounts section below decides WHICH account to use (email, plan, live
/// usage bars, health); a machine row is where a broken or missing login gets
/// fixed. So each chip here names the agent and the login, badges that
/// machine's own health for it, and carries the two repair actions:
///
///   - **Use this account here** — point the machine's ACTIVE login for that
///     agent at a profile it already holds (`agent_profile_use`: no login
///     flow, no logout, no credential touched). Hidden on the login that is
///     already active.
///   - **Sign in / Sign in again** — the sign-in link round-trip, which lives
///     in the machine's settings sheet (it has the link, the code field and
///     the waiting state); this routes there with that agent's tab open,
///     rather than re-implementing the flow per surface.
///
/// A teammate's shared server renders the same chips READ-ONLY: seeing that a
/// shared machine's codex login expired explains a refused start, but only its
/// owner can fix it.
struct DeviceAccountChips: View {
    let viewModel: AgentsViewModel
    let device: SteerDevice
    /// Open this machine's settings sheet with `agent`'s tab selected — where
    /// the remote sign-in link and its code field live.
    let onSignIn: (String) -> Void

    private var rows: [AgentProfileUsageRow] {
        viewModel.deviceAccountRows(device.deviceId)
    }

    // Explicitly builder-annotated: the whole view is ONE conditional, and a
    // bare `if` reads as a statement without it.
    @ViewBuilder
    var body: some View {
        if !rows.isEmpty {
            FlowLayout(spacing: 6) {
                ForEach(rows) { row in
                    chip(row)
                }
            }
        }
    }

    /// Actionable only on one of MY machines that is listening and advertises
    /// the `agent-login` capability: both actions ride the owner→device queue
    /// the server gates on that cap (`agent_profile_use` is refused without
    /// it), and an offline machine would hold the command until it wakes,
    /// which reads as a dead tap.
    private func isActionable(_ row: AgentProfileUsageRow) -> Bool {
        row.mine && device.isOnline && device.canAgentLogin
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
        // A login the machine already runs is nothing to switch to.
        if row.signedIn, !row.active {
            GlassMenuItem("Use this account here", icon: AppIcons.uiSwap) {
                viewModel.useAccountHere(row)
            }
        }
        GlassMenuItem(
            row.signedIn ? "Sign in again" : "Sign in",
            icon: AppIcons.uiSignIn
        ) {
            onSignIn(row.agent)
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
