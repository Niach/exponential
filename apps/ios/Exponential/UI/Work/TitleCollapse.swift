import ExpCore
import SwiftUI

/// EXP-1162: the edges the nav-bar title collapse compares
/// (`DetailChrome.isTitleCollapsed`), in the global space — the title row's
/// bottom (`IssueTitleRowBottomKey`) and the header band's
/// (`workHeaderBand(onBottom:)`). A plain reference: the title row reports on
/// every scroll frame, and only the rule's FLIPPED answer may reach SwiftUI.
/// Shared by the Work screen and the New issue page (EXP-1170).
final class TitleCollapseTracker {
    var titleBottom: CGFloat?
    var headerBottom: CGFloat = 0

    /// The rule's answer for the edges as they stand.
    var collapsed: Bool {
        DetailChrome.isTitleCollapsed(
            hasTitleRow: true,
            titleBottom: titleBottom.map(Double.init),
            headerBottom: Double(headerBottom)
        )
    }
}
