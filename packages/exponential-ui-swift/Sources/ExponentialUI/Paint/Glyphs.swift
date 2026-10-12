import SwiftUI

/// The directional glyphs a painter MIRRORS under rtl (contract §4,
/// `catalog/locale.json` `rtlMirroredIcons`, semantic and Lucide names;
/// `RTL_MIRRORED_ICONS` in the core's generated catalog). Exactly these:
/// media transport, up/down chevrons and everything else never flip.
/// Locked against the JSON by `Round1PaintTests`.
public enum RTLGlyphs {
    public static let mirrored: Set<String> = [
        "ui-back", "ui-chevron-left", "ui-chevron-right", "ui-arrow-right", "ui-send", "ui-undo", "ui-external-link",
        "arrow-left", "arrow-right", "chevron-left", "chevron-right", "send", "undo-2", "external-link",
    ]

    /// Does `name` flip under `rtl`?
    public static func mirrors(_ name: String, rtl: Bool) -> Bool { rtl && mirrored.contains(name) }
}

extension View {
    /// `scaleX(-1)` when `on`: applied OUTSIDE every other transform of
    /// the glyph (CSS `transform: scaleX(-1) <own>`), so a rotated
    /// Collapsible chevron still points down when open.
    @ViewBuilder
    func mirroredForRTL(_ on: Bool) -> some View {
        if on { scaleEffect(x: -1, y: 1, anchor: .center) } else { self }
    }
}

/// The rotating loader; still under reduced motion (contract §6: the
/// surface setting or the platform's).
struct SpinnerView: View {
    let size: CGFloat
    let color: Color
    @State private var spinning = false
    @Environment(\.xuiReducedMotion) private var still

    var body: some View {
        Circle()
            .trim(from: 0.15, to: 1)
            .stroke(color, style: StrokeStyle(lineWidth: max(1.5, size / 10), lineCap: .round))
            .frame(width: size - 2, height: size - 2)
            .rotationEffect(.degrees(spinning && !still ? 360 : 0))
            .animation(still ? nil : .linear(duration: 0.8).repeatForever(autoreverses: false), value: spinning && !still)
            .onAppear { spinning = true }
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

/// EXP-1259 (`catalog/layout.json`, feedback-states.json): the pending
/// Button's delay and the placeholder's ONE pulse, mirrored from the
/// generated layout constants (a test locks them).
enum FeedbackTiming {
    /// A `loading` Button looks idle this long (a fast action never flashes).
    static let pendingDelayMs: Double = 150
    /// The Skeleton pulse: low → high → low over this many ms, ease-in-out.
    static let skeletonPulseMs: Double = 1600
    static let skeletonOpacityLow: Double = 0.45
    static let skeletonOpacityHigh: Double = 0.9
    /// The Skeleton's static opacity under reduced motion.
    static let skeletonReducedOpacity: Double = 0.6
}

/// The Skeleton pulse (EXP-1259, feedback-states.json `placeholder`): ONE
/// slow opacity wave `skeletonOpacityLow` ↔ `High` over `skeletonPulseMs`,
/// ease-in-out; static at `skeletonReducedOpacity` under reduced motion.
/// Only a Skeleton LEAF pulses (a Skeleton with `rows` is a container whose
/// dots and bars pulse each).
struct SkeletonPulse: ViewModifier {
    let on: Bool
    @State private var high = false
    @Environment(\.xuiReducedMotion) private var reduced

    func body(content: Content) -> some View {
        if on && reduced {
            content.opacity(FeedbackTiming.skeletonReducedOpacity)
        } else if on {
            content
                .opacity(high ? FeedbackTiming.skeletonOpacityHigh : FeedbackTiming.skeletonOpacityLow)
                .animation(.easeInOut(duration: FeedbackTiming.skeletonPulseMs / 2000).repeatForever(autoreverses: true), value: high)
                .onAppear { high = true }
        } else {
            content
        }
    }
}
