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
                GlassSubmitButton(AppExport.Copy.export, enabled: app.headPackage != nil) {
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
            // eApps V1: the export carries the head (the draft, else the published package).
            guard let text = app.headPackage, let package = CardJSON(any: try JSONSerialization.jsonObject(with: Data(text.utf8)))
            else { throw AppExportError.noPackage }
            let file = AppExport.build(
                name: app.name,
                icon: app.icon,
                color: app.color,
                description: app.description,
                package: package,
                records: records.map { row in
                    let data = (try? JSONSerialization.jsonObject(with: Data(row.data.utf8))).flatMap(CardJSON.init(any:))
                    return AppExport.Record(id: row.id, kind: row.kind, data: data ?? .object([:]), createdAt: row.createdAt, author: row.authorId ?? row.createdBy)
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

/// #VAPP-254 (`upstreamPickerCases`): which team server fills an upstream the
/// app declares: the Settings sheet's Accounts rows and the import step's. THE
/// picker with the Data tab's chevron pill as its trigger; no servers = nothing.
struct UpstreamPickerPill: View {
    let name: String
    let serverId: String?
    let servers: [AppServerWire]
    var disabled = false
    let testId: String
    let onPick: (String) -> Void

    var body: some View {
        if let picker = Apps.upstreamPicker(name: name, serverId: serverId, servers: servers.map { (id: $0.id, name: $0.name) }) {
            GlassPicker(
                items: picker.options.map { PickerItem(value: $0.value, label: $0.label) },
                mode: .single,
                value: [picker.value],
                onChange: { next in
                    if let next = next.first, next != picker.value { onPick(next) }
                },
                disabled: disabled
            ) {
                // The pill's own tap is off; the trigger takes it.
                ZStack {
                    GlassPill(
                        picker.options.first { $0.value == picker.value }?.label ?? Apps.Copy.Accounts.notSetUp,
                        enabled: !disabled
                    ) {
                        EmptyView()
                    } trailing: {
                        AppIcon(AppIcons.uiChevronDown, size: 12)
                    }
                    .allowsHitTesting(false)
                }
                .contentShape(Capsule())
            }
            .accessibilityIdentifier(testId)
        }
    }
}

/// VAPP-121: the import step (`copy.importTitle`; VAPP-176: its own sheet,
/// opened by the Apps band's `Import…` pill): paste or choose an
/// app file; as the text changes (debounced) the SERVER lists its issues as
/// `path: message` rows; Import (enabled only with none) creates the app
/// and opens its editor. VAPP-234 (fixtures/app-templates.json): the
/// TEMPLATES step above the field: one row per signed template (a tap =
/// `apps.importTemplate` at once, then its editor) and the sample-records
/// switch row. #VAPP-254: an owner's Accounts group, one server picker per
/// upstream the file declares (`upstreamPickerCases`).
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
    /// #VAPP-254: the file's upstreams and what an owner may pick for them.
    @State private var upstreams: [AppImportUpstreamWire] = []
    @State private var servers: [AppServerWire] = []
    /// The owner's picks by upstream name; an absent one = the exact-name default.
    @State private var picks: [String: String] = [:]
    @State private var choosing = false
    @State private var importing = false
    @State private var withRecords = false
    /// The template being imported (every row waits).
    @State private var picking: String?

    var body: some View {
        GlassSheetChrome(
            title: AppExport.Copy.importTitle,
            content: {
                VStack(alignment: .leading, spacing: 8) {
                    templates
                    GlassTextField(
                        AppExport.Copy.importPlaceholder,
                        text: Binding(get: { text }, set: { text = Self.typed($0, after: text) }),
                        lines: 6...12,
                        accessibilityIdentifier: AppExport.TestIds.importText
                    )
                    .font(.expMono(.footnote))
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.asciiCapable)
                    HStack {
                        // EXP-1676: web's outline `Button size="sm"` (h-8), not a chip.
                        Button {
                            UIApplication.endEditing()
                            choosing = true
                        } label: {
                            GlassSubmitLabel(AppExport.Copy.chooseFile, small: true, outline: true)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.pressScale)
                        .fixedSize()
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
                        .font(.expMono(.caption))
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier(AppExport.TestIds.importIssues)
                    }
                    if issues?.isEmpty == true, !upstreams.isEmpty, !servers.isEmpty {
                        upstreamsGroup
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

    /// #VAPP-254: one row per upstream the file declares: its name and the
    /// server picker, starting at the exact-name default.
    private var upstreamsGroup: some View {
        VStack(alignment: .leading, spacing: 8) {
            GlassSectionBand(Apps.Copy.Accounts.title)
            VStack(spacing: 0) {
                ForEach(Array(upstreams.enumerated()), id: \.element.name) { index, upstream in
                    if index > 0 { GlassDivider() }
                    InlinePickerRow {
                        HStack(spacing: 10) {
                            Text(upstream.name)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            UpstreamPickerPill(
                                name: upstream.name,
                                serverId: pickedServerId(upstream),
                                servers: servers,
                                disabled: importing,
                                testId: "\(Apps.TestIds.importUpstream)-\(upstream.name)-picker"
                            ) { picks[upstream.name] = $0 }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("\(Apps.TestIds.importUpstream)-\(upstream.name)")
                }
            }
            .glassSection()
        }
        .padding(.top, 4)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.importUpstreams)
    }

    private func pickedServerId(_ upstream: AppImportUpstreamWire) -> String? {
        guard let picked = picks[upstream.name] else { return upstream.serverId }
        return picked == Apps.upstreamPickerNone ? nil : picked
    }

    /// VAPP-234: the band, one row per template (glyph, name over the muted
    /// description), then the sample-records switch row.
    private var templates: some View {
        VStack(alignment: .leading, spacing: 8) {
            GlassSectionBand(AppTemplates.Copy.title)
            VStack(spacing: 0) {
                ForEach(Array(AppTemplates.templates.enumerated()), id: \.element.id) { index, template in
                    if index > 0 { GlassDivider() }
                    Button {
                        Task { await pickTemplate(template.id) }
                    } label: {
                        HStack(spacing: 10) {
                            AgentRowIcon(icon: template.icon, color: template.color)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(template.name)
                                    .font(.subheadline.weight(.medium))
                                    .foregroundStyle(.white)
                                    .lineLimit(1)
                                Text(template.description)
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                    .lineLimit(1)
                            }
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(picking != nil || importing)
                    .accessibilityIdentifier("\(AppTemplates.TestIds.row)-\(template.id)")
                }
            }
            .glassSection()
            GlassToggleRow(AppTemplates.Copy.includeRecords, isOn: $withRecords)
                .font(.subheadline)
                .disabled(picking != nil)
                .accessibilityIdentifier(AppTemplates.TestIds.records)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .glassRow()
        }
        .padding(.bottom, 4)
    }

    private func pickTemplate(_ template: String) async {
        guard picking == nil, !importing else { return }
        picking = template
        do {
            let created = try await deps.appsApi.importTemplate(accountId: accountId, teamId: teamId, template: template, includeRecords: withRecords)
            dismiss()
            onImported(created.id)
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "import", thing: "the template"),
                description: error.trpcUserMessage
            )
            picking = nil
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
            upstreams = []
            servers = []
            return
        }
        try? await Task.sleep(for: .milliseconds(300))
        guard !Task.isCancelled else { return }
        do {
            let check = try await deps.appsApi.validateImport(accountId: accountId, teamId: teamId, text: value)
            guard !Task.isCancelled, value == text else { return }
            issues = check.issues
            upstreams = check.upstreams
            servers = check.servers
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
            // #VAPP-254: the picks for names the file still declares, never `none`.
            let mappings = upstreams.compactMap { upstream -> AppUpstreamMapping? in
                guard let picked = picks[upstream.name], picked != Apps.upstreamPickerNone else { return nil }
                return AppUpstreamMapping(upstream: upstream.name, serverId: picked)
            }
            let created = try await deps.appsApi.importApp(accountId: accountId, teamId: teamId, text: text, upstreamMappings: mappings)
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
