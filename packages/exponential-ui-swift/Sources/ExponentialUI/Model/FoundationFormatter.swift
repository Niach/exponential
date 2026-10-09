import Foundation
import ExponentialUICore

/// The surface `HostFormatter` over Foundation (round-2 contract §3):
/// numbers, currencies, percents, dates, relative times, byte sizes and
/// plural categories in `locale` (BCP 47) and `timeZone` (IANA; nil = the
/// device's). The core parses the values and picks the relative unit and
/// the plural arm; this only localizes. Rounding is half away from zero on
/// the shortest round-trip decimal; `format` patterns go through the core's
/// TR35 subset (`formatPatternJson`) with Foundation's month and weekday
/// names; a time's narrow no-break space before AM/PM is a plain space (the
/// contract's en-US output).
///
/// Long and full date-times join the date and the time with the locale's
/// `atTime` glue (`October 14, 2026 at 2:30 PM`, what `Intl` prints), never
/// the plain `{1}, {0}` newer ICU builds hand `DateFormatter` on macOS: the
/// date and the time format separately and the joiner comes from a
/// `dateFormat(fromTemplate:)`-free table, so the output is the same on
/// every OS release.
public final class FoundationFormatter: HostFormatter, @unchecked Sendable {
    public let localeId: String
    public let timeZone: TimeZone
    private let foundationLocale: Locale
    private let utc = TimeZone(identifier: "UTC")!
    private let lock = NSLock()
    private var cachedNames: String?

    public init(locale: String, timeZone: String? = nil) {
        localeId = locale.isEmpty ? "en-US" : locale
        foundationLocale = Locale(identifier: localeId)
        self.timeZone = timeZone.flatMap(TimeZone.init(identifier:)) ?? .current
    }

    public func locale() -> String { localeId }

    /// The shortest round-trip decimal of `value` (`1.005`, not `1.00499…`).
    private func decimal(_ value: Double) -> NSDecimalNumber {
        NSDecimalNumber(string: "\(value)", locale: Locale(identifier: "en_US_POSIX"))
    }

    private func numberFormatter(_ style: NumberFormatter.Style, min: Int, max: Int, grouping: Bool) -> NumberFormatter {
        let f = NumberFormatter()
        f.locale = foundationLocale
        f.numberStyle = style
        f.minimumFractionDigits = min
        f.maximumFractionDigits = max
        f.roundingMode = .halfUp
        f.usesGroupingSeparator = grouping
        return f
    }

    public func number(value: Double, decimals: UInt32?, grouping: Bool) -> String {
        guard value.isFinite else { return "" }
        let d = decimals.map(Int.init)
        let f = numberFormatter(.decimal, min: d ?? 0, max: d ?? 3, grouping: grouping)
        return Self.clean(f.string(from: decimal(value)) ?? "")
    }

    public func currency(value: Double, code: String, decimals: UInt32?, grouping: Bool) -> String {
        guard value.isFinite else { return "" }
        let upper = code.uppercased()
        guard upper.count == 3, Locale.Currency.isoCurrencies.contains(where: { $0.identifier == upper }) else { return "" }
        let f = NumberFormatter()
        f.locale = foundationLocale
        f.numberStyle = .currency
        f.currencyCode = upper
        let digits = decimals.map(Int.init) ?? f.maximumFractionDigits
        f.minimumFractionDigits = digits
        f.maximumFractionDigits = digits
        f.roundingMode = .halfUp
        f.usesGroupingSeparator = grouping
        return Self.clean(f.string(from: decimal(value)) ?? "")
    }

    public func percent(value: Double, decimals: UInt32?) -> String {
        guard value.isFinite else { return "" }
        let d = decimals.map(Int.init) ?? 0
        let f = numberFormatter(.percent, min: d, max: d, grouping: true)
        return Self.clean(f.string(from: decimal(value)) ?? "")
    }

    public func date(epochMs: Double, dateOnly: Bool, format: String?, style: String?, time: Bool) -> String {
        let zone = dateOnly ? utc : timeZone
        let date = Date(timeIntervalSince1970: epochMs / 1000)
        if let format {
            var cal = Calendar(identifier: .gregorian)
            cal.timeZone = zone
            let c = cal.dateComponents([.year, .month, .day, .weekday, .hour, .minute, .second], from: date)
            let fields: JSONValue = .object([
                "year": .number(Double(c.year ?? 1970)),
                "month": .number(Double(c.month ?? 1)),
                "day": .number(Double(c.day ?? 1)),
                // Calendar weekdays are 1-based, Sunday first; the core's 0-based.
                "weekday": .number(Double((c.weekday ?? 1) - 1)),
                "hour": .number(Double(c.hour ?? 0)),
                "minute": .number(Double(c.minute ?? 0)),
                "second": .number(Double(c.second ?? 0)),
            ])
            return (try? formatPatternJson(pattern: format, fieldsJson: fields.json, namesJson: names())) ?? ""
        }
        let dateStyle: DateFormatter.Style = switch style {
        case "short": .short
        case "long": .long
        case "full": .full
        default: .medium
        }
        let d = DateFormatter()
        d.locale = foundationLocale
        d.timeZone = zone
        d.dateStyle = dateStyle
        d.timeStyle = .none
        let day = d.string(from: date)
        guard time else { return Self.clean(day) }
        let t = DateFormatter()
        t.locale = foundationLocale
        t.timeZone = zone
        t.dateStyle = .none
        t.timeStyle = .short
        let clock = t.string(from: date)
        return Self.clean(Self.join(day, clock, style: dateStyle, language: foundationLocale.language.languageCode?.identifier ?? "en"))
    }

