import Foundation

/// Tolerant wire-timestamp parser. Synced rows carry timestamps in TWO wire
/// forms: Electric's Postgres text encoding
/// `yyyy-MM-dd HH:mm:ss[.ffffff]+00` (space separator, hour-only offset) and
/// ISO-8601 `yyyy-MM-ddTHH:mm:ss[.SSS]Z` / `+HH:MM` from tRPC-era writes.
/// `ISO8601DateFormatter` alone rejects the Electric form, which blanked every
/// synced row's relative time (EXP-169) and kept the EXP-153 staleness guard
/// permanently fail-open. Mirrors apps/android/.../domain/WireTimestamps.kt.
public enum WireTimestamps {
    // Cached: ISO8601DateFormatter construction isn't free and this runs per
    // visible row per state emission. The fractional/plain pair exists because
    // `.withFractionalSeconds` rejects second-precision strings and vice versa.
    // ISO8601DateFormatter is documented thread-safe but isn't Sendable, so
    // strict concurrency needs the nonisolated(unsafe) opt-out.
    private nonisolated(unsafe) static let isoFractional: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
    private nonisolated(unsafe) static let isoPlain = ISO8601DateFormatter()

    /// Parse either wire form to a `Date`, or nil if neither matches.
    public static func parse(_ value: String) -> Date? {
        // Large chats: the strict fast path first — both wire forms without
        // a formatter (an 8.5k-message chat parsed every stamp through
        // `ISO8601DateFormatter` per snapshot, ~0.1s of each build). Anything
        // it does not fully recognise takes the tolerant path below.
        fastParse(value) ?? tolerantParse(value)
    }

    /// The formatter path: surrounding whitespace, any ISO shape the
    /// formatters take, the Postgres text form normalised first.
    static func tolerantParse(_ value: String) -> Date? {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        // EXP-1358: the Postgres text form (a SPACE separator — what every
        // synced row carries) goes straight to normalisation: both ISO
        // formatters reject it, and those two failed attempts per timestamp
        // used to dominate the session-tree sorts. An ISO string still tries
        // the formatters first; a failed one still falls back to normalising.
        let separator = trimmed.utf8.count > 10 ? trimmed.utf8[trimmed.utf8.index(trimmed.utf8.startIndex, offsetBy: 10)] : 0
        if separator != UInt8(ascii: " "), let date = parseIso(trimmed) { return date }
        // Otherwise normalize the Postgres text form into RFC 3339 and re-try.
        guard let normalized = normalizePostgresText(trimmed) else { return nil }
        return parseIso(normalized)
    }

    /// `yyyy-MM-dd[T| ]HH:mm:ss[.f…][Z|±HH|±HHMM|±HH:MM]` (no offset = UTC,
    /// the fraction truncated to milliseconds exactly like the tolerant
    /// path's normalisation). nil = not exactly that shape, or a field out of
    /// range: the caller falls back to the formatters.
    static func fastParse(_ value: String) -> Date? {
        if let parsed = value.utf8.withContiguousStorageIfAvailable(fastParse) { return parsed }
        // A bridged (non-native) string: one contiguous copy.
        var native = value
        native.makeContiguousUTF8()
        return native.utf8.withContiguousStorageIfAvailable(fastParse) ?? nil
    }

