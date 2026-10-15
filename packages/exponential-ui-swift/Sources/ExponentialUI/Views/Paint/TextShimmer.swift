import SwiftUI
import ExponentialUICore

/// EXP-1324: the Text `shimmer` colours + timing (catalog/style.json
/// `textShimmer`), resolved by the core (`textShimmerJson`) and cached per
/// colour and mode.
public struct TextShimmer: Equatable, Sendable {
    public var base: String
    public var highlight: String
    public var mid: String
    public var durationMs: Double
    public var angleDeg: Double
    public var spreadPx: Double

    static let cache = ShimmerCache()

    /// The shimmer of a text colour (`#rrggbb[aa]`) in `dark` mode.
    public static func resolve(color: String, dark: Bool) -> TextShimmer? {
        let key = "\(color)|\(dark)"
        if let hit = cache.get(key) { return hit }
        let v = JSONValue.parse(textShimmerJson(color: color, dark: dark))
        guard let base = v["base"]?.string, let highlight = v["highlight"]?.string, let mid = v["mid"]?.string else { return nil }
        let s = TextShimmer(base: base, highlight: highlight, mid: mid, durationMs: v["durationMs"]?.number ?? 2000, angleDeg: v["angleDeg"]?.number ?? 20, spreadPx: v["spreadPx"]?.number ?? 60)
        cache.set(key, s)
        return s
    }

    /// The gradient's width for a text box `width` wide: 200% + 2 × spread.
    func gradientWidth(_ width: CGFloat) -> CGFloat { 2 * width + 2 * CGFloat(spreadPx) }

    /// The gradient's x offset at `phase` (0 … 1 of `durationMs`): CSS
    /// background-position slides 100% → 0% (rtl 0% → 100%).
    func offset(width: CGFloat, phase: Double, rtl: Bool) -> CGFloat {
        let position = rtl ? phase : 1 - phase
        return CGFloat(position) * (width - gradientWidth(width))
    }

    /// The linear gradient over a `size` box at CSS angle 90° + `angleDeg`:
    /// start/end points on the CSS gradient line and the five stops.
    func gradient(size: CGSize) -> (Gradient, UnitPoint, UnitPoint) {
        let a = (90 + angleDeg) * .pi / 180
        let (dx, dy) = (sin(a), -cos(a))
        let w = max(Double(size.width), 1), h = max(Double(size.height), 1)
        let length = max(abs(w * dx) + abs(h * dy), 1)
        let half = length / 2
        let start = UnitPoint(x: 0.5 - dx * half / w, y: 0.5 - dy * half / h)
        let end = UnitPoint(x: 0.5 + dx * half / w, y: 0.5 + dy * half / h)
        let f = spreadPx / length
        let b = Color(hex: base) ?? .primary, m = Color(hex: mid) ?? b, hi = Color(hex: highlight) ?? b
        let stops: [Gradient.Stop] = [
            .init(color: b, location: 0.5 - f),
            .init(color: m, location: 0.5 - f / 2),
            .init(color: hi, location: 0.5),
            .init(color: m, location: 0.5 + f / 2),
            .init(color: b, location: 0.5 + f),
        ]
        return (Gradient(stops: stops), start, end)
    }
}

final class ShimmerCache: @unchecked Sendable {
    private var map: [String: TextShimmer] = [:]
    private let lock = NSLock()
    func get(_ k: String) -> TextShimmer? { lock.lock(); defer { lock.unlock() }; return map[k] }
    func set(_ k: String, _ v: TextShimmer) { lock.lock(); map[k] = v; lock.unlock() }
}

extension Color {
    /// `#rrggbbaa` of a resolved sRGB colour (the shimmer's input).
    func hexRGBA(in environment: EnvironmentValues = EnvironmentValues()) -> String {
        let r = resolve(in: environment)
        func byte(_ v: Float) -> Int { Int((min(1, max(0, Double(v))) * 255).rounded()) }
        let a = byte(r.opacity)
        let rgb = String(format: "#%02x%02x%02x", byte(r.red), byte(r.green), byte(r.blue))
        return a == 255 ? rgb : rgb + String(format: "%02x", a)
    }
}

/// Paints `content` (the text, in its ink) as the shimmer: the moving
/// gradient masked by the glyphs. Reduced motion = the plain text.
struct ShimmerMask<Content: View>: View {
    let shimmer: TextShimmer?
    let size: CGSize
    let rtl: Bool
    @ViewBuilder let content: () -> Content
    @Environment(\.xuiReducedMotion) private var reduced

    var body: some View {
        if let s = shimmer, !reduced, size.width > 0, size.height > 0 {
            let gw = s.gradientWidth(size.width)
            let (g, start, end) = s.gradient(size: CGSize(width: gw, height: size.height))
            TimelineView(.animation) { tl in
                let ms = tl.date.timeIntervalSinceReferenceDate * 1000
                let phase = s.durationMs > 0 ? ms.truncatingRemainder(dividingBy: s.durationMs) / s.durationMs : 0
                LinearGradient(gradient: g, startPoint: start, endPoint: end)
                    .frame(width: gw, height: size.height)
                    .offset(x: s.offset(width: size.width, phase: phase, rtl: rtl))
                    .frame(width: size.width, height: size.height, alignment: .topLeading)
                    .mask(alignment: .topLeading) { content() }
            }
            .frame(width: size.width, height: size.height, alignment: .topLeading)
        } else {
            content()
        }
    }
}
