import Foundation

/// VAPP-201 (packages/domain-contract/fixtures/team-zone.json): the TEAM TIME
/// ZONE every app host reads dates in. Replays the TS reference
/// `@exp/domain-contract/team-zone` (desktop `domain::team_zone`, Android
/// `TeamZone.kt`).
public enum TeamZone {
    public enum Copy {
        public static let row = "Time zone"
        public static let device = "Device time zone"
    }

    public enum TestIds {
        public static let row = "team-time-zone"
    }

    /// The zone a team's `time_zone` names, when the tz database knows it;
    /// nil = the device's zone (no team zone, blank, or an unknown name).
    public static func resolveZone(_ teamZone: String?) -> String? {
        guard let zone = teamZone?.trimmingCharacters(in: .whitespacesAndNewlines), !zone.isEmpty else { return nil }
        return TimeZone(identifier: zone) == nil ? nil : zone
    }

    /// A zone's offset from UTC at an instant, in minutes east.
    public static func zoneOffset(_ zone: String, ms: Double) -> Int {
        guard let tz = TimeZone(identifier: zone) else { return 0 }
        let date = Date(timeIntervalSince1970: (ms / 1000).rounded(.down))
        return tz.secondsFromGMT(for: date) / 60
    }

    /// The host's zone: the team zone when it resolves, else the device's.
    public static func hostTimeZone(_ teamZone: String?) -> TimeZone {
        resolveZone(teamZone).flatMap(TimeZone.init(identifier:)) ?? .current
    }

    /// Dogfood G: the calendar day (`YYYY-MM-DD`) at an instant in a zone:
    /// the records month view's `today` and default month
    /// (records-query.json `today`), the day `today()` reads.
    public static func day(_ date: Date, in zone: TimeZone) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = zone
        let c = calendar.dateComponents([.year, .month, .day], from: date)
        return String(format: "%04d-%02d-%02d", c.year ?? 0, c.month ?? 0, c.day ?? 0)
    }
}
