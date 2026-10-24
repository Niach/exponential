import Foundation

/// VAPP-122: app RECORDS ×4 (`packages/domain-contract/fixtures/app-records.json`
/// is the spec, TS reference `src/app-records.ts`; desktop
/// `domain::app_records`, Android `AppRecords.kt`). The `exp:records?…`
/// binding's query grammar and the value it emits over the synced
/// `app_records` rows. `AppRecordsTests` replays `queryCases`, `valueCases`
/// and `countLabelCases`. Values are `CardJSON` (keys sorted when encoded).
public enum AppRecords {
    // MARK: - Fixture constants

    public static let reserved: Set<String> = ["kind", "app", "mine", "user", "sort", "order", "limit", "sum", "by", "month"]
    public static let sortDefault = "createdAt"
    public static let orderDefault = "desc"
    public static let limitDefault = 100
    public static let limitMax = 500
    public static let operators = ["eq", "gt", "gte", "lt", "lte"]
    public static let rowKeys = ["id", "createdAt", "updatedAt", "createdBy", "author"]
    public static let weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    public static let months = [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ]

    private static let rangeOps: Set<String> = ["gt", "gte", "lt", "lte"]

    // MARK: - The query

    public struct Filter: Equatable, Sendable {
        public let field: String
        /// eq | gt | gte | lt | lte
        public let op: String
        public let value: String

        public init(field: String, op: String, value: String) {
            self.field = field
            self.op = op
            self.value = value
        }
    }

    public struct Query: Equatable, Sendable {
        public var kind: String?
        /// Another app's slug; nil = the hosting app.
        public var app: String?
        public var mine: Bool
        public var user: String?
        public var sort: String
        /// asc | desc
        public var order: String
        public var limit: Int
        /// By field, then operator.
        public var filters: [Filter]
        public var sum: String?
        public var by: String?
        public var month: String?
    }

    private static func trimmed(_ value: String?) -> String? {
        guard let value else { return nil }
        let t = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return t.isEmpty ? nil : t
    }

    private static func isMonthKey(_ value: String) -> Bool {
        let scalars = Array(value.unicodeScalars)
        guard scalars.count == 7, scalars[4] == "-" else { return false }
        for (i, s) in scalars.enumerated() where i != 4 {
            guard ("0"..."9").contains(s) else { return false }
        }
        let month = Int(String(value.suffix(2))) ?? 0
        return (1...12).contains(month)
    }

    /// The `exp:records` query params → the query (TS `parseRecordsQuery`).
    /// Unknown keys are data-field filters.
    public static func parseRecordsQuery(_ params: [String: String]) -> Query {
        let limitRaw = params["limit"].flatMap(jsNumber)
        let limit: Int = if let limitRaw, limitRaw.isFinite, limitRaw == limitRaw.rounded(), limitRaw > 0 {
            limitRaw >= Double(limitMax) ? limitMax : Int(limitRaw)
        } else {
            limitDefault
        }
        var filters: [Filter] = []
        for key in params.keys.sorted(by: scalarLess) where !reserved.contains(key) {
            let value = params[key] ?? ""
            if let dot = key.lastIndex(of: "."), dot > key.startIndex,
               rangeOps.contains(String(key[key.index(after: dot)...])) {
                filters.append(Filter(field: String(key[..<dot]), op: String(key[key.index(after: dot)...]), value: value))
            } else if !key.isEmpty {
                filters.append(Filter(field: key, op: "eq", value: value))
            }
        }
        filters.sort { a, b in a.field != b.field ? scalarLess(a.field, b.field) : scalarLess(a.op, b.op) }
        let order = params["order"]
        return Query(
            kind: trimmed(params["kind"]),
            app: trimmed(params["app"]),
            mine: params["mine"] == "1" || params["mine"] == "true",
            user: trimmed(params["user"]),
            sort: trimmed(params["sort"]) ?? sortDefault,
            order: order == "asc" ? "asc" : order == "desc" ? "desc" : orderDefault,
            limit: limit,
            filters: filters,
            sum: trimmed(params["sum"]),
            by: trimmed(params["by"]),
            month: params["month"].flatMap { isMonthKey($0) ? $0 : nil }
        )
    }

    // MARK: - The value

    /// One synced `app_records` row as the host reads it.
    public struct Row: Sendable {
        public let id: String
        public let kind: String
        public let data: [String: CardJSON]
        public let createdBy: String?
        public let createdAt: String
        public let updatedAt: String

        public init(id: String, kind: String, data: [String: CardJSON], createdBy: String?, createdAt: String, updatedAt: String) {
            self.id = id
            self.kind = kind
            self.data = data
            self.createdBy = createdBy
            self.createdAt = createdAt
            self.updatedAt = updatedAt
        }

        /// From a synced row (`data` = its JSON text; a non-object = {}); the
        /// timestamps as the web host's `toISOString` (the same ×4).
        public init(_ entity: AppRecordEntity) {
            let parsed = (try? JSONSerialization.jsonObject(with: Data(entity.data.utf8), options: [.fragmentsAllowed]))
            self.init(
                id: entity.id,
                kind: entity.kind,
                data: CardJSON(any: parsed)?.objectValue ?? [:],
                createdBy: entity.createdBy,
                createdAt: AppRecords.isoTimestamp(entity.createdAt),
                updatedAt: AppRecords.isoTimestamp(entity.updatedAt)
            )
        }
    }

