import SwiftUI

/// EXP-1191: the ONE scroll-to-bottom affordance ×4 — a 32pt round,
/// icon-only button (`ui-arrow-down`, 16pt, secondary emphasis) in the
/// floating chrome's opaque glass fill with a hairline edge and only a very
/// soft shadow. The host centres it 12pt (`JumpToBottomButton.bottomGap`)
/// above its bottom bar / composer and mounts it only while the reader is
/// scrolled away from the newest row; `.jumpToBottomTransition()` is the
/// shared fade + scale it enters and leaves with.
public struct JumpToBottomButton: View {
    /// The circle's diameter.
    public static let size: CGFloat = 32
    /// The glyph inside it.
    public static let glyph: CGFloat = 16
    /// Between the circle and the top edge of the bar below it.
    public static let bottomGap: CGFloat = 12

    let action: () -> Void

    public init(action: @escaping () -> Void) {
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            AppIcon(AppIcons.uiArrowDown, size: Self.glyph, weight: .medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: Self.size, height: Self.size)
                .background(GlassTokens.opaqueCardFill, in: Circle())
                .overlay(
                    Circle().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
                )
                .shadow(color: .black.opacity(0.18), radius: 6, y: 2)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Jump to bottom")
    }
}

extension View {
    /// The jump button's enter / exit: a fade with a slight scale, on the
    /// shared motion tokens (decelerate in, accelerate out; none under
    /// Reduce Motion).
    public func jumpToBottomTransition(_ motion: Motion) -> some View {
        transition(
            .asymmetric(
                insertion: .opacity.combined(with: .scale(scale: 0.85))
                    .animation(motion.decelerate()),
                removal: .opacity.combined(with: .scale(scale: 0.85))
                    .animation(motion.accelerate())
            )
        )
    }
}
