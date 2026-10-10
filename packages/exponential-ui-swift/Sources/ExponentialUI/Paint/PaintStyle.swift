import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives

/// One shadow layer of a resolved `boxShadow`.
public struct ShadowLayer: Equatable, Sendable {
    public var x: CGFloat
    public var y: CGFloat
    public var blur: CGFloat
    public var spread: CGFloat
    public var color: Color
}

/// A linear `backgroundGradient` (contract §2): CSS angle (0 = up,
/// 90 = right), stops at 0…1, painted OVER `backgroundColor`.
public struct GradientPaint: Equatable, Sendable {
    public var angle: CGFloat
    public var stops: [Gradient.Stop]

    /// The gradient line's end points in UNIT coordinates of a `size` box,
    /// exactly CSS's: the line runs through the centre at `angle` and is
    /// long enough that the corners take the end colours
    /// (`|w·sin a| + |h·cos a|`).
    public func points(in size: CGSize) -> (start: UnitPoint, end: UnitPoint) {
        let a = angle * .pi / 180
        let (dx, dy) = (sin(a), -cos(a))
        let w = max(size.width, 1), h = max(size.height, 1)
        let half = (abs(w * dx) + abs(h * dy)) / 2
        let sx = 0.5 - dx * half / w, sy = 0.5 - dy * half / h
        let ex = 0.5 + dx * half / w, ey = 0.5 + dy * half / h
        return (UnitPoint(x: sx, y: sy), UnitPoint(x: ex, y: ey))
    }
}

/// The paint-only `transform` (contract §2) folded into translate + uniform
/// scale + rotation about the box centre (CSS `transform-origin: center`),
/// the gpui painter's `Affine`.
public struct PaintTransform: Equatable, Sendable {
    public var tx: CGFloat = 0
    public var ty: CGFloat = 0
    public var scale: CGFloat = 1
    /// Degrees, clockwise.
    public var rotate: CGFloat = 0

    public init(tx: CGFloat = 0, ty: CGFloat = 0, scale: CGFloat = 1, rotate: CGFloat = 0) {
        self.tx = tx
        self.ty = ty
        self.scale = scale
        self.rotate = rotate
    }

    public static let identity = PaintTransform()

    public var isIdentity: Bool { tx == 0 && ty == 0 && abs(scale - 1) < 1e-4 && abs(rotate) < 1e-3 }

    /// The core's `transform_json` (`[{op: translate|scale|rotate, …}]`,
    /// source order) multiplied left to right like CSS, then decomposed.
    public static func parse(_ json: String?) -> PaintTransform {
        guard let json, case let .array(ops) = JSONValue.parse(json) else { return .identity }
        // Row-major 2×3: x' = a·x + c·y + e, y' = b·x + d·y + f.
        var m: [CGFloat] = [1, 0, 0, 1, 0, 0]
        for op in ops {
            let n: [CGFloat]
            switch op["op"]?.string {
            case "translate": n = [1, 0, 0, 1, CGFloat(op["x"]?.number ?? 0), CGFloat(op["y"]?.number ?? 0)]
            case "scale":
                let f = CGFloat(op["factor"]?.number ?? 1)
                n = [f, 0, 0, f, 0, 0]
            case "rotate":
                let r = CGFloat(op["degrees"]?.number ?? 0) * .pi / 180
                n = [cos(r), sin(r), -sin(r), cos(r), 0, 0]
            default: continue
            }
            m = [
                m[0] * n[0] + m[2] * n[1],
                m[1] * n[0] + m[3] * n[1],
                m[0] * n[2] + m[2] * n[3],
                m[1] * n[2] + m[3] * n[3],
                m[0] * n[4] + m[2] * n[5] + m[4],
                m[1] * n[4] + m[3] * n[5] + m[5],
            ]
        }
        return PaintTransform(tx: m[4], ty: m[5], scale: (m[0] * m[0] + m[1] * m[1]).squareRoot(), rotate: atan2(m[1], m[0]) * 180 / .pi)
    }
}

/// `transition` + `transitionEasing` (contract §2): the resolved duration
/// (the core already zeroes it under reduced motion) and the cubic bezier.
public struct PaintTransition: Equatable, Sendable {
    public var durationMs: Double
    /// `[x1, y1, x2, y2]`; CSS `ease` when the style names none.
    public var easing: [Double]

