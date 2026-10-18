import ExpCore
import SwiftUI
internal import ExponentialUIPrimitives

/// One picture tile of `GlassChoiceTiles`.
public struct GlassChoiceTile: Equatable, Sendable {
    public let label: String
    public let description: String?
    /// The option has a picture (else the tile keeps the frame with the
    /// placeholder).
    public let hasPicture: Bool
    public let identifier: String?

    public init(label: String, description: String? = nil, hasPicture: Bool, identifier: String? = nil) {
        self.label = label
        self.description = description
        self.hasPicture = hasPicture
        self.identifier = identifier
    }
}

/// EXP-1280: THE picture choice of the plan / question card (fixtures
/// `plan-card.json` `tiles` ×4) — the Exponential UI SDK's `ChoiceTiles`
/// primitive at its own recipe look (type, stroke, placeholder, gaps, the
/// selected ring) painted with the glass tokens as the theme's: card = the
/// row fill, secondary (hover) = the card fill, accent (selected) = the
/// active fill, border = the card hairline, the primary ring. The app passes only the columns
/// (phones `phoneColumns`), the attachment loader and the fixture geometry.
public struct GlassChoiceTiles<Picture: View>: View {
    let tiles: [GlassChoiceTile]
    let selected: Set<Int>
    let multiple: Bool
    let enabled: Bool
    let columns: Int
    let onToggle: (Int) -> Void
    let picture: (Int) -> Picture

    public init(
        _ tiles: [GlassChoiceTile],
        selected: Set<Int>,
        multiple: Bool,
        enabled: Bool = true,
        columns: Int = PlanCard.Tiles.columns,
        onToggle: @escaping (Int) -> Void,
        @ViewBuilder picture: @escaping (Int) -> Picture
    ) {
        self.tiles = tiles
        self.selected = selected
        self.multiple = multiple
        self.enabled = enabled
        self.columns = columns
        self.onToggle = onToggle
        self.picture = picture
    }

    public var body: some View {
        let style = Self.style(columns: columns)
        ChoiceTiles(
            tiles.map {
                ChoiceTile(label: $0.label, description: $0.description, hasPicture: $0.hasPicture, identifier: $0.identifier)
            },
            selected: selected,
            multiple: multiple,
            enabled: enabled,
            style: style,
            onToggle: onToggle,
            picture: picture,
            placeholder: {
                AppIcon(AppIcons.editorImage, size: style.placeholderGlyph)
            }
        )
    }

    /// The glass tokens as the SDK theme.
    static var tokens: PrimitiveTokens { PrimitiveTokens(
        foreground: .white,
        mutedForeground: .white.opacity(TextOpacity.tertiary),
        card: GlassTokens.fillRow,
        muted: Color.white.opacity(0.06),
        border: GlassTokens.strokeCard,
        primary: DesignTokens.Palette.primary,
        accent: GlassTokens.fillActive,
        secondary: GlassTokens.fillCard,
        hairline: CGFloat(PlanCard.Tiles.stroke)
    ) }

    /// The SDK's recipe style on the glass tokens at the fixture geometry.
    static func style(columns: Int) -> ChoiceTilesStyle {
        var style = ChoiceTilesStyle.default(tokens)
        style.columns = columns
        style.gap = PlanCard.Tiles.gap
        style.radius = PlanCard.Tiles.radius
        style.padding = PlanCard.Tiles.padding
        style.pictureAspect = PlanCard.Tiles.pictureAspect
        style.pictureRadius = PlanCard.Tiles.pictureRadius
        style.selectedRing = PlanCard.Tiles.selectedRing
        style.strokeWidth = PlanCard.Tiles.stroke
        style.labelGap = PlanCard.Tiles.labelGap
        style.descriptionGap = PlanCard.Tiles.labelGap
        style.placeholderGlyph = PlanCard.Tiles.placeholderGlyph
        style.inertOpacity = PlanCard.Tiles.inertOpacity
        return style
    }
}
