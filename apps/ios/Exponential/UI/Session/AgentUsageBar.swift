import ExpCore
import ExpUI
import SwiftUI

/// EXP-484: the agent rate-limit usage bar — one thin line that says how close
/// the machine running this agent is to its limit, and an expanded list of
/// every window it reported.
///
/// The device is the only writer (it probes its own CLIs and reports
/// `agent_usage` on heartbeat); everything here reads the synced row through
/// the pure rules in ExpCore's `AgentUsagePresentation`, which the web,
/// Android and desktop bars mirror against the same fixture. Which window is
/// on the collapsed bar is a per-phone VIEWING preference
/// (`AgentUsageWindowPrefs`), never machine state.

/// The collapsed bar: a 2pt track for the selected window, tapped to reveal
/// every window. The visible line is deliberately hairline — the tap target
/// around it is not.
struct AgentUsageStrip: View {
    let agent: String
    let usage: AgentUsage

    @State private var expanded = false
    /// nil until `onAppear` reads the pref — `selectWindow` then falls back to
    /// the fullest window on its own.
    @State private var selectedKey: String?

    /// Roughly a row's worth of height around the hairline, so the toggle is
    /// hittable without the bar looking like a control.
    private static let tapHeight: CGFloat = 14

    private var window: AgentUsageWindow? {
        AgentUsagePresentation.selectWindow(usage, preferredKey: selectedKey)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button {
                withAnimation(.easeInOut(duration: 0.18)) { expanded.toggle() }
            } label: {
                AgentUsageTrack(percent: window?.percent, height: 2)
                    .frame(height: Self.tapHeight)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(accessibilityLabel)
            .accessibilityHint(expanded ? "Hide every window" : "Show every window")

            if expanded {
                AgentUsageWindowRows(
                    agent: agent,
                    usage: usage,
                    selectedKey: selectedKey,
                    onSelect: { key in
                        selectedKey = key
                        AgentUsageWindowPrefs.remember(agent: agent, key: key)
                        withAnimation(.easeInOut(duration: 0.18)) { expanded = false }
                    }
                )
                .transition(.opacity)
            }
        }
        // Numbers the machine kept after a failed refresh still read, they
        // just stop claiming to be current.
        .opacity(usage.stale == true ? 0.5 : 1)
        .onAppear {
            guard selectedKey == nil else { return }
            selectedKey = AgentUsageWindowPrefs.read(agent: agent)
        }
    }

    private var accessibilityLabel: String {
        let label = LaunchVocabulary.agentLabel(agent)
        guard let window else { return "\(label) usage" }
        var text = "\(label) usage: \(window.label)"
        if let percent = window.percent {
            text += " \(Int(percent.rounded()))%"
        }
        if let countdown = AgentUsagePresentation.resetCountdown(resetsAt: window.resetsAt) {
            text += ", \(countdown)"
        }
        return text
    }
}

/// Every window the machine reported, one row each, with the one the collapsed
/// bar draws marked. Picking a row is what `AgentUsageWindowPrefs` remembers —
/// the owner passes the current key in and persists the pick, so the same list
/// serves the session strip and the device settings sheet.
struct AgentUsageWindowRows: View {
    let agent: String
    let usage: AgentUsage
    let selectedKey: String?
    let onSelect: (String) -> Void

    /// What the bar actually draws — the remembered key only wins while the
    /// machine still reports it, so the marker can never point at nothing.
    private var effectiveKey: String? {
        AgentUsagePresentation.selectWindow(usage, preferredKey: selectedKey)?.key
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(usage.windows ?? []) { window in
                Button {
                    onSelect(window.key)
                } label: {
                    row(window, selected: window.key == effectiveKey)
                }
                .buttonStyle(.plain)
            }
            if let staleCaption {
                Text(staleCaption)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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

    private func row(_ window: AgentUsageWindow, selected: Bool) -> some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(selected ? AppIcons.uiSelected : AppIcons.uiUnselected, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(selected ? TextOpacity.primary : TextOpacity.quaternary))
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(window.label)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    Text(percentText(window.percent))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                AgentUsageTrack(percent: window.percent, height: 6)
                if let countdown = AgentUsagePresentation.resetCountdown(resetsAt: window.resetsAt) {
                    Text(countdown)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
    }

    private func percentText(_ percent: Double?) -> String {
        guard let percent else { return "—" }
        return "\(Int(percent.rounded()))%"
    }
}

/// The track itself: a rounded rail with the used share filled in the tone the
/// locked severity thresholds pick (≥95 red, ≥75 amber, otherwise a muted
/// white). A window with no percentage draws an empty rail rather than a lie.
private struct AgentUsageTrack: View {
    let percent: Double?
    let height: CGFloat

    private var tone: Color {
        switch AgentUsagePresentation.severity(percent) {
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
        // The rail keeps its own hairline height; a caller that wants a bigger
        // tap target wraps this in a taller frame, which centres it.
        .frame(height: height)
        .accessibilityHidden(true)
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
