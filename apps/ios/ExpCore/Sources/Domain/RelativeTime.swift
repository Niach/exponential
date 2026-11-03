import Foundation

/// The two relative-time wordings ×4 (polish round, pinned):
/// LIST captions (inbox rows, device "Last seen", run rows) = COMPACT
/// `just now · 5m · 2h · 3d` (web inbox-view `relativeTime`, desktop
/// `inbox::relative_time`, rounding like both); ACTIVITY and comments = LONG
/// `5 minutes ago · 2 hours ago · 2 days ago`.
public enum RelativeTime {
    /// Compact list caption. Future instants clamp to "just now".
    public static func compact(_ date: Date, now: Date = Date()) -> String {
        let seconds = max(0, now.timeIntervalSince(date))
        let mins = Int((seconds / 60).rounded())
        if mins < 1 { return "just now" }
        if mins < 60 { return "\(mins)m" }
        let hours = Int((Double(mins) / 60).rounded())
        if hours < 24 { return "\(hours)h" }
        return "\(Int((Double(hours) / 24).rounded()))d"
    }

    /// Compact caption off a synced wire timestamp; empty when it does not parse.
    public static func compact(wire: String, now: Date = Date()) -> String {
        guard let date = WireTimestamps.parse(wire) else { return "" }
        return compact(date, now: now)
    }

    /// Long activity/comment caption: "5 minutes ago", "10 days ago",
    /// "1 month ago" — web's date-fns `formatDistanceToNowStrict` with
    /// `addSuffix` (rounded units, no weeks; Android `longRelativeTime`).
    /// Under a minute, or a future instant, reads "just now" ×4.
    public static func long(_ date: Date, now: Date = Date()) -> String {
        let seconds = Int(max(0, now.timeIntervalSince(date)).rounded())
        func unit(_ n: Int, _ name: String) -> String { "\(n) \(name)\(n == 1 ? "" : "s") ago" }
        if seconds < 60 { return "just now" }
        let minutes = Int((Double(seconds) / 60).rounded())
        if minutes < 60 { return unit(minutes, "minute") }
        let hours = Int((Double(minutes) / 60).rounded())
        if hours < 24 { return unit(hours, "hour") }
        let days = Int((Double(hours) / 24).rounded())
        if days < 30 { return unit(days, "day") }
        let months = Int((Double(days) / 30).rounded())
        if months < 12 { return unit(months, "month") }
        return unit(Int((Double(months) / 12).rounded()), "year")
    }

    /// Long caption off a synced wire timestamp; empty when it does not parse.
    public static func long(wire: String, now: Date = Date()) -> String {
        guard let date = WireTimestamps.parse(wire) else { return "" }
        return long(date, now: now)
    }
}
