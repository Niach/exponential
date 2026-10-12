import Foundation

/// EXP-1253 (EXP-1259 D1): the Agent page is HOME — the disc mark and ONE
/// greeting line ("Evening, Danny") over the composer, nothing beneath it. A 1:1
/// port of the TS reference `packages/domain-contract/src/home-greeting.ts`,
/// locked by `fixtures/composer-home.json` (`cases`, replayed by
/// `HomeGreetingTests`; desktop `ui::home_greeting`, Android
/// `domain/HomeGreeting.kt`). The fixture's `$comment` holds the rules.
public enum HomeGreeting {
    public struct Bucket: Equatable, Sendable {
        public let id: String
        /// Inclusive hour range; the night wraps midnight (`from > to`).
        public let from: Int
        public let to: Int
        public let variants: [String]
    }

    public static let buckets: [Bucket] = [
        Bucket(id: "morning", from: 5, to: 11, variants: [
            "Good morning, {name}", "Coffee and code, {name}?", "Morning, {name}", "Back at it, {name}",
        ]),
        Bucket(id: "afternoon", from: 12, to: 17, variants: [
            "Good afternoon, {name}", "What's next, {name}?", "Afternoon, {name}", "Back at it, {name}",
        ]),
        Bucket(id: "evening", from: 18, to: 22, variants: [
            "Good evening, {name}", "How was your day, {name}?", "Evening, {name}", "Back at it, {name}",
        ]),
        Bucket(id: "night", from: 23, to: 4, variants: [
            "Hello, night owl", "Burning the midnight oil, {name}?", "Night owl, {name}?", "Still up, {name}?",
        ]),
    ]

    /// The home composer's field placeholder (no subject).
    public static let placeholder = "What do you want to build?"
    /// The greeting DAY starts at 05:00, so one night keeps one variant.
    public static let dayStartsAtHour = 5

    /// The page geometry (`geometry`): phones draw the mark at `markPhone`
    /// and the line at `headlinePhone`.
    public enum Geometry {
        public static let mark: Double = 32
        public static let markPhone: Double = 26
        public static let gap: Double = 12
        public static let headline: Double = 30
        public static let headlinePhone: Double = 24
        public static let headlineWeight = 600
        public static let headlineToCard: Double = 24
        /// The Getting started block (no board yet) sits this far below the
        /// options line.
        public static let composerToGettingStarted: Double = 32
    }

    public enum TestId {
        public static let home = "agent-home"
        public static let greeting = "agent-home-greeting"
        public static let mark = "agent-home-mark"
    }

    /// `YYYY-MM-DDTHH:mm` of `date` in `timeZone` (the device's by default).
    public static func localWallClock(_ date: Date, timeZone: TimeZone = .current) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = timeZone
        let parts = calendar.dateComponents([.year, .month, .day, .hour, .minute], from: date)
        return String(
            format: "%04d-%02d-%02dT%02d:%02d",
            parts.year ?? 1970, parts.month ?? 1, parts.day ?? 1, parts.hour ?? 0, parts.minute ?? 0
        )
    }

    /// The first whitespace-separated token of a user row's name, or "".
    public static func firstName(_ name: String?) -> String {
        (name ?? "").split(whereSeparator: \.isWhitespace).first.map(String.init) ?? ""
    }

    /// The greeting for a local wall-clock time (`YYYY-MM-DDTHH:mm`) and a
    /// user row's name. "" for a malformed time.
    public static func greeting(local: String, name: String?) -> String {
        let scalars = Array(local.utf8)
        guard scalars.count >= 16,
              let year = number(scalars, 0, 4), scalars[4] == UInt8(ascii: "-"),
              let month = number(scalars, 5, 2), scalars[7] == UInt8(ascii: "-"),
              let day = number(scalars, 8, 2), scalars[10] == UInt8(ascii: "T"),
              let hour = number(scalars, 11, 2), scalars[13] == UInt8(ascii: ":"),
              number(scalars, 14, 2) != nil
        else { return "" }
        let bucket = bucket(for: hour)
        let shifted = daysFromCivil(year: year, month: month, day: day) * 24 + hour - dayStartsAtHour
        let dayIndex = floorDiv(shifted, 24)
        let count = bucket.variants.count
        let variant = bucket.variants[((dayIndex % count) + count) % count]
        let first = firstName(name)
        return first.isEmpty
            ? variant.replacingOccurrences(of: ", {name}", with: "")
            : variant.replacingOccurrences(of: "{name}", with: first)
    }

    /// The greeting for `now` in the device's zone.
    public static func greeting(now: Date = Date(), name: String?, timeZone: TimeZone = .current) -> String {
        greeting(local: localWallClock(now, timeZone: timeZone), name: name)
    }

    static func bucket(for hour: Int) -> Bucket {
        for bucket in buckets {
            let inside = bucket.from <= bucket.to
                ? hour >= bucket.from && hour <= bucket.to
                : hour >= bucket.from || hour <= bucket.to
            if inside { return bucket }
        }
        return buckets[0]
    }

    /// Whole days from 1970-01-01 to a civil date (Howard Hinnant's
    /// `days_from_civil`), so the index never depends on a time zone.
    static func daysFromCivil(year: Int, month: Int, day: Int) -> Int {
        let y = month <= 2 ? year - 1 : year
        let era = floorDiv(y, 400)
        let yoe = y - era * 400
        let mp = (month + 9) % 12
        let doy = (153 * mp + 2) / 5 + day - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146_097 + doe - 719_468
    }

    private static func floorDiv(_ a: Int, _ b: Int) -> Int {
        let q = a / b
        return (a % b != 0 && (a < 0) != (b < 0)) ? q - 1 : q
    }

    private static func number(_ bytes: [UInt8], _ start: Int, _ length: Int) -> Int? {
        var value = 0
        for byte in bytes[start..<(start + length)] {
            guard byte >= UInt8(ascii: "0"), byte <= UInt8(ascii: "9") else { return nil }
            value = value * 10 + Int(byte - UInt8(ascii: "0"))
        }
        return value
    }
}