    private nonisolated(unsafe) static let isoMillis: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    /// A wire timestamp (the Postgres text Electric syncs, or ISO) as
    /// JavaScript's `toISOString()` (`2026-10-05T18:00:00.000Z`), the web
    /// host's form (Android `isoTimestamp`, desktop `iso_millis` alike);
    /// unparseable text stays as is.
    public static func isoTimestamp(_ raw: String) -> String {
        WireTimestamps.parse(raw).map(isoMillis.string(from:)) ?? raw
    }

    /// JS `Number(text)` for a trimmed, non-empty string (nil = NaN).
    private static func jsNumber(_ raw: String) -> Double? {
        let t = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty else { return 0 }
        // Swift accepts "nan"/"inf"/hex floats JS does not; JS accepts "Infinity".
        let lower = t.lowercased()
        if lower.contains("nan") || lower.contains("inf") || lower.contains("0x") || lower.contains("p") {
            return t == "Infinity" || t == "+Infinity" ? .infinity : t == "-Infinity" ? -.infinity : nil
        }
        return Double(t)
    }

    private static func num(_ value: CardJSON?) -> Double? {
        switch value {
        case let .number(n): return n.isFinite ? n : nil
        case let .string(s):
            guard !s.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, let n = jsNumber(s) else { return nil }
            return n.isFinite ? n : nil
        default: return nil
        }
    }

    /// JS `String(n)` for the numbers records carry.
    private static func numberText(_ n: Double) -> String {
        if n == n.rounded(), abs(n) < 1e21 {
            if abs(n) < 9.007_199_254_740_992e15 { return String(Int64(n)) }
        }
        return String(n)
    }

    private static func text(_ value: CardJSON?) -> String {
        switch value {
        case nil, .null?: return ""
        case let .string(s)?: return s
        case let .bool(b)?: return b ? "true" : "false"
        case let .number(n)?: return numberText(n)
        case let v?: return v.compactString
        }
    }

    private static func scalarLess(_ a: String, _ b: String) -> Bool {
        a.unicodeScalars.lexicographicallyPrecedes(b.unicodeScalars) { $0.value < $1.value }
    }

    private static func scalarCompare(_ a: String, _ b: String) -> Int {
        scalarLess(a, b) ? -1 : scalarLess(b, a) ? 1 : 0
    }

    /// Numbers when both parse, else text (unicode scalars, no locale).
    private static func compare(_ a: CardJSON?, _ b: CardJSON?) -> Int {
        if let na = num(a), let nb = num(b) { return na < nb ? -1 : na > nb ? 1 : 0 }
        return scalarCompare(text(a), text(b))
    }

    private static func matches(_ filter: Filter, _ value: CardJSON?) -> Bool {
        if filter.op == "eq" { return text(value) == filter.value }
        let c = compare(value, .string(filter.value))
        switch filter.op {
        case "gt": return c > 0
        case "gte": return c >= 0
        case "lt": return c < 0
        default: return c <= 0
        }
    }

    private static func field(_ row: Row, _ name: String) -> CardJSON? {
        switch name {
        case "createdAt": return .string(row.createdAt)
        case "updatedAt": return .string(row.updatedAt)
        case "id": return .string(row.id)
        case "createdBy": return row.createdBy.map(CardJSON.string) ?? .null
        default: return row.data[name]
        }
    }

    private static func monthParts(_ key: String) -> (year: Int, month: Int) {
        let parts = key.split(separator: "-").map { Int($0) ?? 0 }
        return (parts.first ?? 0, parts.count > 1 ? parts[1] : 1)
    }

    private static func pad2(_ n: Int) -> String { n < 10 ? "0\(n)" : String(n) }

    /// `YYYY-MM` ± `by` months.
    public static func shiftMonth(_ key: String, by: Int) -> String {
        let (y, m) = monthParts(key)
        let index = y * 12 + (m - 1) + by
        let year = Int((Double(index) / 12).rounded(.down))
        let month = index - year * 12 + 1
        return "\(year)-\(pad2(month))"
    }

    public static func monthLabel(_ key: String) -> String {
        let (y, m) = monthParts(key)
        return "\(months[max(0, min(11, m - 1))]) \(y)"
    }

    /// One day of the month view.
    public struct Day: Equatable, Sendable {
        public let date: String
        public let day: Int
        public let weekday: String
        public var count: Int
        public var sum: Double
    }

