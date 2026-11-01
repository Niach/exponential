import SwiftUI

/// The pill's glyphs by catalog concept (`builtinIcons`
/// `ZoomableImage.zoomOut|zoomIn|fit`, the Menu's `check`).
public enum ZoomGlyph: String, Sendable, CaseIterable {
    case zoomOut = "ui-minus"
    case zoomIn = "ui-add"
    case fit = "ui-fullscreen"
    case check = "ui-check"

    /// The SF Symbol the default glyph draws.
    public var symbol: String {
        switch self {
        case .zoomOut: "minus"
        case .zoomIn: "plus"
        case .fit: "arrow.up.left.and.arrow.down.right"
        case .check: "checkmark"
        }
    }
}

/// The built-in strings the pill reads (`strings.json` `zoomOut`, `zoomIn`,
/// `zoomToFit`, `zoomLevel`, `zoomFit`); English by default.
public struct ZoomStrings: Equatable, Sendable {
    public var zoomOut: String
    public var zoomIn: String
    public var zoomToFit: String
    public var zoomLevel: String
    public var zoomFit: String

    public init(zoomOut: String = "Zoom out", zoomIn: String = "Zoom in", zoomToFit: String = "Zoom to fit", zoomLevel: String = "Zoom level", zoomFit: String = "Fit") {
        self.zoomOut = zoomOut
        self.zoomIn = zoomIn
        self.zoomToFit = zoomToFit
        self.zoomLevel = zoomLevel
        self.zoomFit = zoomFit
    }

    public static let english = ZoomStrings()
}

/// One pill button's look per state (the recipe parts `zoomOut`,
/// `percent`, `zoomIn`, `fit`: base, `hover`, `selected`, `disabled`).
public struct ZoomButtonLook: Equatable, Sendable {
    public var width: CGFloat?
    public var height: CGFloat
    public var radius: CGFloat
    public var paddingHorizontal: CGFloat
    public var foreground: Color
    public var background: Color
    public var hoverForeground: Color
    public var hoverBackground: Color
    public var selectedForeground: Color
    public var selectedBackground: Color
    public var disabledOpacity: Double
    public var glyphSize: CGFloat

    public init(width: CGFloat? = 28, height: CGFloat = 28, radius: CGFloat = 14, paddingHorizontal: CGFloat = 0, foreground: Color = .primary, background: Color = .clear, hoverForeground: Color = .primary, hoverBackground: Color = Color.gray.opacity(0.2), selectedForeground: Color = .primary, selectedBackground: Color = Color.gray.opacity(0.2), disabledOpacity: Double = 0.5, glyphSize: CGFloat = 16) {
        self.width = width
        self.height = height
        self.radius = radius
        self.paddingHorizontal = paddingHorizontal
        self.foreground = foreground
        self.background = background
        self.hoverForeground = hoverForeground
        self.hoverBackground = hoverBackground
        self.selectedForeground = selectedForeground
        self.selectedBackground = selectedBackground
        self.disabledOpacity = disabledOpacity
        self.glyphSize = glyphSize
    }
}

/// THE Menu's look (the eUI `Menu` recipe: `content`, `item`, `check`).
public struct ZoomMenuLook: Equatable, Sendable {
    public var background: Color
    public var border: Color
    public var borderWidth: CGFloat
    public var radius: CGFloat
    public var padding: CGFloat
    public var shadow: Color
    public var shadowRadius: CGFloat
    public var shadowY: CGFloat
    public var minWidth: CGFloat
    public var itemHeight: CGFloat
    public var itemPaddingHorizontal: CGFloat
    public var itemRadius: CGFloat
    public var itemGap: CGFloat
    public var itemForeground: Color
    public var itemHoverForeground: Color
    public var itemHoverBackground: Color
    public var checkSize: CGFloat
    public var checkColor: Color
    public var font: Font

    public init(background: Color = Color.gray.opacity(0.15), border: Color = Color.gray.opacity(0.3), borderWidth: CGFloat = 1, radius: CGFloat = 6, padding: CGFloat = 4, shadow: Color = .black.opacity(0.12), shadowRadius: CGFloat = 6, shadowY: CGFloat = 4, minWidth: CGFloat = 128, itemHeight: CGFloat = 32, itemPaddingHorizontal: CGFloat = 8, itemRadius: CGFloat = 4, itemGap: CGFloat = 8, itemForeground: Color = .primary, itemHoverForeground: Color = .primary, itemHoverBackground: Color = Color.gray.opacity(0.2), checkSize: CGFloat = 16, checkColor: Color = .primary, font: Font = .system(size: 14)) {
        self.background = background
        self.border = border
        self.borderWidth = borderWidth
        self.radius = radius
        self.padding = padding
        self.shadow = shadow
        self.shadowRadius = shadowRadius
        self.shadowY = shadowY
        self.minWidth = minWidth
        self.itemHeight = itemHeight
        self.itemPaddingHorizontal = itemPaddingHorizontal
        self.itemRadius = itemRadius
        self.itemGap = itemGap
        self.itemForeground = itemForeground
        self.itemHoverForeground = itemHoverForeground
        self.itemHoverBackground = itemHoverBackground
        self.checkSize = checkSize
        self.checkColor = checkColor
        self.font = font
    }
}

