import ExpCore
import ExpUI
import SwiftUI

/// EXP-688: agent rate-limit usage as CARDS — one per window the machine
/// reported, grouped Current session / (untitled weekly) / Other.
///
/// This replaces the EXP-484 hairline strip and its radio "pinned window"
/// rows: there is no tracked-window concept any more on any client. The device
/// is still the only writer (it probes its own CLIs and reports `agent_usage`
/// on heartbeat); everything here reads the synced row through the pure rules
/// in ExpCore's `AgentUsagePresentation.usageGroups`, which the web, Android
/// and desktop cards mirror against the same fixture.
///
/// Two hosts: the steering screen's "Usage" sheet (`AgentUsageSheet`) and each
/// agent's tab in Device settings (`compact`).

/// Every group the report yields, headers and all. Session cards carry no
/// header — the card itself already says "Current session" — and neither does
/// any group the rules left untitled (EXP-694's weekly group).
struct AgentUsageCards: View {
    let usage: AgentUsage
    /// Device settings renders the same numbers as FLAT rows (EXP-694): no
    /// card chrome of their own, because the grouped row they sit in already
    /// has it.
    var compact = false

    var body: some View {
        VStack(alignment: .leading, spacing: compact ? 12 : 16) {
            ForEach(AgentUsagePresentation.usageGroups(usage, now: Date())) { group in
                VStack(alignment: .leading, spacing: 8) {
                    if group.key != "session", !group.title.isEmpty {
                        Text(group.title)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                    ForEach(group.cards) { card in
                        AgentUsageCardRow(card: card, compact: compact)
                    }
                }
            }
        }
        // Numbers the machine kept after a failed refresh still read, they
        // just stop claiming to be current.
        .opacity(usage.stale == true ? 0.5 : 1)
    }
}

/// One card: title + `n% used`, the severity-toned track, and the caption
/// (`resets in 2h 10m`, or the idle session window's "Starts when a message is
/// sent") when the rules produced one.
///
/// EXP-694: `compact` is a FLAT row — no `.glassRow()` and no horizontal
/// padding, because the grouped card hosting it already draws both. Only the
/// standalone Usage sheet still carries card chrome.
struct AgentUsageCardRow: View {
    let card: UsageCard
    var compact = false

