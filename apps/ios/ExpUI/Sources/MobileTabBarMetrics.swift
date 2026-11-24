import ExpCore
import SwiftUI

/// #EXP-2109 (chats.json `shell.dock` / `shell.newMenu`): the phone's docked
/// bar and its New popup.
public enum PhoneDockMetrics {
    public static let height = CGFloat(Chats.Shell.Dock.height)
    public static let icon = CGFloat(Chats.Shell.Dock.icon)
    public static let label = CGFloat(Chats.Shell.Dock.label)
    public static let gap = CGFloat(Chats.Shell.Dock.gap)
    /// The Assistant cell: its disc, the mark in it, how far it rises above
    /// the bar, the ring of the page background around it, the glow's blur.
    public static let disc = CGFloat(Chats.Shell.Dock.disc)
    public static let mark = CGFloat(Chats.Shell.Dock.mark)
    public static let raise = CGFloat(Chats.Shell.Dock.raise)
    public static let ring = CGFloat(Chats.Shell.Dock.ring)
    public static let glow = CGFloat(Chats.Shell.Dock.glow)
    /// What a page under the bar reserves above the safe area: the bar and
    /// the part of the disc's ring that rises above it.
    public static var clearance: CGFloat { height + raise + ring }

    public static let menuWidth = CGFloat(Chats.Shell.NewMenu.width)
    public static let menuRowHeight = CGFloat(Chats.Shell.NewMenu.rowHeight)
    public static let menuRadius = CGFloat(Chats.Shell.NewMenu.radius)
    public static let menuGap = CGFloat(Chats.Shell.NewMenu.gap)
}

/// chats.json `shell.bar`: a running app's own screens bar (the floating
/// glass capsule, `PhoneTabCapsule`). The capsule is `height` tall with
/// `padding` inside; each tab starts as wide as its label (label + 2 ×
/// `tabPaddingX`, at least `tabMinWidth`) and the tabs share the rest evenly;
/// a label truncates only when the labels alone do not fit, the widest giving
/// way first (`tabWidths`).
///
/// The numbers live here rather than inside the view so the distribution is
/// TESTED (`MobileTabBarMetricsTests`) instead of eyeballed.
public enum MobileTabBarMetrics {

    /// The capsule's height (`shell.bar.height`).
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

    /// The drag lens (`shell.bar.drag`).
    public static let lensScale = CGFloat(Chats.Shell.Bar.Drag.lensScale)
    public static let cancelDistance = CGFloat(Chats.Shell.Bar.Drag.cancelDistance)
    /// Movement that turns a press into a drag; below it the press is a tap.
    public static let dragSlop: CGFloat = 6

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