/// THE pill's look: the glass capsule (recipe part `pill`), its buttons,
/// the hairline divider, the percent text and THE Menu. `standard(_:)`
/// builds the neutral recipe's look from `PrimitiveTokens`; the eUI painter
/// fills one from the theme's recipes.
public struct ZoomPillStyle: Equatable, Sendable {
    public var metrics: ZoomPillMetrics
    public var background: Color
    public var border: Color
    public var borderWidth: CGFloat
    /// Backdrop blur (a material under `background`).
    public var glass: Bool
    public var shadow: Color
    public var shadowRadius: CGFloat
    public var shadowY: CGFloat
    /// − and + (recipe parts `zoomOut`, `zoomIn`).
    public var button: ZoomButtonLook
    /// The fit button (recipe part `fit`; `selected` = at rest).
    public var fit: ZoomButtonLook
    public var percent: ZoomButtonLook
    public var percentFont: Font
    public var divider: Color
    public var dividerWidth: CGFloat
    public var menu: ZoomMenuLook

    public init(metrics: ZoomPillMetrics = .standard, background: Color, border: Color, borderWidth: CGFloat, glass: Bool = true, shadow: Color = .black.opacity(0.18), shadowRadius: CGFloat = 12, shadowY: CGFloat = 6, button: ZoomButtonLook, fit: ZoomButtonLook? = nil, percent: ZoomButtonLook, percentFont: Font = .system(size: 14, weight: .medium).monospacedDigit(), divider: Color, dividerWidth: CGFloat = 1, menu: ZoomMenuLook) {
        self.metrics = metrics
        self.background = background
        self.border = border
        self.borderWidth = borderWidth
        self.glass = glass
        self.shadow = shadow
        self.shadowRadius = shadowRadius
        self.shadowY = shadowY
        self.button = button
        self.fit = fit ?? button
        self.percent = percent
        self.percentFont = percentFont
        self.divider = divider
        self.dividerWidth = dividerWidth
        self.menu = menu
    }

    /// The neutral recipe's pill in `tokens` (popover fill = `card`).
    public static func standard(_ tokens: PrimitiveTokens = .system) -> ZoomPillStyle {
        let button = ZoomButtonLook(width: 28, height: 28, radius: 14, foreground: tokens.foreground, hoverForeground: tokens.foreground, hoverBackground: tokens.accent, selectedForeground: tokens.foreground, selectedBackground: tokens.accent)
        var percent = button
        percent.width = nil
        percent.paddingHorizontal = 12
        let menu = ZoomMenuLook(background: tokens.card, border: tokens.border, borderWidth: tokens.hairline, radius: tokens.radiusMd, itemHeight: 32, itemRadius: tokens.radiusSm, itemForeground: tokens.foreground, itemHoverForeground: tokens.foreground, itemHoverBackground: tokens.accent, checkColor: tokens.foreground)
        return ZoomPillStyle(background: tokens.card.opacity(0.8), border: tokens.border, borderWidth: tokens.hairline, button: button, percent: percent, divider: tokens.border, menu: menu)
    }
}

/// THE zoom pill (catalog/style.json `zoomableImage`): one glass capsule
/// with − / the percent (a button opening THE Menu of the core's `menu`
/// rows: Fit, 50%, 100%, 200%, 400%) / + / a hairline divider / the fit
/// button (drawn selected while at rest). − is disabled at the min, + at
/// the max. The Menu is this package's own panel above the percent button,
/// never a system menu or popover.
public struct ZoomPill: View {
    let controller: ZoomController
    let style: ZoomPillStyle
    let strings: ZoomStrings
    let glyph: (ZoomGlyph, CGFloat, Color) -> AnyView

    @State private var hovered: String?

    /// `glyph` draws a concept at a size in a colour (nil = SF Symbols).
    public init(controller: ZoomController, style: ZoomPillStyle = .standard(), strings: ZoomStrings = .english, glyph: ((ZoomGlyph, CGFloat, Color) -> AnyView)? = nil) {
        self.controller = controller
        self.style = style
        self.strings = strings
        self.glyph = glyph ?? { g, size, color in
            AnyView(Image(systemName: g.symbol).font(.system(size: size * 0.8, weight: .medium)).foregroundStyle(color).frame(width: size, height: size))
        }
    }

