import SwiftUI

/// The look of `ChoiceTiles` (EXP-1280) = the eUI core `ChoiceTiles` recipe
/// and `plan-card.json` `tiles` ×4: padding 8 (7 under the selected 2pt
/// ring, so the content never shifts), gap 8, radius 10, a 1pt border, the
/// picture at aspect 1.6 with radius 6, picture → label 4, label →
/// description 4, the label 14 medium over the 12 muted description, the
/// placeholder glyph 20 at 0.6, inert tiles at 0.5; fills card / secondary
/// (hover) / accent (selected) off the theme tokens (`default(_:)`).
public struct ChoiceTilesStyle: Equatable, Sendable {
    public var columns: Int
    public var gap: CGFloat
    public var radius: CGFloat
    public var padding: CGFloat
    /// The picture's width / height.
    public var pictureAspect: CGFloat
    public var pictureRadius: CGFloat
    public var selectedRing: CGFloat
    public var tileFill: Color
    public var tileFillHover: Color
    public var tileFillSelected: Color
    public var stroke: Color
    public var strokeWidth: CGFloat
    public var accent: Color
    public var pictureFill: Color
    public var label: Color
    public var description: Color
    public var labelFont: Font
    public var descriptionFont: Font
    /// The gap between the picture and the label.
    public var labelGap: CGFloat
    /// The gap between the label and the description.
    public var descriptionGap: CGFloat
    /// The placeholder glyph's size and opacity (a tile without a picture).
    public var placeholderGlyph: CGFloat
    public var placeholderOpacity: Double
    /// An inert (unpicked, disabled) tile.
    public var inertOpacity: Double

    public init(
        columns: Int = 2,
        gap: CGFloat = 8,
        radius: CGFloat = 10,
        padding: CGFloat = 8,
        pictureAspect: CGFloat = 1.6,
        pictureRadius: CGFloat = 6,
        selectedRing: CGFloat = 2,
        tileFill: Color = PrimitiveTokens.system.card,
        tileFillHover: Color = PrimitiveTokens.system.secondary,
        tileFillSelected: Color = PrimitiveTokens.system.accent,
        stroke: Color = PrimitiveTokens.system.border,
        strokeWidth: CGFloat = 1,
        accent: Color = PrimitiveTokens.system.primary,
        pictureFill: Color = PrimitiveTokens.system.muted,
        label: Color = PrimitiveTokens.system.foreground,
        description: Color = PrimitiveTokens.system.mutedForeground,
        labelFont: Font = .system(size: 14, weight: .medium),
        descriptionFont: Font = .system(size: 12),
        labelGap: CGFloat = 4,
        descriptionGap: CGFloat = 4,
        placeholderGlyph: CGFloat = 20,
        placeholderOpacity: Double = 0.6,
        inertOpacity: Double = 0.5
    ) {
        self.columns = columns
        self.gap = gap
        self.radius = radius
        self.padding = padding
        self.pictureAspect = pictureAspect
        self.pictureRadius = pictureRadius
        self.selectedRing = selectedRing
        self.tileFill = tileFill
        self.tileFillHover = tileFillHover
        self.tileFillSelected = tileFillSelected
        self.stroke = stroke
        self.strokeWidth = strokeWidth
        self.accent = accent
        self.pictureFill = pictureFill
        self.label = label
        self.description = description
        self.labelFont = labelFont
        self.descriptionFont = descriptionFont
        self.labelGap = labelGap
        self.descriptionGap = descriptionGap
        self.placeholderGlyph = placeholderGlyph
        self.placeholderOpacity = placeholderOpacity
        self.inertOpacity = inertOpacity
    }

    /// The control on `tokens` (the recipe's colours: card tiles, secondary
    /// hover, accent selected, border hairlines, the primary ring).
    public static func `default`(_ tokens: PrimitiveTokens) -> ChoiceTilesStyle {
        ChoiceTilesStyle(
            tileFill: tokens.card,
            tileFillHover: tokens.secondary,
            tileFillSelected: tokens.accent,
            stroke: tokens.border,
            strokeWidth: tokens.hairline,
            accent: tokens.primary,
            pictureFill: tokens.muted,
            label: tokens.foreground,
            description: tokens.mutedForeground
        )
    }
}

/// One option of `ChoiceTiles`. `hasPicture` = the tile shows a picture
/// slot (filled by the control's `picture`, else the placeholder).
public struct ChoiceTile: Equatable, Sendable {
    public var label: String
    public var description: String?
    public var hasPicture: Bool
    public var disabled: Bool
    /// An accessibility identifier for UI tests.
    public var identifier: String?

    public init(label: String, description: String? = nil, hasPicture: Bool = true, disabled: Bool = false, identifier: String? = nil) {
        self.label = label
        self.description = description
        self.hasPicture = hasPicture
        self.disabled = disabled
        self.identifier = identifier
    }
}

/// The picture choice (EXP-1280; eUI core `ChoiceTiles`): a grid of
/// `style.columns` equal tiles, each = the picture (`pictureAspect`, cropped,
/// `pictureRadius`) over the label and the muted
/// description; a picked tile wears the `selectedRing` accent ring. Single
/// (`multiple` false: radios) or several picks (checkboxes); `onToggle` gets
/// the option's index. A tile whose option has no picture keeps the frame
/// with `placeholder` in it, so a row keeps one height.
public struct ChoiceTiles<Picture: View, Placeholder: View>: View {
    let tiles: [ChoiceTile]
    let selected: Set<Int>
    let multiple: Bool
    let enabled: Bool
    let style: ChoiceTilesStyle
    let onToggle: (Int) -> Void
    let picture: (Int) -> Picture
    let placeholder: () -> Placeholder

