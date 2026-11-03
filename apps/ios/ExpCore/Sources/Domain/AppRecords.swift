import Foundation

/// VAPP-122: app RECORDS ×4 (`packages/domain-contract/fixtures/app-records.json`
/// is the spec, TS reference `src/app-records.ts`; desktop
/// `domain::app_records`, Android `AppRecords.kt`). VAPP-189: the
/// `records:<kind>?…` query and its value are eUI's ONE evaluator (the SDK's
/// `RecordsQuery` over the core, packages/exponential-ui
/// fixtures/records-query.json); what stays here is Exponential's own:
/// ownership + on-behalf (VAPP-187/188), the count label and (VAPP-178) the
/// editor's DATA tab. `AppRecordsTests` replays their cases. Values are
/// `CardJSON` (keys sorted when encoded).
public enum AppRecords {
    // MARK: - Fixture constants

    public static let rowKeys = ["id", "createdAt", "updatedAt", "createdBy", "authorId", "author"]

    // MARK: - The rows

    /// One synced `app_records` row as the host reads it.
    public struct Row: Sendable {
        public let id: String
        public let kind: String
        public let data: [String: CardJSON]
        public let createdBy: String?
        /// VAPP-187: whose record it is (nil on a legacy row = `createdBy`).
        public let authorId: String?
        public let createdAt: String
        public let updatedAt: String

