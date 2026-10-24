import ExpCore
import SwiftUI

/// #EXP-1384 (chats.json `shell.bar`): the phone bottom strip = the tab
/// capsule (Board · Chats · Apps · Team, each an icon over its label) beside
/// the unchanged split launcher (chat | new issue). The capsule is
/// `height` tall with `padding` inside and TAKES THE WIDTH the screen leaves
/// beside the launcher (`gap` between them); each tab starts as wide as its
/// label (label + 2 × `tabPaddingX`, at least `tabMinWidth`) and the tabs
/// share the rest evenly; a label truncates only when the labels alone do
/// not fit, the widest giving way first (`tabWidths`).
///
/// The numbers live here rather than inside `MobileTabBar` so the
/// distribution is TESTED (`MobileTabBarMetricsTests`) instead of eyeballed.
public enum MobileTabBarMetrics {

    /// SE / mini — the narrowest screen the bar must fit.
    public static let smallestPhoneWidth: CGFloat = 375

    /// The capsule's height (`shell.bar.height`) — the launcher's arms match it.
    public static let barHeight = CGFloat(Chats.Shell.Bar.height)
    /// The capsule's own padding around its tabs (`shell.bar.padding`).
    public static let pillPadding = CGFloat(Chats.Shell.Bar.padding)
    /// A tab's height: the capsule less its padding (44, the HIG minimum).
    public static var tabHeight: CGFloat { barHeight - 2 * pillPadding }
    /// `shell.bar.tabMinWidth` / `tabPaddingX`.
    public static let tabMinWidth = CGFloat(Chats.Shell.Bar.tabMinWidth)
    public static let tabPaddingX = CGFloat(Chats.Shell.Bar.tabPaddingX)
    /// The tab label under its icon.
    public static let labelSize: CGFloat = 10
    /// Capsule → launcher (`shell.bar.gap`).
    public static let launcherGap = CGFloat(Chats.Shell.Bar.gap)
    /// The strip's screen inset.
    public static let inset: CGFloat = 16

    /// The Board tab's caret (`shell.boardTab`): `caretSize`, 2pt after the label.
    public static let caretSize = CGFloat(Chats.Shell.BoardTab.caretSize)
    public static let caretGap: CGFloat = 2

    /// The drag lens (`shell.bar.drag`).
    public static let lensScale = CGFloat(Chats.Shell.Bar.Drag.lensScale)
    public static let cancelDistance = CGFloat(Chats.Shell.Bar.Drag.cancelDistance)
    /// Movement that turns a press into a drag; below it the press is a tap.
    public static let dragSlop: CGFloat = 6

    /// One launcher arm — the 52pt rung every floating slot wears.
    public static let launcherArm: CGFloat = FloatingBarTokens.slot
    /// Two arms split by a hairline.
    public static var capsuleWidth: CGFloat { 2 * launcherArm + GlassTokens.hairline }

    /// The board menu (`shell.boardMenu`).
    public static let menuRowHeight = CGFloat(Chats.Shell.BoardMenu.rowHeight)
    public static let menuMaxVisibleRows = Chats.Shell.BoardMenu.maxVisibleRows
    public static let menuRadius = CGFloat(Chats.Shell.BoardMenu.radius)

    /// The board menu's open height: the rows (New board included) up to
    /// `maxVisibleRows`, plus the strip's padding above and below.
    public static func menuHeight(rows: Int) -> CGFloat {
        CGFloat(min(max(rows, 0), menuMaxVisibleRows)) * menuRowHeight + 2 * pillPadding
    }

    /// The width the tab capsule takes on a screen `screenWidth` wide.
    public static func capsuleTabsWidth(screenWidth: CGFloat, launcher: CGFloat = capsuleWidth) -> CGFloat {
        max(screenWidth - 2 * inset - launcherGap - launcher, 0)
    }

    /// A tab's natural width off its label's: label + 2 × `tabPaddingX`, at
    /// least `tabMinWidth`.
    public static func naturalTabWidth(label: CGFloat) -> CGFloat {
        max(label + 2 * tabPaddingX, tabMinWidth)
    }

    /// The tabs' widths inside `available` (the capsule less its padding):
    /// each starts at its `natural` width (already ≥ `tabMinWidth`) and the
    /// rest is shared evenly; when the naturals do not fit, the widest give
    /// way first (one cap for every tab over it), never below `tabMinWidth`.
    public static func tabWidths(natural: [CGFloat], available: CGFloat) -> [CGFloat] {
        guard !natural.isEmpty else { return [] }
        let widths = natural.map { max($0, tabMinWidth) }
        let count = CGFloat(widths.count)
        let total = widths.reduce(0, +)
        if total <= available {
            let extra = (available - total) / count
            return widths.map { $0 + extra }
        }
        let ascending = widths.sorted()
        var spent: CGFloat = 0
        var cap = ascending.last ?? tabMinWidth
        for (index, width) in ascending.enumerated() {
            let share = (available - spent) / CGFloat(ascending.count - index)
            if share <= width {
                cap = share
                break
            }
            spent += width
        }
        cap = max(cap, tabMinWidth)
        return widths.map { min($0, cap) }
    }
}
