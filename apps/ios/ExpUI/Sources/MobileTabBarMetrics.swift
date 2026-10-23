import SwiftUI

/// Thread 58cb32df (chats.json `shell`): FOUR labelled tabs now — Board ·
/// Devices · Chats · Team, each an icon over its label in a 56×52 slot on
/// the glass capsule — beside the unchanged split launcher, whose arms
/// stretch to the capsule's height so the two sit on one line.
///
/// EXP-973: the phone tab bar's WIDTH budget. The split launcher (chat | new
/// issue) rides every bar-visible route now, not just a board — so the widest
/// bar the app can draw is five tabs (Issues · Inbox · Devices · Threads ·
/// Actions; SLOP-4 retired the sixth, Support) beside the 104pt capsule, and
/// that has to fit the narrowest phone (375pt: SE / mini) with its screen
/// insets intact.
///
/// The numbers live here rather than inside `MobileTabBar` so the fit is
/// TESTED (`MobileTabBarMetricsTests`) instead of eyeballed: the tab bar reads
/// every one of them.
public enum MobileTabBarMetrics {

    /// SE / mini — the narrowest screen the bar must fit.
    public static let smallestPhoneWidth: CGFloat = 375

    /// The tab SLOT: icon over label, past the 44pt HIG minimum both ways.
    public static let tabHeight: CGFloat = 52
    public static let tabWidth: CGFloat = 56
    /// Between the tabs inside the pill (the selected pill fills a slot).
    public static let tabSpacing: CGFloat = 0
    /// The pill's own padding around its tabs.
    public static let pillPadding: CGFloat = 4
    /// The capsule's height — the launcher's arms match it.
    public static var barHeight: CGFloat { tabHeight + 2 * pillPadding }
    /// The tab label under its icon.
    public static let labelSize: CGFloat = 11
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
