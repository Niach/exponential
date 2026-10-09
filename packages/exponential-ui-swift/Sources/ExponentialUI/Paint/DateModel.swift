import Foundation

/// ISO dates (`YYYY-MM-DD`) as UTC `Date`s (the platform formatters show
/// them in the surface locale; calendar arithmetic is the model's).
enum DateModel {
    static func date(fromISO s: String) -> Date? {
        let parts = s.trimmingCharacters(in: .whitespaces).split(separator: "-")
        guard parts.count == 3, let y = Int(parts[0]), let m = Int(parts[1]), let d = Int(parts[2]), (1...12).contains(m), (1...31).contains(d) else { return nil }
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "UTC")!
        return c.date(from: DateComponents(year: y, month: m, day: d))
    }
}
