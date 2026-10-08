import SwiftUI

/// The look of a segmented control: the track and the selected segment.
public struct SegmentedStyle: Equatable, Sendable {
    public var height: CGFloat
    public var trackFill: Color
    public var trackStroke: Color?
    public var segmentFill: Color
    public var segmentStroke: Color?
    public var label: Color
    public var selectedLabel: Color
    public var disabledLabel: Color
    public var radius: CGFloat
    public var inset: CGFloat
    public var horizontalPadding: CGFloat
    public var fontSize: CGFloat
    public var fontWeight: Font.Weight
    /// A font that wins over `fontSize`/`fontWeight` (a Dynamic Type style).
    public var font: Font?
    /// Capsule track and segments instead of `radius` rounded rectangles.
    public var capsule: Bool
    /// `height` is a MINIMUM (the strip grows with Dynamic Type) instead of a
    /// fixed frame.
    public var minimumHeight: Bool
    /// When set, a segment's height comes from this vertical padding around
    /// its content instead of `height - 2 * inset`.
    public var segmentVerticalPadding: CGFloat?
    /// Draw the track (fill, hairline, inset). `false` = the segments alone,
    /// for a strip embedded as the first row of an already-chromed card.
    public var showsTrack: Bool
    /// The track hairline's width.
    public var trackStrokeWidth: CGFloat
    /// Stroke the track hairline on the shape's edge (SwiftUI `stroke`)
    /// instead of inside it (`strokeBorder`, the default).
    public var strokeCentered: Bool
    /// The label's minimum scale factor before it truncates.
    public var minimumScaleFactor: CGFloat

    public init(
        height: CGFloat = 32,
        trackFill: Color = Color.gray.opacity(0.15),
        trackStroke: Color? = nil,
        segmentFill: Color = Color.gray.opacity(0.3),
        segmentStroke: Color? = nil,
        label: Color = .secondary,
        selectedLabel: Color = .primary,
        disabledLabel: Color = Color.secondary.opacity(0.5),
        radius: CGFloat = 8,
        inset: CGFloat = 2,
        horizontalPadding: CGFloat = 12,
        fontSize: CGFloat = 13,
        fontWeight: Font.Weight = .medium,
        font: Font? = nil,
        capsule: Bool = false,
        minimumHeight: Bool = false,
        segmentVerticalPadding: CGFloat? = nil,
        showsTrack: Bool = true,
        trackStrokeWidth: CGFloat = 1,
        strokeCentered: Bool = false,
        minimumScaleFactor: CGFloat = 1
    ) {
        self.height = height
        self.trackFill = trackFill
        self.trackStroke = trackStroke
        self.segmentFill = segmentFill
        self.segmentStroke = segmentStroke
        self.label = label
        self.selectedLabel = selectedLabel
        self.disabledLabel = disabledLabel
        self.radius = radius
        self.inset = inset
        self.horizontalPadding = horizontalPadding
        self.fontSize = fontSize
        self.fontWeight = fontWeight
        self.font = font
        self.capsule = capsule
        self.minimumHeight = minimumHeight
        self.segmentVerticalPadding = segmentVerticalPadding
        self.showsTrack = showsTrack
        self.trackStrokeWidth = trackStrokeWidth
        self.strokeCentered = strokeCentered
        self.minimumScaleFactor = minimumScaleFactor
    }

    /// The label font: `font` when set, else the fixed system size.
    public var resolvedFont: Font {
        font ?? .system(size: fontSize, weight: fontWeight)
    }
}

/// One segment of a `SegmentedControl`.
public struct Segment<Option: Hashable>: Identifiable {
    public var id: Option { value }
    public let value: Option
    public let label: String
    public let icon: AnyView?
    public let disabled: Bool
    /// A view drawn INSTEAD of the icon + label (counts, badges, accessory
    /// dots). It paints its own colours: the style's label colours are not
    /// applied to it. `label` stays the segment's accessibility label.
    public let content: AnyView?
    /// The spoken label when it differs from `label`.
    public let accessibilityLabel: String?
    /// An accessibility identifier for UI tests.
    public let identifier: String?

    public init(_ value: Option, label: String, icon: AnyView? = nil, disabled: Bool = false) {
        self.init(value, label: label, icon: icon, disabled: disabled, content: nil)
    }

