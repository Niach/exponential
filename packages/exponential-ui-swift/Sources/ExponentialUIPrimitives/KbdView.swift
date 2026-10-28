import SwiftUI

/// S-3: the catalog `Kbd` geometry (web `KBD_CLASSES`): `$control.badge`, `$spacing.xs`.
public enum KbdMetrics {
    /// `h-5` / `min-w-5`.
    public static let height: CGFloat = 20
    /// `px-1`.
    public static let horizontalPadding: CGFloat = 4
    /// `gap-1` between the caps of a `KbdGroup`.
    public static let gap: CGFloat = 4
}

/// THE keycap outside a surface (the painter draws the `Kbd` macro inside
/// one): 20pt tall, at least 20pt wide, 4pt side padding, the `radius.sm`
/// corner, the muted fill, 12pt medium muted text.
public struct KbdView: View {
    @Environment(\.primitiveTokens) private var tokens
    let label: String

    public init(_ label: String) {
        self.label = label
    }

    public var body: some View {
        Text(label)
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(tokens.mutedForeground)
            .lineLimit(1)
            .padding(.horizontal, KbdMetrics.horizontalPadding)
            .frame(minWidth: KbdMetrics.height, minHeight: KbdMetrics.height, maxHeight: KbdMetrics.height)
            .background(tokens.muted, in: RoundedRectangle(cornerRadius: tokens.radiusSm, style: .continuous))
    }
}

/// `KbdGroup`: the caps of a chord in a row, 4pt apart.
public struct KbdGroup<Content: View>: View {
    let content: Content

    public init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    public var body: some View {
        HStack(spacing: KbdMetrics.gap) { content }
    }
}
