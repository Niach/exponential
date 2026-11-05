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
            /// VAPP-221: a cell holding one attachment.
            public static let file = "1 file"
            /// VAPP-221: a cell holding `{n}` attachments.
            public static let files = "{n} files"

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
            public static let updatedWidth: Double = 112
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

    /// VAPP-221: an attachment ref (eUI `RECORD_ATTACHMENT_REF`):
    /// `/api/attachments/<lowercase uuid>`, nothing after it.
    static func isAttachmentRef(_ value: CardJSON) -> Bool {
        let prefix = "/api/attachments/"
        guard case let .string(s) = value, s.hasPrefix(prefix) else { return false }
        let id = Array(s.utf8.dropFirst(prefix.utf8.count))
        guard id.count == 36 else { return false }
        for (i, c) in id.enumerated() {
            if [8, 13, 18, 23].contains(i) {
                if c != UInt8(ascii: "-") { return false }
            } else if !((UInt8(ascii: "0")...UInt8(ascii: "9")).contains(c) || (UInt8(ascii: "a")...UInt8(ascii: "f")).contains(c)) {
                return false
            }
        }
        return true
    }

    /// VAPP-221: a field holding files (`format: "attachment"` on a string or
    /// a list's string items): no form field, the app's FileUpload writes it
    /// (TS `isAttachmentField`). `field` = the schema's `properties.<name>`.
    public static func isAttachmentField(_ field: CardJSON?) -> Bool {
        let object = field?.objectValue ?? [:]
        if stringValue(object["type"]) == "array" {
            let items = object["items"]?.objectValue ?? [:]
            return stringValue(items["type"]) == "string" && stringValue(items["format"]) == "attachment"
        }
        return stringValue(object["type"]) == "string" && stringValue(object["format"]) == "attachment"
    }

    /// One cell's text (TS `cellText`, `cellCases`): VAPP-221 files read as
    /// a count (`1 file`, `{n} files`: a ref, or a non-empty list of refs
    /// only), else the value as written (`plainText`).
    public static func cellText(_ value: CardJSON?) -> String {
        if let value, isAttachmentRef(value) { return DataTab.Copy.file }
        if case let .array(items)? = value, !items.isEmpty, items.allSatisfy(isAttachmentRef) {
            return items.count == 1
                ? DataTab.Copy.file
                : DataTab.Copy.files.replacingOccurrences(of: "{n}", with: String(items.count))
        }
        return plainText(value)
    }

    /// A value as written (a form input's text; a cell without files):
    /// null / missing = "", a boolean = Yes / No, a number as written (`3`,
    /// never `3.0`), a list = its items joined `, `, an object = its JSON.
    private static func plainText(_ value: CardJSON?) -> String {
        switch value {
        case nil, .null?: return ""
        case let .bool(b)?: return b ? DataTab.Copy.yes : DataTab.Copy.no
        case let .number(n)?: return numberText(n)
        case let .string(s)?: return s
        case let .array(items)?: return items.map { plainText($0) }.joined(separator: ", ")
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

    /// The form's fields, in column order (TS `formFields`, `formCases`);
    /// VAPP-221: an attachment field gets none.
    public static func formFields(_ schema: CardJSON) -> [DataFormField] {
        let props = properties(schema)
        let required = requiredNames(schema)
        return dataColumns(schema).filter { !isAttachmentField(props[$0]) }.map { name in
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
            return .text(items.map { plainText($0) }.joined(separator: ", "))
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
        public static let vias = ["editor", "app", "mcp", "action", "import", "migration"]

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
        /// Where the write came from (VAPP-241: `share:<name>` = a guest).
        public let via: String?
        public let before: [String: CardJSON]?
        public let patch: [String: CardJSON]?
        public let createdAt: String

        public init(
            id: String = "", op: String, actor: String? = nil, via: String? = nil,
            before: [String: CardJSON]?, patch: [String: CardJSON]?, createdAt: String = ""
        ) {
            self.id = id
            self.op = op
            self.actor = actor
            self.via = via
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
    /// never on a delete. VAPP-241: a guest's submission reads Guest
    /// (app-share.json `historyCases`).
    public static func historyRows(_ events: [RecordEvent], names: [String: String], fallback: String) -> [HistoryRow] {
        events.enumerated().map { i, event in
            HistoryRow(
                id: event.id,
                label: eventLabel(event),
                who: AppShare.isGuestVia(event.via) ? AppShare.Copy.guest : (event.actor.flatMap { names[$0] } ?? fallback),
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

    // MARK: - Spreadsheet import (VAPP-228, fixture `csvImport`)

    /// The Data tab's Import dialog (TS `APP_CSV_IMPORT`): the CSV reader,
    /// the column auto-mapping and the cell coercion; the host appends
    /// `validateRecord` (eUI) issues.
    public enum CsvImport {
        public static let maxRows = 500
        public static let previewRows = 5
        public static let issuesShown = 3
        public static let trueWords = ["true", "yes", "y", "1", "x"]
        public static let falseWords = ["false", "no", "n", "0"]
        public static let authorKeys = ["author", "member", "user", "email", "person"]

        public enum Copy {
            public static let importButton = "Import…"
            /// `Import {kind}`.
            public static let title = "Import {kind}"
            public static let placeholder = "Paste a CSV or choose a file"
            public static let chooseFile = "Choose file"
            public static let skip = "Skip"
            public static let me = "Me"
            /// `Column {n}`.
            public static let column = "Column {n}"
            /// `row {row}: {field}: {message}`.
            public static let issue = "row {row}: {field}: {message}"
            /// `row {row}: {message}`.
            public static let rowIssue = "row {row}: {message}"
            /// `+{n} more`.
            public static let moreIssues = "+{n} more"
            /// `{n} of {total} rows ready`.
            public static let ready = "{n} of {total} rows ready"
            /// `Up to {max} rows per import`.
            public static let tooMany = "Up to {max} rows per import"
            public static let noRows = "No rows"
            /// `Import {n}`.
            public static let importAction = "Import {n}"
            /// `{n} records imported`.
            public static let imported = "{n} records imported"
        }

        public enum Issues {
            public static let notNumber = "\"{value}\" is not a number"
            public static let notInteger = "\"{value}\" is not a whole number"
            public static let notBoolean = "\"{value}\" is not yes or no"
            public static let notDate = "\"{value}\" is not a date (YYYY-MM-DD)"
            public static let slashDate = "\"{value}\" is ambiguous: write YYYY-MM-DD or DD.MM.YYYY"
            public static let notTime = "\"{value}\" is not a time (HH:MM)"
            public static let notDateTime = "\"{value}\" is not an ISO date-time"
            public static let notEnum = "\"{value}\" is not one of {values}"
            public static let notMember = "\"{value}\" is not a member"
            public static let extraCells = "{n} cells, the header has {m}"
        }

        public enum TestIds {
            public static let button = "app-data-import"
            public static let dialog = "app-csv-import"
            public static let text = "app-csv-text"
            public static let file = "app-csv-file"
            public static let map = "app-csv-map"
            public static let author = "app-csv-author"
            public static let preview = "app-csv-preview"
            public static let error = "app-csv-error"
            public static let confirm = "app-csv-confirm"
        }
    }

    /// One data row: its record number (header = 1, blank lines counted)
    /// and its cells as written.
    public struct CsvRow: Equatable, Sendable {
        public let row: Int
        public let cells: [String]

        public init(row: Int, cells: [String]) {
            self.row = row
            self.cells = cells
        }
    }

    /// `csvParse`'s answer: the header + the data rows.
    public struct CsvTable: Equatable, Sendable {
        public let header: [String]
        public let rows: [CsvRow]

        public init(header: [String], rows: [CsvRow]) {
            self.header = header
            self.rows = rows
        }
    }

    /// JS `String.prototype.replace(string, string)`: the FIRST occurrence only.
    private static func replaceFirst(_ text: String, _ target: String, _ replacement: String) -> String {
        guard let range = text.range(of: target) else { return text }
        return text.replacingCharacters(in: range, with: replacement)
    }

    private static func string(of scalars: [Unicode.Scalar]) -> String {
        var out = ""
        out.unicodeScalars.append(contentsOf: scalars)
        return out
    }

    private static func isAsciiDigit(_ s: Unicode.Scalar) -> Bool {
        s.value >= 48 && s.value <= 57
    }

    /// Only ASCII 0–9, its scalar count within `lengths`.
    private static func digits(_ text: String, _ lengths: ClosedRange<Int>) -> Bool {
        let scalars = Array(text.unicodeScalars)
        return lengths.contains(scalars.count) && scalars.allSatisfy(isAsciiDigit)
    }

    private static func csvTrim(_ text: String) -> String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// The delimiter: the most frequent of `,` `;` TAB in the first line
    /// outside quotes (ties in that order, none = `,`).
    private static func csvDelimiter(_ text: [Unicode.Scalar]) -> Unicode.Scalar {
        let candidates: [Unicode.Scalar] = [",", ";", "\t"]
        var counts = [0, 0, 0]
        var quoted = false
        for ch in text {
            if ch == "\"" {
                quoted.toggle()
            } else if !quoted, ch == "\n" || ch == "\r" {
                break
            } else if !quoted, let index = candidates.firstIndex(of: ch) {
                counts[index] += 1
            }
        }
        var best = 0
        for index in 1..<candidates.count where counts[index] > counts[best] {
            best = index
        }
        return candidates[best]
    }

    /// RFC 4180 → the header + the data rows (TS `csvParse`, `parseCases`).
    /// Iterates unicode scalars: `\r\n` is ONE Character in Swift.
    public static func csvParse(_ input: String) -> CsvTable {
        var text = Array(input.unicodeScalars)
        if let first = text.first, first.value == 0xFEFF { text.removeFirst() }
        let delimiter = csvDelimiter(text)
        var records: [[String]] = []
        var record: [String] = []
        var field: [Unicode.Scalar] = []
        var quoted = false
        var i = 0
        func endRecord() {
            record.append(string(of: field))
            records.append(record)
            record = []
            field = []
        }
        while i < text.count {
            let ch = text[i]
            if quoted {
                if ch == "\"" {
                    if i + 1 < text.count, text[i + 1] == "\"" {
                        field.append("\"")
                        i += 2
                        continue
                    }
                    quoted = false
                } else {
                    field.append(ch)
                }
                i += 1
                continue
            }
            if ch == "\"" {
                quoted = true
            } else if ch == delimiter {
                record.append(string(of: field))
                field = []
            } else if ch == "\r" || ch == "\n" {
                endRecord()
                if ch == "\r", i + 1 < text.count, text[i + 1] == "\n" { i += 1 }
            } else {
                field.append(ch)
            }
            i += 1
        }
        if !field.isEmpty || !record.isEmpty { endRecord() }
        let numbered = records.enumerated()
            .map { CsvRow(row: $0.offset + 1, cells: $0.element) }
            .filter { !($0.cells.count == 1 && $0.cells[0].isEmpty) }
        guard let first = numbered.first else { return CsvTable(header: [], rows: []) }
        var seen: [String: Int] = [:]
        var header: [String] = []
        for (index, cell) in first.cells.enumerated() {
            let trimmed = csvTrim(cell)
            let name = trimmed.isEmpty
                ? CsvImport.Copy.column.replacingOccurrences(of: "{n}", with: String(index + 1))
                : trimmed
            let n = (seen[name] ?? 0) + 1
            seen[name] = n
            header.append(n == 1 ? name : "\(name) \(n)")
        }
        var rows: [CsvRow] = []
        for r in numbered.dropFirst() {
            if r.cells.count < header.count {
                rows.append(CsvRow(row: r.row, cells: r.cells + Array(repeating: "", count: header.count - r.cells.count)))
            } else {
                rows.append(r)
            }
        }
        return CsvTable(header: header, rows: rows)
    }

    /// A name's matching key (TS `csvKey`): lowercased, only ASCII a–z and 0–9.
    public static func csvKey(_ name: String) -> String {
        string(of: name.lowercased().unicodeScalars.filter { s in
            (s.value >= 97 && s.value <= 122) || isAsciiDigit(s)
        })
    }

    /// The first pick per field (TS `csvAutoMap`, `mapCases`): equal keys
    /// first, then the ONE free column containing the field's key; a column
    /// maps at most once. A field absent from the answer = Skip (TS null).
    public static func csvAutoMap(fields: [String], header: [String]) -> [String: Int] {
        let keys = header.map(csvKey)
        var taken = Set<Int>()
        var mapping: [String: Int] = [:]
        for field in fields {
            let key = csvKey(field)
            guard !key.isEmpty else { continue }
            if let index = keys.indices.first(where: { !taken.contains($0) && keys[$0] == key }) {
                mapping[field] = index
                taken.insert(index)
            }
        }
        for field in fields {
            let key = csvKey(field)
            if mapping[field] != nil || key.isEmpty { continue }
            let hits = keys.indices.filter { !taken.contains($0) && keys[$0].contains(key) }
            if hits.count == 1 {
                mapping[field] = hits[0]
                taken.insert(hits[0])
            }
        }
        return mapping
    }

    /// The owner's Author pick (TS `csvAuthorColumn`): the first column whose
    /// key is an `authorKeys` entry, else nil (= Me).
    public static func csvAuthorColumn(_ header: [String]) -> Int? {
        header.firstIndex { CsvImport.authorKeys.contains(csvKey($0)) }
    }

    /// One cell read (TS `CsvCell`): a value (`.null` = no value) or the issue's message.
    public enum CsvCell: Equatable, Sendable {
        case value(CardJSON)
        case issue(String)
    }

    private static func csvIssue(_ template: String, _ value: String, _ values: String = "") -> CsvCell {
        .issue(replaceFirst(replaceFirst(template, "{value}", value), "{values}", values))
    }

    private static func pad2(_ s: String) -> String {
        s.unicodeScalars.count == 1 ? "0" + s : s
    }

    private static func csvNumber(_ text: String, integer: Bool) -> CsvCell {
        let dot: Unicode.Scalar = "."
        let comma: Unicode.Scalar = ","
        let compact: [Unicode.Scalar] = Array(text.unicodeScalars).filter { $0 != " " }
        // ^[+-]?[0-9.,]*[0-9][0-9.,]*$
        var body = Array(compact)
        if let first = body.first, first == "+" || first == "-" { body.removeFirst() }
        guard body.allSatisfy({ isAsciiDigit($0) || $0 == dot || $0 == comma }), body.contains(where: isAsciiDigit) else {
            return csvIssue(CsvImport.Issues.notNumber, text)
        }
        let lastDot = compact.lastIndex(of: dot)
        let lastComma = compact.lastIndex(of: comma)
        var normal: [Unicode.Scalar]
        if let d = lastDot, let c = lastComma {
            let decimal = d > c ? dot : comma
            let group = decimal == dot ? comma : dot
            normal = compact.filter { $0 != group }
            if let index = normal.firstIndex(of: decimal) { normal[index] = dot }
        } else if lastDot != nil || lastComma != nil {
            let sep = lastDot != nil ? dot : comma
            let occurrences = compact.filter { $0 == sep }.count
            if occurrences == 1 {
                normal = compact.map { $0 == sep ? dot : $0 }
            } else {
                normal = compact.filter { $0 != sep }
            }
        } else {
            normal = compact
        }
        // ^[+-]?[0-9]+(\.[0-9]+)?$
        var negative = false
        if let first = normal.first, first == "+" || first == "-" {
            negative = first == "-"
            normal.removeFirst()
        }
        let parts = string(of: normal).components(separatedBy: ".")
        guard parts.count <= 2, parts.allSatisfy({ digits($0, 1...Int.max) }),
              let parsed = Double(string(of: normal)), parsed.isFinite
        else { return csvIssue(CsvImport.Issues.notNumber, text) }
        let n = negative ? -parsed : parsed
        if integer, n != n.rounded() { return csvIssue(CsvImport.Issues.notInteger, text) }
        return .value(.number(n))
    }

    private static func csvScalar(type: String?, enumValues: [String]?, format: String?, _ text: String) -> CsvCell {
        if type == "number" || type == "integer" { return csvNumber(text, integer: type == "integer") }
        if type == "boolean" {
            let lower = text.lowercased()
            if CsvImport.trueWords.contains(lower) { return .value(.bool(true)) }
            if CsvImport.falseWords.contains(lower) { return .value(.bool(false)) }
            return csvIssue(CsvImport.Issues.notBoolean, text)
        }
        if let enumValues {
            let lower = text.lowercased()
            if let hit = enumValues.first(where: { $0.lowercased() == lower }) { return .value(.string(hit)) }
            return csvIssue(CsvImport.Issues.notEnum, text, enumValues.joined(separator: ", "))
        }
        if format == "date" {
            let scalars = Array(text.unicodeScalars)
            if scalars.count == 10, scalars[4] == "-", scalars[7] == "-",
               digits(string(of: Array(scalars[0..<4])), 4...4),
               digits(string(of: Array(scalars[5..<7])), 2...2),
               digits(string(of: Array(scalars[8..<10])), 2...2) {
                return .value(.string(text))
            }
            let parts = text.components(separatedBy: ".")
            if parts.count == 3, digits(parts[0], 1...2), digits(parts[1], 1...2), digits(parts[2], 4...4) {
                return .value(.string("\(parts[2])-\(pad2(parts[1]))-\(pad2(parts[0]))"))
            }
            return csvIssue(text.contains("/") ? CsvImport.Issues.slashDate : CsvImport.Issues.notDate, text)
        }
        if format == "time" {
            let parts = text.components(separatedBy: ":")
            if parts.count == 2 || parts.count == 3, digits(parts[0], 1...2), digits(parts[1], 2...2),
               parts.count == 2 || digits(parts[2], 2...2) {
                let seconds = parts.count == 3 ? ":" + parts[2] : ""
                return .value(.string("\(pad2(parts[0])):\(parts[1])\(seconds)"))
            }
            return csvIssue(CsvImport.Issues.notTime, text)
        }
        if format == "date-time" {
            let scalars = Array(text.unicodeScalars)
            var ok = scalars.count >= 11 && scalars[4] == "-" && scalars[7] == "-" && scalars[10] == "T"
            if ok {
                for index in 0..<10 where index != 4 && index != 7 && !isAsciiDigit(scalars[index]) {
                    ok = false
                }
            }
            return ok ? .value(.string(text)) : csvIssue(CsvImport.Issues.notDateTime, text)
        }
        return .value(.string(text))
    }

    /// One cell → the field's value (TS `csvCell`, `cellCases`) on the
    /// trimmed cell; empty = `.value(.null)`. `field` = the schema property.
    public static func csvCell(_ field: CardJSON, _ cell: String) -> CsvCell {
        let text = csvTrim(cell)
        if text.isEmpty { return .value(.null) }
        let object = field.objectValue ?? [:]
        let type = stringValue(object["type"])
        if type != "array" {
            var enumValues: [String]?
            if case let .array(values)? = object["enum"] { enumValues = values.compactMap { stringValue($0) } }
            return csvScalar(type: type, enumValues: enumValues, format: stringValue(object["format"]), text)
        }
        let separator = text.contains(";") ? ";" : ","
        let items = text.components(separatedBy: separator).map(csvTrim).filter { !$0.isEmpty }
        if items.isEmpty { return .value(.null) }
        let itemType = stringValue(object["items"]?.objectValue?["type"]) ?? "string"
        var values: [CardJSON] = []
        for item in items {
            let read = csvScalar(type: itemType, enumValues: nil, format: nil, item)
            switch read {
            case .issue: return read
            case let .value(value): values.append(value)
            }
        }
        return .value(.array(values))
    }

    /// A team member the Author column may name.
    public struct CsvMember: Equatable, Sendable {
        public let id: String
        public let name: String
        public let email: String

        public init(id: String, name: String, email: String) {
            self.id = id
            self.name = name
            self.email = email
        }
    }

    /// The owner's Author pick.
    public struct CsvAuthor: Equatable, Sendable {
        /// The Author column (nil = every row is the importer's).
        public let column: Int?
        public let importer: String
        public let members: [CsvMember]

        public init(column: Int?, importer: String, members: [CsvMember]) {
            self.column = column
            self.importer = importer
            self.members = members
        }
    }

    /// One imported row: its data (nulls omitted), its author when someone
    /// else's (an on-behalf write) and its issues (the host appends its own).
    public struct CsvRecord: Equatable, Sendable {
        public let row: Int
        public let data: [String: CardJSON]
        public let author: String?
        public var issues: [String]

        public init(row: Int, data: [String: CardJSON], author: String? = nil, issues: [String]) {
            self.row = row
            self.data = data
            self.author = author
            self.issues = issues
        }
    }

    /// The rows → records + their issues (TS `csvRecords`, `rowsCases`),
    /// before the host's schema check. `mapping` = field → column (absent = Skip).
    public static func csvRecords(
        schema: CardJSON, header: [String], rows: [CsvRow], mapping: [String: Int], author: CsvAuthor? = nil
    ) -> [CsvRecord] {
        let props = properties(schema)
        let columns = dataColumns(schema)
        return rows.map { csvRow -> CsvRecord in
            let row = String(csvRow.row)
            let cells = csvRow.cells
            func cell(_ column: Int) -> String { column >= 0 && column < cells.count ? cells[column] : "" }
            func at(_ field: String, _ message: String) -> String {
                replaceFirst(replaceFirst(replaceFirst(CsvImport.Copy.issue, "{row}", row), "{field}", field), "{message}", message)
            }
            var issues: [String] = []
            if cells.count > header.count {
                let message = replaceFirst(
                    replaceFirst(CsvImport.Issues.extraCells, "{n}", String(cells.count)), "{m}", String(header.count)
                )
                issues.append(replaceFirst(replaceFirst(CsvImport.Copy.rowIssue, "{row}", row), "{message}", message))
            }
            var data: [String: CardJSON] = [:]
            for name in columns {
                guard let column = mapping[name] else { continue }
                switch csvCell(props[name] ?? .object([:]), cell(column)) {
                case let .issue(message): issues.append(at(name, message))
                case let .value(value): if value != .null { data[name] = value }
                }
            }
            var writeAs: String?
            if let author, let column = author.column {
                let text = csvTrim(cell(column))
                var id: String? = author.importer
                if !text.isEmpty {
                    let lower = text.lowercased()
                    let byEmail = author.members.first { $0.email.lowercased() == lower }
                    let byName = author.members.filter { $0.name.lowercased() == lower }
                    id = byEmail?.id ?? (byName.count == 1 ? byName[0].id : nil)
                }
                if let id {
                    if id != author.importer { writeAs = id }
                } else {
                    issues.append(at(DataTab.Copy.author, replaceFirst(CsvImport.Issues.notMember, "{value}", text)))
                }
            }
            return CsvRecord(row: csvRow.row, data: data, author: writeAs, issues: issues)
        }
    }

    /// The preview's count line (TS `csvSummary`, `copy.ready`).
    public static func csvSummary(_ records: [CsvRecord]) -> String {
        let ready = records.filter { $0.issues.isEmpty }.count
        return replaceFirst(replaceFirst(CsvImport.Copy.ready, "{n}", String(ready)), "{total}", String(records.count))
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

// MARK: - The offline outbox (VAPP-235, fixture `outbox`)

/// VAPP-235 (eApps O1, docs/spec/apps.md §14): the app host queues every
/// record write as ONE entry, lays the entries over the synced rows and
/// replays them in order with an idempotency key (TS `outboxWrites` …
/// `outboxPendingLabel`; desktop `domain::app_records`, Android
/// `AppRecords.kt`; `AppRecordsTests` replays every `outbox.*Cases`). The
/// web stays online-only.
extension AppRecords {
    public enum Outbox {
        public static let ackGraceSeconds: Double = 30
        public static let retryCodes = [
            "UNAUTHORIZED", "TIMEOUT", "TOO_MANY_REQUESTS", "CLIENT_CLOSED_REQUEST",
            "INTERNAL_SERVER_ERROR", "BAD_GATEWAY", "SERVICE_UNAVAILABLE", "GATEWAY_TIMEOUT",
        ]

        public enum Copy {
            public static let pendingOne = "1 change waiting to sync"
            /// `{n} changes waiting to sync`.
            public static let pending = "{n} changes waiting to sync"
        }
    }

    /// The host call an entry came from.
    public enum OutboxCall: String, Equatable, Sendable {
        case upsert, delete, batch
    }

    /// One write of an entry: `create` / `update` carry `kind` + `data`, a
    /// `delete` only its `id`.
    public struct OutboxWrite: Equatable, Sendable {
        public enum Op: String, Equatable, Sendable {
            case create, update, delete
        }

        public let op: Op
        public let id: String
        public let kind: String
        public let data: [String: CardJSON]

        public init(op: Op, id: String, kind: String = "", data: [String: CardJSON] = [:]) {
            self.op = op
            self.id = id
            self.kind = op == .delete ? "" : kind
            self.data = op == .delete ? [:] : data
        }

        /// The fixture / persisted form (`{op, id, kind?, data?}`).
        public var json: CardJSON {
            if op == .delete { return .object(["op": .string(op.rawValue), "id": .string(id)]) }
            return .object(["op": .string(op.rawValue), "id": .string(id), "kind": .string(kind), "data": .object(data)])
        }

        public init?(json: CardJSON) {
            guard let object = json.objectValue,
                  case let .string(raw)? = object["op"], let op = Op(rawValue: raw),
                  case let .string(id)? = object["id"] else { return nil }
            var kind = ""
            if case let .string(k)? = object["kind"] { kind = k }
            self.init(op: op, id: id, kind: kind, data: object["data"]?.objectValue ?? [:])
        }
    }

    /// One queued host call (`{writeId, appId, call, writes, at, state, ackedAt?}`).
    public struct OutboxEntry: Equatable, Sendable {
        public enum State: String, Equatable, Sendable {
            case pending, acked
        }

        public let writeId: String
        public let appId: String
        public let call: OutboxCall
        public let writes: [OutboxWrite]
        /// ISO time the write was made.
        public let at: String
        public var state: State
        public var ackedAt: String?

        public init(
            writeId: String, appId: String, call: OutboxCall, writes: [OutboxWrite], at: String,
            state: State = .pending, ackedAt: String? = nil
        ) {
            self.writeId = writeId
            self.appId = appId
            self.call = call
            self.writes = writes
            self.at = at
            self.state = state
            self.ackedAt = ackedAt
        }

        /// The fixture form (`ackedAt` only when set).
        public var json: CardJSON {
            var object: [String: CardJSON] = [
                "writeId": .string(writeId), "appId": .string(appId), "call": .string(call.rawValue),
                "writes": .array(writes.map(\.json)), "at": .string(at), "state": .string(state.rawValue),
            ]
            if let ackedAt { object["ackedAt"] = .string(ackedAt) }
            return .object(object)
        }

        public init?(json: CardJSON) {
            guard let object = json.objectValue,
                  case let .string(writeId)? = object["writeId"],
                  case let .string(appId)? = object["appId"],
                  case let .string(rawCall)? = object["call"], let call = OutboxCall(rawValue: rawCall),
                  case let .array(writes)? = object["writes"],
                  case let .string(at)? = object["at"] else { return nil }
            var state = State.pending
            if case let .string(raw)? = object["state"], let parsed = State(rawValue: raw) { state = parsed }
            var ackedAt: String?
            if case let .string(raw)? = object["ackedAt"] { ackedAt = raw }
            self.init(
                writeId: writeId, appId: appId, call: call, writes: writes.compactMap(OutboxWrite.init(json:)),
                at: at, state: state, ackedAt: ackedAt
            )
        }

        /// What the call answers from the entry (offline): `upsert` → `{id}`,
        /// `delete` → nil, `batch` → `{ids}` (every write's id in order).
        public var answer: CardJSON? {
            switch call {
            case .upsert: return .object(["id": .string(writes.first?.id ?? "")])
            case .delete: return nil
            case .batch: return .object(["ids": .array(writes.map { .string($0.id) })])
            }
        }
    }

    /// A replay's answer: ok, no network, or the server's tRPC code.
    public enum OutboxResult: Equatable, Sendable {
        case ok
        case transport
        case code(String)
    }

    public enum OutboxOutcome: String, Equatable, Sendable {
        case done, retry, dropped, refused
    }

    /// The replay request of an entry; the host adds the app (`id`).
    public struct OutboxRequest: Equatable, Sendable {
        public let call: OutboxCall
        public let input: [String: CardJSON]
    }

    /// One flush's answer (`flushCases`).
    public struct OutboxFlushResult: Equatable, Sendable {
        public struct Refused: Equatable, Sendable {
            public let writeId: String
            /// `save` | `delete` (`outboxVerb`).
            public let verb: String
        }

        public let sent: [String]
        public let refused: [Refused]
        public let after: [OutboxEntry]
    }

    /// A host call's writes (TS `outboxWrites`, `writesCases`): a create
    /// takes the next client id.
    public static func outboxWrites(call: OutboxCall, args: [String: CardJSON], newIds: [String]) -> [OutboxWrite] {
        var next = 0
        func upsert(_ w: [String: CardJSON]) -> OutboxWrite {
            let data = w["data"]?.objectValue ?? [:]
            var kind = ""
            if case let .string(k)? = w["kind"] { kind = k }
            if case let .string(id)? = w["id"] { return OutboxWrite(op: .update, id: id, kind: kind, data: data) }
            let id = next < newIds.count ? newIds[next] : ""
            next += 1
            return OutboxWrite(op: .create, id: id, kind: kind, data: data)
        }
        func idOf(_ w: [String: CardJSON]) -> String {
            if case let .string(id)? = w["id"] { return id }
            return ""
        }
        switch call {
        case .upsert: return [upsert(args)]
        case .delete: return [OutboxWrite(op: .delete, id: idOf(args))]
        case .batch:
            guard case let .array(list)? = args["writes"] else { return [] }
            return list.compactMap(\.objectValue).map { w in
                if case .string("delete")? = w["op"] { return OutboxWrite(op: .delete, id: idOf(w)) }
                return upsert(w)
            }
        }
    }

    /// How many client ids a host call's creates take (the host mints them
    /// before `outboxWrites`).
    public static func outboxCreates(call: OutboxCall, args: [String: CardJSON]) -> Int {
        outboxWrites(call: call, args: args, newIds: []).filter { $0.op == .create }.count
    }

    /// The replay request of an entry (TS `outboxRequest`, `requestCases`).
    public static func outboxRequest(_ entry: OutboxEntry) -> OutboxRequest {
        let writeId = CardJSON.string(entry.writeId)
        if entry.call == .batch {
            let writes: [CardJSON] = entry.writes.map { w in
                switch w.op {
                case .delete: return .object(["op": .string("delete"), "id": .string(w.id)])
                case .create: return .object(["op": .string("upsert"), "kind": .string(w.kind), "data": .object(w.data), "newId": .string(w.id)])
                case .update: return .object(["op": .string("upsert"), "kind": .string(w.kind), "data": .object(w.data), "id": .string(w.id)])
                }
            }
            return OutboxRequest(call: .batch, input: ["writes": .array(writes), "writeId": writeId])
        }
        guard let w = entry.writes.first else { return OutboxRequest(call: entry.call, input: ["writeId": writeId]) }
        if w.op == .delete {
            return OutboxRequest(call: .delete, input: ["recordId": .string(w.id), "writeId": writeId])
        }
        var input: [String: CardJSON] = ["kind": .string(w.kind), "data": .object(w.data), "writeId": writeId]
        input[w.op == .create ? "newId" : "recordId"] = .string(w.id)
        return OutboxRequest(call: .upsert, input: input)
    }

    /// The visible rows: the synced rows with every entry laid over them,
    /// oldest first (TS `outboxRows`, `rowsCases`).
    public static func outboxRows(_ rows: [Row], entries: [OutboxEntry], me: String?) -> [Row] {
        var out = rows
        for entry in entries {
            for w in entry.writes {
                let i = out.firstIndex { $0.id == w.id }
                if w.op == .delete {
                    if let i { out.remove(at: i) }
                } else if let i {
                    let row = out[i]
                    out[i] = Row(
                        id: row.id, kind: row.kind, data: row.data.merging(w.data) { _, new in new },
                        createdBy: row.createdBy, authorId: row.authorId, createdAt: row.createdAt, updatedAt: row.updatedAt
                    )
                } else if w.op == .create {
                    out.append(Row(id: w.id, kind: w.kind, data: w.data, createdBy: me, authorId: me, createdAt: entry.at, updatedAt: entry.at))
                }
            }
        }
        return out
    }

    /// VAPP-212: `outboxRows` as an overlay on a store that keeps the synced rows.
    public struct OutboxOverlay: Sendable {
        public let upserts: [Row]
        public let deletes: [String]
    }

    /// VAPP-212: the ids `entries` write: the only synced rows `outboxOverlay`
    /// and `outboxPrune` read.
    public static func outboxWrittenIds(_ entries: [OutboxEntry]) -> Set<String> {
        Set(entries.flatMap { $0.writes.map(\.id) })
    }

    /// VAPP-212: what `outboxRows` does to `rows`, only the written rows
    /// travelling: laid over the store, it answers `rowsCases`.
    public static func outboxOverlay(_ rows: [Row], entries: [OutboxEntry], me: String?) -> OutboxOverlay {
        let written = outboxWrittenIds(entries)
        let upserts = outboxRows(rows.filter { written.contains($0.id) }, entries: entries, me: me)
        let kept = Set(upserts.map(\.id))
        return OutboxOverlay(upserts: upserts, deletes: written.filter { !kept.contains($0) }.sorted())
    }

    /// What a replay's answer does to its entry (TS `outboxOutcome`,
    /// `outcomeCases`).
    public static func outboxOutcome(_ entry: OutboxEntry, result: OutboxResult) -> OutboxOutcome {
        switch result {
        case .ok: return .done
        case .transport: return .retry
        case let .code(code):
            if Outbox.retryCodes.contains(code) { return .retry }
            if code == "NOT_FOUND", entry.call != .batch, entry.writes.allSatisfy({ $0.op != .create }) { return .dropped }
            return .refused
        }
    }

    /// The failed-action toast's verb for a refused entry (`save` | `delete`).
    public static func outboxVerb(_ entry: OutboxEntry) -> String {
        entry.writes.allSatisfy { $0.op == .delete } ? "delete" : "save"
    }

    /// One flush (TS `outboxFlush`, `flushCases`): pending entries oldest
    /// first, one at a time; a retry stops it; `results` = each write id's
    /// answer (missing = no network).
    public static func outboxFlush(_ entries: [OutboxEntry], results: [String: OutboxResult], now: String) -> OutboxFlushResult {
        var sent: [String] = []
        var refused: [OutboxFlushResult.Refused] = []
        var after: [OutboxEntry] = []
        var stopped = false
        for entry in entries {
            if stopped || entry.state == .acked {
                after.append(entry)
                continue
            }
            sent.append(entry.writeId)
            switch outboxOutcome(entry, result: results[entry.writeId] ?? .transport) {
            case .done:
                var acked = entry
                acked.state = .acked
                acked.ackedAt = now
                after.append(acked)
            case .retry:
                stopped = true
                after.append(entry)
            case .refused:
                refused.append(.init(writeId: entry.writeId, verb: outboxVerb(entry)))
            case .dropped:
                break
            }
        }
        return OutboxFlushResult(sent: sent, refused: refused, after: after)
    }

    /// Do the synced rows reflect an entry (TS `outboxReflected`)?
    public static func outboxReflected(_ entry: OutboxEntry, rows: [Row]) -> Bool {
        entry.writes.allSatisfy { w in
            let row = rows.first { $0.id == w.id }
            if w.op == .delete { return row == nil }
            guard let row else { return false }
            return w.data.allSatisfy { key, value in jsonEqual(row.data[key], value) }
        }
    }

    /// The entries that stay (TS `outboxPrune`, `pruneCases`): pending ones,
    /// and acked ones the rows don't reflect yet within the grace.
    public static func outboxPrune(_ entries: [OutboxEntry], rows: [Row], now: String) -> [OutboxEntry] {
        let nowDate = WireTimestamps.parse(now)
        return entries.filter { e in
            if e.state != .acked { return true }
            if outboxReflected(e, rows: rows) { return false }
            guard let nowDate, let acked = WireTimestamps.parse(e.ackedAt ?? e.at) else { return false }
            return nowDate.timeIntervalSince(acked) < Outbox.ackGraceSeconds
        }
    }

    /// The header glyph's label for an app's pending entries (TS
    /// `outboxPendingLabel`, `pendingCases`); nil = no glyph.
    public static func outboxPendingLabel(_ entries: [OutboxEntry], appId: String) -> String? {
        let n = entries.filter { $0.appId == appId && $0.state == .pending }.count
        if n == 0 { return nil }
        return n == 1 ? Outbox.Copy.pendingOne : Outbox.Copy.pending.replacingOccurrences(of: "{n}", with: String(n))
    }

    /// `now` as the entries carry it (JS `toISOString`).
    public static func outboxNow(_ date: Date = Date()) -> String {
        isoMillis.string(from: date)
    }

    /// A replay failure as the outbox reads it: the server's tRPC code; an
    /// HTTP failure without one by its status (5xx = a server error, 401 =
    /// signed out, 429 = too many requests); anything else (no network, a
    /// timeout, no instance) = transport.
    public static func outboxResult(_ error: Error) -> OutboxResult {
        if let code = error.trpcErrorCode { return .code(code) }
        if let trpc = error as? TrpcError, case let .httpError(status, _) = trpc {
            switch status {
            case 401: return .code("UNAUTHORIZED")
            case 408: return .code("TIMEOUT")
            case 429: return .code("TOO_MANY_REQUESTS")
            case 500...: return .code("INTERNAL_SERVER_ERROR")
            default: return .code("BAD_REQUEST")
            }
        }
        return .transport
    }
}