    public static let cssEase: [Double] = [0.25, 0.1, 0.25, 1]

    /// The SwiftUI animation (`.timingCurve`); nil at 0 ms or under
    /// reduced motion (contract §6: transitions run at 0 ms).
    public func animation(reduceMotion: Bool = false) -> Animation? {
        guard durationMs > 0, !reduceMotion else { return nil }
        let e = easing.count == 4 ? easing : Self.cssEase
        return .timingCurve(e[0], e[1], e[2], e[3], duration: durationMs / 1000)
    }
}

/// `textDecoration`.
public enum TextDecorationPaint: String, Equatable, Sendable {
    case underline
    case lineThrough = "line-through"
}

/// The painted box of a node: the core's RESOLVED `Visual` as SwiftUI
/// values. Colours are hex strings in the core; nil = not set. Mirrors the
/// gpui painter's `PaintStyle` (per-side borders and per-corner radii
/// expanded, the transform folded, overflow per axis).
public struct PaintStyle: Equatable, Sendable {
    public var background: Color?
    public var gradient: GradientPaint?
    public var color: Color?
    /// The uniform border width (the largest side when they differ: the
    /// value controls that paint one border read).
    public var borderWidth: CGFloat = 0
    /// `[top, right, bottom, left]` (layout-effective: frames include them).
    public var borderWidths: [CGFloat] = [0, 0, 0, 0]
    /// The core's own uniform `borderWidth` (nil when the sides differ or
    /// none is set): what the cross-painter snapshot records.
    public var coreBorderWidth: CGFloat?
    public var borderColor: Color?
    /// `solid | dashed | dotted`.
    public var borderStyle: String = "solid"
    /// The uniform radius (the largest corner).
    public var radius: CGFloat = 0
    /// `[topLeft, topRight, bottomRight, bottomLeft]`.
    public var radii: [CGFloat] = [0, 0, 0, 0]
    public var opacity: Double?
    public var shadows: [ShadowLayer] = []
    public var fontSize: CGFloat?
    public var fontWeight: Int?
    public var lineHeight: CGFloat?
    public var fontFamily: String?
    /// PHYSICAL `left | right | center | justify` (the core resolves
    /// `start`/`end` against the surface direction).
    public var textAlign: String?
    public var letterSpacing: CGFloat?
    public var textDecoration: TextDecorationPaint?
    /// `uppercase | lowercase | capitalize` (nil = none).
    public var textTransform: String?
    public var italic = false
    /// The recipe padding on a MEASURED leaf (the measurer counted it).
    public var paddingHorizontal: CGFloat = 0
    public var paddingVertical: CGFloat = 0
    /// A measured leaf's padding `[top, right, bottom, left]`.
    public var padding: [CGFloat] = [0, 0, 0, 0]
    public var gap: CGFloat = 0
    /// `native: true` on the part's recipe.
    public var native: Bool = false
    /// The `overflow` shorthand (`hidden` / `scroll`; a per-axis value wins).
    public var overflowHidden = false
    public var overflowScroll = false
    public var clipX = false
    public var clipY = false
    public var scrollX = false
    public var scrollY = false
    public var transform = PaintTransform.identity
    public var transition: PaintTransition?
    /// `visibility: hidden`: the box keeps its place, paints nothing and
    /// leaves the accessibility tree.
    public var invisible = false
    /// `pointerEvents: none`: presses pass through.
    public var pointerNone = false
    /// `auto | none | text`.
    public var userSelect: String?
    public var cursor: String?
    /// A Chart's resolved series colours.
    public var seriesColors: [Color] = []
    /// Round 2: a leaf's resolved direction (`ltr | rtl`, the bidi
    /// paragraph direction of its text; `textAlign` is then physical).
    public var direction: String?
    /// Round 2: backdrop blur radius (pt); nil = none.
    public var backdropBlur: CGFloat?
    /// Round 2: a keyframe animation (`{name, timing, reduced?}`), sampled
    /// with the core's `animationFrameJson`.
    public var animation: KeyframeAnimation?
    /// Round 2: `position: sticky` (the offset arrives in `SurfaceModel.sticky`).
    public var sticky = false

    public init() {}

