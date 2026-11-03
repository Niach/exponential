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
/// VAPP-187: a row the viewer may change neither way (`canChangeRecord`:
/// only its author or a team owner) keeps its `…`, DISABLED.
/// VAPP-188: phones show no `by` cell (the wide table's); an OWNER's form
/// leads with the Author picker (`AppDataAuthorPicker`).
/// VAPP-215 (fixture `history`): every add / save / delete toasts Undo for
/// `undoSeconds` s (`AppRecordUndo`); Edit lists the record's History (who ·
/// when · what) with Restore on older rows.
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

    /// The head package (its manifest carries `records`; eApps V1: the
    /// draft, else the published one); nil = a draft.
    private var manifest: CardJSON? {
        guard let text = app.headPackage,
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
        .onAppear {
            model.manifest = manifest
            model.bind(accountId: accountId, appId: app.id, userId: deps.auth.userId, db: deps.db)
            model.loadAccess(accountId: accountId, appId: app.id, trpc: deps.trpc)
        }
        .onChange(of: app.id) { _, next in
            picked = nil
            model.bind(accountId: accountId, appId: next, userId: deps.auth.userId, db: deps.db)
            model.loadAccess(accountId: accountId, appId: next, trpc: deps.trpc)
        }
        .onChange(of: app.headPackage) { _, _ in model.manifest = manifest }
        .onDisappear { model.stop() }
        // Each presentation on its OWN node (EXP-240).
        .background {
            Color.clear
                .sheet(item: $form) { target in
                    AppRecordFormSheet(appId: app.id, target: target, author: model.authorPicker, names: model.names)
                }
        }
        .background {
            Color.clear
                .sheet(item: $menuTarget, onDismiss: runPendingMenu) { target in
                    AppDataRowMenuSheet(
                        title: target.title,
                        canEdit: model.may("update", kind: target.kind, row: target.row),
                        canDelete: model.may("delete", kind: target.kind, row: target.row)
                    ) { action in
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
            // eApps R3: what the server would refuse stays disabled.
            .disabled(!model.may("create", kind: kind.kind))
            .opacity(model.may("create", kind: kind.kind) ? 1 : 0.45)
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
        let locked = !model.canChange(row)
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
            Text(RelativeTime.long(wire: row.updatedAt))
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
            .disabled(locked)
            .opacity(locked ? 0.45 : 1)
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
                let result = try await api.recordsDelete(
                    accountId: account, appId: appId, recordId: target.row.id, via: "editor"
                )
                // VAPP-215: Undo for 10 s.
                AppRecordUndo.toast(
                    .delete, appId: appId, eventIds: result.eventId.map { [$0] } ?? [], via: "editor",
                    accountId: account, api: api, toaster: toaster
                )
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "delete", thing: "the record"),
                    description: error.trpcUserMessage
                )
            }
        }
    }
}

/// VAPP-215 (fixture `history`): the Undo toast a UI records write raises
/// (the Data tab's add / save / delete, a package action's batch): THE
/// success toast + its action `Undo` for `undoSeconds` s. Undo =
/// `apps.records.undo` (each event's `before` as a new write); a refusal =
/// the failed-action toast `Couldn't undo the change` (web `record-undo.ts`).
@MainActor
enum AppRecordUndo {
    /// Toast `write` with Undo; no events (a batch that changed nothing) = no toast.
    static func toast(
        _ write: AppRecords.UndoWrite, appId: String, eventIds: [String], via: String,
        accountId: String, api: AppsApi, toaster: Toaster
    ) {
        guard !eventIds.isEmpty else { return }
        toaster.success(
            AppRecords.undoTitle(write),
            action: ToastAction(label: AppRecords.History.Copy.undo) {
                Task { @MainActor in
                    await undo(appId: appId, eventIds: eventIds, via: via, accountId: accountId, api: api, toaster: toaster)
                }
            },
            durationMs: Int(AppRecords.History.undoSeconds * 1000)
        )
    }

