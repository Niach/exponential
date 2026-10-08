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

/// The Skeleton pulse (the web's `xui-pulse`: opacity 1 → .5 → 1 over 2 s,
/// eased `cubic-bezier(.4, 0, .6, 1)`); static under reduced motion.
struct SkeletonPulse: ViewModifier {
    let on: Bool
    @State private var dim = false
    @Environment(\.xuiReducedMotion) private var reduced

    func body(content: Content) -> some View {
        if on && !reduced {
            content
                .opacity(dim ? 0.5 : 1)
                .animation(.timingCurve(0.4, 0, 0.6, 1, duration: 1).repeatForever(autoreverses: true), value: dim)
                .onAppear { dim = true }
        } else {
            content
        }
    }
}