    /// The CLDR date-time glue: long / full = `atTime` (`{1} 'at' {0}` in
    /// English), short / medium = `{1}, {0}`. Languages without an entry use
    /// the comma.
    static func join(_ day: String, _ clock: String, style: DateFormatter.Style, language: String) -> String {
        let at: [String: String] = ["en": " at ", "de": " um ", "fr": " à ", "es": ", ", "it": " alle ore ", "nl": " om ", "pt": " às "]
        if style == .long || style == .full, let glue = at[language] { return day + glue + clock }
        return day + ", " + clock
    }

    public func relativeTime(value: Int64, unit: String) -> String {
        let f = RelativeDateTimeFormatter()
        f.locale = foundationLocale
        f.dateTimeStyle = .named
        f.unitsStyle = .full
        var c = DateComponents()
        let v = Int(value)
        switch unit {
        case "second": c.second = v
        case "minute": c.minute = v
        case "hour": c.hour = v
        case "day": c.day = v
        case "week": c.weekOfMonth = v
        case "month": c.month = v
        default: c.year = v
        }
        return Self.clean(f.localizedString(from: c))
    }

    /// The CLDR plural category. Foundation has no public plural-rules API:
    /// the cardinal rules of the common languages, `other` for the rest
    /// (East Asian languages have only `other`).
    public func plural(value: Double) -> String {
        let lang = foundationLocale.language.languageCode?.identifier ?? "en"
        let isInt = value.rounded() == value && value.isFinite
        let i = isInt ? abs(value) : -1
        switch lang {
        case "ja", "zh", "ko", "th", "vi", "id", "ms":
            return "other"
        case "fr", "pt":
            return (value >= 0 && value < 2) ? "one" : "other"
        case "ru", "uk":
            guard isInt else { return "other" }
            let n = Int(i), m10 = n % 10, m100 = n % 100
            if m10 == 1 && m100 != 11 { return "one" }
            if (2...4).contains(m10) && !(12...14).contains(m100) { return "few" }
            return "many"
        case "pl":
            guard isInt else { return "other" }
            let n = Int(i), m10 = n % 10, m100 = n % 100
            if n == 1 { return "one" }
            if (2...4).contains(m10) && !(12...14).contains(m100) { return "few" }
            return "many"
        default:
            // en, de, nl, sv, it, es…: `one` = the integer 1 without visible fraction digits.
            return i == 1 ? "one" : "other"
        }
    }

    /// A byte size in the locale's short unit (`Intl` `style: unit`: en `47.1 kB`).
    public func bytes(value: Double, unit: String) -> String {
        guard value.isFinite else { return "" }
        let u: UnitInformationStorage = switch unit {
        case "kilobyte": .kilobytes
        case "megabyte": .megabytes
        case "gigabyte": .gigabytes
        default: .bytes
        }
        let f = MeasurementFormatter()
        f.locale = foundationLocale
        f.unitOptions = .providedUnit
        f.unitStyle = .medium
        f.numberFormatter = numberFormatter(.decimal, min: 0, max: 1, grouping: true)
        return Self.clean(f.string(from: Measurement(value: decimal(value).doubleValue, unit: u)))
    }

    /// Foundation's month / weekday / day-period names for `format_pattern`.
    private func names() -> String {
        lock.lock()
        defer { lock.unlock() }
        if let cachedNames { return cachedNames }
        let d = DateFormatter()
        d.locale = foundationLocale
        func arr(_ a: [String]?) -> JSONValue { .array((a ?? []).map { .string($0) }) }
        let json = JSONValue.object([
            "months": arr(d.monthSymbols),
            "monthsShort": arr(d.shortMonthSymbols),
            "weekdays": arr(d.weekdaySymbols),
            "weekdaysShort": arr(d.shortWeekdaySymbols),
            "dayPeriods": arr([d.amSymbol, d.pmSymbol]),
        ]).json
        cachedNames = json
        return json
    }

    /// A time's narrow no-break space (ICU 72+, before AM/PM) → a plain space.
    static func clean(_ s: String) -> String {
        s.replacingOccurrences(of: "\u{202F}PM", with: " PM").replacingOccurrences(of: "\u{202F}AM", with: " AM")
    }
}
