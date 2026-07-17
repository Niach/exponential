import Foundation

/// EXP-153: client-side staleness guard for `running` coding_sessions rows.
/// A row whose synced `updatedAt` (heartbeat-advanced by the desktop) is
/// older than the contract stale window renders as ABSENT — mirroring the
/// server sweep's DELETE (never as `ended`, that flip is the desktop
/// kill-switch signal) — so a crashed desktop can't pin a phantom
/// "coding now" badge when the sweep lags or isn't running.
public enum CodingSessionLiveness {
    // Cached: ISO8601DateFormatter construction is not free and this runs per
    // row. The fractional/plain pair exists because `.withFractionalSeconds`
    // rejects second-precision strings and vice versa.
    private static let fractional: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
    private static let plain = ISO8601DateFormatter()

    public static func parseIso(_ timestamp: String) -> Date? {
        fractional.date(from: timestamp) ?? plain.date(from: timestamp)
    }

    /// Unparseable liveness signal ⇒ live (fail-open: never hide a session
    /// the server still considers alive; the sweep is the backstop).
    public static func isStale(updatedAt: String, now: Date = Date()) -> Bool {
        guard let seen = parseIso(updatedAt) else { return false }
        return now.timeIntervalSince(seen) * 1000 >= Double(DomainContract.codingSessionStaleMs)
    }

    public static func isLive(_ session: CodingSessionEntity, now: Date = Date()) -> Bool {
        session.status == DomainContract.codingSessionStatusRunning
            && !isStale(updatedAt: session.updatedAt, now: now)
    }
}
