import Foundation

/// Cached, reusable `DateFormatter`s. Creating a `DateFormatter` is expensive,
/// so these shared instances replace the per-call formatters that were inlined
/// across the issue views. Do not mutate their `dateFormat` — add a new case.
enum AppDateFormatters {
    /// `yyyy-MM-dd` — the wire format for `dueDate` (a date-only column).
    static let yyyyMMdd = formatter("yyyy-MM-dd")
    /// `HH:mm` — the wire format for `dueTime` / `endTime`.
    static let HHmm = formatter("HH:mm")
    /// `MMM d` — short display, e.g. "Mar 5".
    static let MMMd = formatter("MMM d")

    private static func formatter(_ format: String) -> DateFormatter {
        let f = DateFormatter()
        f.dateFormat = format
        return f
    }
}
