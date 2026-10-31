import ExpCore
import ExpUI
import Foundation
import GRDB
import SwiftUI

/// VAPP-178 (fixtures/app-records.json `dataTab`, PHONES): the editor's DATA
/// tab = the app's RECORDS of the head package. KINDS = the head manifest's
/// `records` keys sorted by name, each with its live count; none (a draft,
/// or a package without records) = the list-size empty state
/// `copy.noKinds`. TOOLBAR: the kind picker (`{kind} · {n}`, the first kind
/// picked on open) + `Add record`. ROWS of the picked kind, `updatedAt`
/// desc (`AppRecords.dataRows`), in ONE grouped list: title = the first
/// column's cell (`copy.emptyTitle` when empty), subtitle = the other
/// columns' non-empty cells joined ` · `, trailing the relative `updated` +
/// the `…` menu (Edit / Delete). None = `copy.empty` under the toolbar.
/// Add / Edit = `AppRecordFormSheet`; Delete asks first (THE glass alert).
/// Writes = `apps.records.upsert` / `apps.records.delete` as the viewer.
struct AppDataTab: View {
    let app: AppListEntry

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var model = AppDataModel()
    /// The picked kind (nil = the first one).
    @State private var picked: String?
    @State private var form: AppDataFormTarget?
    @State private var menuTarget: AppDataRowTarget?
    /// The menu's pick, run once its sheet has dismissed.
    @State private var pendingMenu: (row: AppDataRowTarget, action: AppDataRowMenuSheet.Action)?
    @State private var deleting: AppDataRowTarget?

    /// The head package (its manifest carries `records`); nil = a draft.
    private var manifest: CardJSON? {
        guard let text = app.package,
              let parsed = try? JSONSerialization.jsonObject(with: Data(text.utf8))
        else { return nil }
        return CardJSON(any: parsed)
    }

    var body: some View {
        let manifest = manifest
        let kinds = AppRecords.dataKinds(manifest: manifest, rowKinds: model.rows.map(\.kind))
        let kind = kinds.first(where: { $0.kind == picked }) ?? kinds.first
        return ZStack {
            AppBackground()
            if let kind {
                content(kinds: kinds, kind: kind, schema: AppRecords.dataSchema(manifest: manifest, kind: kind.kind) ?? .object([:]))
            } else {
                EmptyState(title: AppRecords.DataTab.Copy.noKinds, icon: AppIcons.navApps, size: .list)
                    .padding(.horizontal, 24)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.data)
        .onAppear { model.bind(accountId: accountId, appId: app.id, db: deps.db) }
        .onChange(of: app.id) { _, next in
            picked = nil
            model.bind(accountId: accountId, appId: next, db: deps.db)
        }
        .onDisappear { model.stop() }
        // Each presentation on its OWN node (EXP-240).
        .background {
            Color.clear
                .sheet(item: $form) { target in
                    AppRecordFormSheet(appId: app.id, target: target)
                }
        }
        .background {
            Color.clear
                .sheet(item: $menuTarget, onDismiss: runPendingMenu) { target in
                    AppDataRowMenuSheet(title: target.title) { action in
                        pendingMenu = (target, action)
                        menuTarget = nil
                    }
                }
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $deleting) { target in
                    GlassAlert(
                        title: AppRecords.DataTab.Copy.deleteTitle,
                        message: AppRecords.DataTab.Copy.deleteBody,
                        actions: [
                            GlassAlertAction(AppRecords.DataTab.Copy.cancel, role: .outline, isCancel: true, id: "cancel") {},
                            GlassAlertAction(AppRecords.DataTab.Copy.delete, role: .destructive, id: "app-data-delete-confirm") {
                                delete(target)
                            },
                        ]
                    )
                }
        }
    }