    public var body: some View {
        let bounds = controller.bounds
        let m = style.metrics
        HStack(spacing: m.gap) {
            iconButton("zoomOut", .zoomOut, label: strings.zoomOut, disabled: bounds.atMin) { controller.zoomOut() }
            percentButton
            iconButton("zoomIn", .zoomIn, label: strings.zoomIn, disabled: bounds.atMax) { controller.zoomIn() }
            Rectangle()
                .fill(style.divider)
                .frame(width: style.dividerWidth, height: m.dividerHeight)
                .accessibilityHidden(true)
            iconButton("fit", .fit, look: style.fit, label: strings.zoomToFit, disabled: false, selected: controller.atRest) { controller.fit() }
                .accessibilityAddTraits(controller.atRest ? .isSelected : [])
        }
        .padding(m.padding)
        .frame(height: m.height)
        .background {
            ZStack {
                if style.glass { Capsule().fill(.ultraThinMaterial) }
                Capsule().fill(style.background)
            }
            .shadow(color: style.shadow, radius: style.shadowRadius, y: style.shadowY)
        }
        .overlay { Capsule().strokeBorder(style.border, lineWidth: style.borderWidth) }
        .onHover { controller.pillHover($0) }
        .accessibilityElement(children: .contain)
    }

    private func look(_ base: ZoomButtonLook, part: String, selected: Bool) -> (fg: Color, bg: Color) {
        if selected { return (base.selectedForeground, base.selectedBackground) }
        if hovered == part { return (base.hoverForeground, base.hoverBackground) }
        return (base.foreground, base.background)
    }

    private func iconButton(_ part: String, _ g: ZoomGlyph, look b: ZoomButtonLook? = nil, label: String, disabled: Bool, selected: Bool = false, action: @escaping () -> Void) -> some View {
        let b = b ?? style.button
        let c = look(b, part: part, selected: selected)
        return Button(action: action) {
            glyph(g, b.glyphSize, c.fg)
                .frame(width: b.width ?? b.height, height: b.height)
                .background(RoundedRectangle(cornerRadius: b.radius, style: .continuous).fill(c.bg))
                .contentShape(RoundedRectangle(cornerRadius: b.radius, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .opacity(disabled ? b.disabledOpacity : 1)
        .onHover { hovered = $0 ? part : (hovered == part ? nil : hovered) }
        .accessibilityLabel(Text(label))
    }

    private var percentButton: some View {
        let b = style.percent
        let c = look(b, part: "percent", selected: false)
        return Button {
            controller.toggleMenu()
        } label: {
            Text(controller.percent)
                .font(style.percentFont)
                .monospacedDigit()
                .lineLimit(1)
                .foregroundStyle(c.fg)
                .padding(.horizontal, b.paddingHorizontal)
                .frame(minWidth: style.metrics.percentMinWidth, minHeight: b.height, maxHeight: b.height)
                .background(RoundedRectangle(cornerRadius: b.radius, style: .continuous).fill(c.bg))
                .contentShape(RoundedRectangle(cornerRadius: b.radius, style: .continuous))
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 ? "percent" : (hovered == "percent" ? nil : hovered) }
        .accessibilityLabel(Text(strings.zoomLevel))
        .accessibilityValue(Text(controller.percent))
        .accessibilityAddTraits(.isButton)
        .overlay(alignment: .bottom) {
            if controller.menuOpen {
                ZoomMenuPanel(controller: controller, look: style.menu, glyph: glyph)
                    .fixedSize()
                    .alignmentGuide(.bottom) { d in d[.bottom] + b.height / 2 + style.metrics.height / 2 + 4 }
                    .transition(.opacity)
            }
        }
    }
}

/// THE Menu of the percent button: the core's rows (Fit, then the presets
/// inside the limits), the checked one with its check glyph.
struct ZoomMenuPanel: View {
    let controller: ZoomController
    let look: ZoomMenuLook
    let glyph: (ZoomGlyph, CGFloat, Color) -> AnyView
    @State private var hovered: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(controller.menuItems) { item in
                let hover = hovered == item.id
                let fg = hover ? look.itemHoverForeground : look.itemForeground
                Button {
                    controller.select(item)
                } label: {
                    HStack(spacing: look.itemGap) {
                        Text(item.label).font(look.font).monospacedDigit().foregroundStyle(fg)
                        Spacer(minLength: 0)
                        glyph(.check, look.checkSize, look.checkColor).opacity(item.checked ? 1 : 0)
                    }
                    .padding(.horizontal, look.itemPaddingHorizontal)
                    .frame(minWidth: look.minWidth, minHeight: look.itemHeight, maxHeight: look.itemHeight)
                    .background(RoundedRectangle(cornerRadius: look.itemRadius, style: .continuous).fill(hover ? look.itemHoverBackground : .clear))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .onHover { hovered = $0 ? item.id : (hovered == item.id ? nil : hovered) }
                .accessibilityLabel(Text(item.label))
                .accessibilityAddTraits(item.checked ? [.isButton, .isSelected] : .isButton)
            }
        }
        .padding(look.padding)
        .background {
            RoundedRectangle(cornerRadius: look.radius, style: .continuous)
                .fill(look.background)
                .shadow(color: look.shadow, radius: look.shadowRadius, y: look.shadowY)
        }
        .overlay { RoundedRectangle(cornerRadius: look.radius, style: .continuous).strokeBorder(look.border, lineWidth: look.borderWidth) }
        .accessibilityElement(children: .contain)
        .accessibilityAddTraits(.isModal)
    }
}
