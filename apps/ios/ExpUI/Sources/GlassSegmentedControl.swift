import SwiftUI

/// The chrome constants of the segmented strip, pinned so a drift breaks a
/// build instead of shipping two different-looking strips. Android's mirror is
/// `app/src/test/java/com/exponential/app/ui/components/GlassSegmentedControlDefaultsTest.kt`.
public enum GlassSegmentedControlTokens {
    /// The capsule container's own fill — the dimmest rung, so the SELECTED
    /// segment's `fillActive` is what the eye lands on (EXP-698 round 2).
    public static let containerFill: Color = GlassTokens.fillSection
    /// The capsule container's hairline — the section rung that pairs with it.
    public static let stroke: Color = GlassTokens.strokeSection
    public static let hairline: CGFloat = GlassTokens.hairline
    /// The selected segment's fill — the one bright glass fill.
    public static let activeFill: Color = GlassTokens.fillActive
    /// Inset between the container capsule and the segments.
    public static let capsulePadding: CGFloat = 3
    /// Gap between two segments — none: the segments tile the strip, and the
    /// active capsule is what separates them.
    public static let segmentSpacing: CGFloat = 0
    /// A segment's own vertical padding.
    public static let segmentVerticalPadding: CGFloat = 6
    /// The standalone strip's height — the large control rung, so it lines up
    /// with every other full-width control on the page. A MINIMUM, not a fixed
    /// frame: at larger Dynamic Type the labels have to grow the strip rather
    /// than be squeezed inside it. The `.embedded` style has no height of its
    /// own: the row's insets set it.
    public static let height: CGFloat = DesignTokens.Size.controlLg
}

/// Full-width glass-pill segmented control — the My Work Inbox/My Issues tab
/// language (EXP-192): one flat capsule container holding equal-width
/// segments, the active one filled `fillActive`. Optional per-segment count
/// badge (the text-bearing `primary` fill, not the raw accent).
///
/// EXP-698 moved it out of the app target into ExpUI (the app's `Exponential/**`
/// glob no longer sees it, ExpUI's `ExpUI/Sources/**` does) and off its
/// hand-typed material/white literals onto `GlassSegmentedControlTokens`.
public struct GlassSegmentedControl<Option: Hashable>: View {
    /// EXP-694 (S3): where the strip sits.
    /// `.capsule` is the free-standing control — its own material capsule and
    /// hairline. `.embedded` is the strip as the FIRST ROW of a grouped card:
    /// no fill, no border, no container padding of its own (the row's insets
    /// carry the 8pt), so the card behind it is the only surface. The segments
    /// themselves are identical in both.
    public enum Style {
        case capsule
        case embedded
    }

    let options: [Option]
    let selection: Option
    let label: (Option) -> String
    /// EXP-615: optional 14pt leading mark per segment — the agent strip's
    /// brand icons (`Image("agent-claude")`), which are asset images rather
    /// than registry glyphs. nil renders a label-only segment exactly as
    /// before.
    let icon: (Option) -> Image?
    /// EXP-642: optional per-segment accessibility identifier, so a UI test can
    /// address ONE segment (the Start-coding sheet's Issues/Actions/Chat tabs)
    /// instead of guessing at a label that also matches other controls. nil
    /// leaves the segment identifier-less, exactly as before.
    let identifier: (Option) -> String?
    let badge: (Option) -> Int
    let style: Style
    let onSelect: (Option) -> Void

    public init(
        options: [Option],
        selection: Option,
        label: @escaping (Option) -> String,
        identifier: @escaping (Option) -> String? = { _ in nil },
        badge: @escaping (Option) -> Int = { _ in 0 },
        style: Style = .capsule,
        onSelect: @escaping (Option) -> Void
    ) {
        self.init(
            options: options,
            selection: selection,
            label: label,
            icon: { _ in nil },
            identifier: identifier,
            badge: badge,
            style: style,
            onSelect: onSelect
        )
    }

    /// The icon-bearing variant (EXP-615): same geometry, a leading mark on
    /// the segments that have one.
    public init(
        options: [Option],
        selection: Option,
        label: @escaping (Option) -> String,
        icon: @escaping (Option) -> Image?,
        identifier: @escaping (Option) -> String? = { _ in nil },
        badge: @escaping (Option) -> Int = { _ in 0 },
        style: Style = .capsule,
        onSelect: @escaping (Option) -> Void
    ) {
        self.options = options
        self.selection = selection
        self.label = label
        self.icon = icon
        self.identifier = identifier
        self.badge = badge
        self.style = style
        self.onSelect = onSelect
    }

    @ViewBuilder
    public var body: some View {
        switch style {
        case .capsule:
            segments
                .padding(GlassSegmentedControlTokens.capsulePadding)
                .frame(minHeight: GlassSegmentedControlTokens.height)
                .background(GlassSegmentedControlTokens.containerFill, in: Capsule())
                .overlay(
                    Capsule().stroke(
                        GlassSegmentedControlTokens.stroke,
                        lineWidth: GlassSegmentedControlTokens.hairline
                    )
                )
        case .embedded:
            segments
        }
    }

    private var segments: some View {
        HStack(spacing: GlassSegmentedControlTokens.segmentSpacing) {
            ForEach(options, id: \.self) { option in
                segmentButton(option)
            }
        }
    }

    private func segmentButton(_ option: Option) -> some View {
        let active = option == selection
        return Button {
            onSelect(option)
        } label: {
            HStack(spacing: 6) {
                if let mark = icon(option) {
                    mark
                        .resizable()
                        .scaledToFit()
                        .frame(width: 14, height: 14)
                }
                Text(label(option))
                    // EXP-698: the weight is CONSTANT — only the opacity moves.
                    // A semibold/regular swap re-flowed the strip on every tap
                    // and made two adjacent segments look like two type scales.
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                let count = badge(option)
                if count > 0 {
                    Text("\(count)")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(DesignTokens.Palette.primaryForeground)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(DesignTokens.Palette.primary, in: Capsule())
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, GlassSegmentedControlTokens.segmentVerticalPadding)
            .background(
                active ? GlassSegmentedControlTokens.activeFill : .clear,
                in: Capsule()
            )
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label(option))
        .accessibilityIdentifier(identifier(option) ?? "")
    }
}
