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
}
