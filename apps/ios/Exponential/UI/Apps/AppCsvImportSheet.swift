import ExpCore
import ExpUI
import Foundation
import SwiftUI
import UniformTypeIdentifiers
#if canImport(ExponentialUICore)
import ExponentialUICore
#endif

/// The CSV import's subject: the picked kind + its schema.
struct AppCsvImportTarget: Identifiable {
    let kind: String
    let schema: CardJSON

    var id: String { kind }
}

/// VAPP-228 (fixtures/app-records.json `csvImport`, PHONES): the Data tab's
/// spreadsheet import, the app import sheet's pattern (`AppImportStep`):
/// paste a CSV or choose a file (.csv/.tsv/.txt), then map the columns onto
/// the kind's fields (`AppRecords.csvAutoMap` picks first; a team OWNER also
/// gets the Author picker FIRST, `Me` then the header), preview the first
/// `previewRows` rows as the Data tab's rows with their issues (the model's
/// + eUI `validateRecord` on a row with none), then ONE
/// `apps.records.batch` (`via: import`) of the ready rows; the toast's Undo
/// deletes them again. A BAD_REQUEST = the server's message inline; anything
/// else (the plan limit) = the failed-action toast.
struct AppCsvImportSheet: View {
    let appId: String
    let target: AppCsvImportTarget
    /// The importer (the viewer's user id).
    let me: String?
    /// A team OWNER's members (nil = no Author picker, every row is mine).
    let members: [AppRecords.CsvMember]?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var table = AppRecords.CsvTable(header: [], rows: [])
    /// Field → column (absent = Skip).
    @State private var mapping: [String: Int] = [:]
    /// The Author column (nil = `Me`).
    @State private var authorColumn: Int?
    @State private var records: [AppRecords.CsvRecord] = []
    @State private var choosing = false
    @State private var submitting = false
    @State private var error: String?

    private static let skip = "__skip__"

    private var fields: [String] { AppRecords.dataColumns(target.schema) }

    private var required: Set<String> {
        guard case let .object(object) = target.schema, case let .array(names)? = object["required"] else { return [] }
        return Set(names.compactMap { name -> String? in
            if case let .string(value) = name { return value }
            return nil
        })
    }

    private var ready: [AppRecords.CsvRecord] { records.filter { $0.issues.isEmpty } }
    private var tooMany: Bool { records.count > AppRecords.CsvImport.maxRows }

