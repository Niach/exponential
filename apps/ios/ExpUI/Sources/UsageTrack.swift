import ExpCore
import SwiftUI

// EXP-909: THE meter primitive. One bar shape draws every usage number the app
// prints — the Usage overlay's rate-limit windows, its Context line, and the
// compact three-bar line under an account row — so a percentage looks the same
// wherever it is read. Promoted out of the session screen into ExpUI for that
// reason: the Devices page draws it too now, and a second hand-rolled rail is
// exactly how the two surfaces drifted before.
//
// The ×4 twins: web `@exp/ui` `Meter`, desktop `usage_bar::meter`, Android
// `ui/components/UsageTrack.kt`. The tone follows the locked severity
// thresholds (≥95 red, ≥75 amber, otherwise the muted white fill) through
// `ContextRing.tone`, which is the same palette the composer's ring uses.

/// A rounded rail with the used share filled in the severity tone. A window
/// with no percentage draws an EMPTY rail rather than a lie.
public struct AgentUsageTrack: View {
    let percent: Double?
    let severity: AgentUsageSeverity
    var height: CGFloat

    public init(percent: Double?, severity: AgentUsageSeverity, height: CGFloat = 6) {
        self.percent = percent
        self.severity = severity
        self.height = height
    }

    public var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(GlassTokens.strokeStrong)
                Capsule()
                    .fill(ContextRing.tone(severity))
                    .frame(width: geo.size.width * min(max((percent ?? 0) / 100, 0), 1))
            }
        }
        .frame(height: height)
        .accessibilityHidden(true)
    }
}
