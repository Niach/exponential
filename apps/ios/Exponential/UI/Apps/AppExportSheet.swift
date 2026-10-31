import ExpCore
import ExpUI
import GRDB
import SwiftUI
import UniformTypeIdentifiers

/// VAPP-121 (fixtures/app-export.json): Export = ONE sheet titled
/// `copy.exportTitle`, a single `Include records` switch row (only when the
/// app has synced records, off by default) and the Export button, which
/// writes `<slug>.app.json` into a temporary directory and hands it to the
/// share sheet: the identity, the head package and, switched on, the records
/// as {kind, data}. Built on the device from the synced rows.
struct AppExportSheet: View {
    let app: AppListEntry

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @Environment(\.toaster) private var toaster
    @State private var includeRecords = false
    @State private var records: [AppRecordEntity] = []
    @State private var share: ShareTarget?
    /// The temporary file's directory, removed once the share sheet closes.
    @State private var exportedDir: URL?

    var body: some View {
        GlassSheetChrome(
            title: AppExport.Copy.exportTitle(app.name),
            content: {
                VStack(spacing: 0) {
                    if !records.isEmpty {
                        GlassToggleRow(AppExport.Copy.includeRecords, isOn: $includeRecords)
                            .font(.subheadline)
                            .accessibilityIdentifier(AppExport.TestIds.records)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 10)
                            .glassRow()
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, records.isEmpty ? 0 : 8)
            },
            primaryAction: {
                GlassSubmitButton(AppExport.Copy.export, enabled: app.package != nil) {
                    exportFile()
                }
                .accessibilityIdentifier(AppExport.TestIds.confirm)
            }
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppExport.TestIds.dialog)
        .task(id: app.id) { loadRecords() }
        // The share sheet is the system's (iOS: Export = the share sheet).
        .sheet(item: $share, onDismiss: {
            if let exportedDir { try? FileManager.default.removeItem(at: exportedDir) }
            exportedDir = nil
            dismiss()
        }) { target in
            ActivityShareSheet(items: [target.url])
        }
    }

    private func loadRecords() {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let appId = app.id
        records = (try? pool.read { db in
            try AppRecordEntity.filter(Column("app_id") == appId).fetchAll(db)
        }) ?? []
    }

    private func exportFile() {
        do {
            guard let text = app.package, let package = CardJSON(any: try JSONSerialization.jsonObject(with: Data(text.utf8)))
            else { throw AppExportError.noPackage }
            let file = AppExport.build(
                name: app.name,
                icon: app.icon,
                color: app.color,
                description: app.description,
                package: package,
                records: records.map { row in
                    let data = (try? JSONSerialization.jsonObject(with: Data(row.data.utf8))).flatMap(CardJSON.init(any:))
                    return AppExport.Record(id: row.id, kind: row.kind, data: data ?? .object([:]), createdAt: row.createdAt)
                },
                includeRecords: includeRecords
            )
            let url = try AppExport.writeTemporaryFile(slug: app.slug, file: file)
            exportedDir = url.deletingLastPathComponent()
            share = ShareTarget(url: url, text: AppExport.fileName(slug: app.slug))
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "export", thing: "the app"),
                description: error.trpcUserMessage
            )
        }
    }
}

private enum AppExportError: LocalizedError {
    case noPackage

    var errorDescription: String? { Apps.Copy.previewEmpty }
}

/// VAPP-121: the import step (`copy.importTitle`; VAPP-176: its own sheet,
/// opened by the Apps band's `Import…` pill): paste or choose an
/// app file; as the text changes (debounced) the SERVER lists its issues as
/// `path: message` rows; Import (enabled only with none) creates the app
/// and opens its editor.
struct AppImportStep: View {
    let teamId: String
    /// The imported app's id.
    let onImported: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @Environment(\.toaster) private var toaster
    @State private var text = ""
    /// The server's issues for `text` (nil = not checked yet / empty).
    @State private var issues: [ImportIssue]?
    @State private var choosing = false
    @State private var importing = false

    var body: some View {
        GlassSheetChrome(
            title: AppExport.Copy.importTitle,
            content: {
                VStack(alignment: .leading, spacing: 8) {
                    GlassTextField(
                        AppExport.Copy.importPlaceholder,
                        text: Binding(get: { text }, set: { text = Self.typed($0, after: text) }),
                        lines: 6...12,
                        accessibilityIdentifier: AppExport.TestIds.importText
                    )
                    .font(.system(.footnote, design: .monospaced))
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.asciiCapable)
                    HStack {
                        GlassPill(AppExport.Copy.chooseFile, mode: .action {
                            UIApplication.endEditing()
                            choosing = true
                        })
                        .accessibilityIdentifier(AppExport.TestIds.importFile)
                        Spacer(minLength: 0)
                    }
                    if let issues, !issues.isEmpty {
                        let visible = AppExport.visibleIssues(issues)
                        VStack(alignment: .leading, spacing: 4) {
                            ForEach(visible.shown, id: \.self) { issue in
                                Text(issue.line)
                            }
                            if let more = visible.more {
                                Text(more)
                            }
                        }
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier(AppExport.TestIds.importIssues)
                    }
                }
                .padding(16)
            },
            primaryAction: {
                GlassSubmitButton(
                    AppExport.Copy.importAction,
                    enabled: issues?.isEmpty == true && !importing,
                    loading: importing
                ) {
                    Task { await runImport() }
                }
                .accessibilityIdentifier(AppExport.TestIds.importConfirm)
            }
        )
        .task(id: text) { await validate(text) }
        .fileImporter(isPresented: $choosing, allowedContentTypes: [.json]) { result in
            guard case let .success(url) = result else { return }
            Task { await read(url) }
        }
    }

    /// A typed character lands as-is: the keyboard's smart quotes would turn
    /// JSON's `"` into `“`, so a short append is straightened (a paste or a
    /// chosen file is never touched).
    static func typed(_ next: String, after previous: String) -> String {
        guard next.hasPrefix(previous), next.count - previous.count <= 2 else { return next }
        let added = next.dropFirst(previous.count)
            .replacingOccurrences(of: "\u{201C}", with: "\"")
            .replacingOccurrences(of: "\u{201D}", with: "\"")
            .replacingOccurrences(of: "\u{2018}", with: "'")
            .replacingOccurrences(of: "\u{2019}", with: "'")
        return previous + added
    }

    /// Debounced (300 ms) `apps.validateImport`; empty text = no rows.
    private func validate(_ value: String) async {
        guard !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            issues = nil
            return
        }
        try? await Task.sleep(for: .milliseconds(300))
        guard !Task.isCancelled else { return }
        do {
            let check = try await deps.appsApi.validateImport(accountId: accountId, teamId: teamId, text: value)
            guard !Task.isCancelled, value == text else { return }
            issues = check.issues
        } catch {
            guard !Task.isCancelled else { return }
            issues = nil
        }
    }

    private func read(_ url: URL) async {
        let content: String? = await Task.detached {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            guard let data = try? Data(contentsOf: url), data.count <= AppExport.Limits.importMaxBytes else { return nil }
            return String(data: data, encoding: .utf8)
        }.value
        if let content {
            text = content
        } else {
            toaster.error(FeedbackStates.Errors.title(verb: "read", thing: "the file"))
        }
    }

    private func runImport() async {
        guard issues?.isEmpty == true, !importing else { return }
        importing = true
        do {
            let created = try await deps.appsApi.importApp(accountId: accountId, teamId: teamId, text: text)
            dismiss()
            onImported(created.id)
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "import", thing: "the app"),
                description: error.trpcUserMessage
            )
            importing = false
        }
    }
}
