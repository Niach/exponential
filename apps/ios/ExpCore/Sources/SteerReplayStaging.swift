import Foundation

/// EXP-656: a join replay must never move the reader.
///
/// The relay answers EVERY viewer join with `activity_reset` + a full replay
/// of the room log (apps/steer-relay/src/hub.ts), and a publisher reconnect
/// fans out the same pair. Applying the reset immediately empties the feed,
/// and the replay that follows re-appends the whole history under a viewer who
/// was reading a plan awaiting approval — the feed collapses, the follow pin
/// re-arms, and the reader is yanked to the bottom.
///
/// So a reset does not clear anything: it opens a STAGING window. Activity
/// frames buffer until the replay ends, and the whole thing then swaps in as
/// ONE commit. The end is the relay's `activity_synced` marker where it sends
/// one (EXP-656), a `keepalive` (its own 15s beat means the burst is over) for
/// a publisher-driven republish that carries no marker, and a quiet/deadline
/// timer as the fallback that can never leave a buffer stranded.
///
/// Pure and shared with the tests; the model (AgentSessionModel) owns the
/// buffer, the timers and the feed.
public enum SteerReplayStaging {
    /// The relay frames staging cares about. Everything else (`bye`, `error`,
    /// input echoes, unknown types) is not this rule's business.
    public enum Frame: Equatable, Sendable {
        case activityReset
        case activity
        /// EXP-656: end-of-replay marker, sent right after the join replay.
        case activitySynced
        case keepalive

        /// The wire `t`, or nil for a frame staging does not own.
        public init?(wire: String) {
            switch wire {
            case "activity": self = .activity
            case "activity_reset": self = .activityReset
            case "activity_synced": self = .activitySynced
            case "keepalive": self = .keepalive
            default: return nil
            }
        }
    }

    /// What the model does with the frame.
    public enum Action: Equatable, Sendable {
        /// Open a staging window (a second reset restarts it — the buffered
        /// frames belonged to a replay the relay has just superseded).
        case beginStaging
        /// Buffer this activity frame; the visible feed does not move.
        case stage
        /// Ordinary live frame — fold it into the visible feed now.
        case apply
        /// The replay is over: swap the buffer in as one commit.
        case commit
        /// Nothing to do.
        case ignore
    }

    public static func decide(staging: Bool, frame: Frame) -> Action {
        switch frame {
        case .activityReset:
            return .beginStaging
        case .activity:
            return staging ? .stage : .apply
        case .activitySynced, .keepalive:
            return staging ? .commit : .ignore
        }
    }

    /// The timer fallback for a replay that ends without a marker: commit once
    /// the stream has been quiet for `quiet`, and unconditionally at `max` so a
    /// stalled republish commits what it has (then appends) instead of holding
    /// the buffer forever.
    public static func shouldCommit(
        now: Date, lastFrameAt: Date, startedAt: Date,
        quiet: TimeInterval, max: TimeInterval
    ) -> Bool {
        if now.timeIntervalSince(startedAt) >= max { return true }
        return now.timeIntervalSince(lastFrameAt) >= quiet
    }
}
