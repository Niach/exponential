import SwiftUI

/// EXP-973: the phone tab bar's WIDTH budget. The New-issue circle rides
/// every bar-visible route, not just a board, and the Agent page is a tab now
/// (it was the launcher's chat arm) — so the widest bar the app can draw is
/// six tabs (Agent · Issues · Inbox · Devices · Reviews · Actions) beside the
/// 52pt circle, and that has to fit the narrowest phone (375pt: SE / mini)
/// with its screen insets intact.
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

    /// The New-issue circle — the 52pt rung every floating slot wears.
    public static var circleWidth: CGFloat { FloatingBarTokens.slot }

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