    private static func fastParse(_ b: UnsafeBufferPointer<UInt8>) -> Date? {
        let n = b.count
        guard n >= 19 else { return nil }
        func digits(_ at: Int, _ count: Int) -> Int? {
            var out = 0
            for i in at..<(at + count) {
                let c = b[i]
                guard c >= 48, c <= 57 else { return nil }
                out = out * 10 + Int(c - 48)
            }
            return out
        }
        guard b[4] == 45, b[7] == 45, b[10] == 84 || b[10] == 32, b[13] == 58, b[16] == 58,
              let year = digits(0, 4), let month = digits(5, 2), let day = digits(8, 2),
              let hour = digits(11, 2), let minute = digits(14, 2), let second = digits(17, 2),
              (1...12).contains(month), (1...31).contains(day), hour < 24, minute < 60, second < 60
        else { return nil }
        var i = 19
        var millis = 0
        if i < n, b[i] == 46 {
            i += 1
            var count = 0
            while i < n, b[i] >= 48, b[i] <= 57 {
                if count < 3 { millis = millis * 10 + Int(b[i] - 48) }
                count += 1
                i += 1
            }
            guard count > 0 else { return nil }
            for _ in min(count, 3)..<3 { millis *= 10 }
        }
        var offset = 0
        if i < n {
            let sign = b[i]
            if sign == 90 {
                i += 1
            } else if sign == 43 || sign == 45 {
                i += 1
                guard i + 2 <= n, let hh = digits(i, 2) else { return nil }
                i += 2
                var mm = 0
                if i < n, b[i] == 58 { i += 1 }
                if i < n {
                    guard i + 2 <= n, let m = digits(i, 2) else { return nil }
                    mm = m
                    i += 2
                }
                guard hh < 24, mm < 60 else { return nil }
                offset = (hh * 3600 + mm * 60) * (sign == 45 ? -1 : 1)
            } else {
                return nil
            }
        }
        guard i == n else { return nil }
        // Days from the civil date (Howard Hinnant's algorithm), UTC.
        let y = month <= 2 ? year - 1 : year
        let era = (y >= 0 ? y : y - 399) / 400
        let yoe = y - era * 400
        let mp = (month + 9) % 12
        let doy = (153 * mp + 2) / 5 + day - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        let days = era * 146_097 + doe - 719_468
        // A day past the month's end (Feb 30) is the formatter's to reject.
        guard day <= daysIn(month: month, year: year) else { return nil }
        let seconds = days * 86_400 + hour * 3600 + minute * 60 + second - offset
        return Date(timeIntervalSince1970: Double(seconds) + Double(millis) / 1000)
    }

    private static func daysIn(month: Int, year: Int) -> Int {
        switch month {
        case 2: return (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 ? 29 : 28
        case 4, 6, 9, 11: return 30
        default: return 31
        }
    }

    private static func parseIso(_ value: String) -> Date? {
        isoFractional.date(from: value) ?? isoPlain.date(from: value)
    }

    /// `2026-07-17 10:00:00.123456+00` → `2026-07-17T10:00:00.123+00:00`. Returns
    /// nil when the string is too short to be a full date-time.
    private static func normalizePostgresText(_ value: String) -> String? {
        var chars = Array(value)
        // Need at least `yyyy-MM-ddTHH:mm:ss` (19 chars) to normalize.
        guard chars.count >= 19 else { return nil }
        // Space (Postgres) → 'T' (RFC 3339). Idempotent when already 'T'.
        chars[10] = "T"
        let base = String(chars[0..<19])          // yyyy-MM-ddTHH:mm:ss
        let rest = String(chars[19...])           // "[.fraction][zone]"

        // The fraction (if any) precedes the zone designator; scan from the
        // first '+'/'-'/'Z' (a fraction never contains those).
        let fraction: String
        let zone: String
        if let zoneIndex = rest.firstIndex(where: { $0 == "+" || $0 == "-" || $0 == "Z" }) {
            fraction = normalizeFraction(String(rest[..<zoneIndex]))
            zone = normalizeZone(String(rest[zoneIndex...]))
        } else {
            fraction = normalizeFraction(rest)
            zone = "Z"   // offset-less Postgres text is UTC
        }
        return base + fraction + zone
    }

    /// Pad/truncate the fraction to exactly the 3 digits the fractional ISO
    /// formatter accepts (`.5` → `.500`, `.123456` → `.123`); empty stays empty.
    private static func normalizeFraction(_ fraction: String) -> String {
        guard fraction.hasPrefix(".") else { return "" }
        let digits = String(fraction.dropFirst().prefix(while: { $0.isNumber }))
        guard !digits.isEmpty else { return "" }
        let padded = digits.count >= 3
            ? String(digits.prefix(3))
            : digits.padding(toLength: 3, withPad: "0", startingAt: 0)
        return ".\(padded)"
    }

    /// Hour-only offsets (`+00`) become `+HH:MM`; `Z` and colon forms pass
    /// through unchanged.
    private static func normalizeZone(_ zone: String) -> String {
        if zone == "Z" { return "Z" }
        let sign = zone.prefix(1)
        let body = zone.dropFirst()
        if body.contains(":") { return zone }
        switch body.count {
        case 2: return "\(sign)\(body):00"           // "+00" → "+00:00"
        case 4:                                       // "+0530" → "+05:30"
            return "\(sign)\(body.prefix(2)):\(body.suffix(2))"
        default: return zone
        }
    }
}
