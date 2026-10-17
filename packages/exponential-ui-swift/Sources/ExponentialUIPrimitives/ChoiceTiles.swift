import SwiftUI

/// The look of `ChoiceTiles` (EXP-1280, the eUI core `ChoiceTiles` recipe and
/// `plan-card.json` `tiles` ×4): radius 10, gap 8, picture aspect 1.6 (w/h),
/// padding 8, a 2pt accent ring on a picked tile, the label over a muted
/// description.
public struct ChoiceTilesStyle: Equatable, Sendable {
    public var columns: Int
    public var gap: CGFloat
    public var radius: CGFloat
    public var padding: CGFloat
    /// The picture's width / height.
    public var pictureAspect: CGFloat
    public var selectedRing: CGFloat
    public var tileFill: Color
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
    public var pictureGap: CGFloat

    public init(
        columns: Int = 2,
        gap: CGFloat = 8,
        radius: CGFloat = 10,
        padding: CGFloat = 8,
        pictureAspect: CGFloat = 1.6,
        selectedRing: CGFloat = 2,
        tileFill: Color = Color.gray.opacity(0.08),
        tileFillSelected: Color = Color.gray.opacity(0.16),
        stroke: Color = Color.gray.opacity(0.3),
        strokeWidth: CGFloat = 1,
        accent: Color = .accentColor,
        pictureFill: Color = Color.gray.opacity(0.15),
        label: Color = .primary,
        description: Color = .secondary,
        labelFont: Font = .system(size: 13, weight: .medium),
        descriptionFont: Font = .system(size: 12),
        pictureGap: CGFloat = 6
    ) {
        self.columns = columns
        self.gap = gap
        self.radius = radius
        self.padding = padding
        self.pictureAspect = pictureAspect
        self.selectedRing = selectedRing
        self.tileFill = tileFill
        self.tileFillSelected = tileFillSelected
        self.stroke = stroke
        self.strokeWidth = strokeWidth
        self.accent = accent
        self.pictureFill = pictureFill
        self.label = label
        self.description = description
        self.labelFont = labelFont
        self.descriptionFont = descriptionFont
        self.pictureGap = pictureGap
    }

    /// The control on `tokens`: muted tiles, border hairlines, the primary ring.
    public static func `default`(_ tokens: PrimitiveTokens) -> ChoiceTilesStyle {
        ChoiceTilesStyle(
            tileFill: tokens.muted.opacity(0.5),
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
/// the tile's radius less half its padding) over the label and the muted
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
        let shape = RoundedRectangle(cornerRadius: style.radius, style: .continuous)
        let inner = RoundedRectangle(cornerRadius: max(style.radius - style.padding / 2, 2), style: .continuous)
        return Button {
            onToggle(index)
        } label: {
            VStack(alignment: .leading, spacing: 0) {
                if showPicture {
                    Color.clear
                        .aspectRatio(style.pictureAspect, contentMode: .fit)
                        .frame(maxWidth: .infinity)
                        .overlay {
                            if tile.hasPicture { picture(index) } else { placeholder() }
                        }
                        .background(style.pictureFill)
                        .clipShape(inner)
                        .padding(.bottom, style.pictureGap)
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
                        .padding(.top, 2)
                }
            }
            .padding(style.padding)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .background(picked ? style.tileFillSelected : style.tileFill, in: shape)
            .overlay {
                shape.strokeBorder(
                    picked ? style.accent : style.stroke,
                    lineWidth: picked ? style.selectedRing : style.strokeWidth
                )
            }
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        // Inert without the system's disabled dimming: a picked tile of a
        // closed card stays highlighted (its ring in the accent), the rest
        // fade.
        .allowsHitTesting(live)
        .accessibilityRespondsToUserInteraction(live)
        .opacity(live || picked ? 1 : 0.6)
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
