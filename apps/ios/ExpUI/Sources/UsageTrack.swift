import ExpCore
import SwiftUI
internal import ExponentialUIPrimitives

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

    /// SLOP-18 / VAPP-88: the SDK's `MeterTrack` with one capsule segment
    /// (the used share keeps its round end), over the `strokeStrong` rail.
    public var body: some View {
        MeterTrack(
            segments: [MeterSegment(id: 0, fraction: Self.fraction(percent), color: ContextRing.tone(severity))],
            height: height,
            track: GlassTokens.strokeStrong,
            capsuleSegments: true
        )
    }

    /// The used share, 0…1 — an absent percentage draws an empty rail.
    static func fraction(_ percent: Double?) -> Double {
        min(max((percent ?? 0) / 100, 0), 1)
    }
}
