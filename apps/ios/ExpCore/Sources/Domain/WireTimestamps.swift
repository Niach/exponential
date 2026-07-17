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
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        // Try the ISO-8601 forms first (tRPC-era writes) — cheap nil-returning
        // attempts, no exceptions.
        if let date = parseIso(trimmed) { return date }
        // Otherwise normalize the Postgres text form into RFC 3339 and re-try.
        guard let normalized = normalizePostgresText(trimmed) else { return nil }
        return parseIso(normalized)
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
