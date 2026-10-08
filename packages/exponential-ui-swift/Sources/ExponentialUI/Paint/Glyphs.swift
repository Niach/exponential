import SwiftUI

/// Chrome glyphs the controls need (check marks, chevrons, close, search,
/// calendar, send, attach, loader, play, image): SF Symbols, so a host with
/// no icon registry still gets working controls. Catalog icon NAMES go
/// through `HostPlugin.icon`.
public enum Glyph: String, Sendable {
    case check = "checkmark"
    case chevronDown = "chevron.down"
    case chevronUp = "chevron.up"
    case chevronRight = "chevron.right"
    case chevronLeft = "chevron.left"
    case chevronsUpDown = "chevron.up.chevron.down"
    case close = "xmark"
    case search = "magnifyingglass"
    case calendar = "calendar"
    case send = "arrow.up"
    case attach = "paperclip"
    case loader = "circle.dotted"
    case play = "play.fill"
    case image = "photo"
    case stop = "stop.fill"
    case circle = "circle"
    case externalLink = "arrow.up.right"

    /// The icons.json concept a chrome glyph stands for, when it is one of
    /// the directional `rtlMirroredIcons`.
    var mirroredConcept: String? {
        switch self {
        case .chevronLeft: "ui-chevron-left"
        case .chevronRight: "ui-chevron-right"
        case .send: "ui-send"
        case .externalLink: "ui-external-link"
        default: nil
        }
    }
}

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

struct GlyphView: View {
    let glyph: Glyph
    let size: CGFloat
    let color: Color
    var weight: Font.Weight = .medium
    @Environment(\.xuiRTL) private var rtl
    @Environment(\.xuiGlyphMirrored) private var mirroredOutside

    var body: some View {
        Image(systemName: glyph.rawValue)
            .font(.system(size: size * 0.8, weight: weight))
            .foregroundStyle(color)
            .frame(width: size, height: size)
            .mirroredForRTL(!mirroredOutside && glyph.mirroredConcept.map { RTLGlyphs.mirrors($0, rtl: rtl) } == true)
    }
}

/// A catalog icon by NAME through the host; the placeholder circle when
/// the host has none. Directional glyphs mirror under rtl (an `Icon` NODE
/// mirrors at its box, outside its own transform, and tells its content
/// through `xuiGlyphMirrored` so nothing flips twice).
struct IconView: View {
    let name: String
    let size: CGFloat
    let color: Color
    let model: SurfaceModel
    @Environment(\.xuiRTL) private var rtl
    @Environment(\.xuiGlyphMirrored) private var mirroredOutside

    var body: some View {
        if let view = model.host.icon(name, size: size) {
            view.frame(width: size, height: size).foregroundStyle(color)
                .mirroredForRTL(!mirroredOutside && RTLGlyphs.mirrors(name, rtl: rtl))
        } else {
            Circle()
                .strokeBorder(color.opacity(0.5), lineWidth: 1.5)
                .frame(width: size * 0.8, height: size * 0.8)
                .frame(width: size, height: size)
        }
    }
}

/// The rotating loader; still under reduced motion (contract §6: the
/// surface setting or the platform's).
struct SpinnerView: View {
    let size: CGFloat
    let color: Color
    @State private var spinning = false
    @Environment(\.accessibilityReduceMotion) private var platformReduced
    @Environment(\.xuiReducedMotion) private var surfaceReduced

    var body: some View {
        let still = platformReduced || surfaceReduced
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
    @Environment(\.accessibilityReduceMotion) private var platformReduced
    @Environment(\.xuiReducedMotion) private var surfaceReduced

    func body(content: Content) -> some View {
        if on && !(platformReduced || surfaceReduced) {
            content
                .opacity(dim ? 0.5 : 1)
                .animation(.timingCurve(0.4, 0, 0.6, 1, duration: 1).repeatForever(autoreverses: true), value: dim)
                .onAppear { dim = true }
        } else {
            content
        }
    }
}
