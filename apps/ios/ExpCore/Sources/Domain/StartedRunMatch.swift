import Foundation

/// EXP-536: a remote start is only a COMMAND — the desktop inserts the
/// `coding_sessions` row a moment later — so every surface that sends one
/// waits for that row to sync in and then PUSHES the live session screen,
/// instead of parking a "watch it in the Agents tab" notice. These are the
/// matching rules, mirrored on Android (`domain/StartedRunMatch.kt`) and web
/// (`lib/started-run-match.ts`); a mismatch there is a start that never
/// navigates.
public enum StartedRunKey: Hashable, Sendable {
    /// An action run — keyed by the display-name SNAPSHOT, never the action
    /// id: the builtin "Create action" row carries `action_id` NULL.
    case action(name: String)
    /// A single-issue session — the desktop stamps `issue_id`.
    case issue(id: String)
    /// A batch run: `issue_id` NULL (it spans issues) and no action name.
    /// Nothing narrower exists — a batch row is unmatchable by issue
    /// server-side too — so the userId + startedAt cut carries the identity.
    case batch

    /// The key for a Start-coding send: 1 id is a plain session, 2+ a batch.
    public static func forIssues(_ issueIds: [String]) -> StartedRunKey? {
        guard let first = issueIds.first else { return nil }
        return issueIds.count >= 2 ? .batch : .issue(id: first)
    }
}

public enum StartedRunMatch {

    /// Clock-skew slack on the desktop-written `started_at` — a row stamped
    /// slightly before the send must still count as ours.
    public static let skew: TimeInterval = 120

    /// How long a start may take to surface its row before the watch gives
    /// up. The "waiting for the desktop" caption shares it: one that died
    /// first would strand a late push with no explanation, one that outlived
    /// it would spin forever.
    public static let deadline: TimeInterval = 180

    public static func matches(
        _ session: CodingSessionEntity,
        key: StartedRunKey,
        userId: String,
        cutoff: Date
    ) -> Bool {
        guard session.userId == userId else { return false }
        // Fail CLOSED on an unparseable stamp (unlike liveness): not pushing
        // beats pushing into somebody else's run.
        guard let startedAt = CodingSessionLiveness.parseIso(session.startedAt),
              startedAt >= cutoff
        else { return false }
        switch key {
        case let .action(name):
            return session.actionName == name
        case let .issue(id):
            return session.issueId == id && session.actionName == nil
        case .batch:
            return session.issueId == nil && session.actionName == nil
        }
    }

    /// The first row that IS this start, or nil while none has synced.
    public static func find(
        in sessions: [CodingSessionEntity],
        key: StartedRunKey,
        userId: String,
        cutoff: Date
    ) -> CodingSessionEntity? {
        sessions.first { matches($0, key: key, userId: userId, cutoff: cutoff) }
    }
}
