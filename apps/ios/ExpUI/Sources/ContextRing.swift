import ExpCore
import SwiftUI

// EXP-893: the steer composer's usage RING — a radial context-percentage
// glyph in the collapsed bar's left circle and at the expanded composer's
// footer end, opening the Usage sheet. The desktop's `steer-composer.tsx`
// draws the same ring; the tone follows the locked ×4 severity thresholds
// the usage cards use (≥95 red, ≥75 amber, otherwise the muted white fill).

public enum ContextRingTokens {
    /// The ring's diameter.
    public static let size: CGFloat = 16
    /// The stroke, track and arc alike.
    public static let lineWidth: CGFloat = 2
    /// The gap the ring keeps from its neighbours in the composer footer.
    public static let spacing: CGFloat = 6
}

public struct ContextRing: View {
    /// 0…1, the used share of the context window. `nil` = unknown: an empty
    /// track, never a lie.
    let fraction: Double?
    let severity: AgentUsageSeverity

    public init(fraction: Double?, severity: AgentUsageSeverity) {
        self.fraction = fraction
        self.severity = severity
    }

    /// The arc's colour per severity — the usage track's own palette.
    public static func tone(_ severity: AgentUsageSeverity) -> Color {
        switch severity {
        case .normal: GlassTokens.usageFill
        case .warning: DesignTokens.Semantic.yellow
        case .danger: DesignTokens.Semantic.red
        }
    }

    /// The arc length: an absent share draws none, anything else clamps to
    /// the unit interval.
    public static func arc(_ fraction: Double?) -> Double {
        guard let fraction, fraction.isFinite else { return 0 }
        return min(1, max(0, fraction))
    }

    public var body: some View {
        ZStack {
            Circle()
                .stroke(GlassTokens.strokeStrong, lineWidth: ContextRingTokens.lineWidth)
            Circle()
                .trim(from: 0, to: Self.arc(fraction))
                .stroke(
                    Self.tone(severity),
                    style: StrokeStyle(lineWidth: ContextRingTokens.lineWidth, lineCap: .round)
                )
                .rotationEffect(.degrees(-90))
        }
        .frame(width: ContextRingTokens.size, height: ContextRingTokens.size)
        .accessibilityHidden(true)
    }
}