    @ViewBuilder
    var body: some View {
        if compact {
            content
                .padding(.vertical, 2)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .combine)
        } else {
            content
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassRow()
                .accessibilityElement(children: .combine)
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: compact ? 6 : 8) {
            HStack(spacing: 8) {
                Text(card.title)
                    .font(compact ? .subheadline.weight(.medium) : .body.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 0)
                Text(percentText)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            AgentUsageTrack(percent: card.percent, severity: card.severity)
            if !card.caption.isEmpty {
                Text(card.caption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
    }

    /// A window the machine reported without a number draws an empty rail and
    /// says so, rather than claiming 0% used.
    private var percentText: String {
        guard let percent = card.percent else { return "—" }
        return "\(Int(percent.rounded()))% used"
    }
}

/// The track itself: a rounded rail with the used share filled in the tone the
/// locked severity thresholds pick (≥95 red, ≥75 amber, otherwise a muted
/// white). A window with no percentage draws an empty rail rather than a lie.
struct AgentUsageTrack: View {
    let percent: Double?
    let severity: AgentUsageSeverity
    var height: CGFloat = 6

    private var tone: Color {
        switch severity {
        case .normal: return GlassTokens.usageFill
        case .warning: return DesignTokens.Semantic.yellow
        case .danger: return DesignTokens.Semantic.red
        }
    }

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(GlassTokens.strokeStrong)
                Capsule()
                    .fill(tone)
                    .frame(width: geo.size.width * min(max((percent ?? 0) / 100, 0), 1))
            }
        }
        .frame(height: height)
        .accessibilityHidden(true)
    }
}

/// The steering screen's "Usage" sheet (EXP-688) — the `…` menu's Usage entry.
/// Content-fitted (EXP-687): a machine reporting many windows grows the sheet
/// up to the shared 85 % cap, then scrolls.
///
/// EXP-746: it opens on EITHER half now. The machine's rate-limit report is
/// optional (a fresh run on a machine that reported nothing still has its own
/// numbers) and the ACP engine's per-run context/spend rides above it as its
/// own "Context" block — deliberately NOT folded into `usageGroups`, whose
/// percent cards are fixture-locked ×4 and would draw an empty rail for a
/// token count.
///
/// EXP-849 makes it the run's ACCOUNT surface too: under the numbers sit the
/// logins the host machine reports for this run's agent, each with its own
/// bars, and — claude only, between turns, on the run's own machine — a
/// "Switch to this account" that resumes the run under that login. That is why
/// the usage/context readout is a CONTROL on every client now: "I am out of
/// limit" and "use my other account" are one thought.
struct AgentUsageSheet: View {
    let usage: AgentUsage?
    /// The host machine's sign-in status for THIS session's agent, when it
    /// reported one — the agent name is already the sheet's context, so the
    /// caption drops the `<agent> · ` prefix `accountRow` adds.
    let account: AgentAccount?
    /// EXP-746: this run's own context window and spend off the relay's
    /// latest-wins `usage` event.
    var sessionUsage: AgentSessionUsage? = nil
    /// EXP-849: the host machine's logins for this run's agent
    /// (`AgentSessionModel.accountOptions`). Empty = the machine said nothing
    /// about the agent, so there is nothing to switch between and the block is
    /// absent.
    var accounts: [SessionAccountOption] = []
    /// EXP-849: whether this run's agent can change login at all (claude). A
    /// codex run still lists its accounts — read-only, because switching there
    /// means starting the next run on the other one.
    var supportsSwitch: Bool = false
    /// EXP-849: why switching onto a given login would be refused right now
    /// (`AgentSessionModel.accountSwitchRefusal`, the ×4 rule). The control
    /// stays and says so — nil for the rows a switch would take.
    var switchRefusal: ((SessionAccountOption) -> String?)? = nil
    /// A switch is on the wire — every row's control waits.
    var switching: Bool = false
    var onSwitch: ((SessionAccountOption) -> Void)? = nil

