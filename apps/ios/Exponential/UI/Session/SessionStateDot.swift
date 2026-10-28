import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the ONE session state dot. EXP-1295: its tone is the
/// `SessionState`'s fixture tone (`session-card.json` `tones`: muted |
/// foreground | amber | rose). It pulses ONLY while `pulsing` (the agent
/// inside a turn, EXP-848) and the state is `working`.
struct SessionStateDot: View {
    let state: SessionState
    var pulsing: Bool = false
    var size: CGFloat = 9

    var body: some View {
        if pulsing, state == .working {
            PulsingLiveDot(size: size)
        } else {
            Circle()
                .fill(Self.color(state))
                .frame(width: size, height: size)
        }
    }

    /// The dot's fill: a working session (EXP-1374: or one whose background
    /// tasks still run) is the live green, the attention tones their
    /// semantic colour, everything else neutral.
    static func color(_ state: SessionState) -> Color {
        if state == .working { return DesignTokens.Semantic.green }
        return sessionStateBadgeColor(state) ?? DesignTokens.Semantic.neutral
    }
}

/// The badge a session mark wears for its state — only the two attention
/// tones (`amber` needs you, `rose` usage limit) badge; muted and foreground
/// states draw none.
func sessionStateBadgeColor(_ state: SessionState) -> Color? {
    switch state.tone {
    case "amber": DesignTokens.Semantic.yellow
    case "rose": DesignTokens.Semantic.red
    default: nil
    }
}

/// EXP-804: the session's usage wall — `Rate limited · resets in 2h`.
///
/// A quiet amber pill that renders BESIDE the state, never instead of it.
/// The label itself is `AgentUsagePresentation.blockedBadgeLabel`, locked ×4;
/// nothing here decides wording. Nil label = not blocked = no pill.
struct SessionBlockedBadge: View {
    /// The raw `sessions.blocked` jsonb text off the wire.
    let blocked: String?
    /// Pinned by the tests; production passes the wall clock so the countdown
    /// re-reads whenever the row redraws.
    var now: Date = Date()

    private var label: String? {
        AgentUsagePresentation.blockedBadgeLabel(
            AgentUsagePresentation.parseBlocked(blocked),
            now: now
        )
    }

    var body: some View {
        if let label {
            HStack(spacing: 4) {
                AppIcon(AppIcons.uiClock, size: 10)
                Text(label)
                    .font(.caption2.weight(.semibold))
                    .lineLimit(1)
            }
            .foregroundStyle(DesignTokens.Semantic.yellow)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(DesignTokens.Semantic.yellow.opacity(0.12), in: Capsule())
            .accessibilityElement(children: .combine)
            .accessibilityLabel(label)
        }
    }
}
