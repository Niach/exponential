import Foundation

/// EXP-637: how an ENDED coding session reads once the agent reported its
/// close-out (`exponential_sessions_end` stamps `summary` + `outcome`; since
/// EXP-673 it ENDS only an automation-started run — a person-started one
/// reports first and ends later, with its tab).
///
/// The labels are byte-identical across web (`lib/coding-session-display.ts`
/// `sessionOutcomeLabel`), desktop (`ended_runs.rs`), Android
/// (`domain/RunOutcome.kt`) and here — a run reads the same wherever it is
/// listed. Glyph and tint are per-platform (iOS draws them in ExpUI's
/// `EndedRunRow`); only the words are the contract.
public enum RunOutcomePresentation {
    /// The label a run's outcome wears. An ended row with NO outcome simply
    /// says "Ended": every path except the agent's own close-out (kill switch,
    /// tab close, PR merge, sweep) leaves the column NULL.
    public static func label(_ outcome: String?) -> String {
        switch outcome {
        case DomainContract.codingSessionOutcomeDone: return "Done"
        case DomainContract.codingSessionOutcomeBlocked: return "Blocked"
        case DomainContract.codingSessionOutcomeNoChanges: return "No changes"
        default: return "Ended"
        }
    }

    /// Whether an ENDED row carries the agent's close-out — the rule the runs
    /// lists filter on. Those are the rows with a summary worth expanding.
    /// Keyed on `outcome` (its only writer is `exponential_sessions_end`),
    /// NOT on `ended_by` (EXP-673): a person-started run's end is the tab
    /// close or kill that came after its report. Mirrored on web
    /// (`use-agents-data.ts`) and Android (`recentRunRows`).
    public static func hasCloseOut(_ session: CodingSessionEntity) -> Bool {
        session.status == DomainContract.codingSessionStatusEnded
            && session.outcome != nil
    }
}

/// EXP-637: whether an ended run can be picked up again, and on which machine.
///
/// Resume relaunches the pinned agent in the run's OWN worktree, so it is
/// bound to the machine that ran it: `steer.startSession({ resumeSessionId })`
/// refuses a foreign session (owner-only), a still-live one, another machine,
/// and one whose device does not advertise `resume-run`. The clients mirror
/// those gates so the affordance never appears where the send would bounce.
public enum RunResume {
    /// The machine a Resume would go to, or nil when this run cannot be
    /// resumed from here. Prefers the caller's OWN devices row when a shared
    /// teammate row happens to carry the same machine id.
    public static func target(
        for session: CodingSessionEntity,
        devices: [SteerDevice],
        currentUserId: String?
    ) -> SteerDevice? {
        guard CodingSessionOwnership.isOwn(session, userId: currentUserId),
              session.status == DomainContract.codingSessionStatusEnded,
              let deviceId = session.deviceId, !deviceId.isEmpty
        else { return nil }
        let candidates = devices.filter { $0.deviceId == deviceId }
        let device = candidates.first(where: \.isMine) ?? candidates.first
        guard let device, device.isOnline, device.canResumeRun else { return nil }
        return device
    }
}
