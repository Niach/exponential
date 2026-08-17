import SwiftUI

// Motion (EXP-523) — the hand-written half of the shared motion system.
//
// `DesignTokens.Motion` (generated from packages/design-tokens/tokens.json,
// the same source the web CSS vars and the Android/desktop tokens come from)
// carries the raw durations and cubic-bezier control points. This file owns
// the `BezierCurve` type those literals construct — the same arrangement as
// the desktop's hand-written `Srgb8` — and the Reduce Motion bridge.
//
// Call sites NEVER read `DesignTokens.Motion` directly. They read
// `@Environment(\.motion)` and get an `Animation?` that is nil under Reduce
// Motion, so `withAnimation(motion.standard) { … }` and
// `.animation(motion.standard, value:)` both degrade to an instant state
// change with no `if reduceMotion` branch at the call site.

/// The four CSS cubic-bezier control points (P1/P2; P0 = (0,0) and P3 = (1,1)
/// are implicit) — the one form web, Compose, SwiftUI and gpui all accept.
public struct BezierCurve: Sendable, Equatable {
    public let x1: Double
    public let y1: Double
    public let x2: Double
    public let y2: Double

    public init(x1: Double, y1: Double, x2: Double, y2: Double) {
        self.x1 = x1
        self.y1 = y1
        self.x2 = x2
        self.y2 = y2
    }

    public func animation(duration: TimeInterval) -> Animation {
        .timingCurve(x1, y1, x2, y2, duration: duration)
    }
}

/// Reduce-Motion-aware access to the shared motion tokens. Obtained from the
/// environment (`@Environment(\.motion) private var motion`), never built at a
/// call site.
public struct Motion: Sendable {
    public let reduceMotion: Bool

    public init(reduceMotion: Bool) {
        self.reduceMotion = reduceMotion
    }

    /// Hover / opacity micro-feedback.
    public var fast: Animation? { timing(DesignTokens.Motion.Duration.fast) }
    /// The default: enter/exit, expand/collapse, position changes.
    public var standard: Animation? { timing(DesignTokens.Motion.Duration.standard) }
    /// Large surfaces — sheets, full-screen pushes, docks.
    public var slow: Animation? { timing(DesignTokens.Motion.Duration.slow) }

    /// For things arriving on screen (enter-only transitions).
    public func decelerate(
        _ duration: TimeInterval = DesignTokens.Motion.Duration.standard
    ) -> Animation? {
        reduceMotion ? nil : DesignTokens.Motion.Ease.decelerate.animation(duration: duration)
    }

    /// For things leaving it (exit-only transitions).
    public func accelerate(
        _ duration: TimeInterval = DesignTokens.Motion.Duration.fast
    ) -> Animation? {
        reduceMotion ? nil : DesignTokens.Motion.Ease.accelerate.animation(duration: duration)
    }

    /// Shape of an ambient loop. A breathing opacity wants to ease at both
    /// ends; a ring expanding outward from a dot wants to decelerate only.
    public enum PulseCurve: Sendable {
        case easeInOut
        case easeOut
    }

    /// Ambient status loops (the "working…" pulses). Their periods and curves
    /// are NOT tokenised — each indicator picks its own, they are a different
    /// design axis from the shared enter/exit durations — but they route
    /// through here so Reduce Motion is honoured in ONE place, and so an
    /// open-ended repeat never drives the render loop when the user has asked
    /// for stillness (EXP-70).
    public func pulse(
        duration: TimeInterval,
        autoreverses: Bool = true,
        curve: PulseCurve = .easeInOut
    ) -> Animation? {
        guard !reduceMotion else { return nil }
        let base: Animation
        switch curve {
        case .easeInOut: base = Animation.easeInOut(duration: duration)
        case .easeOut: base = Animation.easeOut(duration: duration)
        }
        return base.repeatForever(autoreverses: autoreverses)
    }

    /// The default curve at an arbitrary duration. Named `timing` rather than
    /// `curve` so it does not read as shadowed by `pulse`'s `curve:` label.
    private func timing(_ duration: TimeInterval) -> Animation? {
        reduceMotion ? nil : DesignTokens.Motion.Ease.standard.animation(duration: duration)
    }
}

public extension EnvironmentValues {
    /// A plain enum cannot read the environment, so the motion tokens are
    /// vended through a COMPUTED environment value derived from the built-in
    /// Reduce Motion key. Reading `\.motion` therefore registers the
    /// dependency on `accessibilityReduceMotion`, and views re-render when the
    /// user toggles the setting mid-session — which a one-shot read of
    /// `UIAccessibility.isReduceMotionEnabled` would not do.
    var motion: Motion { Motion(reduceMotion: accessibilityReduceMotion) }
}
