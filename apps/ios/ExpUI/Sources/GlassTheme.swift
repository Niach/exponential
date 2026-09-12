import SwiftUI

// The glass vocabulary (EXP-698). Every fill, stroke and radius below is a
// `GlassTokens` read — no materials, no shadows, no hand-typed palette. The
// hand-written `Zinc` ramp that used to live here is GONE: it was a second,
// slightly-off copy of `DesignTokens.Palette`.

// MARK: - Glass Modifiers

/// A BORDERED panel around free content — the outermost chrome on a page that
/// is not already inside a group. Radius `xl`, `fillCard`, `strokeCard`.
public struct GlassCard: ViewModifier {
    public var cornerRadius: CGFloat = GlassTokens.cardRadius
    /// Lays the opaque card surface beneath the glass tint — for cards that
    /// FLOAT over scrolling content (the bulk-selection bar, the start
    /// notices), where the low-alpha fill alone lets the feed bleed through.
    /// The same switch `GlassButton` carries.
    public var isOpaque: Bool = false

    public init(cornerRadius: CGFloat = GlassTokens.cardRadius, isOpaque: Bool = false) {
        self.cornerRadius = cornerRadius
        self.isOpaque = isOpaque
    }

    public func body(content: Content) -> some View {
        content
            .background(GlassTokens.fillCard)
            .background(isOpaque ? DesignTokens.Palette.card : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius)
                    .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
            )
    }
}

/// A gapped list item — radius `md`, `fillRow` + `strokeRow`.
public struct GlassRow: ViewModifier {
    /// Brighter fill + stroke for a selected/primary row (Android
    /// `glassRow(active:)` parity — EXP-274 moved question options onto rows,
    /// whose fixed radius survives multi-line content where the capsule
    /// button clipped it).
    public var isActive: Bool = false
    /// Lays the opaque card surface beneath the glass tint — for a row that
    /// FLOATS over scrolling content (the steer screen's Latest-changes chip),
    /// where the low-alpha fill alone lets the feed bleed through. The same
    /// switch `GlassCard` and `GlassButton` carry (Android `glassRow(opaque:)`
    /// parity, EXP-743).
    public var isOpaque: Bool = false

    public init(isActive: Bool = false, isOpaque: Bool = false) {
        self.isActive = isActive
        self.isOpaque = isOpaque
    }

    public func body(content: Content) -> some View {
        content
            .background(isActive ? GlassTokens.fillActive : GlassTokens.fillRow)
            .background(isOpaque ? DesignTokens.Palette.card : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
            .overlay(
                RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                    .stroke(
                        isActive ? GlassTokens.strokeActive : GlassTokens.strokeRow,
                        lineWidth: GlassTokens.hairline
                    )
            )
    }
}

/// EXP-818: ONE FLAT list row — the row every LIST wears now. No stroke and no
/// fill of its own: rows stack with NO gap under a `GlassSectionBand` and read
/// as a table instead of as a pile of cards, and the only paint is the active
/// row's `fillActive` (a phone has no hover to wash). `GlassRow` stays for the
/// few real CARDS — a transcript's tool output, a diff, a settings section.
///
/// Web `ListRow` (`components/ui/glass-rows.tsx`) / desktop
/// `surface::flat_row` twin; padding, gap and the tap are the caller's, exactly
/// as on the other two.
public struct FlatRow: ViewModifier {
    /// The selected row (a master-detail's open item).
    public var isActive: Bool = false

    public init(isActive: Bool = false) {
        self.isActive = isActive
    }

    public func body(content: Content) -> some View {
        content
            .background(isActive ? GlassTokens.fillActive : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
    }
}

/// A capsule pill — `fillCard` + `strokeCard` at rest, `fillActive` +
/// `strokeActive` active.
public struct GlassButton: ViewModifier {
    public var isActive: Bool = false
    /// Lays the opaque card surface beneath the glass tint — for pills
    /// floating over scrolling content, where the low-alpha fill alone lets
    /// the content bleed through (Android glassButton `opaque` parity,
    /// EXP-165).
    public var isOpaque: Bool = false

    public init(isActive: Bool = false, isOpaque: Bool = false) {
        self.isActive = isActive
        self.isOpaque = isOpaque
    }