        public init(
            id: String, kind: String, data: [String: CardJSON], createdBy: String?, authorId: String? = nil,
            createdAt: String, updatedAt: String
        ) {
            self.id = id
            self.kind = kind
            self.data = data
            self.createdBy = createdBy
            self.authorId = authorId
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
                authorId: entity.authorId,
                createdAt: AppRecords.isoTimestamp(entity.createdAt),
                updatedAt: AppRecords.isoTimestamp(entity.updatedAt)
            )
        }
    }

    // MARK: - Ownership (VAPP-187)

    /// A record's AUTHOR (`authorId ?? createdBy`): whose record it is, what
    /// `mine` / `user=` and the per-member views read (TS `recordAuthor`).
    public static func recordAuthor(_ row: Row) -> String? {
        row.authorId ?? row.createdBy
    }

    /// The fixture `ownership` block: the default per-kind policy
    /// (`author` | `member` per op) and the server's FORBIDDEN refusal.
    public enum Ownership {
        public static let policy: [String: String] = ["update": "author", "delete": "author"]
        public static let refusal = "Only the record's author or a team owner can change it"
    }

    /// May `viewer` (with team `role`; nil = not a member) `op` (`update` |
    /// `delete`) the record? Policy `author` = the author or a team owner
    /// (TS `canChangeRecord`, fixture `ownership.canChangeCases`).
    public static func canChangeRecord(_ row: Row, viewer: String?, role: String?, op: String = "update") -> Bool {
        guard let viewer, let role else { return false }
        if role == "owner" { return true }
        if Ownership.policy[op] == "member" { return true }
        guard let author = recordAuthor(row) else { return false }
        return author == viewer
    }

    // MARK: - Per-kind policy + app access (eApps R3)

    /// eApps R3: who may read, create, update and delete a kind's records
    /// under the member's app access: a hand port of eUI's
    /// `src/host/record-policy.ts` (packages/exponential-ui
    /// fixtures/record-policy.json is the spec; `AppRecordsTests` replays it).
    public enum Policy {
        public static let ops = ["read", "create", "update", "delete"]
        public static let levels = ["member", "author", "owners"]
        public static let access = ["none", "read", "write"]
        public static let defaults: [String: String] = ["read": "member", "create": "member", "update": "author", "delete": "author"]

        public enum Copy {
            public static let none = "You have no access to this app"
            public static let readOnly = "You have read-only access to this app"
            public static let author: [String: String] = [
                "read": "Only the record's author or a team owner can read it",
                "update": "Only the record's author or a team owner can change it",
                "delete": "Only the record's author or a team owner can change it",
            ]
            public static let owners: [String: String] = [
                "read": "Only team owners can read these records",
                "create": "Only team owners can add these records",
                "update": "Only team owners can change these records",
                "delete": "Only team owners can change these records",
            ]
        }

        /// A kind's effective policy (its `policy` over the defaults; an
        /// invalid entry keeps the default). `kind` = the package's
        /// `records.<kind>` object.
        public static func effective(_ kind: CardJSON?) -> [String: String] {
            var out = defaults
            guard let policy = kind?.objectValue?["policy"]?.objectValue else { return out }
            for op in ops {
                guard case let .string(level)? = policy[op], levels.contains(level), !(op == "create" && level == "author") else { continue }
                out[op] = level
            }
            return out
        }

        /// The policy of `kind` in a package (`records.<kind>`).
        public static func of(package: CardJSON?, kind: String) -> [String: String] {
            effective(package?.objectValue?["records"]?.objectValue?[kind])
        }

        /// Why `viewer` (team `role`, nil = not a member; app `access`, nil =
        /// write) may NOT `op` a record (`row` for read/update/delete), or nil.
        public static func refusal(_ policy: [String: String], op: String, viewer: String?, role: String?, access: String? = nil, row: Row? = nil) -> String? {
            let access = access ?? "write"
            guard viewer != nil, role != nil, access != "none" else { return Copy.none }
            if access == "read", op != "read" { return Copy.readOnly }
            if role == "owner" { return nil }
            let level = policy[op] ?? defaults[op] ?? "member"
            if level == "member" { return nil }
            if level == "owners" || op == "create" { return Copy.owners[op] }
            if let row, let author = recordAuthor(row), author == viewer { return nil }
            return Copy.author[op]
        }

        public static func allowed(_ policy: [String: String], op: String, viewer: String?, role: String?, access: String? = nil, row: Row? = nil) -> Bool {
            refusal(policy, op: op, viewer: viewer, role: role, access: access, row: row) == nil
        }
    }

    // MARK: - On-behalf writes (VAPP-188)

    /// The fixture `onBehalf` block: the server's refusals (FORBIDDEN /
    /// BAD_REQUEST) of a write naming another member as its author.
    public enum OnBehalf {
        public static let refusal = "Only a team owner can write for another member"
        public static let notMember = "The author must be a member of the team"
    }

    /// `writeAsVerdict`'s answer.
    public enum WriteAsVerdict: String, Equatable, Sendable {
        case ok
        case forbidden
        case notMember
    }

    /// May `viewer` (team `role`; nil = not a member) write a record as
    /// `author` (nil = as themself)? Another member = owners only, and the
    /// author must be one of `members` (TS `writeAsVerdict`, `writeAsCases`).
    public static func writeAsVerdict(viewer: String?, role: String?, author: String?, members: [String]) -> WriteAsVerdict {
        guard let viewer, role != nil else { return .forbidden }
        guard let author, author != viewer else { return .ok }
        if role != "owner" { return .forbidden }
        return members.contains(author) ? .ok : .notMember
    }

    /// An imported record's author (TS `importAuthor`, `importCases`):
    /// absent, the importer or a stranger = the importer; another member =
    /// them when the importer is an owner, else the importer too (VAPP-192:
    /// a member importing a teammate's export keeps the rows as their own).
    public static func importAuthor(importer: String, role: String?, author: String?, members: [String]) -> String? {
        guard let author, author != importer, members.contains(author) else { return importer }
        return role == "owner" ? author : importer
    }

    /// The `by` cell (TS `recordByline`, `dataTab.bylineCases`): the
    /// author's name (else `fallback` = `Apps.Copy.byAgent`), `{writer} for
    /// {author}` when someone else with a name wrote it.
    public static func recordByline(_ row: Row, names: [String: String], fallback: String) -> String {
        let authorId = recordAuthor(row)
        let named = authorId.flatMap { names[$0] } ?? ""
        let author = named.isEmpty ? fallback : named
        var writer = ""
        if let createdBy = row.createdBy, createdBy != authorId {
            writer = names[createdBy] ?? ""
        }
        if writer.isEmpty { return author }
        return DataTab.Copy.byFor
            .replacingOccurrences(of: "{writer}", with: writer)
            .replacingOccurrences(of: "{author}", with: author)
    }

    /// One option of the owner's Author picker: a member and their name.
    public struct AuthorOption: Equatable, Sendable {
        public let id: String
        public let name: String

        public init(id: String, name: String) {
            self.id = id
            self.name = name
        }
    }

    /// The Author picker's members by name (case-insensitive, ties by id;
    /// TS `authorOptions`, `dataTab.authorOptionCases`).
    public static func authorOptions(_ users: [AuthorOption]) -> [AuthorOption] {
        users.sorted { a, b in
            let x = a.name.lowercased()
            let y = b.name.lowercased()
            if x != y { return scalarLess(x, y) }
            return scalarLess(a.id, b.id)
        }
    }

    /// The picker's default (the record's author on edit, the viewer on
    /// create) and what Save sends (the pick when it differs from that base,
    /// else nil = omit; TS `formAuthor`, `dataTab.formAuthorCases`).
    public static func formAuthor(viewer: String?, record: Row?, picked: String?) -> FormAuthor {
        let base: String?
        if let record {
            base = recordAuthor(record)
        } else {
            base = viewer
        }
        var send: String?
        if let picked, picked != base { send = picked }
        return FormAuthor(defaultAuthor: base ?? viewer, send: send)
    }

    /// `formAuthor`'s answer: the picker's default and what Save sends.
    public struct FormAuthor: Equatable, Sendable {
        public let defaultAuthor: String?
        public let send: String?

        public init(defaultAuthor: String?, send: String?) {
            self.defaultAuthor = defaultAuthor
            self.send = send
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

    // MARK: - The Data tab (VAPP-178, fixture `dataTab`)

    /// The editor's records browser and form (TS `APP_DATA_TAB`).
    public enum DataTab {
        public enum Copy {
            /// `{kind} · {n}`.
            public static let kindOption = "{kind} · {n}"
            public static let addRecord = "Add record"
            public static let edit = "Edit"
            public static let delete = "Delete"
            public static let deleteTitle = "Delete this record?"
            public static let deleteBody = "The app stops showing it."
            public static let by = "by"
            public static let updated = "updated"
            public static let empty = "No records yet"
            public static let noKinds = "This app keeps no records"
            public static let emptyTitle = "—"
            /// `New {kind}`.
            public static let newTitle = "New {kind}"
            /// `Edit {kind}`.
            public static let editTitle = "Edit {kind}"
            public static let save = "Save"
            public static let cancel = "Cancel"
            public static let yes = "Yes"
            public static let no = "No"
            public static let none = "None"
            public static let optional = "optional"
            public static let listHint = "Comma-separated"
            /// VAPP-188: the owner's Author field.
            public static let author = "Author"
            /// VAPP-188: an on-behalf row's `by` (`{writer} for {author}`).
            public static let byFor = "{writer} for {author}"

            public static func kindOption(_ kind: String, count: Int) -> String {
                kindOption
                    .replacingOccurrences(of: "{kind}", with: kind)
                    .replacingOccurrences(of: "{n}", with: String(count))
            }

            public static func newTitle(_ kind: String) -> String {
                newTitle.replacingOccurrences(of: "{kind}", with: kind)
            }

            public static func editTitle(_ kind: String) -> String {
                editTitle.replacingOccurrences(of: "{kind}", with: kind)
            }
        }

        /// The wide table's fixed columns (pt); phones draw no table.
        public enum Columns {
            public static let byWidth: Double = 96
            public static let updatedWidth: Double = 88
            public static let menuWidth: Double = 40
            public static let minWidth: Double = 64
            public static let gap: Double = 8
            /// A field type (or a string's format) → its fixed width.
            public static let typeWidths: [String: Double] = [
                "date": 104, "time": 64, "date-time": 152, "number": 64, "integer": 64, "boolean": 56,
            ]
        }

        /// A string `format` → the text field's placeholder.
        public static let placeholders: [String: String] = [
            "date": "YYYY-MM-DD",
            "time": "HH:MM",
            "date-time": "YYYY-MM-DDTHH:MM",
        ]

        public enum TestIds {
            public static let kind = "app-data-kind"
            public static let add = "app-data-add"
            public static let row = "app-data-row"
            public static let menu = "app-data-row-menu"
            public static let form = "app-data-form"
            public static let save = "app-data-save"
            public static let error = "app-data-error"
            /// VAPP-215: the record form's History section, a row, its Restore.
            public static let history = "app-data-history"
            public static let historyRow = "app-data-history-row"
            public static let restore = "app-data-restore"
        }
    }

    /// One kind of the head manifest with its live count.
    public struct DataKind: Equatable, Sendable {
        public let kind: String
        public let count: Int

        public init(kind: String, count: Int) {
            self.kind = kind
            self.count = count
        }
    }

    /// The head manifest's kinds, sorted by name, each with its live count
    /// (TS `dataKinds`, `kindCases`); a row of an undeclared kind is ignored.
    /// `manifest` = the head package (nil = a draft), `rowKinds` = the kind
    /// of every synced row of the app.
    public static func dataKinds(manifest: CardJSON?, rowKinds: [String]) -> [DataKind] {
        let declared = manifest?.objectValue?["records"]?.objectValue ?? [:]
        var counts: [String: Int] = [:]
        for kind in rowKinds { counts[kind, default: 0] += 1 }
        return declared.keys.sorted(by: scalarLess).map { DataKind(kind: $0, count: counts[$0] ?? 0) }
    }

    /// The head manifest's `records.<kind>.schema` (nil = undeclared).
    public static func dataSchema(manifest: CardJSON?, kind: String) -> CardJSON? {
        manifest?.objectValue?["records"]?.objectValue?[kind]?.objectValue?["schema"]
    }

    private static func properties(_ schema: CardJSON) -> [String: CardJSON] {
        schema.objectValue?["properties"]?.objectValue ?? [:]
    }

    private static func requiredNames(_ schema: CardJSON) -> [String] {
        guard case let .array(items)? = schema.objectValue?["required"] else { return [] }
        return items.compactMap { item in
            if case let .string(name) = item { return name }
            return nil
        }
    }

    /// The columns (TS `dataColumns`, `columnCases`): the `required` names
    /// in their order, then every other property sorted by name (never the
    /// declaration order: a Swift dictionary has none).
    public static func dataColumns(_ schema: CardJSON) -> [String] {
        let names = properties(schema)
        var required: [String] = []
        for name in requiredNames(schema) where names[name] != nil && !required.contains(name) {
            required.append(name)
        }
        let rest = names.keys.filter { !required.contains($0) }.sorted(by: scalarLess)
        return required + rest
    }

    /// A kind's rows, `updatedAt` desc, ties by id asc (TS `dataRows`,
    /// `rowsCases`).
    public static func dataRows(_ rows: [Row], kind: String) -> [Row] {
        rows
            .filter { $0.kind == kind }
            .sorted { a, b in
                a.updatedAt == b.updatedAt ? scalarLess(a.id, b.id) : scalarLess(b.updatedAt, a.updatedAt)
            }
    }

    /// One cell's text (TS `cellText`, `cellCases`): null / missing = "",
    /// a boolean = Yes / No, a number as written (`3`, never `3.0`), a list
    /// = its items joined `, `, an object = its JSON.
    public static func cellText(_ value: CardJSON?) -> String {
        switch value {
        case nil, .null?: return ""
        case let .bool(b)?: return b ? DataTab.Copy.yes : DataTab.Copy.no
        case let .number(n)?: return numberText(n)
        case let .string(s)?: return s
        case let .array(items)?: return items.map { cellText($0) }.joined(separator: ", ")
        case let v?: return v.compactString
        }
    }

    /// A form field's control.
    public enum DataControl: String, Equatable, Sendable {
        case text, number, `switch`, select, list
    }

    /// One labelled field of the Add / Edit form (TS `DataFormField`).
    public struct DataFormField: Equatable, Sendable {
        public let name: String
        public let control: DataControl
        public let required: Bool
        /// A date / time / date-time text field's hint.
        public let placeholder: String?
        /// A select's values (a string enum).
        public let options: [String]?
        /// A list's item type (string | number | integer | boolean).
        public let itemType: String?

        public init(
            name: String, control: DataControl, required: Bool,
            placeholder: String? = nil, options: [String]? = nil, itemType: String? = nil
        ) {
            self.name = name
            self.control = control
            self.required = required
            self.placeholder = placeholder
            self.options = options
            self.itemType = itemType
        }
    }

    private static func stringValue(_ value: CardJSON?) -> String? {
        if case let .string(s)? = value { return s }
        return nil
    }

    /// The form's fields, in column order (TS `formFields`, `formCases`).
    public static func formFields(_ schema: CardJSON) -> [DataFormField] {
        let props = properties(schema)
        let required = requiredNames(schema)
        return dataColumns(schema).map { name in
            let field = props[name]?.objectValue ?? [:]
            let isRequired = required.contains(name)
            switch stringValue(field["type"]) {
            case "boolean":
                return DataFormField(name: name, control: .switch, required: isRequired)
            case "number", "integer":
                return DataFormField(name: name, control: .number, required: isRequired)
            case "array":
                let item = stringValue(field["items"]?.objectValue?["type"]) ?? "string"
                return DataFormField(name: name, control: .list, required: isRequired, itemType: item)
            default:
                if case let .array(values)? = field["enum"] {
                    let options = values.compactMap { stringValue($0) }
                    return DataFormField(name: name, control: .select, required: isRequired, options: options)
                }
                let placeholder = stringValue(field["format"]).flatMap { DataTab.placeholders[$0] }
                return DataFormField(name: name, control: .text, required: isRequired, placeholder: placeholder)
            }
        }
    }

    /// A control's input: a switch holds a boolean, every other control text.
    public enum DataInput: Equatable, Sendable {
        case text(String)
        case flag(Bool)

        public var text: String {
            if case let .text(value) = self { return value }
            return ""
        }

        public var flag: Bool {
            if case let .flag(value) = self { return value }
            return false
        }
    }

    /// A stored value → the control's initial input (TS `formInput`,
    /// `inputCases`): a list joined `, `, a number as written, null = empty.
    public static func formInput(_ control: DataControl, _ value: CardJSON?) -> DataInput {
        if control == .switch { return .flag(value == .bool(true)) }
        switch value {
        case nil, .null?: return .text("")
        case let .array(items)? where control == .list:
            return .text(items.map { cellText($0) }.joined(separator: ", "))
        default: return .text(text(value))
        }
    }

    /// JS `Number(text)` when it parses to a finite number, else the raw
    /// text (so the server names it).
    private static func parseNumber(_ raw: String) -> CardJSON {
        guard !raw.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              let n = jsNumber(raw), n.isFinite
        else { return .string(raw) }
        return .number(n)
    }

    private static func parseItem(_ raw: String, _ type: String?) -> CardJSON {
        switch type {
        case "number", "integer": return parseNumber(raw)
        case "boolean": return raw == "true" ? .bool(true) : raw == "false" ? .bool(false) : .string(raw)
        default: return .string(raw)
        }
    }

    /// The inputs → the record's data (TS `formData`, `parseCases`): an
    /// empty field = null, numbers and list items parse (unparsable stays the
    /// raw text); CREATE omits the nulls, EDIT keeps them (null clears).
    public static func formData(_ schema: CardJSON, inputs: [String: DataInput], editing: Bool) -> [String: CardJSON] {
        var data: [String: CardJSON] = [:]
        for field in formFields(schema) {
            let input = inputs[field.name]
            let value: CardJSON
            if field.control == .switch {
                value = .bool(input?.flag ?? false)
            } else {
                let raw = input?.text ?? ""
                if field.control == .list {
                    let items = raw
                        .split(separator: ",", omittingEmptySubsequences: false)
                        .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                        .filter { !$0.isEmpty }
                    value = items.isEmpty ? .null : .array(items.map { parseItem($0, field.itemType) })
                } else if raw.isEmpty {
                    value = .null
                } else {
                    value = field.control == .number ? parseNumber(raw) : .string(raw)
                }
            }
            if value != .null || editing { data[field.name] = value }
        }
        return data
    }

    // MARK: - Record history + undo (VAPP-215, fixture `history`)

    /// What one `app_record_events` row means, its undo and its History row
    /// (TS `APP_RECORD_HISTORY`).
    public enum History {
        public static let undoSeconds: Double = 10
        public static let retentionDays = 90
        public static let batch = 500
        public static let historyLimit = 50
        public static let maxFields = 3
        public static let ops = ["create", "update", "delete", "restore"]
        public static let vias = ["editor", "app", "mcp", "action", "import"]

        public enum Copy {
            public static let history = "History"
            public static let restore = "Restore"
            public static let undo = "Undo"
            public static let created = "Created"
            public static let deleted = "Deleted"
            public static let restored = "Restored"
            /// `Changed {fields}`.
            public static let changed = "Changed {fields}"
            /// `+{n}`.
            public static let more = "+{n}"
            public static let saved = "Saved"
            public static let empty = "No history yet"
            public static let undoCreate = "Record added"
            public static let undoUpdate = "Record saved"
            public static let undoDelete = "Record deleted"
            public static let undoBatch = "Records saved"
            public static let undoVerb = "undo"
            public static let undoThing = "the change"
        }
    }

    /// One event as `apps.records.history` returns it (newest first); the
    /// pure rules read `op`, `before` and `patch`.
    public struct RecordEvent: Equatable, Sendable {
        public let id: String
        /// `create` | `update` | `delete` | `restore`.
        public let op: String
        public let actor: String?
        public let before: [String: CardJSON]?
        public let patch: [String: CardJSON]?
        public let createdAt: String

        public init(
            id: String = "", op: String, actor: String? = nil,
            before: [String: CardJSON]?, patch: [String: CardJSON]?, createdAt: String = ""
        ) {
            self.id = id
            self.op = op
            self.actor = actor
            self.before = before
            self.patch = patch
            self.createdAt = createdAt
        }
    }

    /// The write that undoes an event: a delete, or a restore of `data`.
    public enum InverseWrite: Equatable, Sendable {
        case delete
        case restore([String: CardJSON])
    }

    /// The record an event left (TS `eventState`, `stateCases`): nil = gone.
    public static func eventState(_ event: RecordEvent) -> [String: CardJSON]? {
        if event.op == "delete" { return nil }
        if event.op == "update" {
            return (event.before ?? [:]).merging(event.patch ?? [:]) { _, new in new }
        }
        return event.patch ?? [:]
    }

    /// Undo = a new write of the event's `before` (TS `inverseWrite`,
    /// `inverseCases`).
    public static func inverseWrite(_ event: RecordEvent) -> InverseWrite {
        guard let before = event.before else { return .delete }
        return .restore(before)
    }

    /// Deep JSON equality (objects by key, numbers by value; a missing value
    /// = nil differs from null; TS `jsonEqual`).
    public static func jsonEqual(_ a: CardJSON?, _ b: CardJSON?) -> Bool {
        a == b
    }

    /// A History row's title (TS `eventLabel`, `labelCases`).
    public static func eventLabel(_ event: RecordEvent) -> String {
        typealias C = History.Copy
        switch event.op {
        case "create": return C.created
        case "delete": return C.deleted
        case "restore": return C.restored
        default: break
        }
        let before = event.before ?? [:]
        let patch = event.patch ?? [:]
        let changed = patch.keys
            .filter { !jsonEqual(before[$0], patch[$0]) }
            .sorted(by: scalarLess)
        if changed.isEmpty { return C.saved }
        let max = History.maxFields
        let shown = changed.prefix(max).joined(separator: ", ")
        let rest = changed.count - max
        let fields = rest > 0 ? shown + " " + C.more.replacingOccurrences(of: "{n}", with: String(rest)) : shown
        return C.changed.replacingOccurrences(of: "{fields}", with: fields)
    }

    /// One History row.
    public struct HistoryRow: Equatable, Sendable {
        public let id: String
        public let label: String
        public let who: String
        public let at: String
        public let restorable: Bool

        public init(id: String, label: String, who: String, at: String, restorable: Bool) {
            self.id = id
            self.label = label
            self.who = who
            self.at = at
            self.restorable = restorable
        }
    }

    /// The History list (TS `historyRows`, `rowsCases`): the events in the
    /// server's order (newest first); Restore on every row but the newest,
    /// never on a delete.
    public static func historyRows(_ events: [RecordEvent], names: [String: String], fallback: String) -> [HistoryRow] {
        events.enumerated().map { i, event in
            HistoryRow(
                id: event.id,
                label: eventLabel(event),
                who: event.actor.flatMap { names[$0] } ?? fallback,
                at: event.createdAt,
                restorable: i > 0 && event.op != "delete"
            )
        }
    }

    /// The UI write an Undo toast follows.
    public enum UndoWrite: String, Sendable {
        case create, update, delete, batch
    }

    /// The Undo toast's title for a UI write (TS `undoTitle`).
    public static func undoTitle(_ write: UndoWrite) -> String {
        switch write {
        case .create: return History.Copy.undoCreate
        case .update: return History.Copy.undoUpdate
        case .delete: return History.Copy.undoDelete
        case .batch: return History.Copy.undoBatch
        }
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
