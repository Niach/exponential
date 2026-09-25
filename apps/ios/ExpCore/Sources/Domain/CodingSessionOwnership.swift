import Foundation

/// EXP-312 follow-up: a live coding session is viewable and steerable ONLY by
/// its owner, so the session LISTS (the Agents surface) show the signed-in
/// user's own runs and nothing else — a teammate's row was unopenable and read
/// as "your computer is not online". Sessions stay synced: the issue-detail
/// badge and the Reviews surface still see everyone's, this is a list rule.
public enum CodingSessionOwnership {
    /// No resolved userId (no signed-in account) owns nothing — the list shows
    /// its empty state rather than every member's sessions.
    public static func isOwn(_ session: CodingSessionEntity, userId: String?) -> Bool {
        guard let userId, !userId.isEmpty else { return false }
        return session.userId == userId
    }

    public static func own(
        _ sessions: [CodingSessionEntity], userId: String?
    ) -> [CodingSessionEntity] {
        sessions.filter { isOwn($0, userId: userId) }
    }

    /// The Agents surface is scoped to the ACTIVE team as well as the caller,
    /// matching web's `use-agents-data.ts` (`team_id = active AND user_id =
    /// me`): an own run in another team belongs under THAT team, not under
    /// whichever one happens to be open. Every session row carries a non-null
    /// synced `team_id` (trigger-denormalized for issue rows, explicit on
    /// batch/action rows), so this also holds for issueless runs. No active
    /// team resolved yet shows nothing, like no resolved userId.
    public static func isOwn(
        _ session: CodingSessionEntity, userId: String?, teamId: String?
    ) -> Bool {
        guard let teamId, !teamId.isEmpty else { return false }
        return isOwn(session, userId: userId) && session.teamId == teamId
    }

    public static func own(
        _ sessions: [CodingSessionEntity], userId: String?, teamId: String?
    ) -> [CodingSessionEntity] {
        sessions.filter { isOwn($0, userId: userId, teamId: teamId) }
    }

    /// EXP-1075: the flip side of that scoping — with the lists team-scoped, a
    /// live run of the caller's in ANOTHER team goes silent everywhere (the
    /// Agents tab dot is active-team-only by the rule above). This is the one
    /// signal that says "your runs are over there": the caller's own live
    /// sessions counted per team.
    public struct TeamLiveRuns: Equatable {
        public var count: Int
        public var needsInput: Bool

        public init(count: Int = 0, needsInput: Bool = false) {
            self.count = count
            self.needsInput = needsInput
        }
    }

    /// Own + live (`CodingSessionLiveness`, so a heartbeat-stale row counts as
    /// absent exactly like it does for the tab dot), grouped by `teamId`.
    /// `needsInput` follows `CodingSessionDisplayState` — the SAME masking the
    /// Agents tab dot's amber goes through (`prState: nil`: the switcher has no
    /// issue row in hand, and in_review already outranks the flag).
    /// Tolerates a caller passing an already-own-only list.
    public static func liveByTeam(
        _ sessions: [CodingSessionEntity], userId: String?, now: Date = Date()
    ) -> [String: TeamLiveRuns] {
        var byTeam: [String: TeamLiveRuns] = [:]
        for session in sessions {
            guard isOwn(session, userId: userId), !session.teamId.isEmpty else { continue }
            guard CodingSessionLiveness.isLive(session, now: now) else { continue }
            var entry = byTeam[session.teamId] ?? TeamLiveRuns()
            entry.count += 1
            if CodingSessionDisplayState.of(session: session, prState: nil) == .needsInput {
                entry.needsInput = true
            }
            byTeam[session.teamId] = entry
        }
        return byTeam
    }

    /// The board switcher's dot: is there live own work OUTSIDE the active
    /// team, and does any of it want the user? The active team is excluded —
    /// its runs already light the Agents tab.
    public static func otherTeamsLive(
        _ byTeam: [String: TeamLiveRuns], activeTeamId: String?
    ) -> (any: Bool, needsInput: Bool) {
        var anyLive = false
        var needsInput = false
        for (teamId, runs) in byTeam where teamId != activeTeamId && runs.count > 0 {
            anyLive = true
            if runs.needsInput { needsInput = true }
        }
        return (anyLive, needsInput)
    }
}
