import ExpCore
import ExpUI
import SwiftUI

/// EXP-829/EXP-849: the Devices page's **Accounts** section — the DECISION
/// surface for agent logins.
///
/// One row per agent ACCOUNT (an agent plus the login the machines named):
/// who it is, what it may spend, how much of that is left, and whether it
/// still works (EXP-849 `health`). Attention first — a signed-out or refused
/// login leads, then anything at or over the danger threshold, then the rest.
///
/// EXP-849 split the two surfaces that used to be one list:
///   - HERE (Accounts) you decide WHICH account to use: the numbers are the
///     freshest machine's (they are the account's limits, so every machine
///     reads the same ones) and the machines holding it are QUIET chips — a
///     presence indicator with a check where the account is that machine's
///     ACTIVE login, not a control.
///   - Repair lives on the machine rows above ("My machines"): re-login, "use
///     this account here", the per-machine health badge. A chip here used to
///     be the only way into that, which made Accounts a settings menu in
///     disguise.
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

    /// The picked agent tab, or nil while it follows the first reported agent.
    /// View state: a fresh page opens on the leading agent.
    @State private var agentTab: String?

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
                            refreshing: viewModel.refreshingAccounts.contains(group.key),
                            onRefresh: { viewModel.refreshAccount(group) }
                        )
                    }
                }
            }
            if let error = viewModel.accountError {
                Text(error)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .padding(.horizontal, 4)
                    .frame(maxWidth: .infinity, alignment: .leading)
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
    /// below it already carry the filled strip.
    private var accountsBand: some View {
        GlassSectionHeader("Accounts") {
            if viewModel.accountsAutoRefresh {
                Text("Refreshes every 5 minutes")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .accessibilityIdentifier("accounts-header")
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

/// One account: the login (amber "Not signed in" when there is none), its
/// health when there is something to say about it, the refresh glyph when one
/// of MY machines may run it, the quiet machine chips, and the freshest
/// report's cards.
private struct AgentAccountRow: View {
    let group: AgentAccountUsageGroup
    let now: Date
    let refreshing: Bool
    let onRefresh: () -> Void

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
                if group.refreshTarget != nil {
                    refreshControl
                }
            }
            FlowLayout(spacing: 6) {
                ForEach(group.rows) { row in
                    AgentAccountDeviceChip(row: row)
                }
            }
            if hasWindows, let usage = group.usage {
                VStack(alignment: .leading, spacing: 4) {
                    AgentUsageCards(usage: usage, compact: true)
                    // Numbers past the freshness window say how old they are;
                    // the device's own stale flag already dims the cards and
                    // is a separate signal.
                    if !fresh, usage.stale != true, !asOf.isEmpty {
                        Text("as of \(asOf)")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                .opacity(fresh ? 1 : 0.5)
            } else {
                Text(asOf.isEmpty ? "No usage reported" : "No usage reported · as of \(asOf)")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
        .accessibilityIdentifier("account-row-\(group.key)")
    }

    /// `Not signed in` / the email / the bare plan — plus a muted ` · <plan>`
    /// tail when both the email and the plan are known (web parity).
    private var title: Text {
        let caption = AgentAccountsRows.groupCaption(group)
        let lead = Text(caption)
            .font(.subheadline.weight(.medium))
            .foregroundStyle(group.signedIn ? Color.white : DesignTokens.Semantic.yellow)
        guard group.signedIn, group.email != nil, let plan = group.plan else { return lead }
        return lead + Text(" · \(plan)")
            .font(.subheadline)
            .foregroundStyle(Color.white.opacity(TextOpacity.quaternary))
    }

    /// EXP-849: the health badge the ×4 rule produces — a signed-out login
    /// already SAYS so in the title (a badge beside it would be the same word
    /// twice), and the two quiet states have nothing to report, so in practice
    /// this is the expired credential the caption cannot express.
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

    /// The refresh glyph — a spinner while a queued refresh waits for the
    /// machine, greyed inside the device's rate-limit floor.
    @ViewBuilder
    private var refreshControl: some View {
        if refreshing {
            ProgressView()
                .controlSize(.small)
                .tint(.white)
                .frame(width: DesignTokens.Size.controlSm, height: DesignTokens.Size.controlSm)
                .accessibilityLabel("Refreshing usage")
        } else {
            CircleIconButton(
                AppIcons.uiRefresh,
                accessibilityLabel: "Refresh usage",
                size: DesignTokens.Size.controlSm,
                glyphSize: AppIcon.Size.small,
                enabled: AgentAccountsRows.refreshAllowedAt(group.usage, now: now) == nil,
                action: onRefresh
            )
        }
    }
}

/// EXP-818's machine chip, EXP-849's quiet indicator: the online dot, the
/// machine (· profile), a CHECK where the account is that machine's ACTIVE
/// login, and an amber warning glyph where THAT machine's copy of the login is
/// broken. Not a control — every account ACTION lives on the machine's own row
/// ("My machines"), which is the surface that can repair one.
private struct AgentAccountDeviceChip: View {
    let row: AgentProfileUsageRow

    var body: some View {
        GlassPill(
            AgentAccountsRows.chipLabel(row),
            mode: .readonly,
            dot: row.online
                ? DesignTokens.Semantic.green
                : Color.white.opacity(TextOpacity.quaternary)
        ) {
            EmptyView()
        } trailing: {
            if let badge = row.health.badgeLabel {
                AppIcon(AppIcons.uiWarning, size: GlassPillSize.sm.glyphSize)
                    .foregroundStyle(DesignTokens.Semantic.yellow)
                    .accessibilityLabel("\(badge) on this machine")
            } else if row.signedIn, row.active {
                AppIcon(AppIcons.uiCheck, size: GlassPillSize.sm.glyphSize)
                    .foregroundStyle(DesignTokens.Semantic.green)
                    .accessibilityLabel("Active on this machine")
            }
        }
        .opacity(row.online ? 1 : 0.7)
        .accessibilityIdentifier("account-chip-\(row.key)")
    }
}
