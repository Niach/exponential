import ExpCore
import ExpUI
import SwiftUI

/// EXP-829/EXP-849: the Devices page's **Accounts** section — the DECISION
/// surface for agent logins.
///
/// One row per agent ACCOUNT (an agent plus the login the devices named): who
/// it is, what it may spend, how much of that is left, and whether it still
/// works (EXP-849 `health`). Attention first — a signed-out or refused login
/// leads, then anything at or over the danger threshold, then the rest.
///
/// EXP-849 split the two surfaces that used to be one list:
///   - HERE (Accounts) you decide WHICH account to use: the numbers are the
///     freshest device's (they are the account's limits, so every device reads
///     the same ones) and the devices holding it are chips — the online dot, a
///     check where the account is that device's ACTIVE login, and (EXP-862) the
///     same menu the device rows' chips carry.
///   - The device rows above ("My devices") remain the SETUP surface: which
///     device, which logins it holds, what it may run.
///
/// EXP-862: the header carries "+ Add account" (the sign-in a device runs for a
/// login it does not hold yet) and each row a bare "+" for signing the SAME
/// account in on another device. What went: the refresh buttons and the
/// "Refreshes every 5 minutes" caption — the section refreshes itself, and a
/// button for it was a control that mostly said "too soon".
///
/// With two or more agents reporting, the rows sit under per-agent TABS
/// (claude | codex) instead of stacked bands: codex's two windows used to push
/// claude's five off the screen.
///
/// The section owns no model of its own: rows, groups, ordering, health and
/// the refresh floor are `AgentAccountsRows` / `AgentAccountHealth` (ExpCore,
/// the ×4 rules), the refresh round-trip is `AgentsViewModel` — this file is
/// layout.
struct AgentAccountsSection: View {
    let viewModel: AgentsViewModel
    /// EXP-862: sign in on a device — the header's "+ Add account", every
    /// row's "+", and a chip whose login is signed out. The page owns the
    /// sheet.
    let onSignIn: (AgentLoginTarget) -> Void
    /// EXP-862: a chip's "Remove account". The page owns the confirm.
    let onRemove: (AgentProfileUsageRow) -> Void

    /// The picked agent tab, or nil while it follows the first reported agent.
    /// View state: a fresh page opens on the leading agent.
    @State private var agentTab: String?
    /// EXP-862: the "+ Add account" sheet (device + agent, then the login).
    @State private var addingAccount = false

    var body: some View {
        let now = Date()
        let agents = viewModel.accountAgents
        let tab = resolvedTab(agents)
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 0) {
                accountsBand
                if !viewModel.accountsLoaded {
                    loadingRow
                } else if agents.isEmpty {
                    emptyRow
                } else {
                    // EXP-849: tabs, not bands — a lone agent is not a choice.
                    if agents.count > 1 {
                        agentTabs(agents, selection: tab)
                    }
                    ForEach(viewModel.groupsForAgent(tab)) { group in
                        AgentAccountRow(
                            group: group,
                            now: now,
                            viewModel: viewModel,
                            addTargets: viewModel.addAccountTargets(group),
                            onSignIn: onSignIn,
                            onRemove: onRemove
                        )
                    }
                }
            }
        }
    }

    /// The tab the rows follow: the pick while the agent still reports, else
    /// the leading (contract-ordered) one.
    private func resolvedTab(_ agents: [String]) -> String {
        if let agentTab, agents.contains(agentTab) { return agentTab }
        return agents.first ?? "claude"
    }

    /// EXP-849: the per-agent tabs, as the section's first row — the same
    /// brand-marked strip the launch options wear, so "claude" looks the same
    /// everywhere.
    private func agentTabs(_ agents: [String], selection: String) -> some View {
        GlassSegmentedControl(
            options: agents,
            selection: selection,
            label: { LaunchVocabulary.agentLabel($0) },
            icon: { AgentBrandMark.image($0) },
            style: .embedded,
            onSelect: { agentTab = $0 }
        )
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .flatRow()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("accounts-agent-tabs")
    }

    /// The section's own heading — plain text, because the per-agent tabs
    /// below it already carry the filled strip. EXP-862: its one control is
    /// "+ Add account" (the "+ Add device" twin on web and the IDE); the
    /// "Refreshes every 5 minutes" caption is gone — the section keeps itself
    /// current and saying so was noise.
    private var accountsBand: some View {
        GlassSectionHeader("Accounts") {
            GlassPill("Add account", icon: AppIcons.uiAdd, mode: .action {
                addingAccount = true
            })
            .accessibilityIdentifier("add-account-button")
        }
        .accessibilityIdentifier("accounts-header")
        .sheet(isPresented: $addingAccount) {
            AddAccountSheet(viewModel: viewModel)
        }
    }

    private var loadingRow: some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small).tint(.white)
            Text("Loading…")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
    }

    /// Web parity, word for word: the same glyph and sentence the web section
    /// shows when no synced machine has reported an account.
    private var emptyRow: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.small)
            Text("No machine has reported an agent account yet.")
                .font(.caption)
            Spacer(minLength: 0)
        }
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
    }
}

