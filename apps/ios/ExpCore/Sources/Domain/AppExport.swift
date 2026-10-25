import Foundation

/// VAPP-121: an app as ONE JSON file ×4
/// (`packages/domain-contract/fixtures/app-export.json` is the spec, TS
/// reference `src/app-export.ts`; desktop `domain::app_export`, Android
/// `AppExport.kt`). `AppExportTests` replays `exportCases` and pins the
/// constants below to the fixture. Import validation is the SERVER's
/// (`apps.validateImport`, the one authority ×4).
public enum AppExport {
    public static let format = "exponential-app/1"
    public static let fileSuffix = ".app.json"
    public static let mime = "application/json"

    public enum Limits {
        public static let importMaxBytes = 4_194_304
        public static let recordsMax = 2000
        public static let issuesShown = 8
    }

    public enum Copy {
        public static let export = "Export"
        public static let exportTitle = "Export {name}"
        public static let includeRecords = "Include records"
        public static let `import` = "Import\u{2026}"
        public static let importTitle = "Import an app"
        public static let importPlaceholder = "Paste an app file or a package"
        public static let chooseFile = "Choose file"
        public static let importAction = "Import"
        public static let importNote = "Imported"
        public static let moreIssues = "and {n} more"

        /// `Export {name}`.
        public static func exportTitle(_ name: String) -> String {
            exportTitle.replacingOccurrences(of: "{name}", with: name)
        }

        /// `and {n} more`.
        public static func moreIssues(_ n: Int) -> String {
            moreIssues.replacingOccurrences(of: "{n}", with: String(n))
        }
    }

    /// Web `data-testid`s ×4.
    public enum TestIds {
        public static let dialog = "app-export-dialog"
        public static let records = "app-export-records"
        public static let confirm = "app-export-confirm"
        public static let menuItem = "app-menu-export"
        public static let editorButton = "app-editor-export"
        public static let `import` = "app-import"
        public static let importText = "app-import-text"
        public static let importFile = "app-import-file"
        public static let importIssues = "app-import-issues"
        public static let importConfirm = "app-import-confirm"
    }

    /// One record as the export input reads it (the synced row).
    public struct Record: Sendable, Equatable {
        public let id: String
        public let kind: String
        public let data: CardJSON
        public let createdAt: String

        public init(id: String, kind: String, data: CardJSON, createdAt: String) {
            self.id = id
            self.kind = kind
            self.data = data
            self.createdAt = createdAt
        }
    }

    /// `working-hours` → `working-hours.app.json`.
    public static func fileName(slug: String) -> String {
        slug + fileSuffix
    }

    /// The file for an app: its identity, the head package and, when switched
    /// on, every record as `{kind, data}` oldest first (createdAt, then id).
    /// Never ids, authors, times or grants; no `records` key when there are
    /// none or the switch is off.
    public static func build(
        name: String,
        icon: String?,
        color: String?,
        description: String?,
        package: CardJSON,
        records: [Record],
        includeRecords: Bool
    ) -> CardJSON {
        var file: [String: CardJSON] = [
            "format": .string(format),
            "app": .object([
                "name": .string(name),
                "icon": icon.map(CardJSON.string) ?? .null,
                "color": color.map(CardJSON.string) ?? .null,
                "description": .string(description ?? ""),
            ]),
            "package": package,
        ]
        if includeRecords, !records.isEmpty {
            let at = { (r: Record) in WireTimestamps.parse(r.createdAt)?.timeIntervalSince1970 ?? 0 }
            let sorted = records.sorted { a, b in
                let ta = at(a), tb = at(b)
                return ta != tb ? ta < tb : a.id < b.id
            }
            file["records"] = .array(sorted.map { .object(["kind": .string($0.kind), "data": $0.data]) })
        }
        return .object(file)
    }

    /// The file's text: two-space JSON and a trailing newline.
    public static func text(_ file: CardJSON) -> String {
        let options: JSONSerialization.WritingOptions = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        guard let data = try? JSONSerialization.data(withJSONObject: file.any, options: options),
              let text = String(data: data, encoding: .utf8)
        else { return file.compactString + "\n" }
        return text + "\n"
    }

    /// Writes the file into a fresh temporary directory (the share sheet's
    /// URL) and returns its location.
    public static func writeTemporaryFile(slug: String, file: CardJSON) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("app-export-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent(fileName(slug: slug))
        try Data(text(file).utf8).write(to: url, options: .atomic)
        return url
    }

    /// The issue rows shown under the import field: at most
    /// `limits.issuesShown`, then `and {n} more` (nil when none hidden).
    public static func visibleIssues(_ issues: [ImportIssue]) -> (shown: [ImportIssue], more: String?) {
        let shown = Array(issues.prefix(Limits.issuesShown))
        let hidden = issues.count - shown.count
        return (shown, hidden > 0 ? Copy.moreIssues(hidden) : nil)
    }
}

/// One server import issue: a JSON pointer into the file and its message.
public struct ImportIssue: Decodable, Sendable, Equatable, Hashable {
    public let path: String
    public let message: String

    public init(path: String, message: String) {
        self.path = path
        self.message = message
    }

    /// The row's text: `path: message`.
    public var line: String { "\(path): \(message)" }
}
