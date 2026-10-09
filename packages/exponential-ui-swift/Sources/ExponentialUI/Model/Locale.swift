import SwiftUI
import ExponentialUICore

/// The surface locale (round-1 contract §4): the built-in string table,
/// formatting in the SURFACE locale (the platform's ICU: Foundation), and
/// the data the core owns (week start from its CLDR table, never
/// `Calendar.current`). The natives read these.
extension SurfaceModel {
    /// The surface locale (BCP 47, `en-US` default).
    var surfaceLocale: String { settings.locale.isEmpty ? "en-US" : settings.locale }

    /// A built-in string (`catalog/strings.json` with the host's overrides)
    /// with `{name}` placeholders filled through the core's `formatString`.
    func builtinString(_ id: String, _ params: [String: JSONValue] = [:]) -> String {
        let overrides = appliedSettings?.stringsJson ?? ""
        if stringsKey != overrides || stringTable.isEmpty {
            let table = (try? stringTableJson(overridesJson: overrides.isEmpty ? nil : overrides)).map(JSONValue.parse)?.object ?? [:]
            stringTable = table.compactMapValues(\.string)
            stringsKey = overrides
        }
        let template = stringTable[id] ?? id
        if params.isEmpty { return template }
        return (try? formatString(template: template, paramsJson: JSONValue.object(params).json)) ?? template
    }

    /// `n` with `precision` fraction digits (nil = up to 2), grouped, in the
    /// surface locale: NumberField and Table number cells.
    func formatNumber(_ n: Double, precision: Int?) -> String {
        let locale = surfaceLocale
        let key = "\(locale)|\(precision.map(String.init) ?? "auto")"
        let f: NumberFormatter
        if let hit = numberFormatters[key] {
            f = hit
        } else {
            f = NumberFormatter()
            f.locale = Locale(identifier: locale)
            f.numberStyle = .decimal
            f.usesGroupingSeparator = true
            f.roundingMode = .halfUp
            f.minimumFractionDigits = precision ?? 0
            f.maximumFractionDigits = precision ?? 2
            numberFormatters[key] = f
        }
        return f.string(from: NSNumber(value: n)) ?? String(n)
    }

    /// Typed number text → a number: the surface locale's notation first
    /// (`1.234,5` in `de`), then the plain JS notation.
    func parseNumber(_ text: String) -> Double? {
        let t = text.trimmingCharacters(in: .whitespaces)
        if t.isEmpty { return nil }
        let f = NumberFormatter()
        f.locale = Locale(identifier: surfaceLocale)
        f.numberStyle = .decimal
        f.isLenient = true
        if let n = f.number(from: t) { return n.doubleValue }
        return Double(t)
    }

    /// An ISO date (`yyyy-mm-dd`, a date-time) as the surface locale's
    /// medium date (`Oct 7, 2026`, `7. Okt. 2026`), UTC so it never shifts.
    func formatDate(_ iso: String) -> String? {
        guard let date = DateModel.date(fromISO: String(iso.prefix(10))) else { return nil }
        let locale = surfaceLocale
        let f: DateFormatter
        if let hit = dateFormatters[locale] {
            f = hit
        } else {
            f = DateFormatter()
            f.locale = Locale(identifier: locale)
            f.timeZone = TimeZone(identifier: "UTC")
            f.dateStyle = .medium
            f.timeStyle = .none
            dateFormatters[locale] = f
        }
        return f.string(from: date)
    }

    /// The week's first day for the surface locale (0 = Sunday): the core's
    /// CLDR table (`weekStart`).
    var surfaceWeekStart: Int { Int(weekStart(locale: surfaceLocale)) }

    /// A NumberField's precision: `precision`, else the step's decimals
    /// (the core's `number_step_precision`).
    static func numberPrecision(_ props: Props) -> Int {
        if let p = props.num("precision") { return Int(max(0, min(100, p.rounded(.down)))) }
        let step = props.num("step").flatMap { $0 != 0 && $0.isFinite ? $0 : nil } ?? 1
        let s = String(step)
        guard let dot = s.firstIndex(of: "."), !s.contains("e") else { return 0 }
        let decimals = s[s.index(after: dot)...]
        return decimals == "0" ? 0 : decimals.count
    }

    /// The text a NumberField shows: its `value` in the surface locale at
    /// its precision ("" when empty). The core's `text` is the `en` form.
    func numberFieldText(owner: Props) -> String {
        guard let v = owner["value"], !v.isNull, v.displayText != "" else { return "" }
        guard let n = v.number ?? Double(v.displayText) else { return v.displayText }
        return formatNumber(n, precision: Self.numberPrecision(owner))
    }

    /// The live parts of a native `owner` with this part name, in slot order.
    func liveParts(_ owner: String, _ part: String) -> [NodeInfo] {
        nodes.filter { !$0.removed && $0.owner == owner && $0.part == part }
    }
}
