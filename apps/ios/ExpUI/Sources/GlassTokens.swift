import SwiftUI
import UIKit

/// The ONE place mobile reads the glass tokens (EXP-698).
///
/// Every glass fill, stroke and radius on iOS resolves through here, and every
/// member below is a plain read of `DesignTokens` — generated from
/// `packages/design-tokens/tokens.json`, the same file Android, web and the
/// desktop generate from. A literal `Color.white.opacity(…)` or a bare `12`
/// corner in ExpUI is a drift bug: it is how iOS ended up with a hand-typed
/// `Zinc` palette and nine slightly different whites. (The app target still
/// carries a handful of literals, migrating surface by surface; the only one
/// left in ExpUI is the icon grid's deliberately brighter selection ring.)
///
/// The look is FLAT: no `.ultraThinMaterial` / `.thinMaterial` backdrop, no
/// drop shadows, no gradient palette — a low-alpha white fill over the app
/// background, plus a hairline.
public enum GlassTokens {

    // MARK: - Fills

    /// The dimmest rung — a section band inside an already-chromed surface.
    public static let fillSection: Color = DesignTokens.Glass.fillSection
    /// A list row, and the borderless group container (`.glassSection()`).
    public static let fillRow: Color = DesignTokens.Glass.fillRow
    /// A bordered panel, a pill, a circle button, a chip.
    public static let fillCard: Color = DesignTokens.Glass.fillCard
    /// The cutout main panel's wash over the page gradient (EXP-723; web +
    /// desktop paint it, phones stay full-bleed under the tab bar).
    public static let fillPanel: Color = DesignTokens.Glass.fillPanel
    /// Selected / pressed: the one bright fill.
    public static let fillActive: Color = DesignTokens.Glass.fillActive

    // MARK: - Strokes

    /// Row hairline — also every divider (`GlassDivider`).
    public static let strokeRow: Color = DesignTokens.Glass.strokeRow
    public static let strokeSection: Color = DesignTokens.Glass.strokeSection
    /// The bordered-panel / pill hairline.
    public static let strokeCard: Color = DesignTokens.Glass.strokeCard
    /// The brightest resting hairline (segmented strip, usage track, tab bar).
    public static let strokeStrong: Color = DesignTokens.Glass.strokeStrong
    /// Pairs with `fillActive`.
    public static let strokeActive: Color = DesignTokens.Glass.strokeActive

    /// Every glass stroke is a hairline — one physical pixel on a 2x screen.
    public static let hairline: CGFloat = 0.5

    // MARK: - Meters

    /// The FILLED share of a meter at its resting severity — the agent-usage
    /// rails (EXP-698). It has to out-read `strokeStrong`, the track it sits
    /// in, without becoming a semantic tone: a normal window is not a warning.
    /// Deliberately NOT one of the fill rungs above, which are surface tints
    /// and far too dim to read as a filled bar.
    public static let usageFill: Color = Color.white.opacity(0.30)

    // MARK: - Radii

    /// A gapped list item.
    public static let rowRadius: CGFloat = DesignTokens.Radius.md
    /// The borderless group container (web's `GlassGroup`).
    public static let groupRadius: CGFloat = DesignTokens.Radius.lg
    /// A bordered panel around free content.
    public static let cardRadius: CGFloat = DesignTokens.Radius.xl
    /// A metadata chip.
    public static let chipRadius: CGFloat = DesignTokens.Radius.sm
    /// A text input.
    public static let fieldRadius: CGFloat = DesignTokens.Radius.lg

    // MARK: - Geometry

    /// The ONE trailing-control size: every list-row and toolbar circle.
    public static let controlSize: CGFloat = DesignTokens.Size.controlMd

    // MARK: - Background

    public static let backgroundTop: Color = DesignTokens.Glass.backgroundTop
    public static let backgroundBottom: Color = DesignTokens.Glass.backgroundBottom

    /// `fillCard` composited over the opaque `card` surface — the fill for
    /// chrome that FLOATS over scrolling content (the tab-bar pill), where the
    /// low-alpha fill alone lets the content bleed through. DERIVED, never a
    /// literal, so it lands on the identical gray Android's `OpaqueCardFill`
    /// and `GlassMenuSurface`'s two-layer background composite to.
    public static let opaqueCardFill: Color = composite(fillCard, over: DesignTokens.Palette.card)

    /// Source-over composite of two opaque-background token colors.
    static func composite(_ top: Color, over base: Color) -> Color {
        let t = channels(of: top)
        let b = channels(of: base)
        return Color(
            red: b.red * (1 - t.alpha) + t.red * t.alpha,
            green: b.green * (1 - t.alpha) + t.green * t.alpha,
            blue: b.blue * (1 - t.alpha) + t.blue * t.alpha,
            opacity: b.alpha
        )
    }

    private static func channels(
        of color: Color
    ) -> (red: Double, green: Double, blue: Double, alpha: Double) {
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        UIColor(color).getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return (Double(red), Double(green), Double(blue), Double(alpha))
    }
}