    public init(_ v: FfiVisual) {
        background = v.backgroundColor.flatMap { Color(hex: $0) }
        color = v.color.flatMap { Color(hex: $0) }
        let uniform = CGFloat(max(0, v.borderWidth ?? 0))
        borderWidths = v.borderWidths.map { $0.map { CGFloat(max(0, $0)) } }.flatMap { $0.count == 4 ? $0 : nil } ?? [uniform, uniform, uniform, uniform]
        borderWidth = borderWidths.max() ?? 0
        coreBorderWidth = v.borderWidth.map { CGFloat(max(0, $0)) }
        direction = v.direction
        backdropBlur = v.backdropBlur.flatMap { $0 > 0 ? CGFloat($0) : nil }
        animation = v.animationJson.flatMap(KeyframeAnimation.init(json:))
        sticky = v.sticky
        borderColor = v.borderColor.flatMap { Color(hex: $0) }
        borderStyle = v.borderStyle ?? "solid"
        let r = CGFloat(max(0, v.borderRadius ?? 0))
        radii = v.cornerRadii.map { $0.map { CGFloat(max(0, $0)) } }.flatMap { $0.count == 4 ? $0 : nil } ?? [r, r, r, r]
        radius = radii.max() ?? 0
        opacity = v.opacity.map { Double($0) }
        if let json = v.boxShadowJson, case let .array(layers) = JSONValue.parse(json) {
            shadows = layers.compactMap { l in
                guard let o = l.object else { return nil }
                return ShadowLayer(x: CGFloat(o["x"]?.number ?? 0), y: CGFloat(o["y"]?.number ?? 0), blur: CGFloat(o["blur"]?.number ?? 0), spread: CGFloat(o["spread"]?.number ?? 0), color: o["color"]?.string.flatMap { Color(hex: $0) } ?? .black.opacity(0.2))
            }
        }
        if let json = v.backgroundGradientJson, let g = JSONValue.parse(json).object {
            let stops: [Gradient.Stop] = (g["stops"]?.array ?? []).compactMap { s in
                guard let c = s["color"]?.string.flatMap({ Color(hex: $0) }) else { return nil }
                return Gradient.Stop(color: c, location: CGFloat(min(1, max(0, s["offset"]?.number ?? 0))))
            }
            if stops.count >= 2 { gradient = GradientPaint(angle: CGFloat(g["angle"]?.number ?? 180), stops: stops) }
        }
        fontSize = v.fontSize.map { CGFloat($0) }
        fontWeight = v.fontWeight.map { Int($0) }
        lineHeight = v.lineHeight.map { CGFloat($0) }
        fontFamily = v.fontFamily
        textAlign = v.textAlign
        letterSpacing = v.letterSpacing.map { CGFloat($0) }
        textDecoration = v.textDecoration.flatMap(TextDecorationPaint.init(rawValue:))
        textTransform = v.textTransform.flatMap { $0 == "none" ? nil : $0 }
        italic = v.fontStyle == "italic"
        paddingHorizontal = CGFloat(v.paddingHorizontal ?? 0)
        paddingVertical = CGFloat(v.paddingVertical ?? 0)
        padding = v.padding.map { $0.map { CGFloat($0) } }.flatMap { $0.count == 4 ? $0 : nil } ?? [paddingVertical, paddingHorizontal, paddingVertical, paddingHorizontal]
        gap = CGFloat(v.gap ?? 0)
        native = v.native
        overflowHidden = v.overflowHidden
        overflowScroll = v.overflowScroll
        (clipX, scrollX) = Self.overflow(v.overflowX)
        (clipY, scrollY) = Self.overflow(v.overflowY)
        if v.overflowHidden {
            clipX = clipX || v.overflowX == nil
            clipY = clipY || v.overflowY == nil
        }
        if v.overflowScroll, v.overflowX == nil, v.overflowY == nil {
            (clipX, clipY, scrollX, scrollY) = (true, true, true, true)
        }
        transform = PaintTransform.parse(v.transformJson)
        if let ms = v.transitionMs, ms > 0 {
            transition = PaintTransition(durationMs: Double(ms), easing: (v.transitionEasing ?? []).map(Double.init))
        }
        invisible = v.visibilityHidden
        pointerNone = v.pointerEventsNone
        userSelect = v.userSelect
        cursor = v.cursor
        seriesColors = (v.seriesColors ?? []).compactMap { Color(hex: $0) }
    }