    var body: some View {
        let ready = ready
        return GlassSheetChrome(title: csvFill(AppRecords.CsvImport.Copy.title, "{kind}", target.kind), headerTrailing: {
            GlassPill(AppRecords.DataTab.Copy.cancel, mode: .action { dismiss() })
                .disabled(submitting)
                .accessibilityIdentifier("app-csv-cancel")
        }) {
            VStack(alignment: .leading, spacing: 12) {
                GlassTextField(
                    AppRecords.CsvImport.Copy.placeholder,
                    text: Binding(get: { text }, set: { text = AppImportStep.typed($0, after: text) }),
                    lines: 6...12,
                    accessibilityIdentifier: AppRecords.CsvImport.TestIds.text
                )
                .font(.expMono(.footnote))
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                HStack {
                    // EXP-1676: web's outline `Button size="sm"` (h-8), not a chip.
                    Button {
                        UIApplication.endEditing()
                        choosing = true
                    } label: {
                        GlassSubmitLabel(AppRecords.CsvImport.Copy.chooseFile, small: true, outline: true)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.pressScale)
                    .fixedSize()
                    .accessibilityIdentifier(AppRecords.CsvImport.TestIds.file)
                    Spacer(minLength: 0)
                }
                if !table.header.isEmpty {
                    mapSection
                }
                if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, records.isEmpty {
                    Text(AppRecords.CsvImport.Copy.noRows)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 4)
                }
                if !records.isEmpty {
                    previewSection
                }
                // FEED-79: invalid input = inline, never a toast.
                if let error {
                    Text(error)
                        .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier(AppRecords.CsvImport.TestIds.error)
                }
            }
            .padding(16)
        } primaryAction: {
            GlassSubmitButton(
                csvFill(AppRecords.CsvImport.Copy.importAction, "{n}", String(ready.count)),
                enabled: !tooMany && !ready.isEmpty,
                loading: submitting
            ) {
                runImport()
            }
            .accessibilityIdentifier(AppRecords.CsvImport.TestIds.confirm)
        }
        .interactiveDismissDisabled(submitting)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppRecords.CsvImport.TestIds.dialog)
        .onChange(of: text) { _, next in
            error = nil
            let parsed = AppRecords.csvParse(next)
            // A new header = a new first pick.
            if parsed.header != table.header {
                mapping = AppRecords.csvAutoMap(fields: fields, header: parsed.header)
                authorColumn = members == nil ? nil : AppRecords.csvAuthorColumn(parsed.header)
            }
            table = parsed
            recompute()
        }
        .onChange(of: mapping) { _, _ in recompute() }
        .onChange(of: authorColumn) { _, _ in recompute() }
        .fileImporter(
            isPresented: $choosing,
            allowedContentTypes: [.commaSeparatedText, .tabSeparatedText, .plainText]
        ) { result in
            guard case let .success(url) = result else { return }
            Task { await read(url) }
        }
    }

    // MARK: - Mapping

    private var mapSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            if members != nil {
                VStack(alignment: .leading, spacing: 6) {
                    label(AppRecords.DataTab.Copy.author, required: true)
                    columnPicker(
                        title: AppRecords.DataTab.Copy.author,
                        value: authorColumn,
                        none: AppRecords.CsvImport.Copy.me,
                        noneFirst: true
                    ) { next in
                        authorColumn = next
                    }
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier(AppRecords.CsvImport.TestIds.author)
            }
            ForEach(fields, id: \.self) { name in
                VStack(alignment: .leading, spacing: 6) {
                    label(name, required: required.contains(name))
                    columnPicker(
                        title: name,
                        value: mapping[name],
                        none: AppRecords.CsvImport.Copy.skip,
                        noneFirst: false,
                        identifier: "app-csv-field-\(name)"
                    ) { next in
                        mapping[name] = next
                    }
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppRecords.CsvImport.TestIds.map)
    }

    private func label(_ name: String, required: Bool) -> some View {
        HStack(spacing: 6) {
            Text(name)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
            if !required {
                Text(AppRecords.DataTab.Copy.optional)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
        .padding(.horizontal, 4)
    }

    /// THE select control (the record form's): the header's columns, `none`
    /// (= nil) last, or first for the Author picker.
    private func columnPicker(
        title: String,
        value: Int?,
        none: String,
        noneFirst: Bool,
        identifier: String? = nil,
        onPick: @escaping (Int?) -> Void
    ) -> some View {
        let columns = table.header.enumerated().map { PickerItem(value: String($0.offset), label: $0.element) }
        let noneItem = PickerItem(value: Self.skip, label: none)
        let items = noneFirst ? [noneItem] + columns : columns + [noneItem]
        let current = value.map(String.init) ?? Self.skip
        let shown = value.flatMap { $0 >= 0 && $0 < table.header.count ? table.header[$0] : nil } ?? none
        return GlassPicker(
            items: items,
            mode: .single,
            value: [current],
            onChange: { next in
                guard let next = next.first else { return }
                onPick(next == Self.skip ? nil : Int(next))
            },
            title: title
        ) {
            GlassPickerRowLabel(title, value: shown)
                .font(.subheadline)
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
        }
        .glassSection()
        .accessibilityIdentifier(identifier ?? "app-csv-field-\(title)")
    }

    // MARK: - Preview

    private var previewSection: some View {
        let fields = fields
        return VStack(alignment: .leading, spacing: 6) {
            VStack(spacing: 0) {
                ForEach(Array(records.prefix(AppRecords.CsvImport.previewRows)), id: \.row) { record in
                    previewRow(record, fields: fields)
                }
            }
            .glassSection()
            Text(AppRecords.csvSummary(records))
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .padding(.horizontal, 4)
            if tooMany {
                Text(csvFill(AppRecords.CsvImport.Copy.tooMany, "{max}", String(AppRecords.CsvImport.maxRows)))
                    .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 4)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppRecords.CsvImport.TestIds.preview)
    }

    /// The Data tab's row look: the first field's cell over the others
    /// (` · `), then the row's issues (at most `issuesShown`, then `+{n} more`).
    private func previewRow(_ record: AppRecords.CsvRecord, fields: [String]) -> some View {
        let first = fields.first.map { AppRecords.cellText(record.data[$0]) } ?? ""
        let subtitle = fields.dropFirst()
            .map { AppRecords.cellText(record.data[$0]) }
            .filter { !$0.isEmpty }
            .joined(separator: UserMessageCaption.separator)
        let shown = AppRecords.CsvImport.issuesShown
        return VStack(alignment: .leading, spacing: 2) {
            Text(first.isEmpty ? AppRecords.DataTab.Copy.emptyTitle : first)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(first.isEmpty ? Color.white.opacity(TextOpacity.secondary) : Color.white)
                .lineLimit(1)
            if !subtitle.isEmpty {
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
            if !record.issues.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(record.issues.prefix(shown).enumerated()), id: \.offset) { _, issue in
                        Text(issue)
                    }
                    if record.issues.count > shown {
                        Text(csvFill(AppRecords.CsvImport.Copy.moreIssues, "{n}", String(record.issues.count - shown)))
                    }
                }
                .font(.expMono(.caption))
                .foregroundStyle(DesignTokens.Semantic.red)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 2)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .combine)
    }

    // MARK: - Records

    /// The model's records, then the schema check (eUI `validateRecord`)
    /// appended to a row with no issues so far.
    private func recompute() {
        let author: AppRecords.CsvAuthor? = {
            guard let members, let me else { return nil }
            return AppRecords.CsvAuthor(column: authorColumn, importer: me, members: members)
        }()
        let schema = target.schema
        records = AppRecords.csvRecords(
            schema: schema, header: table.header, rows: table.rows, mapping: mapping, author: author
        ).map { record in
            guard record.issues.isEmpty else { return record }
            let issues = Self.schemaIssues(schema: schema, record: record)
            guard !issues.isEmpty else { return record }
            var next = record
            next.issues = issues
            return next
        }
    }

    /// `row {row}: {path}: {message}` per eUI issue (the path without its
    /// leading `/`); no eUI core linked = none (the server checks again).
    private static func schemaIssues(schema: CardJSON, record: AppRecords.CsvRecord) -> [String] {
        #if canImport(ExponentialUICore)
        guard let json = try? validateRecordJson(schemaJson: schema.compactString, dataJson: CardJSON.object(record.data).compactString),
              let parsed = try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [[String: Any]]
        else { return [] }
        return parsed.map { issue in
            var path = (issue["path"] as? String) ?? ""
            if path.hasPrefix("/") { path.removeFirst() }
            let message = (issue["message"] as? String) ?? ""
            return csvFill(csvFill(csvFill(AppRecords.CsvImport.Copy.issue, "{row}", String(record.row)), "{field}", path), "{message}", message)
        }
        #else
        return []
        #endif
    }

    // MARK: - File + import

    private func read(_ url: URL) async {
        let content: String? = await Task.detached {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            guard let data = try? Data(contentsOf: url) else { return nil }
            return String(data: data, encoding: .utf8)
        }.value
        if let content {
            text = content
        } else {
            toaster.error(FeedbackStates.Errors.title(verb: "read", thing: "the file"))
        }
    }

    /// ONE `apps.records.batch` of the ready rows, in row order; then dismiss
    /// and toast `{n} records imported` with Undo.
    private func runImport() {
        let ready = ready
        guard !submitting, !tooMany, !ready.isEmpty else { return }
        UIApplication.endEditing()
        submitting = true
        error = nil
        let kind = target.kind
        let writes: [CardJSON] = ready.map { record in
            var write: [String: CardJSON] = ["op": .string("upsert"), "kind": .string(kind), "data": .object(record.data)]
            if let author = record.author { write["author"] = .string(author) }
            return .object(write)
        }
        let api = deps.appsApi
        let account = accountId
        let appId = appId
        Task {
            defer { submitting = false }
            do {
                let result = try await api.recordsBatch(accountId: account, appId: appId, writes: writes, via: "import")
                dismiss()
                AppRecordUndo.toast(
                    .batch, appId: appId, eventIds: result.eventIds ?? [], via: "editor",
                    accountId: account, api: api, toaster: toaster,
                    title: csvFill(AppRecords.CsvImport.Copy.imported, "{n}", String(result.ids.count))
                )
            } catch {
                if error.trpcErrorCode == "BAD_REQUEST", !error.isPlanLimitError {
                    self.error = error.trpcUserMessage
                } else {
                    toaster.error(
                        FeedbackStates.Errors.title(verb: "import", thing: "the records"),
                        description: error.trpcUserMessage
                    )
                }
            }
        }
    }
}

/// A copy template's `{token}` filled in.
private func csvFill(_ template: String, _ token: String, _ value: String) -> String {
    template.replacingOccurrences(of: token, with: value)
}
