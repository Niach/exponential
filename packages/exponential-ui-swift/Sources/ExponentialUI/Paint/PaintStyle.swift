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

/// The painted box of a node: the core's RESOLVED `Visual` as SwiftUI
/// values. Colours are hex strings in the core; nil = not set.
public struct PaintStyle: Equatable, Sendable {
    public var background: Color?
    public var color: Color?
    public var borderWidth: CGFloat = 0
    public var borderColor: Color?
    public var radius: CGFloat = 0
    public var opacity: Double?
    public var shadows: [ShadowLayer] = []
    public var fontSize: CGFloat?
    public var fontWeight: Int?
    public var lineHeight: CGFloat?
    public var fontFamily: String?
    public var textAlign: String?
    /// The recipe padding on a MEASURED leaf (the measurer counted it).
    public var paddingHorizontal: CGFloat = 0
    public var paddingVertical: CGFloat = 0
    public var gap: CGFloat = 0
    /// `native: true` on the part's recipe.
    public var native: Bool = false
    public var overflowHidden = false
    public var overflowScroll = false

    public init() {}

    public init(_ v: FfiVisual) {
        background = v.backgroundColor.flatMap { Color(hex: $0) }
        color = v.color.flatMap { Color(hex: $0) }
        borderWidth = CGFloat(v.borderWidth ?? 0)
        borderColor = v.borderColor.flatMap { Color(hex: $0) }
        radius = CGFloat(v.borderRadius ?? 0)
        opacity = v.opacity.map { Double($0) }
        if let json = v.boxShadowJson, case let .array(layers) = JSONValue.parse(json) {
            shadows = layers.compactMap { l in
                guard let o = l.object else { return nil }
                return ShadowLayer(x: CGFloat(o["x"]?.number ?? 0), y: CGFloat(o["y"]?.number ?? 0), blur: CGFloat(o["blur"]?.number ?? 0), spread: CGFloat(o["spread"]?.number ?? 0), color: o["color"]?.string.flatMap { Color(hex: $0) } ?? .black.opacity(0.2))
            }
        }
        fontSize = v.fontSize.map { CGFloat($0) }
        fontWeight = v.fontWeight.map { Int($0) }
        lineHeight = v.lineHeight.map { CGFloat($0) }
        fontFamily = v.fontFamily
        textAlign = v.textAlign
        paddingHorizontal = CGFloat(v.paddingHorizontal ?? 0)
        paddingVertical = CGFloat(v.paddingVertical ?? 0)
        gap = CGFloat(v.gap ?? 0)
        native = v.native
        overflowHidden = v.overflowHidden
        overflowScroll = v.overflowScroll
    }

    /// The inset of the content box inside a measured leaf (padding + border).
    public var inset: (CGFloat, CGFloat) { (paddingHorizontal + borderWidth, paddingVertical + borderWidth) }
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

extension View {
    /// A box painted from a style: background, border (inside), radius,
    /// shadow and opacity. The border is drawn as an overlay so children are
    /// never offset (their frames already include it).
    @ViewBuilder
    func paintedBox(_ s: PaintStyle, size: CGSize) -> some View {
        let r = min(s.radius, min(size.width, size.height) / 2)
        self
            .background {
                if let bg = s.background {
                    RoundedRectangle(cornerRadius: r, style: .continuous).fill(bg)
                }
            }
            .overlay {
                if s.borderWidth > 0 {
                    RoundedRectangle(cornerRadius: r, style: .continuous).strokeBorder(s.borderColor ?? .clear, lineWidth: s.borderWidth)
                }
            }
            .modifier(ShadowsModifier(shadows: s.shadows, radius: r, size: size))
            .opacity(s.opacity ?? 1)
    }
}

private struct ShadowsModifier: ViewModifier {
    let shadows: [ShadowLayer]
    let radius: CGFloat
    let size: CGSize

    func body(content: Content) -> some View {
        content.background {
            ForEach(Array(shadows.enumerated()), id: \.offset) { _, l in
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(l.color)
                    .padding(-l.spread)
                    .blur(radius: l.blur / 2)
                    .offset(x: l.x, y: l.y)
            }
        }
    }
}

extension CGRect {
    init(_ f: FfiFrame) {
        self.init(x: CGFloat(f.x), y: CGFloat(f.y), width: CGFloat(f.w), height: CGFloat(f.h))
    }
}
