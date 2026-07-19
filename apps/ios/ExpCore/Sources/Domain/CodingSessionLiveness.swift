import Foundation

/// EXP-153: client-side staleness guard for live coding_sessions rows.
/// A row whose synced `updatedAt` (heartbeat-advanced by the desktop) is
/// older than the contract stale window renders as ABSENT — mirroring the
/// server sweep's DELETE (never as `ended`, that flip is the desktop
/// kill-switch signal) — so a crashed desktop can't pin a phantom
/// "coding now" badge when the sweep lags or isn't running.
/// EXP-194: liveness spans both `running` and `in_review` — the terminal stays
/// alive (watchable/steerable) after the PR opens and the issue parks in
/// review, so the badge and the bottom-nav agents dot both keep counting it.
public enum CodingSessionLiveness {
    /// Parse the synced `updatedAt` heartbeat. Delegates to WireTimestamps so
    /// Electric's Postgres text form (space separator, hour-only offset) parses
    /// too — this is what ACTIVATES the guard on synced rows. An ISO8601-only
    /// parser returned nil for every synced heartbeat, keeping the guard
    /// permanently fail-open (phantom "coding now" badges). Nil still means
    /// fail-open by design (see isStale): never hide a live session.
    public static func parseIso(_ timestamp: String) -> Date? {
        WireTimestamps.parse(timestamp)
    }

    /// Unparseable liveness signal ⇒ live (fail-open: never hide a session
    /// the server still considers alive; the sweep is the backstop).
    public static func isStale(updatedAt: String, now: Date = Date()) -> Bool {
        guard let seen = parseIso(updatedAt) else { return false }
        return now.timeIntervalSince(seen) * 1000 >= Double(DomainContract.codingSessionStaleMs)
    }

    public static func isLive(_ session: CodingSessionEntity, now: Date = Date()) -> Bool {
        (session.status == DomainContract.codingSessionStatusRunning
            || session.status == DomainContract.codingSessionStatusInReview)
            && !isStale(updatedAt: session.updatedAt, now: now)
    }
}
