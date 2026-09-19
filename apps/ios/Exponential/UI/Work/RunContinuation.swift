import ExpCore
import Foundation
import SwiftUI

/// EXP-935: the Work screen's HOLD while a run is being continued — a Resume
/// from the nav bar, or an account switch from the usage sheet. Both END the
/// run they continue on purpose, and the successor row arrives a moment later
/// (a start is only a COMMAND), so the screen has to stay put in between and
/// swap the new run in place.
///
/// The screen owns it, both faces read it. It used to be a flag on the run
/// view bridged up through the `RunChrome` preference — which dies with the
/// view the moment the reader switches to the Issue face, taking the watch
/// with it and letting the ended edge pop the screen back to the list. The
/// state machine is `ContinuationHold` (ExpCore, tested); the watch is the
/// shared `StartedRunWatcher`, mounted for as long as the SCREEN is.
@MainActor @Observable
final class RunContinuation {

    enum Kind {
        case resume
        /// EXP-849: a resume naming another login profile.
        case accountSwitch
    }

    private(set) var hold = ContinuationHold()
    private(set) var kind: Kind = .resume

    /// The synced row this start produces. Owned here so no face switch can
    /// cancel the watch.
    let watcher = StartedRunWatcher()

    private var deadlineTask: Task<Void, Never>?

    /// The screen must not leave: a continuation is on its way.
    var isPending: Bool { hold.isPending }

    /// An account switch specifically — the usage sheet's spinner.
    var isSwitching: Bool { hold.isPending && kind == .accountSwitch }

    /// The command is on the wire.
    func sending(_ kind: Kind) {
        self.kind = kind
        hold.sending()
        watcher.sending()
        deadlineTask?.cancel()
        deadlineTask = nil
    }

    /// Delivered — watch for the successor, and stop holding if it never
    /// comes (the watcher's own deadline, so caption and hold end together).
    func sent(
        key: StartedRunKey,
        userId: String?,
        device: SteerDevice,
        db: DatabaseManager,
        accountId: String
    ) {
        hold.sent()
        watcher.begin(key: key, userId: userId, device: device, db: db, accountId: accountId)
        deadlineTask?.cancel()
        deadlineTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(ContinuationHold.deadline))
            guard let self, !Task.isCancelled else { return }
            self.hold.tick()
        }
    }

    /// The server refused the send.
    func failed(_ message: String) {
        hold.expire()
        watcher.failed(message)
        deadlineTask?.cancel()
        deadlineTask = nil
    }

    /// The successor synced in — the screen swaps it in place.
    func landed(_ sessionId: String) {
        hold.landed(sessionId)
        deadlineTask?.cancel()
        deadlineTask = nil
    }

    func stop() {
        watcher.stop()
        deadlineTask?.cancel()
        deadlineTask = nil
    }
}
