import SwiftUI

/// The pinned geometry of a `GlassMetaRow`. It lives here rather than inline so
/// the New-issue page's rows, the Due-date picker that sits among them and the
/// issue Properties sheet all measure the same — a row that quietly grows two
/// points stops lining up with the divider above it.
public enum GlassMetaRowTokens {
    public static let horizontalPadding: CGFloat = 16
    public static let verticalPadding: CGFloat = 12
    /// The glyph leading the VALUE — smaller than the `.body` rung, because it
    /// reads with the `.subheadline` value beside it, not on its own.
    public static let glyphSize: CGFloat = 14
}

/// The ONE property row of a `.glassSection()` group (EXP-698 r5): a secondary
/// label on the left, and the value on the right led by its own tinted glyph.
/// The whole row is the Button — a tap anywhere opens the picker, which is why
/// there is no chevron to advertise it.
///
/// It started life as `CreateIssueView.metadataRow`; the issue Properties sheet
/// grew its own near-copy with a chevron and a leading gutter glyph, so mobile
/// showed two different property lists for the same five properties. This is
/// the shared one both now render, stacked with `GlassDivider()` between rows
/// (Android's `MetaRow`, dp for pt).
public struct GlassMetaRow: View {
    let label: String
    let icon: String
    let iconColor: Color
    let value: String
    let action: () -> Void

    public init(
        label: String,
        icon: String,
        iconColor: Color,
        value: String,
        action: @escaping () -> Void
    ) {
        self.label = label
        self.icon = icon
        self.iconColor = iconColor
        self.value = value
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            // 6pt between the glyph and its value, 16/12 paddings.
            HStack(spacing: 6) {
                Text(label)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                Spacer(minLength: 8)

                AppIcon(icon, size: GlassMetaRowTokens.glyphSize)
                    .foregroundStyle(iconColor)

                Text(value)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
            }
            .padding(.horizontal, GlassMetaRowTokens.horizontalPadding)
            .padding(.vertical, GlassMetaRowTokens.verticalPadding)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
