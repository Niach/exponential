import SwiftUI

/// EXP-973: the phone tab bar's WIDTH budget. The split launcher (chat | new
/// issue) rides every bar-visible route now, not just a board — so the widest
/// bar the app can draw is five tabs (Issues · My Work · Devices · Actions ·
/// Reviews; SLOP-4 retired the sixth, Support) beside the 104pt capsule, and
/// that has to fit the narrowest phone (375pt: SE / mini) with its screen
/// insets intact.
///
/// The numbers live here rather than inside `MobileTabBar` so the fit is
/// TESTED (`MobileTabBarMetricsTests`) instead of eyeballed: the tab bar reads
/// every one of them.
public enum MobileTabBarMetrics {

    /// SE / mini — the narrowest screen the bar must fit.
    public static let smallestPhoneWidth: CGFloat = 375

    /// The tab SLOT: 44pt on both axes, the HIG minimum.
    public static let tabHeight: CGFloat = 44
    public static let tabWidth: CGFloat = 44
    /// Between the tabs inside the pill.
    public static let tabSpacing: CGFloat = 2
    /// The pill's own padding around its tabs.
    public static let pillPadding: CGFloat = 3
    /// Pill → launcher.
    public static let launcherGap: CGFloat = 8
    /// The bar's screen inset.
    public static let inset: CGFloat = 12

    /// One launcher arm — the 52pt rung every floating slot wears.
    public static let launcherArm: CGFloat = FloatingBarTokens.slot
    /// The lone chat circle.
    public static var circleWidth: CGFloat { FloatingBarTokens.slot }
    /// Two arms split by a hairline.
    public static var capsuleWidth: CGFloat { 2 * launcherArm + GlassTokens.hairline }

    /// The pill holding the tabs.
    public static func pillWidth(tabs: Int) -> CGFloat {
        CGFloat(tabs) * tabWidth
            + CGFloat(max(tabs - 1, 0)) * tabSpacing
            + 2 * pillPadding
    }

    /// Everything the bar claims: both insets, the pill, the gap, the
    /// launcher.
    public static func barWidth(tabs: Int, launcher: CGFloat) -> CGFloat {
        2 * inset + pillWidth(tabs: tabs) + launcherGap + launcher
    }
}