    var body: some View {
        GlassSheetChrome(title: "Usage") {
            VStack(alignment: .leading, spacing: 16) {
                if let account {
                    Text(AgentUsagePresentation.accountCaption(account))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let sessionUsage {
                    SessionContextBlock(usage: sessionUsage)
                }
                if let usage {
                    AgentUsageCards(usage: usage)
                }
                if let staleCaption {
                    Text(staleCaption)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                accountsBlock
            }
            .padding(.horizontal, 16)
            .padding(.top, 4)
            .padding(.bottom, 24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// EXP-849: the run's accounts — one row per login the host machine
    /// reports for its agent, the machine's current one marked, each with its
    /// own bars, and the switch where one is allowed.
    @ViewBuilder
    private var accountsBlock: some View {
        if !accounts.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text(SessionAccountSwitch.sectionTitle)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                ForEach(accounts) { option in
                    SessionAccountRow(
                        option: option,
                        showsSwitch: supportsSwitch && onSwitch != nil,
                        refusal: switchRefusal?(option),
                        switching: switching,
                        onSwitch: { onSwitch?(option) }
                    )
                }
                // The one-time cost, stated BEFORE the tap — the relaunch
                // re-reads the transcript on the account moved to.
                if supportsSwitch, accounts.count > 1 {
                    Text(SessionAccountSwitch.costNote)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .accessibilityIdentifier("session-accounts")
        }
    }

    /// Numbers the machine could not refresh say so, in the same words the
    /// other three clients use.
    private var staleCaption: String? {
        guard usage?.stale == true else { return nil }
        let asOf = agentUsageRelativeDate(usage?.fetchedAt)
        return asOf.isEmpty ? nil : "as of \(asOf)"
    }
}

/// EXP-849: one login of the run's host machine inside the Usage sheet — who it
/// is, its health, whether it is that machine's CURRENT login, its own bars,
/// and the switch. The control STAYS when a switch would be refused and the
/// reason sits under the row: a vanished button teaches nothing. Mirrors
/// Android's `SessionAccountRow` field for field.
struct SessionAccountRow: View {
    let option: SessionAccountOption
    /// Whether this run's agent supports switching at all (claude). A codex run
    /// lists its accounts without controls.
    let showsSwitch: Bool
    /// Why a switch onto this login is refused right now, nil when it is not.
    let refusal: String?
    let switching: Bool
    let onSwitch: () -> Void

    /// `max · Active login` — the plan (when it is not already the title) and
    /// the machine's CURRENT login. Never a claim about which account THIS run
    /// is on: that stays server-side.
    private var subtitle: String {
        var parts: [String] = []
        if let plan = option.plan, plan != option.caption { parts.append(plan) }
        if option.active { parts.append("Active login") }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(option.caption)
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(
                                option.signedIn ? Color.white : DesignTokens.Semantic.yellow
                            )
                            .lineLimit(1)
                            .truncationMode(.middle)
                        if let badge = option.health.badgeLabel {
                            Text(badge)
                                .font(.caption2)
                                .foregroundStyle(DesignTokens.Semantic.yellow)
                                .lineLimit(1)
                        }
                    }
                    if !subtitle.isEmpty {
                        Text(subtitle)
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
                if showsSwitch {
                    switchControl
                }
            }
            if let usage = option.usage, !(usage.windows ?? []).isEmpty {
                AgentUsageCards(usage: usage, compact: true)
            }
            // The refusal rides the ROW the control is on, so it is read where
            // the tap was meant to happen.
            if let refusal {
                Text(refusal)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassRow()
        .accessibilityIdentifier("session-account-\(option.profileId)")
    }

    @ViewBuilder
    private var switchControl: some View {
        if switching {
            ProgressView()
                .controlSize(.small)
                .tint(.white)
                .accessibilityLabel("Switching account")
        } else {
            GlassPill(
                SessionAccountSwitch.switchLabel,
                icon: AppIcons.uiSwap,
                mode: .action(onSwitch),
                enabled: refusal == nil
            )
            .accessibilityIdentifier("switch-account-\(option.profileId)")
        }
    }
}

/// EXP-746: the run's own context window and spend — `124k / 200k (62%)` with
/// the same severity-toned track the rate-limit cards use, and the spend
/// beside it when there is one worth printing. The strings come from the
/// ×4-locked pure rules; nothing here formats a number itself.
struct SessionContextBlock: View {
    let usage: AgentSessionUsage

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(AgentUsagePresentation.contextSectionTitle)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    Text(
                        AgentUsagePresentation.formatContextUsage(
                            used: usage.contextUsed, size: usage.contextSize
                        ) ?? "—"
                    )
                    .font(.subheadline.weight(.medium).monospacedDigit())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    Spacer(minLength: 0)
                    if let cost = AgentUsagePresentation.formatUsageCost(usage.costUsd) {
                        Text(cost)
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                }
                AgentUsageTrack(
                    percent: usage.percent.map(Double.init),
                    severity: AgentUsagePresentation.severity(usage.percent.map(Double.init))
                )
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassRow()
            .accessibilityElement(children: .combine)
        }
    }
}

/// The AgentsView relative-date idiom, shared by the usage surfaces: Electric
/// syncs timestamps as Postgres text (space separator, hour-only offset),
/// which `ISO8601DateFormatter` alone rejects — `WireTimestamps` handles both
/// wire forms (EXP-169). Empty for an absent or unreadable stamp, so callers
/// can drop the caption entirely.
func agentUsageRelativeDate(_ value: String?) -> String {
    guard let value, let date = WireTimestamps.parse(value) else { return "" }
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .short
    return formatter.localizedString(for: date, relativeTo: Date())
}