    private static let utc: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }()

    /// Every day of `key` (`YYYY-MM`), empty.
    public static func monthDays(_ key: String) -> [Day] {
        let (y, m) = monthParts(key)
        guard let first = utc.date(from: DateComponents(year: y, month: m, day: 1)),
              let range = utc.range(of: .day, in: .month, for: first)
        else { return [] }
        let firstWeekday = utc.component(.weekday, from: first) - 1 // 0 = Sunday
        return range.map { day in
            Day(date: "\(key)-\(pad2(day))", day: day, weekday: weekdays[(firstWeekday + day - 1) % 7], count: 0, sum: 0)
        }
    }

    /// Sums like 0.1 + 0.2 stay readable (JS `Math.round(n * 1e6) / 1e6`).
    private static func round6(_ n: Double) -> Double {
        (n * 1_000_000 + 0.5).rounded(.down) / 1_000_000
    }

    private static func monthKeyOf(_ now: Date) -> String {
        let parts = utc.dateComponents([.year, .month], from: now)
        return "\(parts.year ?? 1970)-\(pad2(parts.month ?? 1))"
    }

    /// `now`'s UTC date, `YYYY-MM-DD` (the month view's `today`).
    private static func dayKey(of now: Date) -> String {
        let parts = utc.dateComponents([.year, .month, .day], from: now)
        return "\(parts.year ?? 1970)-\(pad2(parts.month ?? 1))-\(pad2(parts.day ?? 1))"
    }

    /// The value `exp:records` emits for `query` over `rows` (every synced
    /// row of the app, any kind): nil without a kind; else
    /// {rows, count, sum?, month?} (TS `recordsValue`). `me` = the viewer
    /// (nil: `mine` matches nothing), `names` = user id → display name.
    public static func recordsValue(
        rows: [Row], query: Query, me: String?, now: Date, names: [String: String]
    ) -> CardJSON? {
        guard let kind = query.kind else { return nil }
        let monthKey: String? = query.by != nil ? (query.month ?? monthKeyOf(now)) : nil
        var matched = rows.filter { row in
            if row.kind != kind { return false }
            if query.mine, me == nil || row.createdBy != me { return false }
            if let user = query.user, row.createdBy != user { return false }
            if let monthKey, let by = query.by, !text(row.data[by]).hasPrefix(monthKey) { return false }
            return query.filters.allSatisfy { matches($0, field(row, $0.field)) }
        }
        matched.sort { a, b in
            let c = compare(field(a, query.sort), field(b, query.sort))
            if c != 0 { return query.order == "asc" ? c < 0 : c > 0 }
            return scalarLess(a.id, b.id)
        }
        var out: [String: CardJSON] = [
            "rows": .array(matched.prefix(query.limit).map { row -> CardJSON in
                var object = row.data
                object["id"] = .string(row.id)
                object["createdAt"] = .string(row.createdAt)
                object["updatedAt"] = .string(row.updatedAt)
                object["createdBy"] = row.createdBy.map(CardJSON.string) ?? .null
                object["author"] = .string(row.createdBy.flatMap { names[$0] } ?? "")
                return .object(object)
            }),
            "count": .number(Double(matched.count)),
        ]
        if let sum = query.sum {
            out["sum"] = .number(round6(matched.reduce(0) { $0 + (num($1.data[sum]) ?? 0) }))
        }
        if let monthKey, let by = query.by {
            var days = monthDays(monthKey)
            var index: [String: Int] = [:]
            for (i, day) in days.enumerated() { index[day.date] = i }
            for row in matched {
                guard let i = index[String(text(row.data[by]).prefix(10))] else { continue }
                days[i].count += 1
                if let sum = query.sum { days[i].sum = round6(days[i].sum + (num(row.data[sum]) ?? 0)) }
            }
            out["month"] = .object([
                "key": .string(monthKey),
                "label": .string(monthLabel(monthKey)),
                "prev": .string(shiftMonth(monthKey, by: -1)),
                "next": .string(shiftMonth(monthKey, by: 1)),
                "today": .string(dayKey(of: now)),
                "days": .array(days.map { day -> CardJSON in
                    .object([
                        "date": .string(day.date),
                        "day": .number(Double(day.day)),
                        "weekday": .string(day.weekday),
                        "count": .number(Double(day.count)),
                        "sum": .number(day.sum),
                    ])
                }),
                "labels": .array(days.map { .string(String($0.day)) }),
                "counts": .array(days.map { .number(Double($0.count)) }),
                "sums": .array(days.map { .number($0.sum) }),
            ])
        }
        return .object(out)
    }

    // MARK: - Settings → Apps → the app

    private static func grouped(_ n: Int) -> String {
        let digits = String(abs(n))
        var out = ""
        for (i, ch) in digits.enumerated() {
            if i > 0, (digits.count - i) % 3 == 0 { out.append(",") }
            out.append(ch)
        }
        return n < 0 ? "-" + out : out
    }

    /// "1,240 records", "1,240 of 25,000 records" (max = the team's quota,
    /// nil = unlimited), "No records".
    public static func recordsCountLabel(count: Int, max: Int?) -> String {
        if count == 0 { return Apps.Copy.recordsNone }
        let n = grouped(count)
        guard let max else {
            return count == 1 ? Apps.Copy.recordsOne : Apps.Copy.records.replacingOccurrences(of: "{n}", with: n)
        }
        return Apps.Copy.recordsOf
            .replacingOccurrences(of: "{n}", with: n)
            .replacingOccurrences(of: "{max}", with: grouped(max))
    }
}
