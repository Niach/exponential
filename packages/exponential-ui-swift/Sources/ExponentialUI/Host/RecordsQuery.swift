import Foundation
import ExponentialUICore

/// VAPP-189: THE records query, the core's one evaluator
/// (`@exponential_app/eui/host/query`, eui-core `host::query`,
/// `fixtures/records-query.json`). A binding's URL sugar parses INTO the
/// structured query; `RecordsQuery.value(rows:…)` evaluates it over the rows
/// of whatever store the host has.
public struct RecordsQuery: Sendable, Equatable {
    /// The structured query as the core spells it: `{from, app, mine, user,
    /// where, sort, limit, sum, month}`.
    public let structured: JSONValue
    /// The same, as the core's JSON (what `value` hands back to it).
    public let json: String

    init(json: String) {
        self.json = json
        self.structured = JSONValue.parse(json)
    }

    /// The kind (nil = no kind: the value is null).
    public var from: String? { structured["from"]?.string }
    /// Another app's slug; the HOST decides whose rows that is.
    public var app: String? { structured["app"]?.string }

    /// Query params (as `BindingSource.params` gives them) → the query.
    /// `kind` = a neutral source's own name, which wins over a `kind` param.
    public static func parse(params: [String: String], kind: String? = nil) -> RecordsQuery {
        let paramsJson = JSONValue.object(params.mapValues { .string($0) }).json
        let json = (try? parseRecordsQueryJson(paramsJson: paramsJson, kind: kind)) ?? #"{"from":null}"#
        return RecordsQuery(json: json)
    }

    /// `records:<kind>?…` → the query; nil = not a records source.
    public static func parse(source: String) -> RecordsQuery? {
        parseRecordsSourceJson(source: source).map(RecordsQuery.init(json:))
    }

    /// `records:<kind>?…` (parsed) → the query; nil = another scheme.
    public static func parse(source: BindingSource) -> RecordsQuery? {
        source.scheme == "records" ? parse(params: source.params, kind: source.name) : nil
    }

    /// The value the binding emits over `rows` (every row of the app, any
    /// kind): `.null` without a kind, else `{rows, count, sum?, month?}`.
    /// `me` = the viewer (nil: `mine` matches nothing), `now` = ISO 8601
    /// (the month view's default month), `names` = user id → display name.
    public func value(rows: [RecordRow], me: String?, now: Date = Date(), names: [String: String] = [:]) -> JSONValue {
        value(rows: rows, me: me, now: RecordsQuery.iso(now), names: names)
    }

    /// `value` with `now` as ISO 8601 text.
    public func value(rows: [RecordRow], me: String?, now: String, names: [String: String]) -> JSONValue {
        let rowsJson = JSONValue.array(rows.map(\.json)).json
        let context = JSONValue.object([
            "me": me.map(JSONValue.string) ?? .null,
            "now": .string(now),
            "names": .object(names.mapValues { .string($0) }),
        ]).json
        guard let out = try? recordsValueJson(rowsJson: rowsJson, queryJson: json, contextJson: context) else { return .null }
        return JSONValue.parse(out)
    }

    /// The raw JSON value (for a host that compares emits as text).
    public func valueJson(rows: [RecordRow], me: String?, now: String, names: [String: String]) -> String {
        value(rows: rows, me: me, now: now, names: names).json
    }

    private nonisolated(unsafe) static let isoMillis: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    static func iso(_ date: Date) -> String { isoMillis.string(from: date) }
}

/// One stored record as a host hands it to the evaluator.
public struct RecordRow: Sendable, Equatable {
    public var id: String
    public var kind: String
    /// The record's data (an object; anything else reads as `{}`).
    public var data: JSONValue
    public var createdBy: String?
    /// Whose record it is (#VAPP-187); nil = `createdBy`.
    public var authorId: String?
    public var createdAt: String
    public var updatedAt: String

    public init(id: String, kind: String, data: JSONValue, createdBy: String?, authorId: String? = nil, createdAt: String, updatedAt: String) {
        self.id = id
        self.kind = kind
        self.data = data
        self.createdBy = createdBy
        self.authorId = authorId
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    var json: JSONValue {
        .object([
            "id": .string(id),
            "kind": .string(kind),
            "data": data.object.map(JSONValue.object) ?? .object([:]),
            "createdBy": createdBy.map(JSONValue.string) ?? .null,
            "authorId": authorId.map(JSONValue.string) ?? .null,
            "createdAt": .string(createdAt),
            "updatedAt": .string(updatedAt),
        ])
    }
}
