import ExpCore
import ExpUI
import SwiftUI

/// EXP-909: a machine's agent logins, listed UNDER its row on the Devices page
/// — one flat sub-row per login.
///
/// This replaces both the chip strip that used to sit here and the separate
/// cross-device "Accounts" section below it (EXP-829/EXP-849). Folding the two
/// killed a whole class of confusion: an account lived in two places with two
/// orderings, two health badges and two menus, and the section's email-merged
/// groups claimed one set of numbers for logins that are per MACHINE. A login
/// belongs to the machine that holds it, so it reads there and nowhere else.
///
/// Each sub-row says WHO (`loginLabel` — the email, else the plan, else the
/// profile label; never a status), badges its health, carries the unchanged
/// action menu, and draws its own compact usage line.
///
/// A teammate's shared server renders the same rows READ-ONLY: seeing that a
/// shared machine's codex login expired explains a refused start, but only its
/// owner can fix it.
struct DeviceLogins: View {
    let viewModel: AgentsViewModel
    let device: SteerDevice
    /// EXP-944: the clock the usage bars' reset countdowns age on. The device
    /// list is where a limit is actually planned around, so its bars say WHEN
    /// they reset; the tight surfaces pass nothing and get bars alone.
    var now: Date?
    /// A team device: the rows are a statement, never a control — no menus and
    /// no "Add account".
    var readOnly = false
    /// Sign this login in — the host opens `AgentLoginSheet` on it. The
    /// PROFILE rides along, not just the agent: a machine holding two claude
    /// logins would otherwise re-login whichever one it is currently using,
    /// which on an expired sibling repairs the wrong one.
    let onSignIn: (AgentProfileUsageRow) -> Void
    /// EXP-862: ask to remove this login from the machine. The host owns the
    /// confirm (`AgentAccountsRows.removeAccountConfirmCopy`) — a destructive
    /// action never fires straight off a menu row.
    let onRemove: (AgentProfileUsageRow) -> Void

    /// EXP-862: the device-bound "Add account" sheet (agent, then the login).
    @State private var addingAccount = false