    /// Run the undo; a refusal is the failed-action toast.
    static func undo(appId: String, eventIds: [String], via: String, accountId: String, api: AppsApi, toaster: Toaster) async {
        do {
            try await api.recordsUndo(accountId: accountId, appId: appId, eventIds: eventIds, via: via)
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: AppRecords.History.Copy.undoVerb, thing: AppRecords.History.Copy.undoThing),
                description: error.trpcUserMessage
            )
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

/// VAPP-188: an OWNER's Author picker — the viewer (the create's default)
/// and the team's members by name (`AppRecords.authorOptions`).
struct AppDataAuthorPicker {
    let viewer: String?
    let options: [AppRecords.AuthorOption]
}

/// A record row's `…` — THE Menu in its sheet presentation: Edit · Delete
/// (destructive). A pick hands the action back; the host acts once the
/// sheet has dismissed.
struct AppDataRowMenuSheet: View {
    enum Action { case edit, delete }

    let title: String
    /// eApps R3: only the ops the kind's policy allows the viewer.
    var canEdit = true
    var canDelete = true
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: title) {
            VStack(spacing: 0) {
                if canEdit {
                    Button { onPick(.edit) } label: {
                        GlassMenuItemLabel(AppRecords.DataTab.Copy.edit, icon: AppIcons.uiEdit)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-data-menu-edit")
                }
                if canDelete {
                    Button { onPick(.delete) } label: {
                        GlassMenuItemLabel(AppRecords.DataTab.Copy.delete, icon: AppIcons.uiDelete, destructive: true)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-data-menu-delete")
                }
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
/// VAPP-188: `author` (owners only, nil = none) = the FIRST field, the same
/// select control (required, no None) over the members by name, defaulting
/// to `formAuthor(...).defaultAuthor`; Save sends `author` only when
/// `formAuthor(...).send` names one. A `notMember` BAD_REQUEST = inline.
/// VAPP-215: EDIT lists the record's History under the fields
/// (`apps.records.history`, read on appear): one row per event, newest
/// first (`AppRecords.historyRows`), `who · when` muted under the label,
/// Restore (`apps.records.restore`) on every older row but deletes; none =
/// `No history yet`. A restore dismisses and toasts Undo.
struct AppRecordFormSheet: View {
    let appId: String
    let target: AppDataFormTarget
    let author: AppDataAuthorPicker?
    /// VAPP-215: member names for the History rows.
    let names: [String: String]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.dismiss) private var dismiss
    @State private var inputs: [String: AppRecords.DataInput]
    @State private var saving = false
    @State private var error: String?
    /// VAPP-188: the Author picker's pick (a user id; "" = none).
    @State private var pickedAuthor: String
    /// VAPP-215: the record's events (nil = loading).
    @State private var events: [AppRecords.RecordEvent]?
    /// VAPP-215: the event a Restore is running for.
    @State private var restoring: String?

    private let fields: [AppRecords.DataFormField]

    init(appId: String, target: AppDataFormTarget, author: AppDataAuthorPicker? = nil, names: [String: String] = [:]) {
        self.appId = appId
        self.target = target
        self.author = author
        self.names = names
        let base = AppRecords.formAuthor(viewer: author?.viewer, record: target.row, picked: nil).defaultAuthor
        _pickedAuthor = State(initialValue: base ?? "")
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
        // The header pill: labelled, a bare first trailing closure is ambiguous
        // between headerTrailing and pinnedHeader.
        GlassSheetChrome(title: title, headerTrailing: {
            GlassPill(AppRecords.DataTab.Copy.cancel, mode: .action { dismiss() })
                .accessibilityIdentifier("app-data-cancel")
        }) {
            VStack(alignment: .leading, spacing: 12) {
                if let author {
                    authorField(author)
                }
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
                if let record = target.row {
                    historySection(record)
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
        label(field.name, required: field.required)
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
        return selectControl(
            title: field.name,
            items: items,
            current: current,
            shown: current.isEmpty ? AppRecords.DataTab.Copy.none : current,
            identifier: identifier
        ) { next in
            inputs[field.name] = .text(next)
        }
    }

    /// VAPP-188: the owner's Author field — the same select control,
    /// required (no None): the members by name, the value a user id.
    private func authorField(_ author: AppDataAuthorPicker) -> some View {
        let name = AppRecords.DataTab.Copy.author
        let items = author.options.map { PickerItem(value: $0.id, label: $0.name) }
        let current = pickedAuthor
        // A pick outside the team (the author left) reads as the fallback.
        let shown = author.options.first(where: { $0.id == current })?.name ?? Apps.Copy.byAgent
        return VStack(alignment: .leading, spacing: 6) {
            label(name, required: true)
            selectControl(
                title: name, items: items, current: current, shown: shown,
                identifier: "app-data-field-\(name)"
            ) { next in
                pickedAuthor = next
            }
        }
    }

    /// THE select control of the form: the picker opening inline in this
    /// sheet, its row reading `shown`.
    private func selectControl(
        title: String,
        items: [PickerItem<String>],
        current: String,
        shown: String,
        identifier: String,
        onPick: @escaping (String) -> Void
    ) -> some View {
        GlassPicker(
            items: items,
            mode: .single,
            value: [current],
            onChange: { next in
                if let next = next.first { onPick(next) }
            },
            title: title
        ) {
            GlassPickerRowLabel(title, value: shown)
                .font(.subheadline)
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
        }
        .glassSection()
        .accessibilityIdentifier(identifier)
    }

    // MARK: - History (VAPP-215)

    @ViewBuilder
    private func historySection(_ record: AppRecords.Row) -> some View {
        let rows = events.map { AppRecords.historyRows($0, names: names, fallback: Apps.Copy.byAgent) } ?? []
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(AppRecords.History.Copy.history, loading: events == nil)
            if events != nil, rows.isEmpty {
                EmptyState(title: AppRecords.History.Copy.empty, size: .list)
                    .padding(.vertical, 8)
            } else if !rows.isEmpty {
                VStack(spacing: 0) {
                    ForEach(rows, id: \.id) { row in
                        historyRow(row, recordId: record.id)
                    }
                }
                .glassSection()
            }
        }
        .padding(.top, 8)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppRecords.DataTab.TestIds.history)
        .task(id: record.id) { await loadHistory(record.id) }
    }

    private func historyRow(_ row: AppRecords.HistoryRow, recordId: String) -> some View {
        let when = RelativeTime.long(wire: row.at)
        return HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(row.label)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(when.isEmpty ? row.who : "\(row.who)\(UserMessageCaption.separator)\(when)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if row.restorable {
                // The small ghost button (the Versions sheet's Preview).
                Button { restore(recordId: recordId, eventId: row.id) } label: {
                    Text(AppRecords.History.Copy.restore)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, GlassPillTokens.horizontalPaddingSm)
                        .frame(height: GlassPillTokens.heightSm)
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .disabled(restoring != nil)
                .opacity(restoring == nil || restoring == row.id ? 1 : 0.45)
                .accessibilityIdentifier(AppRecords.DataTab.TestIds.restore)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AppRecords.DataTab.TestIds.historyRow)
    }

    private func loadHistory(_ recordId: String) async {
        events = nil
        let loaded = try? await deps.appsApi.recordsHistory(accountId: accountId, appId: appId, recordId: recordId)
        events = loaded ?? []
    }

    /// Restore = the record as that event left it, as a new write; then
    /// dismiss and toast Undo for the events it wrote.
    private func restore(recordId: String, eventId: String) {
        guard restoring == nil else { return }
        restoring = eventId
        let api = deps.appsApi
        let account = accountId
        let appId = appId
        Task {
            defer { restoring = nil }
            do {
                let result = try await api.recordsRestore(
                    accountId: account, appId: appId, recordId: recordId, eventId: eventId, via: "editor"
                )
                dismiss()
                AppRecordUndo.toast(
                    .update, appId: appId, eventIds: result.eventIds, via: "editor",
                    accountId: account, api: api, toaster: toaster
                )
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "restore", thing: "the record"),
                    description: error.trpcUserMessage
                )
            }
        }
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
        // VAPP-188: owners only, and only when the pick moves the author.
        let sendAuthor: String? = author.flatMap { picker in
            AppRecords.formAuthor(
                viewer: picker.viewer, record: target.row, picked: pickedAuthor.isEmpty ? nil : pickedAuthor
            ).send
        }
        Task {
            defer { saving = false }
            do {
                let result = try await api.recordsUpsert(
                    accountId: account, appId: appId, kind: kind, data: data, recordId: recordId, author: sendAuthor,
                    via: "editor"
                )
                dismiss()
                // VAPP-215: Undo for 10 s.
                AppRecordUndo.toast(
                    result.created ? .create : .update, appId: appId, eventIds: result.eventId.map { [$0] } ?? [],
                    via: "editor", accountId: account, api: api, toaster: toaster
                )
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
    /// VAPP-187: the viewer and their role in the app's team (nil = not a
    /// member), for `canChange`.
    private(set) var viewer: String?
    private(set) var role: String?
    /// VAPP-188: the app's team members by name (`authorOptions`).
    private(set) var members: [AppRecords.AuthorOption] = []
    /// eApps R3: the viewer's app access (`appAccess.mine`; nil = write until known).
    private(set) var access: String?
    /// eApps R3: the head package (each kind's `policy`).
    var manifest: CardJSON?

    @ObservationIgnored private var task: Task<Void, Never>?

    nonisolated init() {}

    /// eApps R3: may the viewer `op` (a row of) `kind` under its policy and
    /// their app access (eUI record-policy.json)?
    func may(_ op: String, kind: String, row: AppRecords.Row? = nil) -> Bool {
        AppRecords.Policy.allowed(AppRecords.Policy.of(package: manifest, kind: kind), op: op, viewer: viewer, role: role, access: access, row: row)
    }

    /// May the viewer change the row at all (update or delete)? Neither =
    /// the row's `…` is disabled.
    func canChange(_ row: AppRecords.Row) -> Bool {
        may("update", kind: row.kind, row: row) || may("delete", kind: row.kind, row: row)
    }

    /// eApps R3: the viewer's own level for the app (`appAccess.mine`).
    func loadAccess(accountId: String, appId: String, trpc: TrpcClient) {
        Task { [weak self] in
            let level = try? await McpServersApi(trpc: trpc).appAccessMine(accountId: accountId, appId: appId).level
            await MainActor.run { self?.access = level }
        }
    }

    /// VAPP-215: the team's member names by id (the History rows' `who`).
    var names: [String: String] {
        Dictionary(members.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
    }

    /// VAPP-188: a team OWNER's Author picker; nil = no picker (and no
    /// `author` is ever sent).
    var authorPicker: AppDataAuthorPicker? {
        role == "owner" ? AppDataAuthorPicker(viewer: viewer, options: members) : nil
    }

    func bind(accountId: String, appId: String, userId: String?, db: DatabaseManager) {
        stop()
        viewer = userId
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let me = userId ?? ""
        let observation = ValueObservation.tracking { db -> ([AppRecordEntity], String?, [AppRecords.AuthorOption]) in
            let entities = try AppRecordEntity.filter(Column("app_id") == appId).fetchAll(db)
            let role = try String.fetchOne(
                db,
                sql: #"""
                    SELECT "role" FROM "team_members"
                    WHERE "team_id" = (SELECT "team_id" FROM "apps" WHERE "id" = ?) AND "user_id" = ?
                    LIMIT 1
                    """#,
                arguments: [appId, me]
            )
            // VAPP-188: the team's members with their names (else email).
            let memberRows = try GRDB.Row.fetchAll(
                db,
                sql: #"""
                    SELECT "tm"."user_id" AS "id", "u"."name" AS "name", "u"."email" AS "email"
                    FROM "team_members" AS "tm"
                    LEFT JOIN "users" AS "u" ON "u"."id" = "tm"."user_id"
                    WHERE "tm"."team_id" = (SELECT "team_id" FROM "apps" WHERE "id" = ?)
                    """#,
                arguments: [appId]
            )
            let members = memberRows.map { row -> AppRecords.AuthorOption in
                let id = (row["id"] as String?) ?? ""
                let name = (row["name"] as String?) ?? ""
                let email = (row["email"] as String?) ?? ""
                return AppRecords.AuthorOption(id: id, name: name.isEmpty ? email : name)
            }
            return (entities, role, AppRecords.authorOptions(members))
        }
        task = Task { [weak self] in
            do {
                for try await (entities, role, members) in observation.values(in: pool) {
                    guard let self else { return }
                    self.rows = entities.map(AppRecords.Row.init)
                    self.role = role
                    self.members = members
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
