import Foundation

/// ISO dates (`YYYY-MM-DD`) the DatePicker speaks, UTC-free.
public enum DateModel {
    public static func parseISO(_ s: String) -> (Int, Int, Int)? {
        let parts = s.trimmingCharacters(in: .whitespaces).split(separator: "-")
        guard parts.count == 3, let y = Int(parts[0]), let m = Int(parts[1]), let d = Int(parts[2]), (1...12).contains(m), (1...31).contains(d) else { return nil }
        return (y, m, d)
    }

    public static func iso(_ y: Int, _ m: Int, _ d: Int) -> String {
        String(format: "%04d-%02d-%02d", y, m, d)
    }

    static var calendar: Calendar {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "UTC")!
        return c
    }

    public static func date(fromISO s: String) -> Date? {
        guard let (y, m, d) = parseISO(s) else { return nil }
        return calendar.date(from: DateComponents(year: y, month: m, day: d))
    }

    public static func iso(from date: Date) -> String {
        let c = calendar.dateComponents([.year, .month, .day], from: date)
        return iso(c.year ?? 1970, c.month ?? 1, c.day ?? 1)
    }
}
