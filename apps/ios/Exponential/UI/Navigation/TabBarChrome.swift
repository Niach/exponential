import SwiftUI

/// Whether a screen below the navigator has claimed the tab bar's slot
/// (EXP-698 r5).
///
/// The bulk-selection bar is the one thing that does: on all four clients it is
/// now the SAME opaque card in the SAME place, and on mobile that place is
/// where the tab bar sits. Stacking the two would put a floating bar over a
/// floating bar; hiding the tab bar while a selection is live gives the
/// selection its own slot and makes leaving selection mode read as the tab bar
/// coming back.
///
/// A reference box in the environment rather than a binding threaded through
/// every screen: `IssueListView` renders at two depths (the Issues root and a
/// pushed `.board`) and both must reach the same flag.
@Observable
final class TabBarChrome {
    var suppressed = false

    init() {}
}
