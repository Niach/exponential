import SwiftUI

// EXP-893: the ONE floating bottom bar — `[left circle] [centre capsule]
// [right circle]` in 52pt glass chrome. It used to be drawn three times over
// (the issue detail bar's `circleChrome`, the tab bar's `fab`, the review
// page's `barCircle`/`barPill`), each with its own copy of the opaque fill,
// the `strokeStrong` hairline, the black-35% r16 y6 shadow and the 5pt inner
// padding. The phone's Work screen swaps the slots per face, so the chrome
// has to be one recipe or the bar would jump between faces.

public enum FloatingBarTokens {
    /// Every slot is this tall — the circles and the labelled pills alike
    /// (EXP-698), so the row's centre alignment lands them on one line.
    public static let slot: CGFloat = 52
    /// Between the slots.
    public static let spacing: CGFloat = 12
    /// The bar's screen inset — the tab bar's outer padding.
    public static let inset: CGFloat = 20
    public static let topPadding: CGFloat = 8
    public static let bottomPadding: CGFloat = 4
    /// The capsule's content band and the padding that lifts it to `slot`.
    public static let capsuleContentHeight: CGFloat = 42
    public static let capsuleInnerPadding: CGFloat = 5
    public static let capsuleHorizontalPadding: CGFloat = 14
    /// The solid pill's own horizontal padding — it carries a label, so it
    /// is wider than a circle.
    public static let solidPillHorizontalPadding: CGFloat = 28
    /// The soft lift off the scrolling content beneath.
    public static let shadowOpacity: Double = 0.35
    public static let shadowRadius: CGFloat = 16
    public static let shadowY: CGFloat = 6
    /// The badge dot at a circle's top-trailing corner — the tab bar's disc.
    public static let badgeSize: CGFloat = 8
    /// Where the badge sits, re-based on the 52pt square (the launcher's rule).
    public static let badgeOffset: CGFloat = 12
}

// MARK: - Chrome

extension View {
    /// The 52pt opaque glass circle every bar circle wears.
    public func floatingBarCircleChrome() -> some View {
        frame(width: FloatingBarTokens.slot, height: FloatingBarTokens.slot)
            .background(GlassTokens.opaqueCardFill, in: Circle())
            .overlay(
                Circle().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
            )
            .floatingBarShadow()
            .contentShape(Circle())
    }

    /// The shared lift.
    public func floatingBarShadow() -> some View {
        shadow(
            color: .black.opacity(FloatingBarTokens.shadowOpacity),
            radius: FloatingBarTokens.shadowRadius,
            y: FloatingBarTokens.shadowY
        )
    }
}

// MARK: - Slots

/// A bar circle: the glyph (or whatever the caller draws) in the shared
/// chrome, an optional badge at its top-trailing corner, and one action.
public struct FloatingBarCircle<Content: View, Badge: View>: View {
    let accessibilityLabel: String
    let enabled: Bool
    let action: () -> Void
    let content: Content
    let badge: Badge

    public init(
        accessibilityLabel: String,
        enabled: Bool = true,
        action: @escaping () -> Void,
        @ViewBuilder content: () -> Content,
        @ViewBuilder badge: () -> Badge
    ) {
        self.accessibilityLabel = accessibilityLabel
        self.enabled = enabled
        self.action = action
        self.content = content()
        self.badge = badge()
    }

    public var body: some View {
        Button(action: action) {
            content
                .floatingBarCircleChrome()
                .overlay(alignment: .topTrailing) {
                    badge.offset(
                        x: -FloatingBarTokens.badgeOffset, y: FloatingBarTokens.badgeOffset
                    )
                }
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(accessibilityLabel)
    }
}

extension FloatingBarCircle where Badge == EmptyView {
    public init(
        accessibilityLabel: String,
        enabled: Bool = true,
        action: @escaping () -> Void,
        @ViewBuilder content: () -> Content
    ) {
        self.init(
            accessibilityLabel: accessibilityLabel,
            enabled: enabled,
            action: action,
            content: content,
            badge: { EmptyView() }
        )
    }
}

/// The badge disc a circle wears at its corner — a session state, an
/// unread mark. Static: a pulsing variant is the caller's own.
public struct FloatingBarBadgeDot: View {
    let color: Color

    public init(color: Color) {
        self.color = color
    }

    public var body: some View {
        Circle()
            .fill(color)
            .frame(width: FloatingBarTokens.badgeSize, height: FloatingBarTokens.badgeSize)
    }
}

/// The centre capsule: a full-width glass pill whose content reads as the
/// prompt it expands into (`+ Comment`, `Type / for commands`).
public struct FloatingBarCapsule<Content: View>: View {
    let accessibilityLabel: String
    let action: () -> Void
    let content: Content

    public init(
        accessibilityLabel: String,
        action: @escaping () -> Void,
        @ViewBuilder content: () -> Content
    ) {
        self.accessibilityLabel = accessibilityLabel
        self.action = action
        self.content = content()
    }

    public var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                content
                Spacer(minLength: 0)
            }
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .padding(.horizontal, FloatingBarTokens.capsuleHorizontalPadding)
            .frame(height: FloatingBarTokens.capsuleContentHeight)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(FloatingBarTokens.capsuleInnerPadding)
            .background(GlassTokens.opaqueCardFill, in: Capsule())
            .overlay(
                Capsule().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
            )
            .floatingBarShadow()
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibilityLabel)
    }
}

/// The bar's labelled PRIMARY action (EXP-706): a SOLID white capsule with
/// dark content — the near-white primary the web review bar wears — at the
/// same height as the glass circles flanking it. No hairline: a
/// white-on-white stroke only muddies the edge; the shadow still lifts it.
public struct FloatingBarSolidPill<Content: View>: View {
    let accessibilityLabel: String
    let enabled: Bool
    let action: () -> Void
    let content: Content

    public init(
        accessibilityLabel: String,
        enabled: Bool = true,
        action: @escaping () -> Void,
        @ViewBuilder content: () -> Content
    ) {
        self.accessibilityLabel = accessibilityLabel
        self.enabled = enabled
        self.action = action
        self.content = content()
    }

    public var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                content
            }
            .foregroundStyle(.black.opacity(0.9))
            .padding(.horizontal, FloatingBarTokens.solidPillHorizontalPadding)
            .frame(height: FloatingBarTokens.slot)
            .background(Capsule().fill(.white))
            .floatingBarShadow()
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(accessibilityLabel)
    }
}

// MARK: - The bar

/// `[leading] [centre] [trailing]` on one centre line. Any slot may be empty:
/// the centre takes the width when it is there, and an empty centre leaves
/// the trailing circle at the right edge.
public struct FloatingBottomBar<Leading: View, Center: View, Trailing: View>: View {
    let leading: Leading
    let center: Center
    let trailing: Trailing

    public init(
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder center: () -> Center,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.leading = leading()
        self.center = center()
        self.trailing = trailing()
    }

    public var body: some View {
        HStack(alignment: .center, spacing: FloatingBarTokens.spacing) {
            leading
            center
                .frame(maxWidth: .infinity)
                .layoutPriority(1)
            Spacer(minLength: 0)
            trailing
        }
        .padding(.horizontal, FloatingBarTokens.inset)
        .padding(.top, FloatingBarTokens.topPadding)
        .padding(.bottom, FloatingBarTokens.bottomPadding)
    }
}