    public init(
        _ value: Option,
        label: String,
        icon: AnyView? = nil,
        disabled: Bool = false,
        content: AnyView?,
        accessibilityLabel: String? = nil,
        identifier: String? = nil
    ) {
        self.value = value
        self.label = label
        self.icon = icon
        self.disabled = disabled
        self.content = content
        self.accessibilityLabel = accessibilityLabel
        self.identifier = identifier
    }
}

/// A segmented control: a track of equal (`fill`) or hugging segments, the
/// selected one raised. Single selection; a multiple-selection group is a
/// row of toggles, not a segmented control.
public struct SegmentedControl<Option: Hashable>: View {
    let segments: [Segment<Option>]
    @Binding var selection: Option
    let style: SegmentedStyle
    let fill: Bool

    public init(_ segments: [Segment<Option>], selection: Binding<Option>, style: SegmentedStyle, fill: Bool = true) {
        self.segments = segments
        self._selection = selection
        self.style = style
        self.fill = fill
    }

    public var body: some View {
        if style.capsule {
            control(track: Capsule(), segment: Capsule(), hit: Capsule())
        } else {
            control(
                track: RoundedRectangle(cornerRadius: style.radius),
                segment: RoundedRectangle(cornerRadius: max(0, style.radius - style.inset)),
                hit: Rectangle()
            )
        }
    }

    @ViewBuilder
    private func control(track: some InsettableShape, segment: some InsettableShape, hit: some Shape) -> some View {
        let strip = HStack(spacing: 0) {
            ForEach(segments) { item in
                segmentButton(item, shape: segment, hit: hit)
            }
        }
        if style.showsTrack {
            strip
                .padding(style.inset)
                .modifier(TrackHeight(height: style.height, minimum: style.minimumHeight))
                .background(style.trackFill, in: track)
                .overlay {
                    if let stroke = style.trackStroke {
                        if style.strokeCentered {
                            track.stroke(stroke, lineWidth: style.trackStrokeWidth)
                        } else {
                            track.strokeBorder(stroke, lineWidth: style.trackStrokeWidth)
                        }
                    }
                }
        } else {
            strip
        }
    }

    private func segmentButton(_ segment: Segment<Option>, shape: some InsettableShape, hit: some Shape) -> some View {
        let selected = segment.value == selection
        return Button {
            selection = segment.value
        } label: {
            label(segment, selected: selected)
                .padding(.horizontal, style.horizontalPadding)
                .frame(maxWidth: fill ? .infinity : nil)
                .modifier(SegmentHeight(
                    height: style.height - 2 * style.inset,
                    verticalPadding: style.segmentVerticalPadding
                ))
                .background(selected ? style.segmentFill : Color.clear, in: shape)
                .overlay {
                    if selected, let stroke = style.segmentStroke {
                        shape.strokeBorder(stroke, lineWidth: 1)
                    }
                }
                .contentShape(hit)
        }
        .buttonStyle(.plain)
        .disabled(segment.disabled)
        .accessibilityAddTraits(selected ? [.isSelected] : [])
        .modifier(SegmentAccessibility(label: segment.accessibilityLabel, identifier: segment.identifier))
    }

    @ViewBuilder
    private func label(_ segment: Segment<Option>, selected: Bool) -> some View {
        if let content = segment.content {
            content
        } else {
            HStack(spacing: 6) {
                if let icon = segment.icon { icon }
                Text(segment.label)
                    .font(style.resolvedFont)
                    .lineLimit(1)
                    .minimumScaleFactor(style.minimumScaleFactor)
            }
            .foregroundStyle(segment.disabled ? style.disabledLabel : (selected ? style.selectedLabel : style.label))
        }
    }
}

private struct TrackHeight: ViewModifier {
    let height: CGFloat
    let minimum: Bool

    func body(content: Content) -> some View {
        if minimum {
            content.frame(minHeight: height)
        } else {
            content.frame(height: height)
        }
    }
}

private struct SegmentHeight: ViewModifier {
    let height: CGFloat
    let verticalPadding: CGFloat?

    func body(content: Content) -> some View {
        if let verticalPadding {
            content.padding(.vertical, verticalPadding)
        } else {
            content.frame(height: height)
        }
    }
}

private struct SegmentAccessibility: ViewModifier {
    let label: String?
    let identifier: String?

    func body(content: Content) -> some View {
        switch (label, identifier) {
        case let (label?, identifier?):
            content.accessibilityLabel(label).accessibilityIdentifier(identifier)
        case let (label?, nil):
            content.accessibilityLabel(label)
        case let (nil, identifier?):
            content.accessibilityIdentifier(identifier)
        case (nil, nil):
            content
        }
    }
}