    private func content(kinds: [AppRecords.DataKind], kind: AppRecords.DataKind, schema: CardJSON) -> some View {
        let columns = AppRecords.dataColumns(schema)
        let rows = AppRecords.dataRows(model.rows, kind: kind.kind)
        return ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                toolbar(kinds: kinds, kind: kind, schema: schema)
                if rows.isEmpty {
                    if model.loaded {
                        EmptyState(title: AppRecords.DataTab.Copy.empty, icon: AppIcons.navApps, size: .list)
                            .padding(.vertical, 8)
                    }
                } else {
                    VStack(spacing: 0) {
                        ForEach(rows, id: \.id) { row in
                            rowView(row, kind: kind.kind, schema: schema, columns: columns)
                        }
                    }
                    .glassSection()
                }
            }
            .padding(16)
        }
        .scrollContentBackground(.hidden)
    }

    /// The kind picker (THE picker, `{kind} · {n}`) and `Add record`.
    private func toolbar(kinds: [AppRecords.DataKind], kind: AppRecords.DataKind, schema: CardJSON) -> some View {
        HStack(spacing: 8) {
            GlassPicker(
                items: kinds.map { PickerItem(value: $0.kind, label: AppRecords.DataTab.Copy.kindOption($0.kind, count: $0.count)) },
                mode: .single,
                value: [kind.kind],
                onChange: { next in
                    if let next = next.first { picked = next }
                }
            ) {
                // The pill's own tap is off; the trigger takes it.
                ZStack {
                    GlassPill(AppRecords.DataTab.Copy.kindOption(kind.kind, count: kind.count), size: .md) {
                        EmptyView()
                    } trailing: {
                        AppIcon(AppIcons.uiChevronDown, size: 12)
                    }
                    .monospacedDigit()
                    .allowsHitTesting(false)
                }
                .contentShape(Capsule())
            }
            .accessibilityIdentifier(AppRecords.DataTab.TestIds.kind)
            Spacer(minLength: 8)
            GlassPill(AppRecords.DataTab.Copy.addRecord, icon: AppIcons.uiAdd, size: .md, mode: .action {
                UIApplication.endEditing()
                form = AppDataFormTarget(kind: kind.kind, schema: schema, row: nil)
            })
            .accessibilityIdentifier(AppRecords.DataTab.TestIds.add)
        }
    }

    /// A row: the first column's cell over the others (` · `), the relative
    /// `updated`, the `…` menu.
    private func rowView(_ row: AppRecords.Row, kind: String, schema: CardJSON, columns: [String]) -> some View {
        let first = columns.first.map { AppRecords.cellText(row.data[$0]) } ?? ""
        let title = first.isEmpty ? AppRecords.DataTab.Copy.emptyTitle : first
        let subtitle = columns.dropFirst()
            .map { AppRecords.cellText(row.data[$0]) }
            .filter { !$0.isEmpty }
            .joined(separator: UserMessageCaption.separator)
        let target = AppDataRowTarget(row: row, kind: kind, schema: schema, title: title)
        return HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(first.isEmpty ? Color.white.opacity(TextOpacity.secondary) : Color.white)
                    .lineLimit(1)
                if !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Text(RelativeTime.compact(wire: row.updatedAt))
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Button {
                UIApplication.endEditing()
                menuTarget = target
            } label: {
                AppIcon(AppIcons.uiMore, size: AppIcon.Size.small, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 24, height: 24)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Menu for \(title)")
            .accessibilityIdentifier(AppRecords.DataTab.TestIds.menu)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppRecords.DataTab.TestIds.row)
    }

    private func runPendingMenu() {
        guard let pick = pendingMenu else { return }
        pendingMenu = nil
        switch pick.action {
        case .edit: form = AppDataFormTarget(kind: pick.row.kind, schema: pick.row.schema, row: pick.row.row)
        case .delete: deleting = pick.row
        }
    }

    /// The confirmed Delete: `apps.records.delete`; the synced row is the
    /// echo, a failure = the failed-action toast.
    private func delete(_ target: AppDataRowTarget) {
        let api = deps.appsApi
        let account = accountId
        let appId = app.id
        Task {
            do {
                try await api.recordsDelete(accountId: account, appId: appId, recordId: target.row.id)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "delete", thing: "the record"),
                    description: error.trpcUserMessage
                )
            }
        }
    }
}

