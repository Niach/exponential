import Foundation
import ExponentialUICore

/// VAPP-189: THE records query, the core's one evaluator
/// (`@exponential_app/eui/host/query`, eui-core `host::query`,
/// `fixtures/records-query.json`). A binding's URL sugar parses INTO the
/// structured query; `RecordsQuery.value(rows:…)` evaluates it over the rows
/// of whatever store the host has.
public struct RecordsQuery: Sendable, Equatable {
    /// The structured query as the core spells it: `{from, app, mine, user,
    /// where, or, sort, limit, sum, month, group, aggregate, running, before}`.
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
    /// `kind` = the source's own name (`records:<kind>`); a `kind` param is a field filter.
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

    /// VAPP-190: the JSON query form (a package's `queries`, the `q` param:
    /// group, aggregate, running, before, in / between / or) → the query and
    /// every problem with it (`[{path, message}]`; a problem drops the part,
    /// the rest still runs). `kind` wins over `from`.
    public static func parse(json: String, kind: String? = nil) -> (query: RecordsQuery, issues: [JSONValue]) {
        guard let out = try? parseRecordsJson(queryJson: json, kind: kind) else {
            return (RecordsQuery(json: #"{"from":null}"#), [.object(["path": .string(""), "message": .string("a query is JSON")])])
        }
        let parsed = JSONValue.parse(out)
        return (RecordsQuery(json: parsed["query"]?.json ?? #"{"from":null}"#), parsed["issues"]?.array ?? [])
    }

    /// VAPP-190: a date's bucket key (day `2026-10-05`, week `2026-W41`,
    /// month `2026-10`, quarter `2026-Q4`, year `2026`); nil = no date that
    /// precise (or an unknown bucket).
    public static func dateBucket(_ date: String, by bucket: String) -> String? {
        ExponentialUICore.dateBucket(date: date, bucket: bucket)
    }

    /// The value the binding emits over `rows` (every row of the app, any
    /// kind): `.null` without a kind, else `{rows, count, sum?, month?,
    /// groups?, totals?, before?}`.
    /// `me` = the viewer (nil: `mine` matches nothing), `now` = ISO 8601
    /// (the month view's default month), `names` = user id → display name.
    /// VAPP-214: `computed` = the queried kind's computed fields (name →
    /// CEL, `RecordsQuery.computed(records:kind:)`), evaluated on read.
    /// `today` = the host's calendar day in the team zone (`YYYY-MM-DD`):
    /// the month view's `today` and default month; nil = `now`'s UTC day.
    /// VAPP-219 (R5): `refs` = the queried kind's refs
    /// (`RecordsQuery.refs(records:kind:)`), what `expand` follows; `issues`
    /// = identifier → the issue summary `{id, identifier, title, status}` an
    /// `issue` ref inlines (members resolve through `names`).
    /// VAPP-222 (R6): `transitions` = the queried kind's state machine
    /// (`records.<kind>.transitions`, `.null` = none): every row carries
    /// `transitions`, the next states the viewer (`me`, `role`) may take.
    public func value(rows: [RecordRow], me: String?, now: Date = Date(), today: String? = nil, names: [String: String] = [:], computed: [String: String] = [:], refs: JSONValue = .null, issues: [String: JSONValue] = [:], transitions: JSONValue = .null, role: String? = nil) -> JSONValue {
        value(rows: rows, me: me, now: RecordsQuery.iso(now), today: today, names: names, computed: computed, refs: refs, issues: issues, transitions: transitions, role: role)
    }

    /// `value` with `now` as ISO 8601 text.
    public func value(rows: [RecordRow], me: String?, now: String, today: String? = nil, names: [String: String], computed: [String: String] = [:], refs: JSONValue = .null, issues: [String: JSONValue] = [:], transitions: JSONValue = .null, role: String? = nil) -> JSONValue {
        let rowsJson = JSONValue.array(rows.map(\.json)).json
        let context = contextJson(me: me, now: now, today: today, names: names, computed: computed, refs: refs, issues: issues, transitions: transitions, role: role)
        guard let out = try? recordsValueJson(rowsJson: rowsJson, queryJson: json, contextJson: context) else { return .null }
        return JSONValue.parse(out)
    }

    /// VAPP-212: the same value over `store`; the rows stay in Rust, so only
    /// the query crosses the FFI. `overlay` = the host's unsynced writes for
    /// this one query (the store keeps its rows).
    public func value<Source>(store: RecordsStore<Source>, me: String?, now: Date = Date(), today: String? = nil, names: [String: String] = [:], computed: [String: String] = [:], refs: JSONValue = .null, issues: [String: JSONValue] = [:], transitions: JSONValue = .null, role: String? = nil, overlay: RecordsOverlay? = nil) -> JSONValue {
        value(store: store, me: me, now: RecordsQuery.iso(now), today: today, names: names, computed: computed, refs: refs, issues: issues, transitions: transitions, role: role, overlay: overlay)
    }

    /// `value(store:…)` with `now` as ISO 8601 text.
    public func value<Source>(store: RecordsStore<Source>, me: String?, now: String, today: String? = nil, names: [String: String] = [:], computed: [String: String] = [:], refs: JSONValue = .null, issues: [String: JSONValue] = [:], transitions: JSONValue = .null, role: String? = nil, overlay: RecordsOverlay? = nil) -> JSONValue {
        let context = contextJson(me: me, now: now, today: today, names: names, computed: computed, refs: refs, issues: issues, transitions: transitions, role: role)
        let overlayJson = overlay.flatMap { $0.isEmpty ? nil : $0.json }
        guard let out = try? store.handle.valueJson(queryJson: json, contextJson: context, overlayJson: overlayJson) else { return .null }
        return JSONValue.parse(out)
    }

    private func contextJson(me: String?, now: String, today: String?, names: [String: String], computed: [String: String], refs: JSONValue, issues: [String: JSONValue], transitions: JSONValue, role: String?) -> String {
        JSONValue.object([
            "me": me.map(JSONValue.string) ?? .null,
            "now": .string(now),
            "today": today.map(JSONValue.string) ?? .null,
            "names": .object(names.mapValues { .string($0) }),
            "computed": .object(computed.mapValues { .string($0) }),
            "refs": refs,
            "issues": .object(issues),
            "transitions": transitions,
            "role": role.map(JSONValue.string) ?? .null,
        ]).json
    }

    /// The raw JSON value (for a host that compares emits as text).
    public func valueJson(rows: [RecordRow], me: String?, now: String, names: [String: String]) -> String {
        value(rows: rows, me: me, now: now, names: names).json
    }

    /// VAPP-214: a package's computed fields of `kind` (`records.<kind>.computed`,
    /// the string ones); what a host passes as `computed`.
    public static func computed(records: JSONValue?, kind: String?) -> [String: String] {
        guard let kind, let fields = records?[kind]?["computed"]?.object else { return [:] }
        return fields.compactMapValues(\.string)
    }

    /// VAPP-219 (R5): a package's refs of `kind` (`records.<kind>.refs`);
    /// what a host passes as `refs` (`.null` = none).
    public static func refs(records: JSONValue?, kind: String?) -> JSONValue {
        guard let kind, let refs = records?[kind]?["refs"], refs.object != nil else { return .null }
        return refs
    }

    /// VAPP-222 (R6): a package's state machine of `kind`
    /// (`records.<kind>.transitions`); what a host passes as `transitions`.
    public static func transitions(records: JSONValue?, kind: String?) -> JSONValue {
        guard let kind, let t = records?[kind]?["transitions"], t.object != nil else { return .null }
        return t
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

private func recordsDeltaJson(upserts: [RecordRow], deletes: [String]) -> String {
    JSONValue.object([
        "upserts": .array(upserts.map(\.json)),
        "deletes": .array(deletes.map(JSONValue.string)),
    ]).json
}

/// VAPP-212: a host's unsynced writes laid over one query: `deletes` hide
/// rows, an upsert replaces a known id in place, a new id joins the end.
public struct RecordsOverlay: Sendable, Equatable {
    public var upserts: [RecordRow]
    public var deletes: [String]

    public init(upserts: [RecordRow] = [], deletes: [String] = []) {
        self.upserts = upserts
        self.deletes = deletes
    }

    var isEmpty: Bool { upserts.isEmpty && deletes.isEmpty }

    var json: String { recordsDeltaJson(upserts: upserts, deletes: deletes) }
}

/// VAPP-212: one app's rows on the Rust side, newest first (`createdAt`
/// descending, then `id`). `sync` takes the host's synced rows as it reads
/// them (any `Equatable` type) and pushes only what changed, so `row` runs only
/// for new and changed ones. When several bindings share a store, call `sync`
/// then `value(store:…)` on one actor to keep a binding's rows and value together.
public final class RecordsStore<Source: Equatable>: @unchecked Sendable {
    let handle = RecordStore.newestFirst()
    private let lock = NSLock()
    private var last: [String: Source] = [:]

    public init() {}

    /// How many rows the store holds.
    public var count: Int { Int(handle.count()) }

    public func sync(_ items: [Source], id: (Source) -> String, row: (Source) -> RecordRow) {
        lock.lock()
        defer { lock.unlock() }
        var seen = [String: Source](minimumCapacity: items.count)
        var upserts: [RecordRow] = []
        for item in items {
            let key = id(item)
            seen[key] = item
            if last[key] != item { upserts.append(row(item)) }
        }
        let deletes = last.keys.filter { seen[$0] == nil }
        if upserts.isEmpty && deletes.isEmpty { return }
        guard (try? handle.applyJson(deltaJson: recordsDeltaJson(upserts: upserts, deletes: deletes))) != nil else { return }
        last = seen
    }
}