    private var rows: [AgentProfileUsageRow] {
        viewModel.deviceLoginRows(device.deviceId)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if rows.isEmpty {
                emptyLine
            } else {
                ForEach(rows) { row in
                    loginRow(row)
                }
            }
            if showsAddAccount {
                addAccountRow
            }
            // The MATERIAL outcome of a menu action lands by SYNC (the machine
            // re-reports its accounts), but a refusal would otherwise be
            // silent — including the honest one the server answers for a
            // machine too old to know the command. It belongs HERE, under the
            // rows that fired it: a caption at the bottom of the page is
            // off-screen from this machine.
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

    // MARK: - The empty states

    /// Two different silences, said differently: a machine that has not
    /// reported its accounts YET is still working, and one that reported NONE
    /// has nothing to run with. `Checking…` vs `No login reported`, ×4.
    private var emptyLine: some View {
        Text(device.agentAccounts == nil ? "Checking…" : AgentUsagePresentation.noLoginReported)
            .font(.caption2)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier("device-logins-empty-\(device.deviceId)")
    }

    // MARK: - One login

    private func loginRow(_ row: AgentProfileUsageRow) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                // EXP-849: resolved through the brand-mark helper — an agent
                // id with no asset draws the neutral glyph, never a blank.
                if let mark = AgentBrandMark.image(row.agent) {
                    mark
                        .resizable()
                        .scaledToFit()
                        .frame(width: 14, height: 14)
                }
                Text(AgentAccountsRows.loginLabel(row))
                    .font(.caption)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
                // The plan is a muted tail, and only when the label is the
                // EMAIL — otherwise the label already IS the plan.
                if row.email != nil, let plan = row.plan {
                    Text("· \(plan)")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                        .lineLimit(1)
                }
                if let badge = AgentAccountsRows.healthBadge(row) {
                    Text(badge)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.yellow)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                if viewModel.isAccountActionPending(row) {
                    ProgressView()
                        .controlSize(.mini)
                        .tint(.white)
                        .accessibilityLabel("Working…")
                } else if isActionable(row) {
                    GlassMenu {
                        menuItems(row)
                    } label: {
                        GhostIconLabel(AppIcons.uiMore)
                    }
                    .accessibilityLabel("\(AgentAccountsRows.loginLabel(row)) actions")
                    .accessibilityIdentifier("device-login-menu-\(row.key)")
                }
            }
            usageLine(row)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityIdentifier("device-login-row-\(row.key)")
    }

    /// The login's own numbers as the compact three-bar line, or the one
    /// caption that says why there are none. EXP-862: a signed-in login
    /// nothing has probed YET reads "Checking…" — "No usage reported" made a
    /// machine that is simply still working read as broken.
    @ViewBuilder
    private func usageLine(_ row: AgentProfileUsageRow) -> some View {
        let age = AgentUsagePresentation.usageAge(row.usage)
        if !AgentUsagePresentation.miniWindows(row.usage).isEmpty {
            VStack(alignment: .leading, spacing: 2) {
                AgentUsageMini(usage: row.usage, now: now)
                if let age {
                    Text(age)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .opacity(age == nil ? 1 : 0.5)
        } else {
            Text(captionWithoutUsage(row))
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
    }

    private func captionWithoutUsage(_ row: AgentProfileUsageRow) -> String {
        let asOf = AgentUsagePresentation.relativeDate(row.usage?.fetchedAt ?? row.checkedAt)
        if row.signedIn, asOf.isEmpty { return "Checking…" }
        return asOf.isEmpty ? "No usage reported" : "No usage reported · as of \(asOf)"
    }

    /// Actionable only on one of MY machines that is listening and advertises
    /// the `agent-login` capability: every action rides the owner→device queue
    /// the server gates on that cap, and an offline machine would hold the
    /// command until it wakes, which reads as a dead tap. A row whose state
    /// leaves no entry at all (a healthy login on a build with neither
    /// `account-switch` nor `account-remove`) carries no menu rather than an
    /// empty one.
    private func isActionable(_ row: AgentProfileUsageRow) -> Bool {
        guard !readOnly, row.mine, device.isOnline, device.canAgentLogin else { return false }
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
    private func menuItems(_ row: AgentProfileUsageRow) -> some View {
        // ONE menu per state, byte-identical with web's `accountChipActions`,
        // the desktop rows and Android. EXP-944: a signed-out or refused login
        // keeps Sign in as its first, repairing entry — but no longer ENDS
        // there: a dead NAMED profile can be removed too (codex logins expire
        // far more often than claude's and were left with a menu of one). Only
        // the ambient login still offers the sign-in alone; it is the CLI's
        // own config dir, not ours to delete.
        if AgentAccountsRows.chipSignsIn(row) {
            GlassMenuItem("Sign in", icon: AppIcons.uiSignIn) {
                onSignIn(row)
            }
        }
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

    // MARK: - Adding one

    /// EXP-909 REVERSES EXP-845's visible-but-disabled control: the row is
    /// ABSENT unless a sign-in could actually be queued here — my machine,
    /// listening, advertising `agent-login`, with at least one agent installed
    /// — and never on a team device. A permanently greyed control on a machine
    /// that can never take a login taught nothing; the device row above
    /// already says why it is quiet.
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

    private var addAccountRow: some View {
        Button {
            addingAccount = true
        } label: {
            HStack(spacing: 6) {
                AppIcon(AppIcons.uiAdd, size: AppIcon.Size.small)
                Text("Add account")
                    .font(.caption)
                Spacer(minLength: 0)
            }
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("add-account-\(device.deviceId)")
        .sheet(isPresented: $addingAccount) {
            AddAccountSheet(viewModel: viewModel, device: device)
        }
    }
}
