import Foundation

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
        // EXP-888: a sweep end (`ended_by == "stale"`) is NOT an end — the
        // host ignores the flip and heartbeats the row back to `running`.
        // Resuming it would put a second agent on the run's worktree.
        guard CodingSessionOwnership.isOwn(session, userId: currentUserId),
              PastRuns.hasEnded(session),
              let deviceId = session.deviceId, !deviceId.isEmpty
        else { return nil }
        let candidates = devices.filter { $0.deviceId == deviceId }
        let device = candidates.first(where: \.isMine) ?? candidates.first
        guard let device, device.isOnline, device.canResumeRun else { return nil }
        return device
    }
}