    public func body(content: Content) -> some View {
        content
            .background(isActive ? GlassTokens.fillActive : GlassTokens.fillCard)
            .background(isOpaque ? DesignTokens.Palette.card : Color.clear)
            .clipShape(Capsule())
            .overlay(
                Capsule()
                    .stroke(
                        isActive ? GlassTokens.strokeActive : GlassTokens.strokeCard,
                        lineWidth: GlassTokens.hairline
                    )
            )
    }
}

/// The BORDERLESS group container — the iOS twin of web's `GlassGroup`
/// (EXP-698): radius `lg`, `fillRow`, and NO outer stroke. Rows inside it are
/// separated by `GlassDivider` hairlines, never by their own borders. A
/// bordered panel is `GlassCard`.
public struct GlassSection: ViewModifier {
    public init() {}

    public func body(content: Content) -> some View {
        content
            .background(GlassTokens.fillRow)
            .clipShape(RoundedRectangle(cornerRadius: GlassTokens.groupRadius))
    }
}

// MARK: - Divider

/// The ONE hairline rule between rows of a group — `strokeRow` at the same
/// physical thickness as every glass stroke.
public struct GlassDivider: View {
    public init() {}

    public var body: some View {
        Rectangle()
            .fill(GlassTokens.strokeRow)
            .frame(height: GlassTokens.hairline)
    }
}

// MARK: - Section header

/// The ONE section header on iOS (EXP-698): sentence-case title, an optional
/// count, and a trailing slot pushed to the right edge. It replaces the five
/// hand-rolled `sectionHeader` / `sectionLabel` / `sectionStack` idioms that
/// each picked their own font, opacity and padding, and it is what the `Form`
/// sections hand to `header:` instead of the uppercased system label.
///
/// Deliberate exception: the emoji picker's category headers stay uppercase —
/// they are a shared cross-client convention, not this app's section language.
public struct GlassSectionHeader<Trailing: View>: View {
    let title: String
    let trailing: Trailing

    public init(_ title: String, @ViewBuilder trailing: () -> Trailing) {
        self.title = title
        self.trailing = trailing()
    }

    public var body: some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer(minLength: 0)
            trailing
        }
        .textCase(nil)
        .padding(.horizontal, 4)
        .padding(.top, 4)
        .padding(.bottom, 8)
    }
}

extension GlassSectionHeader where Trailing == EmptyView {
    public init(_ title: String) {
        self.init(title) { EmptyView() }
    }
}

/// EXP-818: the GROUP BAND — the Linear group header, and what every LIST
/// section wears now. A full-width strip filled `fillSection` carrying the
/// group's name (an optional leading glyph, an optional trailing control),
/// sitting 4pt over its flat rows (`.flatRow()`): the rows read as a table
/// under a highlighted header rather than as a stack of cards.
///
/// Web `GlassSectionHeader` / desktop `surface::glass_section_band` twin —
/// same fill, radius, padding and 85% label. `GlassSectionHeader` above stays
/// the PLAIN-TEXT heading for free content (a settings section's title over a
/// card); a list takes this one. Labels are SENTENCE CASE, never uppercase,
/// and there is no count slot.
///
/// The trailing 4pt IS the gap to the rows below, so a host stacks band + rows
/// in a `VStack(spacing: 0)`.
public struct GlassSectionBand<Leading: View, Trailing: View>: View {
    let title: String
    let leading: Leading
    let trailing: Trailing

    public init(
        _ title: String,
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.title = title
        self.leading = leading()
        self.trailing = trailing()
    }

    public var body: some View {
        HStack(spacing: 6) {
            leading
            Text(title)
                .font(.subheadline.weight(.medium))
                // 85% is the band's own rung — the shared one web
                // (`text-foreground/85`) and desktop (`foreground.opacity(0.85)`)
                // paint their band label with, brighter than a plain header's
                // `TextOpacity.secondary` because the fill sits behind it.
                .foregroundStyle(.white.opacity(0.85))
                .lineLimit(1)
            Spacer(minLength: 0)
            trailing
        }
        .textCase(nil)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GlassTokens.fillSection)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
        .padding(.bottom, 4)
    }
}

extension GlassSectionBand where Leading == EmptyView, Trailing == EmptyView {
    public init(_ title: String) {
        self.init(title) { EmptyView() } trailing: { EmptyView() }
    }
}

extension GlassSectionBand where Leading == EmptyView {
    public init(_ title: String, @ViewBuilder trailing: () -> Trailing) {
        self.init(title) { EmptyView() } trailing: trailing
    }
}

