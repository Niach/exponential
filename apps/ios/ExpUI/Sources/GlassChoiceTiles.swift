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
/// primitive (the eUI core `ChoiceTiles` painter's look) on the glass
/// tokens: the row fill, the card hairline, a picked tile = the active fill
/// with the `selectedRing` primary ring; `columns` per row (phones
/// `phoneColumns`), `radius` 10, `gap` 8, `padding` 8, pictures at
/// `pictureAspect` 1.6.
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
        columns: Int = 2,
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
        ChoiceTiles(
            tiles.map {
                ChoiceTile(label: $0.label, description: $0.description, hasPicture: $0.hasPicture, identifier: $0.identifier)
            },
            selected: selected,
            multiple: multiple,
            enabled: enabled,
            style: Self.style(columns: columns),
            onToggle: onToggle,
            picture: picture,
            placeholder: {
                AppIcon(AppIcons.editorImage, size: 18)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        )
    }

    /// The SDK style on the glass tokens.
    static func style(columns: Int) -> ChoiceTilesStyle {
        ChoiceTilesStyle(
            columns: columns,
            tileFill: GlassTokens.fillRow,
            tileFillSelected: GlassTokens.fillActive,
            stroke: GlassTokens.strokeCard,
            strokeWidth: GlassTokens.hairline,
            accent: DesignTokens.Palette.primary,
            pictureFill: Color.white.opacity(0.06),
            label: .white,
            description: .white.opacity(TextOpacity.tertiary),
            labelFont: .caption.weight(.medium),
            descriptionFont: .caption2
        )
    }
}