/// A row the `…` menu / the delete prompt acts on.
struct AppDataRowTarget: Identifiable {
    let row: AppRecords.Row
    let kind: String
    let schema: CardJSON
    let title: String

    var id: String { row.id }
}

/// The Add / Edit form's subject: a kind (+ its schema) and the row being
/// edited (nil = a new record).
struct AppDataFormTarget: Identifiable {
    let kind: String
    let schema: CardJSON
    let row: AppRecords.Row?

    var id: String { row.map { "edit-\($0.id)" } ?? "new-\(kind)" }
}

/// A record row's `…` — THE Menu in its sheet presentation: Edit · Delete
/// (destructive). A pick hands the action back; the host acts once the
/// sheet has dismissed.
struct AppDataRowMenuSheet: View {
    enum Action { case edit, delete }

    let title: String
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: title) {
            VStack(spacing: 0) {
                Button { onPick(.edit) } label: {
                    GlassMenuItemLabel(AppRecords.DataTab.Copy.edit, icon: AppIcons.uiEdit)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("app-data-menu-edit")
                Button { onPick(.delete) } label: {
                    GlassMenuItemLabel(AppRecords.DataTab.Copy.delete, icon: AppIcons.uiDelete, destructive: true)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("app-data-menu-delete")
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
        }
    }
}

/// ADD / EDIT (phones: a sheet) titled `New {kind}` / `Edit {kind}`: one
/// labelled field per column (`copy.optional` muted after an optional one),
/// the control from `AppRecords.formFields`; edit pre-fills with
/// `formInput`. SAVE turns the inputs into data with `formData` (create
/// omits the nulls, edit keeps them) and calls `apps.records.upsert`. A
/// BAD_REQUEST (the schema's issues) = the server's message INLINE under
/// the form; anything else (the plan limit included) = the toast.
struct AppRecordFormSheet: View {
    let appId: String
    let target: AppDataFormTarget

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.dismiss) private var dismiss
    @State private var inputs: [String: AppRecords.DataInput]
    @State private var saving = false
    @State private var error: String?

    private let fields: [AppRecords.DataFormField]

    init(appId: String, target: AppDataFormTarget) {
        self.appId = appId
        self.target = target
        let fields = AppRecords.formFields(target.schema)
        self.fields = fields
        var seeded: [String: AppRecords.DataInput] = [:]
        for field in fields {
            seeded[field.name] = AppRecords.formInput(field.control, target.row?.data[field.name])
        }
        _inputs = State(initialValue: seeded)
    }

    private var title: String {
        target.row == nil
            ? AppRecords.DataTab.Copy.newTitle(target.kind)
            : AppRecords.DataTab.Copy.editTitle(target.kind)
    }

    var body: some View {
        GlassSheetChrome(title: title) {
            GlassPill(AppRecords.DataTab.Copy.cancel, mode: .action { dismiss() })
                .accessibilityIdentifier("app-data-cancel")
        } content: {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(fields, id: \.name) { field in
                    fieldView(field)
                }
                // FEED-79: invalid input = inline under the form, never a toast.
                if let error {
                    Text(error)
                        .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier(AppRecords.DataTab.TestIds.error)
                }
            }
            .padding(16)
        } primaryAction: {
            GlassSubmitButton(
                AppRecords.DataTab.Copy.save,
                loading: saving,
                pendingWord: FeedbackStates.Pending.save
            ) {
                save()
            }
            .accessibilityIdentifier(AppRecords.DataTab.TestIds.save)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppRecords.DataTab.TestIds.form)
    }

    // MARK: - Fields

    private func label(_ field: AppRecords.DataFormField) -> some View {
        HStack(spacing: 6) {
            Text(field.name)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
            if !field.required {
                Text(AppRecords.DataTab.Copy.optional)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
        .padding(.horizontal, 4)
    }

    private func textBinding(_ name: String) -> Binding<String> {
        Binding(
            get: { inputs[name]?.text ?? "" },
            set: { inputs[name] = .text($0) }
        )
    }

    private func flagBinding(_ name: String) -> Binding<Bool> {
        Binding(
            get: { inputs[name]?.flag ?? false },
            set: { inputs[name] = .flag($0) }
        )
    }

    @ViewBuilder
    private func fieldView(_ field: AppRecords.DataFormField) -> some View {
        let identifier = "app-data-field-\(field.name)"
        switch field.control {
        case .switch:
            HStack(spacing: 12) {
                label(field)
                    .frame(maxWidth: .infinity, alignment: .leading)
                GlassToggleRow(nil, isOn: flagBinding(field.name))
                    .accessibilityLabel(field.name)
                    .accessibilityIdentifier(identifier)
            }
        case .select:
            VStack(alignment: .leading, spacing: 6) {
                label(field)
                select(field, identifier: identifier)
            }
        case .text, .number, .list:
            VStack(alignment: .leading, spacing: 6) {
                label(field)
                GlassTextField(
                    field.control == .list ? AppRecords.DataTab.Copy.listHint : (field.placeholder ?? ""),
                    text: textBinding(field.name),
                    accessibilityIdentifier: identifier
                )
                .font(.subheadline)
                .keyboardType(field.control == .number ? .numbersAndPunctuation : .default)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled(field.control != .text)
            }
        }
    }

    /// A string enum: its values; an optional one leads with `copy.none`
    /// (= null, the empty input). Inside this sheet the picker opens inline.
    private func select(_ field: AppRecords.DataFormField, identifier: String) -> some View {
        let options = field.options ?? []
        let current = inputs[field.name]?.text ?? ""
        var items: [PickerItem<String>] = field.required ? [] : [PickerItem(value: "", label: AppRecords.DataTab.Copy.none)]
        items += options.map { PickerItem(value: $0, label: $0) }
        return GlassPicker(
            items: items,
            mode: .single,
            value: [current],
            onChange: { next in
                if let next = next.first { inputs[field.name] = .text(next) }
            },
            title: field.name
        ) {
            GlassPickerRowLabel(field.name, value: current.isEmpty ? AppRecords.DataTab.Copy.none : current)
                .font(.subheadline)
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
        }
        .glassSection()
        .accessibilityIdentifier(identifier)
    }

    // MARK: - Save

    private func save() {
        guard !saving else { return }
        UIApplication.endEditing()
        saving = true
        error = nil
        let data = AppRecords.formData(target.schema, inputs: inputs, editing: target.row != nil)
        let api = deps.appsApi
        let account = accountId
        let kind = target.kind
        let recordId = target.row?.id
        let appId = appId
        Task {
            defer { saving = false }
            do {
                _ = try await api.recordsUpsert(accountId: account, appId: appId, kind: kind, data: data, recordId: recordId)
                dismiss()
            } catch {
                if error.trpcErrorCode == "BAD_REQUEST", !error.isPlanLimitError {
                    self.error = error.trpcUserMessage
                } else {
                    toaster.error(
                        FeedbackStates.Errors.title(verb: "save", thing: "the record"),
                        description: error.trpcUserMessage
                    )
                }
            }
        }
    }
}

/// The app's synced records, live off GRDB (trash never syncs), bound only
/// while the Data tab is on screen.
@MainActor
@Observable
final class AppDataModel {
    private(set) var rows: [AppRecords.Row] = []
    private(set) var loaded = false

    @ObservationIgnored private var task: Task<Void, Never>?

    nonisolated init() {}

    func bind(accountId: String, appId: String, db: DatabaseManager) {
        stop()
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> [AppRecordEntity] in
            try AppRecordEntity.filter(Column("app_id") == appId).fetchAll(db)
        }
        task = Task { [weak self] in
            do {
                for try await entities in observation.values(in: pool) {
                    guard let self else { return }
                    self.rows = entities.map(AppRecords.Row.init)
                    self.loaded = true
                }
            } catch {}
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }
}
