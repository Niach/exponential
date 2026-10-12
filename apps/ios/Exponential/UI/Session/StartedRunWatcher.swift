import ExpCore
import Foundation
import GRDB

/// EXP-536: every surface that remote-starts a run — a single issue, a batch,
/// an action — pushes the live session screen once the desktop picks it up,
/// instead of parking a notice pointing at the Agents tab. A start is only a
/// COMMAND, so this holds an informational caption while the synced
/// `coding_sessions` row is on its way, then hands the row's id to the view's
/// navigation exactly once. `StartedRunMatch` (ExpCore, mirrored on Android
/// and web) owns which row IS this start.
///
/// One object per surface, driven from `@State`; `begin` takes its
/// dependencies so a view can own it without an init dance.
@MainActor @Observable
final class StartedRunWatcher {

    /// The consumed-once navigation target (the ActionsListView idiom).
    struct StartedSession: Hashable, Identifiable {
        let sessionId: String
        var id: String { sessionId }
    }

    /// Informational "waiting for the desktop" caption — nil once the run
    /// arrives or the deadline passes.
    private(set) var sentCaption: String?

    /// A refused delivery, or a start the desktop never picked up. Persists
    /// until the next attempt so it can't be missed.
    private(set) var failure: String?

    /// Set exactly once per start; the view clears it as it pushes.
    var startedSession: StartedSession?

    private var watchTask: Task<Void, Never>?
    private var deadlineTask: Task<Void, Never>?

    /// A fresh attempt supersedes the previous outcome (success or error).
    func sending() {
        stop()
        sentCaption = nil
        failure = nil
    }

    /// The server refused the send — the caption must not linger.
    func failed(_ message: String) {
        stop()
        sentCaption = nil
        failure = message
    }

    /// Delivered: caption up, then watch the synced rows for the desktop's.
    ///
    /// EXP-1271 (EXP-1259 D11): a host that draws its OWN starting state (the
    /// Agent composer: the mark + "Starting on <device>") passes `onTimeout`
    /// with the shorter `feedback-states.json` deadline: no caption, no inline
    /// failure, the host raises the timeout toast. Review #3: nothing was
    /// aborted, so the watch KEEPS going past that deadline and a late row
    /// still lands. Final review: that quiet watch is bounded by
    /// `StartedRunMatch.watchWindow` (30 min, the startId dedupe window),
    /// `cutoff` = the FIRST send of a reused startId (the device dedupes a
    /// Retry, so the row that lands is the original one), and `onLanded`
    /// hands the row to the host's model directly, so a landing while the
    /// page is off-screen is still seen.
    func begin(
        key: StartedRunKey,
        userId: String?,
        device: SteerDevice,
        db: DatabaseManager,
        accountId: String,
        deadline: TimeInterval = StartedRunMatch.deadline,
        cutoff sentAt: Date? = nil,
        onTimeout: (@MainActor () -> Void)? = nil,
        onLanded: (@MainActor (String) -> Void)? = nil
    ) {
        stop()
        let label = device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
        sentCaption = onTimeout == nil ? "Start sent to \(label). Waiting for the desktop…" : nil
        failure = nil
        // A wall-clock deadline INDEPENDENT of DB emissions: with no
        // coding_sessions writes the observation never fires again, so an
        // emission-gated check alone would show the caption forever. It fails
        // LOUD — a run the desktop refused (conflicted worktree, failed
        // doctor) must not read like one still starting. The desktop holds the
        // reason (it notifies there); say where to look. Armed BEFORE the
        // guard below so an unresolvable watch can't strand the caption
        // either.
        deadlineTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(deadline))
            guard let self, !Task.isCancelled else { return }
            self.sentCaption = nil
            guard let onTimeout else {
                self.stop()
                self.failure = "\(label) never started this session. "
                    + "Open the Exponential desktop app there to see why."
                return
            }
            onTimeout()
            self.quietWatchEnds(after: StartedRunMatch.watchWindow - deadline)
        }
        guard let userId, let pool = try? db.pool(forAccountId: accountId) else { return }
        let cutoff = (sentAt ?? Date()).addingTimeInterval(-StartedRunMatch.skew)
        // The whole live table is observed rather than a filtered query: a
        // BATCH row carries neither an issue id nor an action name, so there
        // is nothing narrower to key a WHERE on.
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity.fetchAll(db)
        }
        watchTask = Task { [weak self] in
            do {
                for try await sessions in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    guard let match = StartedRunMatch.find(
                        in: sessions, key: key, userId: userId, cutoff: cutoff
                    ) else { continue }
                    self.stop()
                    self.sentCaption = nil
                    if let onLanded {
                        onLanded(match.id)
                    } else {
                        self.startedSession = StartedSession(sessionId: match.id)
                    }
                    return
                }
            } catch {}
        }
    }

    /// Nothing was aborted at the host's timeout: the watch goes on quietly
    /// until the window closes, then ends without a second error. A task of
    /// its own (weak) so an abandoned host releases the watcher at once.
    private func quietWatchEnds(after window: TimeInterval) {
        deadlineTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(max(0, window)))
            guard let self, !Task.isCancelled else { return }
            self.stop()
        }
    }

    func stop() {
        watchTask?.cancel()
        watchTask = nil
        deadlineTask?.cancel()
        deadlineTask = nil
    }
}
