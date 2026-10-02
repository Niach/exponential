import Foundation

/// EXP-1162 — the Work screen's DETAIL CHROME: the nav-bar title collapse and
/// the scrim strips at the top and bottom edges.
///
/// ONE pure rule, mirrored ×4 (web `lib/detail-chrome.ts`, desktop
/// `domain::detail_chrome`, Android `domain/DetailChrome.kt`) and locked by
/// the contract fixture `domain-contract/fixtures/detail-chrome.json` — same
/// cases, same constants. Points here = px on web/desktop = dp on Android.
public enum DetailChrome {
    /// The collapsed title fades in over this long…
    public static let collapseMs: Double = 160
    /// …while rising this far.
    public static let collapseRise: Double = 4
    /// The strip under the header band that fades scrim + blur to nothing.
    public static let edgeTop: Double = 24
    /// How far above the floating bar's top the bottom strip reaches.
    public static let edgeBottom: Double = 32
    /// The blur behind the scrim.
    public static let edgeBlur: Double = 8
    /// The page background's alpha over that blur.
    public static let scrim: Double = 0.72

    /// Whether the header shows the COLLAPSED title (identifier over the
    /// title). A face with no title row of its own is always collapsed; a
    /// title row not measured yet stays expanded; otherwise it BREAKS the
    /// moment the row's bottom edge meets the header band's (same coordinate
    /// space) — a threshold, never a progressive morph.
    public static func isTitleCollapsed(
        hasTitleRow: Bool, titleBottom: Double?, headerBottom: Double
    ) -> Bool {
        guard hasTitleRow else { return true }
        guard let titleBottom else { return false }
        return titleBottom <= headerBottom
    }
}