// MARK: - Background

/// The app ground: the shared `glass.background*` pair, top to bottom.
public struct AppBackground: View {
    public init() {}

    public var body: some View {
        LinearGradient(
            colors: [GlassTokens.backgroundTop, GlassTokens.backgroundBottom],
            startPoint: .top,
            endPoint: .bottom
        )
        .ignoresSafeArea()
    }
}

/// EXP-698: the 24pt wash a scrolling surface wears directly under a sticky
/// translucent header. Without it a line of body text that happens to sit at
/// the header's edge is SLICED through its letterforms — half the glyph reads
/// crisp, half reads behind the material — which is what made the steering
/// feed look broken at the top of every scroll. The gradient starts at the
/// page's own background colour so the fade reads as the page receding, not
/// as a grey band.
public struct StickyHeaderFade: View {
    public static let height: CGFloat = 24

    public init() {}

    public var body: some View {
        LinearGradient(
            colors: [
                GlassTokens.backgroundTop,
                GlassTokens.backgroundTop.opacity(0),
            ],
            startPoint: .top,
            endPoint: .bottom
        )
        .frame(height: Self.height)
        .allowsHitTesting(false)
    }
}

// MARK: - View Extensions

extension View {
    /// Overlays `StickyHeaderFade` at the top edge — for a scroller that runs
    /// under a translucent nav bar.
    public func stickyHeaderFade() -> some View {
        overlay(alignment: .top) { StickyHeaderFade() }
    }

    public func glassCard(
        cornerRadius: CGFloat = GlassTokens.cardRadius,
        isOpaque: Bool = false
    ) -> some View {
        modifier(GlassCard(cornerRadius: cornerRadius, isOpaque: isOpaque))
    }

    public func glassRow(isActive: Bool = false, isOpaque: Bool = false) -> some View {
        modifier(GlassRow(isActive: isActive, isOpaque: isOpaque))
    }

    /// EXP-818: the flat LIST row (`FlatRow`) — no stroke, no fill, the rows
    /// under a `GlassSectionBand`.
    public func flatRow(isActive: Bool = false) -> some View {
        modifier(FlatRow(isActive: isActive))
    }

    public func glassButton(isActive: Bool = false, isOpaque: Bool = false) -> some View {
        modifier(GlassButton(isActive: isActive, isOpaque: isOpaque))
    }

    public func glassSection() -> some View {
        modifier(GlassSection())
    }

    public func appBackground() -> some View {
        background { AppBackground() }
    }
}

// MARK: - Text Styles

public enum TextOpacity {
    public static let primary: Double = 1.0
    public static let secondary: Double = 0.7
    public static let tertiary: Double = 0.5
    public static let quaternary: Double = 0.3
}

// EXP-594: the indigo `Accent` enum is retired — the main scheme is white/
// glass. Solid accent fills that carry text use `DesignTokens.Palette.primary`
// (+ `primaryForeground` text), selections use the glass fills/strokes, and
// icon/dot accents are plain white.

// MARK: - Status Colors

// Semantic status/priority colors come from the shared design tokens
// (packages/design-tokens) so all clients stay in lockstep — as, since
// EXP-698, does every glass fill/stroke/radius (`GlassTokens`).
public enum StatusColor {
    public static let backlog = DesignTokens.Semantic.neutral
    public static let inProgress = DesignTokens.Semantic.yellow
    /// Up for review (EXP-120) — green, the color `done` used to carry.
    public static let inReview = DesignTokens.Semantic.green
    /// Completed (EXP-120): moved from green to blue when in_review took green.
    public static let done = DesignTokens.Semantic.blue
    /// Cancelled is a muted terminal RESOLUTION, not an error (REV2-85): the
    /// desktop/web treatment (`text-muted-foreground`) is the shared one, and
    /// red here made the same issue read as a failure on a phone and as a
    /// closed one on the desk.
    public static let cancelled = DesignTokens.Semantic.neutral
    /// Duplicate is a muted terminal resolution (like backlog's neutral gray).
    public static let duplicate = DesignTokens.Semantic.neutral
}

// MARK: - Priority Colors

public enum PriorityColor {
    public static let none = DesignTokens.Semantic.neutral
    public static let low = DesignTokens.Semantic.blue
    public static let medium = DesignTokens.Semantic.yellow
    public static let high = DesignTokens.Semantic.orange
    public static let urgent = DesignTokens.Semantic.red
}
