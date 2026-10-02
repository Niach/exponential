import SwiftUI

/// EXP-893: what the session view REPORTS UP to the Work screen — the state
/// its nav bar and its face tabs draw from. The socket and
/// the feed stay the session view's; the chrome around them is the screen's,
/// so the two talk through one preference instead of a shared model.
struct RunChrome: Equatable {
    /// The socket is live (phase `.live`). EXP-1162: the title dot that read
    /// the socket phase is gone (the face tabs read the synced row); this
    /// stays so a live run's report never equals the default the screen
    /// discards (`chromeChanged`).
    var live = false
    /// The run is over as far as the screen can tell (`isOver`).
    var over = false
    /// A question or plan card is pending — the composer band is gone.
    var cardPending = false
    /// The latest worktree diff, when there is one. EXP-932: the +/− COUNTS
    /// are not reported here — the screen reads them off the run's retained
    /// model (`SteerSessionStore.peek`), which knows them on the faces this
    /// view is unmounted on too.
    var hasDiff = false
    /// An open PR this run can merge.
    var canMerge = false
    /// This viewer may Stop the run (own + live row).
    var canKill = false

    struct Key: PreferenceKey {
        static let defaultValue = RunChrome()
        /// EXP-1152: ONE view reports, but the pager keeps sibling pages
        /// alive beside it, and a sibling's DEFAULT must not land last and
        /// wipe the report — a real value wins in either order.
        static func reduce(value: inout RunChrome, nextValue: () -> RunChrome) {
            let next = nextValue()
            if next != defaultValue { value = next }
        }
    }
}

/// EXP-893: what the Work screen ASKS of the session view — the nav bar's
/// Stop pill is the screen's, the kill confirm and the model are the view's.
/// A one-shot TOKEN: every request carries its own id, so a repeated Stop
/// (the first one unconsumed, say, because no model was attached yet) is a
/// new value and fires the view's `onChange` again.
struct RunRequest: Equatable {
    enum Kind: Equatable {
        case stop
    }

    let id = UUID()
    let kind: Kind

    static var stop: RunRequest { RunRequest(kind: .stop) }
}
