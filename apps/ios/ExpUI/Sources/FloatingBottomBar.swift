import SwiftUI

// EXP-893: the ONE floating bottom bar — `[left circle] [centre capsule]
// [right circle]` in 52pt glass chrome. It used to be drawn three times over
// (the issue detail bar's `circleChrome`, the tab bar's `fab`, the review
// page's `barCircle`/`barPill`), each with its own copy of the opaque fill,
// the `strokeStrong` hairline and the 5pt inner padding. The phone's Work
// screen swaps the slots per face, so the chrome has to be one recipe or the
// bar would jump between faces.
//
// EXP-916: the geometry is ANDROID's `FloatingBottomBar` (the reference the
// three phones are locked to): a 20pt inset, 10pt between the slots, 52pt
// circles with 20pt white glyphs, a capsule padded 18pt whose glyph and label
// sit 8pt apart, and NO drop shadow — the bar sits flat on the content with
// only its hairline for an edge. `FloatingBarCluster` is every Changes
// bar's layout (Reviews page and Work screen alike): nothing stretches, the
// circles and the white Merge pill hug their content 12pt apart, centred.

public enum FloatingBarTokens {
    /// Every slot is this tall — the circles and the labelled pills alike
    /// (EXP-698), so the row's centre alignment lands them on one line.
    public static let slot: CGFloat = 52
    /// Between the slots (Android's 10dp).
    public static let spacing: CGFloat = 10
    /// Between the slots of a CLUSTER (Android's review bar, 12dp).
    public static let clusterSpacing: CGFloat = 12
    /// The bar's screen inset — the tab bar's outer padding.
    public static let inset: CGFloat = 20
    public static let topPadding: CGFloat = 8
    public static let bottomPadding: CGFloat = 4
    /// The capsule's content band and the padding that lifts it to `slot`.
    public static let capsuleContentHeight: CGFloat = 42
    public static let capsuleInnerPadding: CGFloat = 5
    /// The capsule's label inset (Android's 18dp) — 13 here because the
    /// 5pt inner padding above adds the rest.
    public static let capsuleHorizontalPadding: CGFloat = 13
    /// Glyph → label inside a capsule or pill.
    public static let capsuleGap: CGFloat = 8
    /// The solid pill's own horizontal padding — it carries a label, so it
    /// is wider than a circle.
    public static let solidPillHorizontalPadding: CGFloat = 28
    /// A bar circle's glyph (Android's 20dp) — `AppIcon.Size.large`.
    public static let glyph: CGFloat = 20
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
            .contentShape(Circle())
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
/// prompt it expands into (`+ Comment`, `Type / for commands`). A VERB never
/// rides it — Merge is the solid pill below, in a cluster.
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
            HStack(spacing: FloatingBarTokens.capsuleGap) {
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
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibilityLabel)
    }
}

/// The bar's labelled PRIMARY action (EXP-706): a SOLID white capsule with
/// dark content — the near-white primary the web review bar wears — at the
/// same height as the glass circles flanking it. No hairline: a
/// white-on-white stroke only muddies the edge.
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
            HStack(spacing: FloatingBarTokens.capsuleGap) {
                content
            }
            .foregroundStyle(.black.opacity(0.9))
            .padding(.horizontal, FloatingBarTokens.solidPillHorizontalPadding)
            .frame(height: FloatingBarTokens.slot)
            .background(Capsule().fill(.white))
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

/// EXP-916: the CHANGES bar — `[leading] [centre] [trailing]` as a CENTRED
/// cluster whose slots hug their content, 12pt apart (Android's
/// `FloatingBarCluster`): files · Merge PR · reject on the Reviews page,
/// files · Merge PR · switcher on the Work screen's Changes face. Nothing
/// stretches: the white Merge pill is as wide as its label, and a missing
/// slot leaves no gap.
public struct FloatingBarCluster<Leading: View, Center: View, Trailing: View>: View {
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
        HStack(alignment: .center, spacing: FloatingBarTokens.clusterSpacing) {
            leading
            center
            trailing
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, FloatingBarTokens.inset)
        .padding(.top, FloatingBarTokens.topPadding)
        .padding(.bottom, FloatingBarTokens.bottomPadding)
    }
}