/// One account: the login, its health when there is something to say about it,
/// the device chips (plus a bare "+" for signing it in on another device), and
/// the freshest report's cards.
private struct AgentAccountRow: View {
    let group: AgentAccountUsageGroup
    let now: Date
    /// The chips' actions ride the same owner→device queue the device rows use.
    let viewModel: AgentsViewModel
    /// EXP-862: the caller's online, login-capable devices that do NOT hold
    /// this account yet — the "+" chip's menu ("Sign in on <device>").
    let addTargets: [SteerDevice]
    let onSignIn: (AgentLoginTarget) -> Void
    let onRemove: (AgentProfileUsageRow) -> Void

    private var fresh: Bool {
        AgentUsagePresentation.isFresh(fetchedAt: group.usage?.fetchedAt, now: now)
    }

    private var hasWindows: Bool {
        !(group.usage?.windows ?? []).isEmpty
    }

    /// The "as of …" text off the freshest stamp on record, empty when none.
    private var asOf: String {
        agentUsageRelativeDate(group.usage?.fetchedAt ?? group.checkedAt)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                title
                    .lineLimit(1)
                    .truncationMode(.middle)
                healthBadge
                Spacer(minLength: 0)
            }
            FlowLayout(spacing: 6) {
                ForEach(group.rows) { row in
                    AgentAccountDeviceChip(
                        row: row,
                        device: viewModel.devices?.first { $0.deviceId == row.deviceId },
                        viewModel: viewModel,
                        onSignIn: onSignIn,
                        onRemove: onRemove
                    )
                }
                addChip
            }
            usageBlock
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
        .accessibilityIdentifier("account-row-\(group.key)")
    }

    /// The email / the bare plan / the profile's label — who the account IS.
    /// EXP-862: never its sign-in state; the chip badges say that.
    private var title: Text {
        let caption = AgentAccountsRows.groupCaption(group)
        let lead = Text(caption)
            .font(.subheadline.weight(.medium))
            .foregroundStyle(Color.white)
        guard group.email != nil, let plan = group.plan else { return lead }
        return lead + Text(" · \(plan)")
            .font(.subheadline)
            .foregroundStyle(Color.white.opacity(TextOpacity.quaternary))
    }

    /// EXP-849/EXP-862: the health badge the ×4 rule produces — for BOTH
    /// attention states now that the title no longer announces a signed-out
    /// login.
    @ViewBuilder
    private var healthBadge: some View {
        if let label = AgentAccountsRows.healthBadge(group) {
            GlassPill(
                label,
                icon: AppIcons.uiWarning,
                tint: DesignTokens.Semantic.yellow
            )
            .accessibilityIdentifier("account-health-\(group.key)")
        }
    }

    /// EXP-862: sign this SAME account in on another device — a bare "+" chip
    /// whose menu lists the devices that could take it ("Sign in on <device>",
    /// ×4). Absent when every eligible device already holds the account.
    @ViewBuilder
    private var addChip: some View {
        if !addTargets.isEmpty {
            GlassMenu {
                ForEach(addTargets) { device in
                    GlassMenuItem(
                        "Sign in on \(LaunchVocabulary.deviceName(device))",
                        icon: device.isServer ? AppIcons.uiServer : AppIcons.uiDevice
                    ) {
                        onSignIn(AgentLoginTarget(
                            deviceId: device.deviceId,
                            deviceLabel: LaunchVocabulary.deviceName(device),
                            agent: group.agent
                        ))
                    }
                }
            } label: {
                GlassPill("", icon: AppIcons.uiAdd, mode: .readonly)
            }
            .accessibilityLabel("Add a device to this account")
            .accessibilityIdentifier("account-add-device-\(group.key)")
        }
    }

    /// The freshest report's cards, or the one line that says why there are
    /// none. EXP-862: a signed-in login nothing has probed YET reads
    /// "Checking…" — "No usage reported" made a device that is simply still
    /// working read as broken.
    @ViewBuilder
    private var usageBlock: some View {
        if hasWindows, let usage = group.usage {
            VStack(alignment: .leading, spacing: 4) {
                AgentUsageCards(usage: usage, compact: true)
                // Numbers past the freshness window say how old they are; the
                // device's own stale flag already dims the cards and is a
                // separate signal.
                if !fresh, usage.stale != true, !asOf.isEmpty {
                    Text("as of \(asOf)")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .opacity(fresh ? 1 : 0.5)
        } else if group.signedIn, asOf.isEmpty {
            Text("Checking…")
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        } else {
            Text(asOf.isEmpty ? "No usage reported" : "No usage reported · as of \(asOf)")
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
    }
}

/// EXP-818's device chip: the online dot, the device (· profile), a CHECK where
/// the account is that device's ACTIVE login, and an amber warning glyph where
/// THAT device's copy of the login is broken.
///
/// EXP-862 gave it the SAME menu the device rows' chips carry (Sign in / Set as
/// default / Remove account, from the one shared rule): a person looking at an
/// account here should not have to go find the device row to fix it. A
/// teammate's device, an offline one, or one whose build takes none of the
/// commands renders the chip as the statement it used to be.
private struct AgentAccountDeviceChip: View {
    let row: AgentProfileUsageRow
    /// The live device row behind this chip — its caps decide the menu. Nil
    /// (a device that left the list) renders read-only.
    let device: SteerDevice?
    let viewModel: AgentsViewModel
    let onSignIn: (AgentLoginTarget) -> Void
    let onRemove: (AgentProfileUsageRow) -> Void

    @ViewBuilder
    var body: some View {
        if let device, isActionable(device) {
            GlassMenu {
                menuItems(device)
            } label: {
                pill
            }
            .accessibilityLabel("\(AgentAccountsRows.chipLabel(row)) actions")
            .accessibilityIdentifier("account-chip-\(row.key)")
        } else {
            pill
                .accessibilityIdentifier("account-chip-\(row.key)")
        }
    }

    /// The same gate the device rows apply: mine, listening, and advertising
    /// `agent-login` — plus at least one entry to show.
    private func isActionable(_ device: SteerDevice) -> Bool {
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
    private func menuItems(_ device: SteerDevice) -> some View {
        if AgentAccountsRows.chipSignsIn(row) {
            GlassMenuItem("Sign in", icon: AppIcons.uiSignIn) {
                onSignIn(AgentLoginTarget(
                    deviceId: row.deviceId,
                    deviceLabel: row.deviceLabel.isEmpty ? row.deviceId : row.deviceLabel,
                    agent: row.agent,
                    profileId: row.profileId
                ))
            }
        } else {
            if AgentAccountsRows.chipSetsDefault(row, canSwitchAccount: device.canSwitchAccount) {
                GlassMenuItem("Set as default", icon: AppIcons.uiSwap) {
                    viewModel.useAccountHere(row)
                }
            }
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

    private var pill: some View {
        GlassPill(
            AgentAccountsRows.chipLabel(row),
            mode: .readonly,
            dot: row.online
                ? DesignTokens.Semantic.green
                : Color.white.opacity(TextOpacity.quaternary)
        ) {
            EmptyView()
        } trailing: {
            if viewModel.isAccountActionPending(row) {
                ProgressView()
                    .controlSize(.mini)
                    .tint(.white)
                    .accessibilityLabel("Working…")
            } else if let badge = row.health.badgeLabel {
                AppIcon(AppIcons.uiWarning, size: GlassPillSize.sm.glyphSize)
                    .foregroundStyle(DesignTokens.Semantic.yellow)
                    .accessibilityLabel("\(badge) on this device")
            } else if row.signedIn, row.active {
                AppIcon(AppIcons.uiCheck, size: GlassPillSize.sm.glyphSize)
                    .foregroundStyle(DesignTokens.Semantic.green)
                    .accessibilityLabel("Active on this device")
            }
        }
        .opacity(row.online ? 1 : 0.7)
    }
}