    /// `overflowX|Y` → (clips, scrolls).
    static func overflow(_ v: String?) -> (Bool, Bool) {
        switch v {
        case "hidden", "clip": (true, false)
        case "scroll", "auto": (true, true)
        default: (false, false)
        }
    }

    /// The inset of the content box inside a measured leaf (padding +
    /// border), horizontal and vertical (the leading/top sides).
    public var inset: (CGFloat, CGFloat) { (padding[3] + borderWidths[3], padding[0] + borderWidths[0]) }

    /// The content insets per side (padding + border).
    public var insets: EdgeInsets {
        EdgeInsets(top: padding[0] + borderWidths[0], leading: padding[3] + borderWidths[3], bottom: padding[2] + borderWidths[2], trailing: padding[1] + borderWidths[1])
    }

    public var hasBorder: Bool { borderWidths.contains { $0 > 0 } && borderColor != nil }
    public var uniformBorder: Bool { Set(borderWidths).count == 1 }
    public var clips: Bool { clipX || clipY }
    public var scrolls: Bool { scrollX || scrollY }

    /// The radii clamped to the box (CSS: a corner never exceeds half the
    /// shorter side).
    public func clampedRadii(_ size: CGSize) -> [CGFloat] {
        let cap = max(0, min(size.width, size.height) / 2)
        return radii.map { min($0, cap) }
    }

    /// The box's shape at `size`.
    public func shape(_ size: CGSize) -> BoxShape { BoxShape(radii: clampedRadii(size)) }
}

/// A part's resolved look: its `PaintStyle` plus the flat style map
/// (`width`, `height`, `native`…), the painter's view of the core's
/// `resolve_recipe`.
public struct PartStyle: Equatable, Sendable {
    public var style: PaintStyle
    public var props: Props

    public func px(_ key: String) -> CGFloat? { props.px(key) }
    public var width: CGFloat? { px("width") }
    public var height: CGFloat? { px("height") }
    /// `native` as the recipe says it: nil when the recipe is silent.
    public var native: Bool? { props["native"]?.bool }
    public var color: Color? { style.color }
    public var fontFamily: String? { props.str("fontFamily").isEmpty ? nil : props.str("fontFamily") }

    static let empty = PartStyle(style: PaintStyle(), props: [:])
}

/// The rounded box with one radius per corner (`[topLeft, topRight,
/// bottomRight, bottomLeft]`), insettable so borders stroke inside it.
public struct BoxShape: InsettableShape, Equatable {
    public var radii: [CGFloat]
    public var insetAmount: CGFloat = 0

    public init(radii: [CGFloat], insetAmount: CGFloat = 0) {
        self.radii = radii.count == 4 ? radii : [0, 0, 0, 0]
        self.insetAmount = insetAmount
    }

    public init(radius: CGFloat) { self.init(radii: [radius, radius, radius, radius]) }

    public func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: insetAmount, dy: insetAmount)
        guard r.width > 0, r.height > 0 else { return Path(r) }
        let k = radii.map { max(0, $0 - insetAmount) }
        if k.allSatisfy({ $0 == 0 }) { return Path(r) }
        return UnevenRoundedRectangle(topLeadingRadius: k[0], bottomLeadingRadius: k[3], bottomTrailingRadius: k[2], topTrailingRadius: k[1], style: .continuous).path(in: r)
    }

    public func inset(by amount: CGFloat) -> BoxShape { BoxShape(radii: radii, insetAmount: insetAmount + amount) }
}

extension View {
    /// A box painted from a style: shadow, background, gradient (over the
    /// background), border (inside, per side, dashed / dotted), per-corner
    /// radii and opacity. The border is an overlay so children are never
    /// offset (their frames already include it).
    @ViewBuilder
    func paintedBox(_ s: PaintStyle, size: CGSize) -> some View {
        let shape = s.shape(size)
        self
            .background {
                ZStack {
                    if let bg = s.background { shape.fill(bg) }
                    if let g = s.gradient {
                        let p = g.points(in: size)
                        shape.fill(LinearGradient(stops: g.stops, startPoint: p.start, endPoint: p.end))
                    }
                }
            }
            .overlay {
                if s.hasBorder { BorderView(style: s, shape: shape) }
            }
            .modifier(ShadowsModifier(shadows: s.shadows, shape: shape))
            .opacity(s.opacity ?? 1)
    }
}

