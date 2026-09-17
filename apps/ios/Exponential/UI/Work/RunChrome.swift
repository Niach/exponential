import SwiftUI

/// EXP-893: what the session view REPORTS UP to the Work screen — the state
/// its nav bar, its title dot and its face switcher draw from. The socket and
/// the feed stay the session view's; the chrome around them is the screen's,
/// so the two talk through one preference instead of a shared model.
struct RunChrome: Equatable {
    /// The socket is live (phase `.live`).
    var live = false
    /// Dialling or re-dialling — pulse the title dot.
    var connecting = false
    /// The host machine is asleep, or the socket is gone for good.
    var paused = false
    /// A question or plan card waits on the steerer.
    var awaitingInput = false
    /// FEED-26: nothing has happened for ten minutes.
    var stale = false
    /// EXP-848: the agent is inside a turn — the dot pulses.
    var busy = false
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
    /// A resume or account switch is on the wire: the run ENDS on purpose and
    /// the screen must hold for the continuation instead of leaving.
    var continuationPending = false

    struct Key: PreferenceKey {
        static let defaultValue = RunChrome()
        static func reduce(value: inout RunChrome, nextValue: () -> RunChrome) {
            value = nextValue()
        }
    }
}

/// EXP-893: what the Work screen ASKS of the session view — the nav bar's
/// Stop pill is the screen's, the kill confirm and the model are the view's.
enum RunRequest: Equatable {
    case stop
}