    public init(
        _ tiles: [ChoiceTile],
        selected: Set<Int>,
        multiple: Bool = false,
        enabled: Bool = true,
        style: ChoiceTilesStyle = ChoiceTilesStyle(),
        onToggle: @escaping (Int) -> Void,
        @ViewBuilder picture: @escaping (Int) -> Picture,
        @ViewBuilder placeholder: @escaping () -> Placeholder
    ) {
        self.tiles = tiles
        self.selected = selected
        self.multiple = multiple
        self.enabled = enabled
        self.style = style
        self.onToggle = onToggle
        self.picture = picture
        self.placeholder = placeholder
    }

    public var body: some View {
        let columns = min(max(style.columns, 1), 4)
        let anyPicture = tiles.contains(where: \.hasPicture)
        let rows = stride(from: 0, to: tiles.count, by: columns).map { start in
            Array(start..<min(start + columns, tiles.count))
        }
        VStack(alignment: .leading, spacing: style.gap) {
            ForEach(rows, id: \.first) { row in
                HStack(alignment: .top, spacing: style.gap) {
                    ForEach(row, id: \.self) { index in
                        tile(index, showPicture: anyPicture)
                    }
                    ForEach(0..<(columns - row.count), id: \.self) { _ in
                        Color.clear.frame(maxWidth: .infinity, maxHeight: 1)
                    }
                }
                .fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .contain)
    }

    private func tile(_ index: Int, showPicture: Bool) -> some View {
        let tile = tiles[index]
        let picked = selected.contains(index)
        let live = enabled && !tile.disabled
        return ChoiceTileCell(
            tile: tile,
            picked: picked,
            live: live,
            multiple: multiple,
            showPicture: showPicture,
            style: style,
            onTap: { onToggle(index) },
            picture: { picture(index) },
            placeholder: placeholder
        )
    }
}

/// One tile: the picture (or the placeholder) over the label and the muted
/// description; hover tints it `tileFillHover`, a pick wears the ring with the
/// padding reduced by it.
private struct ChoiceTileCell<Picture: View, Placeholder: View>: View {
    let tile: ChoiceTile
    let picked: Bool
    let live: Bool
    let multiple: Bool
    let showPicture: Bool
    let style: ChoiceTilesStyle
    let onTap: () -> Void
    let picture: () -> Picture
    let placeholder: () -> Placeholder

    @State private var hovering = false

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: style.radius, style: .continuous)
        let inner = RoundedRectangle(cornerRadius: style.pictureRadius, style: .continuous)
        let border = picked ? style.selectedRing : style.strokeWidth
        let fill = picked ? style.tileFillSelected : (hovering && live ? style.tileFillHover : style.tileFill)
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 0) {
                if showPicture {
                    Color.clear
                        .aspectRatio(style.pictureAspect, contentMode: .fit)
                        .frame(maxWidth: .infinity)
                        .overlay {
                            if tile.hasPicture {
                                picture()
                            } else {
                                placeholder()
                                    .frame(width: style.placeholderGlyph, height: style.placeholderGlyph)
                                    .foregroundStyle(style.description)
                                    .opacity(style.placeholderOpacity)
                            }
                        }
                        .background(style.pictureFill)
                        .clipShape(inner)
                        .padding(.bottom, style.labelGap)
                }
                Text(tile.label)
                    .font(style.labelFont)
                    .foregroundStyle(style.label)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                if let description = tile.description, !description.isEmpty {
                    Text(description)
                        .font(style.descriptionFont)
                        .foregroundStyle(style.description)
                        .multilineTextAlignment(.leading)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, style.descriptionGap)
                }
            }
            // The ring eats into the padding, so a pick never shifts content.
            .padding(max(style.padding - (border - style.strokeWidth), 0))
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .background(fill, in: shape)
            .overlay {
                shape.strokeBorder(picked ? style.accent : style.stroke, lineWidth: border)
            }
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        // Inert without the system's disabled dimming: a picked tile of a
        // closed card stays highlighted (its ring in the accent), the rest
        // fade to `inertOpacity`.
        .allowsHitTesting(live)
        .accessibilityRespondsToUserInteraction(live)
        .opacity(live || picked ? 1 : style.inertOpacity)
        .accessibilityAddTraits(picked ? [.isSelected] : [])
        .accessibilityValue(multiple ? (picked ? "on" : "off") : "")
        .modifier(TileIdentifier(identifier: tile.identifier))
    }
}

public extension ChoiceTiles where Placeholder == EmptyView {
    init(
        _ tiles: [ChoiceTile],
        selected: Set<Int>,
        multiple: Bool = false,
        enabled: Bool = true,
        style: ChoiceTilesStyle = ChoiceTilesStyle(),
        onToggle: @escaping (Int) -> Void,
        @ViewBuilder picture: @escaping (Int) -> Picture
    ) {
        self.init(
            tiles, selected: selected, multiple: multiple, enabled: enabled, style: style,
            onToggle: onToggle, picture: picture, placeholder: { EmptyView() }
        )
    }
}

private struct TileIdentifier: ViewModifier {
    let identifier: String?

    func body(content: Content) -> some View {
        if let identifier {
            content.accessibilityIdentifier(identifier)
        } else {
            content
        }
    }
}