/// The border: a uniform one strokes the shape inside its edge (dashed /
/// dotted through the dash pattern); per-side widths fill the band between
/// the outer shape and the inner one inset per side (one colour, contract
/// §2: per-side COLOURS are excluded).
struct BorderView: View {
    let style: PaintStyle
    let shape: BoxShape

    var body: some View {
        let color = style.borderColor ?? .clear
        if style.uniformBorder {
            let w = style.borderWidths[0]
            shape.strokeBorder(color, style: Self.stroke(width: w, style: style.borderStyle))
        } else if style.borderStyle == "solid" {
            SideBand(widths: style.borderWidths, radii: shape.radii).fill(color, style: FillStyle(eoFill: true))
        } else {
            // Dashed / dotted per side: one dashed line along the middle of
            // each side's band.
            Canvas { ctx, size in
                let w = style.borderWidths
                let sides: [(CGPoint, CGPoint, CGFloat)] = [
                    (CGPoint(x: 0, y: w[0] / 2), CGPoint(x: size.width, y: w[0] / 2), w[0]),
                    (CGPoint(x: size.width - w[1] / 2, y: 0), CGPoint(x: size.width - w[1] / 2, y: size.height), w[1]),
                    (CGPoint(x: 0, y: size.height - w[2] / 2), CGPoint(x: size.width, y: size.height - w[2] / 2), w[2]),
                    (CGPoint(x: w[3] / 2, y: 0), CGPoint(x: w[3] / 2, y: size.height), w[3]),
                ]
                for (a, b, width) in sides where width > 0 {
                    var p = Path()
                    p.move(to: a)
                    p.addLine(to: b)
                    ctx.stroke(p, with: .color(color), style: Self.stroke(width: width, style: style.borderStyle))
                }
            }
            .clipShape(shape)
        }
    }

    /// CSS-like dash patterns: dashed = 3w on / 3w off (Chromium paints
    /// 3:3 for thin borders), dotted = round dots one width apart.
    static func stroke(width: CGFloat, style: String) -> StrokeStyle {
        switch style {
        case "dashed": StrokeStyle(lineWidth: width, dash: [width * 3, width * 3])
        case "dotted": StrokeStyle(lineWidth: width, lineCap: .round, dash: [0, width * 2])
        default: StrokeStyle(lineWidth: width)
        }
    }
}

/// The band between the box and its content edge with a width per side
/// (even-odd: the outer rounded rect minus the inner one).
struct SideBand: Shape {
    let widths: [CGFloat]
    let radii: [CGFloat]

    func path(in rect: CGRect) -> Path {
        var p = BoxShape(radii: radii).path(in: rect)
        let inner = CGRect(x: rect.minX + widths[3], y: rect.minY + widths[0], width: max(0, rect.width - widths[1] - widths[3]), height: max(0, rect.height - widths[0] - widths[2]))
        // The inner corner shrinks by the wider of its two sides.
        let k = [
            max(0, radii[0] - max(widths[0], widths[3])),
            max(0, radii[1] - max(widths[0], widths[1])),
            max(0, radii[2] - max(widths[2], widths[1])),
            max(0, radii[3] - max(widths[2], widths[3])),
        ]
        p.addPath(BoxShape(radii: k).path(in: inner))
        return p
    }
}

private struct ShadowsModifier: ViewModifier {
    let shadows: [ShadowLayer]
    let shape: BoxShape

    func body(content: Content) -> some View {
        if shadows.isEmpty {
            content
        } else {
            content.background {
                ZStack {
                    // CSS paints the first shadow on top: draw in reverse.
                    ForEach(Array(shadows.enumerated().reversed()), id: \.offset) { _, l in
                        shape
                            .fill(l.color)
                            .padding(-l.spread)
                            .blur(radius: l.blur / 2)
                            .offset(x: l.x, y: l.y)
                    }
                }
            }
        }
    }
}

extension CGRect {
    init(_ f: FfiFrame) {
        self.init(x: CGFloat(f.x), y: CGFloat(f.y), width: CGFloat(f.w), height: CGFloat(f.h))
    }
}
