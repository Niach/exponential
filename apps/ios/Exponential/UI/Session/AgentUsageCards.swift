import ExpCore
import ExpUI
import SwiftUI

/// EXP-688: agent rate-limit usage as CARDS — one per window the machine
/// reported, grouped Current session / Weekly limits / Other.
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
/// header — the card itself already says "Current session".
struct AgentUsageCards: View {
    let usage: AgentUsage
    /// Device settings renders the same cards a shade tighter, inside a form
    /// row that already has its own chrome.
    var compact = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            ForEach(AgentUsagePresentation.usageGroups(usage, now: Date())) { group in
                VStack(alignment: .leading, spacing: 8) {
                    if group.key != "session" {
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
struct AgentUsageCardRow: View {
    let card: UsageCard
    var compact = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
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
        .padding(.horizontal, 12)
        .padding(.vertical, compact ? 8 : 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassRow()
        .accessibilityElement(children: .combine)
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
        case .normal: return .white.opacity(0.35)
        case .warning: return DesignTokens.Semantic.yellow
        case .danger: return DesignTokens.Semantic.red
        }
    }

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.white.opacity(0.12))
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
/// Opens at the medium detent and expands to large when a machine reports many
/// windows.
struct AgentUsageSheet: View {
    let usage: AgentUsage
    /// The host machine's sign-in status for THIS session's agent, when it
    /// reported one — the agent name is already the sheet's context, so the
    /// caption drops the `<agent> · ` prefix `accountRow` adds.
    let account: AgentAccount?

    var body: some View {
        GlassSheetChrome(title: "Usage", detents: [.medium, .large]) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    if let account {
                        Text(AgentUsagePresentation.accountCaption(account))
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    AgentUsageCards(usage: usage)
                    if let staleCaption {
                        Text(staleCaption)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                .padding(.horizontal, 14)
                .padding(.top, 4)
                .padding(.bottom, 24)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    /// Numbers the machine could not refresh say so, in the same words the
    /// other three clients use.
    private var staleCaption: String? {
        guard usage.stale == true else { return nil }
        let asOf = agentUsageRelativeDate(usage.fetchedAt)
        return asOf.isEmpty ? nil : "as of \(asOf)"
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
