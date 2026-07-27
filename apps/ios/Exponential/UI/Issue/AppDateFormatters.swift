import Foundation

/// Cached, reusable `DateFormatter`s. Creating a `DateFormatter` is expensive,
/// so these shared instances replace the per-call formatters that were inlined
/// across the issue views. Do not mutate their `dateFormat` — add a new case.
enum AppDateFormatters {
    /// `yyyy-MM-dd` — the wire format for `dueDate` (a date-only column).
    static let yyyyMMdd = wireFormatter("yyyy-MM-dd")
    /// `MMM d` — short display, e.g. "Mar 5". Deliberately device-locale.
    static let MMMd = displayFormatter("MMM d")

    /// Fixed-format formatter for machine-readable wire strings (Apple QA1480):
    /// `en_US_POSIX` + explicit Gregorian pin the output against non-Gregorian
    /// device calendars (Buddhist year 2569, Japanese era years). Time zone is
    /// deliberately NOT pinned — dueDate is a floating LOCAL value; pinning UTC
    /// would shift the calendar day for non-UTC users.
    private static func wireFormatter(_ format: String) -> DateFormatter {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.dateFormat = format
        return f
    }

    private static func displayFormatter(_ format: String) -> DateFormatter {
        let f = DateFormatter()
        f.dateFormat = format
        return f
    }
}
