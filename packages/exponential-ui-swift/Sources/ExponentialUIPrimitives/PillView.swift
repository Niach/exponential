import SwiftUI

/// The pill geometry: one capsule height, horizontal padding, a leading
/// dot or glyph, a label. The painter sizes it from the `Pill` recipe; the
/// Exponential app's `GlassPill` maps its sizes onto it.
public struct PillStyle: Equatable, Sendable {
    public var height: CGFloat
    public var horizontalPadding: CGFloat
    public var spacing: CGFloat
    public var fontSize: CGFloat
    public var fontWeight: Font.Weight
    public var fill: Color
    public var stroke: Color?
    public var strokeWidth: CGFloat
    public var label: Color
    public var dotSize: CGFloat
    public var glyphSize: CGFloat
    /// A font that wins over `fontSize`/`fontWeight` (an app's Dynamic Type
    /// style, e.g. `.caption.weight(.medium)`); nil = the fixed system size.
    public var font: Font?
    /// A surface laid UNDER `fill` (an opaque card beneath a translucent
    /// glass fill, so content scrolling behind the pill cannot bleed through).
    public var underlay: Color?
    /// Stroke the hairline on the capsule's edge (half outside, SwiftUI
    /// `stroke`) instead of inside it (`strokeBorder`, the default).
    public var strokeCentered: Bool

    public init(
        height: CGFloat = 24,
        horizontalPadding: CGFloat = 8,
        spacing: CGFloat = 4,
        fontSize: CGFloat = 12,
        fontWeight: Font.Weight = .medium,
        fill: Color = .clear,
        stroke: Color? = nil,
        strokeWidth: CGFloat = 1,
        label: Color = .primary,
        dotSize: CGFloat = 6,
        glyphSize: CGFloat = 12,
        font: Font? = nil,
        underlay: Color? = nil,
        strokeCentered: Bool = false
    ) {
        self.height = height
        self.horizontalPadding = horizontalPadding
        self.spacing = spacing
        self.fontSize = fontSize
        self.fontWeight = fontWeight
        self.fill = fill
        self.stroke = stroke
        self.strokeWidth = strokeWidth
        self.label = label
        self.dotSize = dotSize
        self.glyphSize = glyphSize
        self.font = font
        self.underlay = underlay
        self.strokeCentered = strokeCentered
    }

    /// The label font: `font` when set, else the fixed system size.
    public var resolvedFont: Font {
        font ?? .system(size: fontSize, weight: fontWeight)
    }
}

/// A capsule label with an optional leading dot or glyph. Not a button:
/// wrap it in one to make it pressable (the painter does).
public struct PillView<Leading: View, Trailing: View>: View {
    let label: String
    let style: PillStyle
    let dot: Color?
    let leading: Leading
    let trailing: Trailing

    public init(_ label: String, style: PillStyle, dot: Color? = nil, @ViewBuilder leading: () -> Leading, @ViewBuilder trailing: () -> Trailing) {
        self.label = label
        self.style = style
        self.dot = dot
        self.leading = leading()
        self.trailing = trailing()
    }

    public var body: some View {
        HStack(spacing: style.spacing) {
            if let dot {
                Circle().fill(dot).frame(width: style.dotSize, height: style.dotSize)
            }
            leading
            if !label.isEmpty {
                Text(label).font(style.resolvedFont).lineLimit(1)
            }
            trailing
        }
        .foregroundStyle(style.label)
        .padding(.horizontal, style.horizontalPadding)
        .frame(height: style.height)
        .background(style.fill, in: Capsule())
        .background {
            if let underlay = style.underlay {
                Capsule().fill(underlay)
            }
        }
        .overlay {
            if let stroke = style.stroke {
                if style.strokeCentered {
                    Capsule().stroke(stroke, lineWidth: style.strokeWidth)
                } else {
                    Capsule().strokeBorder(stroke, lineWidth: style.strokeWidth)
                }
            }
        }
    }
}

extension PillView where Leading == EmptyView, Trailing == EmptyView {
    public init(_ label: String, style: PillStyle, dot: Color? = nil) {
        self.init(label, style: style, dot: dot, leading: { EmptyView() }, trailing: { EmptyView() })
    }
}
