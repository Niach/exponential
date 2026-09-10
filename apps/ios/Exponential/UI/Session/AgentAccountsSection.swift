import ExpCore
import ExpUI
import SwiftUI

/// EXP-829: the Devices page's **Accounts** section — web/desktop EXP-818's
/// Usage page folded into Devices, so ONE page says which machines exist and
/// which agent accounts are live on them.
///
/// One row per agent ACCOUNT (an agent plus the login the machines named)
/// under an agent band, attention first: signed-out accounts lead, then
/// anything at or over the danger threshold, then the rest. The machines
/// holding the account are chips on the row — a chip wears a CHECK where the
/// account is the ACTIVE login on that machine, and tapping a chip of one of
/// MY machines opens that machine's settings sheet (where agent sign-in
/// lives — the same destination the machine row's Edit offers). The numbers
/// are the FRESHEST machine's report (they are the account's limits, so every
/// machine reads the same ones), drawn by the same `AgentUsageCards` the
/// Usage sheet and Device settings render; stale numbers keep the dimmed
/// "as of …" treatment.
///
/// The section owns no model of its own: rows, groups, ordering and the
/// refresh floor are `AgentAccountsRows` (ExpCore, the ×4 rule), the refresh
/// round-trip is `AgentsViewModel` — this file is layout.
struct AgentAccountsSection: View {
    let viewModel: AgentsViewModel
    /// A chip of one of the caller's own machines was tapped: open that
    /// machine's device settings sheet.
    let onOpenDevice: (String) -> Void

    var body: some View {
        let now = Date()
        Group {
            GlassSectionHeader("Accounts") {
                if viewModel.accountsAutoRefresh {
                    Text("Refreshes every 5 minutes")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .accessibilityIdentifier("accounts-header")

            if !viewModel.accountsLoaded {
                loadingRow
            } else if viewModel.accountSections.isEmpty {
                emptyRow
            } else {
                ForEach(viewModel.accountSections) { section in
                    // Contract agent order — a band only renders when a
                    // machine reported the agent.
                    Text(LaunchVocabulary.agentLabel(section.agent))
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.horizontal, 4)
                        .padding(.top, 4)
                    ForEach(section.groups) { group in
                        AgentAccountRow(
                            group: group,
                            now: now,
                            refreshing: viewModel.refreshingAccounts.contains(group.key),
                            onRefresh: { viewModel.refreshAccount(group) },
                            onOpenDevice: onOpenDevice
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
        .glassRow()
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
        .glassRow()
    }
}

/// One account: the login (amber "Not signed in" when there is none), the
/// refresh glyph when one of MY machines may run it, the machine chips, and
/// the freshest report's cards.
private struct AgentAccountRow: View {
    let group: AgentAccountUsageGroup
    let now: Date
    let refreshing: Bool
    let onRefresh: () -> Void
    let onOpenDevice: (String) -> Void

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
                Spacer(minLength: 0)
                if group.refreshTarget != nil {
                    refreshControl
                }
            }
            FlowLayout(spacing: 6) {
                ForEach(group.rows) { row in
                    AgentAccountDeviceChip(row: row, onOpenDevice: onOpenDevice)
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
        .glassRow()
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

/// EXP-818's machine chip: the online dot, the machine (· profile), and a
/// CHECK when the account is the ACTIVE login on that machine. A chip of one
/// of MY machines opens its settings sheet — sign-in and account switching
/// live there; a teammate's shared server is read-only here.
private struct AgentAccountDeviceChip: View {
    let row: AgentProfileUsageRow
    let onOpenDevice: (String) -> Void

    private var mode: GlassPillMode {
        row.mine ? .action { onOpenDevice(row.deviceId) } : .readonly
    }

    var body: some View {
        GlassPill(
            AgentAccountsRows.chipLabel(row),
            mode: mode,
            dot: row.online
                ? DesignTokens.Semantic.green
                : Color.white.opacity(TextOpacity.quaternary)
        ) {
            EmptyView()
        } trailing: {
            if row.signedIn, row.active {
                AppIcon(AppIcons.uiCheck, size: GlassPillSize.sm.glyphSize)
                    .foregroundStyle(DesignTokens.Semantic.green)
                    .accessibilityLabel("Active on this machine")
            }
        }
        .opacity(row.online ? 1 : 0.7)
        .accessibilityIdentifier("account-chip-\(row.key)")
    }
}
