import SwiftUI

// The bordered card of the glass vocabulary (GlassTheme.swift), in its own
// file so the share extension's toast (#EXP-1845) draws the same card.

/// A BORDERED panel around free content — the outermost chrome on a page that
/// is not already inside a group. Radius `xl`, `opaqueCardFill`, `strokeCard`.
/// N-12 (owner 2026-10-22): ONE gray ×4 — the fill is ALWAYS the opaque card
/// composite (web `bg-glass-card-opaque`), never the translucent wash, so a
/// card reads the same tone wherever it sits.
public struct GlassCard: ViewModifier {
    public var cornerRadius: CGFloat = GlassTokens.cardRadius
    /// Kept for the call sites; N-12 made every card opaque, so it no longer
    /// changes the look.
    public var isOpaque: Bool = false

    public init(cornerRadius: CGFloat = GlassTokens.cardRadius, isOpaque: Bool = false) {
        self.cornerRadius = cornerRadius
        self.isOpaque = isOpaque
    }

    public func body(content: Content) -> some View {
        content
            .background(GlassTokens.opaqueCardFill)
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius)
                    .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
            )
    }
}

extension View {
    public func glassCard(
        cornerRadius: CGFloat = GlassTokens.cardRadius,
        isOpaque: Bool = false
    ) -> some View {
        modifier(GlassCard(cornerRadius: cornerRadius, isOpaque: isOpaque))
    }
}
