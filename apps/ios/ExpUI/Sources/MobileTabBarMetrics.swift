import SwiftUI

/// EXP-973: the phone tab bar's WIDTH budget. The split launcher (chat | new
/// issue) rides every bar-visible route now, not just a board — so the widest
/// bar the app can draw is six tabs beside the 104pt capsule, and that has to
/// fit the narrowest phone (375pt: SE / mini) with its screen insets intact.
///
/// The numbers live here rather than inside `MobileTabBar` so the fit is
/// TESTED (`MobileTabBarMetricsTests`) instead of eyeballed: the tab bar reads
/// every one of them.
public enum MobileTabBarMetrics {

    /// SE / mini — the narrowest screen the bar must fit.
    public static let smallestPhoneWidth: CGFloat = 375

    /// Six tabs and the capsule no longer fit at full width, so the crowded
    /// bar (helpdesk on) trims the tab SLOTS; the 44pt height — the HIG
    /// minimum on the axis a thumb misses — never moves.
    public static let tabHeight: CGFloat = 44
    public static let tabWidthRoomy: CGFloat = 44
    public static let tabWidthCompact: CGFloat = 40
    /// Between the tabs inside the pill.
    public static let tabSpacingRoomy: CGFloat = 2
    public static let tabSpacingCompact: CGFloat = 0
    /// The pill's own padding around its tabs.
    public static let pillPadding: CGFloat = 3
    /// Pill → launcher.
    public static let launcherGap: CGFloat = 8
    /// The bar's screen inset.
    public static let insetRoomy: CGFloat = 12
    public static let insetCompact: CGFloat = 8

    /// One launcher arm — the 52pt rung every floating slot wears.
    public static let launcherArm: CGFloat = FloatingBarTokens.slot
    /// The lone chat circle.
    public static var circleWidth: CGFloat { FloatingBarTokens.slot }
    /// Two arms split by a hairline.
    public static var capsuleWidth: CGFloat { 2 * launcherArm + GlassTokens.hairline }

    /// Six tabs (the Support entry is on) is the crowded bar.
    public static func isCompact(tabs: Int) -> Bool { tabs >= 6 }

    public static func tabWidth(tabs: Int) -> CGFloat {
        isCompact(tabs: tabs) ? tabWidthCompact : tabWidthRoomy
    }

    public static func tabSpacing(tabs: Int) -> CGFloat {
        isCompact(tabs: tabs) ? tabSpacingCompact : tabSpacingRoomy
    }

    public static func inset(tabs: Int) -> CGFloat {
        isCompact(tabs: tabs) ? insetCompact : insetRoomy
    }

    /// The pill holding the tabs.
    public static func pillWidth(tabs: Int) -> CGFloat {
        CGFloat(tabs) * tabWidth(tabs: tabs)
            + CGFloat(max(tabs - 1, 0)) * tabSpacing(tabs: tabs)
            + 2 * pillPadding
    }

    /// Everything the bar claims: both insets, the pill, the gap, the
    /// launcher.
    public static func barWidth(tabs: Int, launcher: CGFloat) -> CGFloat {
        2 * inset(tabs: tabs) + pillWidth(tabs: tabs) + launcherGap + launcher
    }
}
