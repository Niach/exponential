import ExpCore
import ExpUI
import SwiftUI

/// VAPP-110 / VAPP-177 (fixtures/apps.json `editorTabs`): THE EDITOR on a
/// phone. Header = the app's glyph + name + the version pill, then the
/// trailing `…` (`app-editor-menu`, THE Menu in its sheet presentation):
/// Versions (the count as trailing meta) · Settings · Export (once the app
/// has a version) · Delete app (destructive; the creator or an owner).
/// Under the header THE segmented control Chat | Preview | Data and ONE
/// pane:
///
/// * Chat = the member's EDITOR SESSION with the app (a root with `app_id` +
///   `user_id` = me) as THE thread screen's channel (`ThreadScreen` with an
///   editor slot: ChannelMessageRow rows, cards answering in place, THE
///   composer). No session yet = the same pane, empty (`copy.chatEmpty`),
///   whose composer's first send is `sessions.send({appId, text, deviceId})`
///   on my default (else first online) device; none online = the composer
///   disabled with `copy.noDevice`.
/// * Preview = the app host rendering the head package's first template
///   (`AppPreview`); a version previewed from the Versions sheet re-renders
///   with the `Viewing v{n}` banner + Restore (a NEW head, toast
///   `copy.restored`) + Current.
/// * Data (VAPP-178, app-records.json `dataTab`) = the app's records
///   (`AppDataTab`): the kind picker + Add record, one grouped list, the
///   form as a sheet.
///
/// The menu's Versions and Settings open the wide clients' TAB bodies as
/// sheets titled by the item (`AppVersionsSheet`, `AppSettingsSheet`).
///
/// VAPP-176 a DRAFT (no version yet, New app's "Untitled app") opens on
/// Chat; while its chat is empty the page-size empty state
/// `copy.starterTitle` offers one pill per `copy.starters` entry, a tap
/// SENDS that text as the first message (the composer's own first-send
/// path), and the composer opens focused with `copy.draftPlaceholder`.
/// The editor keys the app by ID, so a draft's slug change (the agent
/// naming it) changes nothing here.
///
/// LIFETIME: entering starts nothing; leaving ENDS my live editor session
/// (`sessions.end` on the root: the tree); the next send continues it.
/// Leaving an EMPTY draft (no editor session) calls `apps.discardDraft`.
struct AppEditorScreen: View {
    let appId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.dismiss) private var dismiss
    @State private var model = AppEditorModel()
    @State private var tab: Tab = .chat
    /// The version shown in the Preview instead of the head (nil = head).
    @State private var viewing: Int?
    @State private var viewingPackage: String?
    @State private var restoring = false
    /// VAPP-177: the header's `…` (THE Menu's sheet).
    @State private var menuOpen = false
    /// The menu's (or a sheet's) pick, run once its sheet has dismissed.
    @State private var pendingMenu: AppMenuSheet.Action?
    /// VAPP-177: the menu's Versions / Settings sheet.
    @State private var editorSheet: EditorSheet?
    /// VAPP-121: the Export sheet.
    @State private var exportOpen = false
    /// The delete confirm prompt.
    @State private var deleteOpen = false
    @State private var deleting = false
    /// The first message's draft (no session yet): the composer and a
    /// draft's starter pills share it, so both send the same way.
    @State private var firstDraft: AppFirstMessageDraft?

    /// The phones' tabs (`editorTabs.phone`).
    enum Tab: Hashable, CaseIterable {
        case chat, preview, data

        var label: String {
            switch self {
            case .chat: Apps.Copy.Tabs.chat
            case .preview: Apps.Copy.Tabs.preview
            case .data: Apps.Copy.Tabs.data
            }
        }
    }

    /// The menu items that open a tab body as a sheet.
    enum EditorSheet: String, Identifiable {
        case versions, settings

        var id: String { rawValue }
    }

    var body: some View {
        Group {
            if let app = model.app {
                if let rootId = model.rootId {
                    ThreadScreen(rootId: rootId, editor: slot(app))
                        .id(rootId)
                } else {
                    AppEditorShell(title: title(app), band: band(app), pane: pane(app), trailing: menuButton(app)) {
                        chatEmpty(app)
                    }
                }
            } else if model.missing {
                ZStack {
                    AppBackground()
                    EmptyState(.threadNotFound)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            } else {
                ZStack {
                    AppBackground()
                    SkeletonRows()
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.editor)
        .onAppear {
            model.bind(accountId: accountId, appId: appId, userId: deps.auth.userId, db: deps.db)
        }
        .onDisappear {
            endLiveSession()
            discardEmptyDraft()
            model.stop()
        }
        .task(id: appId) {
            if firstDraft?.appId != appId {
                firstDraft = AppFirstMessageDraft(
                    appId: appId,
                    accountId: accountId,
                    sessionsApi: deps.sessionsApi,
                    attachmentsApi: deps.attachmentsApi
                )
            }
        }
        .onChange(of: model.app?.teamId, initial: true) { _, team in firstDraft?.teamId = team ?? "" }
        .onChange(of: firstDraft == nil) { _, _ in firstDraft?.teamId = model.app?.teamId ?? "" }
        .onChange(of: viewing) { _, number in loadViewing(number) }
        // Each presentation on its OWN node (EXP-240).
        .background {
            Color.clear
                .sheet(isPresented: $menuOpen, onDismiss: runPendingMenu) {
                    if let app = model.app {
                        AppMenuSheet(
                            title: app.name,
                            canExport: app.headVersion > 0,
                            canManage: model.canManage(userId: deps.auth.userId),
                            versionsCount: model.versions.count
                        ) { action in
                            pendingMenu = action
                            menuOpen = false
                        }
                    }
                }
        }
        .background {
            Color.clear
                .sheet(item: $editorSheet, onDismiss: runPendingMenu) { sheet in
                    switch sheet {
                    case .versions:
                        AppVersionsSheet(
                            model: model,
                            restoring: restoring,
                            onPreview: { number in previewVersion(number) },
                            onRestore: { number in
                                if let app = model.app { restore(app, number: number) }
                            }
                        )
                    case .settings:
                        AppSettingsSheet(model: model) { action in
                            pendingMenu = action
                            editorSheet = nil
                        }
                    }
                }
        }
        .background {
            Color.clear
                .sheet(isPresented: $exportOpen) {
                    if let app = model.app {
                        AppExportSheet(app: app)
                    }
                }
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $deleteOpen) {
                    GlassAlert(
                        title: Apps.Copy.deleteTitle(model.app?.name ?? ""),
                        message: Apps.Copy.deleteBody,
                        actions: [
                            GlassAlertAction("Cancel", role: .outline, isCancel: true, id: "cancel") {},
                            GlassAlertAction(Apps.Copy.delete, role: .destructive, enabled: !deleting, id: "app-delete-confirm") {
                                deleteApp()
                            },
                        ]
                    )
                }
        }
    }

    // MARK: - Chrome

    private func slot(_ app: AppListEntry) -> ThreadEditorSlot {
        ThreadEditorSlot(title: AnyView(title(app)), band: AnyView(band(app)), pane: pane(app), trailing: menuButton(app))
    }

    /// VAPP-177: the header's trailing `…` round button (`app-editor-menu`),
    /// THE Menu in its sheet presentation (the Apps row's `…` alike).
    private func menuButton(_ app: AppListEntry) -> AnyView? {
        AnyView(
            Button {
                UIApplication.endEditing()
                menuOpen = true
            } label: {
                PageHeaderGlyph(icon: AppIcons.uiMore, dot: false)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("More")
            .accessibilityIdentifier(Apps.TestIds.menu)
        )
    }

    private func title(_ app: AppListEntry) -> some View {
        HStack(spacing: 6) {
            AgentRowIcon(icon: app.glyph, color: app.color ?? AppGlyph.neutralColor)
            Text(app.name)
                .font(.headline)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
                .accessibilityAddTraits(.isHeader)
            AppVersionPill(app: app)
        }
    }

    private func band(_ app: AppListEntry) -> some View {
        GlassSegmentedControl(
            options: Tab.allCases,
            selection: tab,
            label: \.label,
            identifier: { "\(Apps.TestIds.tabs)-\($0)" }
        ) { next in
            tab = next
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.tabs)
    }

    /// The pane covering the chat (nil = the Chat tab).
    private func pane(_ app: AppListEntry) -> AnyView? {
        switch tab {
        case .chat:
            return nil
        case .preview:
            return AnyView(preview(app))
        case .data:
            return AnyView(AppDataTab(app: app))
        }
    }

    /// A menu (or Settings sheet) pick, once its sheet is gone: one
    /// presentation at a time.
    private func runPendingMenu() {
        guard let action = pendingMenu else { return }
        pendingMenu = nil
        switch action {
        case .versions: editorSheet = .versions
        case .settings: editorSheet = .settings
        case .export: exportOpen = model.app.map { $0.headVersion > 0 } ?? false
        case .delete: deleteOpen = model.canManage(userId: deps.auth.userId)
        }
    }

    // MARK: - Chat

    /// No session yet: the empty chat and THE floating composer whose send
    /// opens the editor session. VAPP-176: a DRAFT's empty chat = the
    /// page-size starter state, its composer focused.
    private func chatEmpty(_ app: AppListEntry) -> some View {
        let draft = Apps.isDraft(headVersion: app.headVersion)
        return ZStack {
            AppBackground()
            if draft {
                starters(app)
            } else {
                EmptyState(title: Apps.Copy.chatEmpty, icon: AppIcons.navApps, size: .list)
                    .padding(.horizontal, 24)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .contentShape(Rectangle())
                    .onTapGesture { UIApplication.endEditing() }
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            AppFirstMessageComposer(
                draft: firstDraft,
                deviceId: model.deviceId,
                isDraftApp: draft
            )
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.chat)
    }

    /// VAPP-176: `copy.starterTitle` with one action pill per starter; a
    /// tap sends its text as the first message (disabled without a device,
    /// like the composer). Scrolls when the keyboard leaves it no room.
    private func starters(_ app: AppListEntry) -> some View {
        let sending = firstDraft?.steerSending ?? false
        let enabled = model.deviceId != nil && firstDraft != nil && !sending
        let pills = Apps.Copy.starters.map { text in
            GlassPill(text, size: .md, mode: .action { sendStarter(text) }, enabled: enabled)
                .accessibilityIdentifier(Apps.TestIds.starter)
        }
        return GeometryReader { geometry in
            ScrollView {
                EmptyState(title: Apps.Copy.starterTitle, icon: AppIcons.navApps, size: .page) {
                    ViewThatFits(in: .horizontal) {
                        VStack(spacing: 8) {
                            HStack(spacing: 8) { pills[0]; pills[1] }
                            HStack(spacing: 8) { pills[2]; pills[3] }
                        }
                        VStack(spacing: 8) {
                            ForEach(pills.indices, id: \.self) { pills[$0] }
                        }
                    }
                }
                .frame(maxWidth: .infinity, minHeight: geometry.size.height)
            }
            .scrollBounceBehavior(.basedOnSize)
            .scrollDismissesKeyboard(.interactively)
        }
    }

    private func sendStarter(_ text: String) {
        guard let firstDraft, model.deviceId != nil, !firstDraft.steerSending else { return }
        Task { _ = await firstDraft.sendMessage(text, attachmentIds: nil) }
    }

    // MARK: - Preview

    private func preview(_ app: AppListEntry) -> some View {
        ZStack {
            AppBackground()
            VStack(spacing: 12) {
                if let viewing {
                    HStack(spacing: 8) {
                        Text(Apps.Copy.viewing(viewing))
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.white)
                        Spacer(minLength: 8)
                        GlassPill(Apps.Copy.restore, mode: .action { restore(app, number: viewing) }, primary: true, enabled: !restoring)
                            .accessibilityIdentifier("app-editor-restore")
                        GlassPill(Apps.Copy.head, mode: .action { self.viewing = nil })
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassRow()
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier(Apps.TestIds.viewing)
                }
                AppPreview(packageJSON: viewing != nil ? viewingPackage : app.package, appId: app.id, teamId: app.teamId, grants: viewing == nil)
            }
            .padding(16)
        }
    }

    /// VAPP-177: a Versions row's Preview: the sheet closes and the Preview
    /// tab shows that version (the head = the head, no banner).
    private func previewVersion(_ number: Int) {
        let head = model.app?.headVersion
        editorSheet = nil
        viewing = number == head ? nil : number
        tab = .preview
    }

    private func loadViewing(_ number: Int?) {
        viewingPackage = nil
        guard let number, let app = model.app, number != app.headVersion else {
            if number != nil { viewing = nil }
            return
        }
        Task {
            do {
                let package = try await deps.appsApi.package(accountId: accountId, id: app.id, version: number)
                if viewing == number { viewingPackage = package }
            } catch {
                if viewing == number { viewing = nil }
            }
        }
    }

    /// `apps.restore` of version `number` (the banner's or a Versions
    /// row's): a NEW head, toast `copy.restored`, the preview back on it.
    private func restore(_ app: AppListEntry, number: Int) {
        guard !restoring else { return }
        restoring = true
        Task {
            defer { restoring = false }
            do {
                let result = try await deps.appsApi.restore(accountId: accountId, id: app.id, number: number)
                toaster.success(Apps.Copy.restored(result.restored, as: result.version))
                viewing = nil
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "restore", thing: "the version"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    // MARK: - Delete

    /// The confirmed Delete app: `apps.delete`, then back to the Apps list;
    /// a failure = the failed-action toast.
    private func deleteApp() {
        guard let app = model.app, !deleting else { return }
        deleting = true
        Task {
            defer { deleting = false }
            do {
                try await deps.appsApi.delete(accountId: accountId, id: app.id)
                dismiss()
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "delete", thing: "the app"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    // MARK: - Lifetime

    /// VAPP-176: leaving an EMPTY draft (no version, no editor session:
    /// nothing was ever sent) discards it; the server deletes only my own
    /// still-empty draft (anything else = a no-op).
    private func discardEmptyDraft() {
        guard let app = model.app, Apps.isDraft(headVersion: app.headVersion), model.rootId == nil else { return }
        let api = deps.appsApi
        let account = accountId
        let id = app.id
        Task.detached {
            _ = try? await api.discardDraft(accountId: account, id: id)
        }
    }

    /// Leaving the editor ends my live editor session (its whole tree).
    private func endLiveSession() {
        guard let rootId = model.rootId, model.rootLive else { return }
        let api = deps.sessionsApi
        let account = accountId
        Task.detached {
            try? await api.end(accountId: account, sessionId: rootId)
        }
    }
}

// MARK: - Versions (VAPP-177)

/// The menu's Versions sheet = the wide clients' Versions TAB body: ONE
/// grouped list newest first, each row the `v{n}` pill + the note (medium;
/// `copy.noNote` muted) over a muted `who · when`; the head row = the
/// `Current` badge, every other row a ghost Preview + a Restore pill.
struct AppVersionsSheet: View {
    let model: AppEditorModel
    let restoring: Bool
    let onPreview: (Int) -> Void
    let onRestore: (Int) -> Void

    var body: some View {
        GlassSheetChrome(title: Apps.Copy.Menu.versions) {
            VStack(spacing: 0) {
                if model.versions.isEmpty {
                    EmptyState(title: Apps.Copy.versionsEmpty, icon: AppIcons.uiHistory, size: .list)
                        .padding(.vertical, 8)
                } else {
                    VStack(spacing: 0) {
                        ForEach(model.versions) { version in
                            row(version, head: model.app?.headVersion ?? 0)
                        }
                    }
                    .glassSection()
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Apps.TestIds.versions)
        }
    }

    private func row(_ version: AppVersionEntity, head: Int) -> some View {
        let author = version.createdBy.flatMap { id in model.users.first { $0.id == id } }
        let note = version.note?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let who = author?.name ?? Apps.Copy.byAgent
        let when = RelativeTime.compact(wire: version.createdAt)
        return HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    GlassPill(Apps.versionLabel(version.number))
                        .monospacedDigit()
                    Text(note.isEmpty ? Apps.Copy.noNote : note)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(note.isEmpty ? Color.white.opacity(TextOpacity.secondary) : Color.white)
                        .lineLimit(1)
                }
                Text(when.isEmpty ? who : "\(who)\(UserMessageCaption.separator)\(when)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if version.number == head {
                GlassPill(Apps.Copy.head)
            } else {
                Button { onPreview(version.number) } label: {
                    Text(Apps.Copy.preview)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, GlassPillTokens.horizontalPaddingSm)
                        .frame(height: GlassPillTokens.heightSm)
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier(Apps.TestIds.versionPreview)
                GlassPill(Apps.Copy.restore, mode: .action { onRestore(version.number) }, enabled: !restoring)
                    .accessibilityIdentifier(Apps.TestIds.versionRestore)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(Apps.TestIds.versionRow)-\(version.number)")
    }
}

// MARK: - Settings (VAPP-177)

/// The menu's Settings sheet = the wide clients' Settings TAB body, built
/// from the existing pieces: the board form's identity row (icon + colour +
/// name, the name saved when editing ends), the description (saved when
/// editing ends), the records line, the Tools group (a team OWNER flips the
/// grants: `apps.update {id, headVersion, grantedTools}`), then Export (a
/// version) and Delete app (the creator or an owner). Read-only for
/// everyone else. Export / Delete hand back to the editor, which runs them
/// once this sheet has dismissed.
struct AppSettingsSheet: View {
    let model: AppEditorModel
    let onAction: (AppMenuSheet.Action) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var seeded = false
    @State private var name = ""
    @State private var icon = "puzzle"
    @State private var color = AppSettingsSheet.defaultColor
    @State private var description = ""
    @State private var toolBusy: String?
    @FocusState private var nameFocused: Bool
    @FocusState private var descriptionFocused: Bool

    /// The web settings tab's defaults for an app with no icon / colour.
    static let defaultColor = "#6366f1"

    private var canManage: Bool { model.canManage(userId: deps.auth.userId) }

    var body: some View {
        GlassSheetChrome(title: Apps.Copy.Menu.settings, height: .full) {
            ScrollView {
                if let app = model.app {
                    content(app)
                        .padding(.horizontal, 16)
                        .padding(.bottom, 24)
                }
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.settings)
        .onAppear { seed() }
        .onChange(of: model.app?.id) { _, _ in
            seeded = false
            seed()
        }
        // The live echo, only while the field is idle.
        .onChange(of: model.app?.name) { _, next in
            guard !nameFocused, let next else { return }
            name = next
        }
        .onChange(of: model.app?.description) { _, next in
            guard !descriptionFocused, let next else { return }
            description = next
        }
    }

    private func content(_ app: AppListEntry) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            identityRow
            GlassTextField(
                Apps.Copy.descriptionPlaceholder,
                text: $description,
                lines: 3...8,
                accessibilityIdentifier: "app-settings-description"
            )
            .font(.subheadline)
            .focused($descriptionFocused)
            .disabled(!canManage)
            .onChange(of: description) { _, next in
                if next.count > Apps.descriptionMax { description = String(next.prefix(Apps.descriptionMax)) }
            }
            .onChange(of: descriptionFocused) { _, focused in
                if !focused { saveDescription() }
            }
            Text(AppRecords.recordsCountLabel(count: model.recordsCount, max: nil))
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .padding(.horizontal, 4)
                .accessibilityIdentifier("app-settings-records")
            tools(app)
            actions(app)
        }
    }

    /// The board form's ONE row: icon, colour, name (EXP-862, ×4).
    private var identityRow: some View {
        HStack(spacing: 8) {
            IconPicker(
                selection: Binding(
                    get: { icon },
                    set: { next in
                        guard next != icon else { return }
                        icon = next
                        if let app = model.app { patch(AppUpdateInput(id: app.id, icon: next)) }
                    }
                ),
                tint: Color(hex: color)
            )
            ColorSwatchPicker(selection: Binding(
                get: { color },
                set: { next in
                    guard next != color else { return }
                    color = next
                    if let app = model.app { patch(AppUpdateInput(id: app.id, color: next)) }
                }
            ))
            GlassTextField(Apps.Copy.namePlaceholder, text: $name, verticalPadding: 9, accessibilityIdentifier: "app-settings-name")
                .font(.subheadline)
                .focused($nameFocused)
                .onSubmit { saveName() }
                .onChange(of: nameFocused) { _, focused in
                    if !focused { saveName() }
                }
        }
        .disabled(!canManage)
    }

    /// VAPP-120: `copy.tools` = the title + hint, then one row per action
    /// the head package declares: the display title over the function name,
    /// a switch (an owner only) or `Always asks` for an ungrantable one.
    private func tools(_ app: AppListEntry) -> some View {
        let declared = app.declaredActions
        let granted = app.grantableGranted
        return VStack(alignment: .leading, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(AppActions.Copy.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                Text(AppActions.Copy.hint)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .padding(.horizontal, 4)
            VStack(spacing: 0) {
                if declared.isEmpty {
                    Text(AppActions.Copy.none)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                } else {
                    ForEach(declared, id: \.self) { tool in
                        toolRow(tool, granted: granted.contains(tool))
                    }
                }
            }
            .glassSection()
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("app-settings-tools")
    }

    private func toolRow(_ tool: String, granted: Bool) -> some View {
        let title = AppActions.actionTitle(tool)
        return HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(tool)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if AppActions.actionGrantable(tool) {
                GlassToggleRow(nil, isOn: Binding(
                    get: { granted },
                    set: { toggle(tool, on: $0) }
                ))
                .disabled(!model.isOwner || toolBusy != nil)
                .accessibilityLabel("\(AppActions.Copy.granted): \(title)")
            } else {
                // The ruling: a destructive or owner-only action always asks.
                Text(AppActions.Copy.always)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("app-tool-\(tool)")
    }

    /// Export (once the app has a version) and, at the end, Delete app (the
    /// creator or an owner).
    @ViewBuilder
    private func actions(_ app: AppListEntry) -> some View {
        if app.headVersion > 0 || canManage {
            HStack(spacing: 8) {
                if app.headVersion > 0 {
                    GlassPill(Apps.Copy.Menu.export, icon: AppIcons.uiShare, mode: .action {
                        UIApplication.endEditing()
                        onAction(.export)
                    })
                    .accessibilityIdentifier("app-export")
                }
                Spacer(minLength: 8)
                if canManage {
                    GlassPill(Apps.Copy.Menu.delete, icon: AppIcons.uiDelete, mode: .action {
                        UIApplication.endEditing()
                        onAction(.delete)
                    }, tint: DesignTokens.Palette.destructive)
                    .accessibilityIdentifier("app-delete")
                }
            }
            .padding(.top, 4)
        }
    }

    // MARK: - Writes

    /// Seed the drafts from the APP once (never on a resync of the same one).
    private func seed() {
        guard !seeded, let app = model.app else { return }
        seeded = true
        name = app.name
        icon = app.icon ?? "puzzle"
        color = app.color ?? Self.defaultColor
        description = app.description
    }

    private func saveName() {
        guard canManage, let app = model.app else { return }
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            name = app.name
            return
        }
        guard trimmed != app.name else { return }
        patch(AppUpdateInput(id: app.id, name: trimmed))
    }

    private func saveDescription() {
        guard canManage, let app = model.app, description != app.description else { return }
        patch(AppUpdateInput(id: app.id, description: description))
    }

    /// VAPP-156: a grant writes the whole (grantable) list WITH the head on
    /// screen; a head published meanwhile refuses (CONFLICT) = the toast,
    /// and the row resyncs.
    private func toggle(_ tool: String, on: Bool) {
        guard model.isOwner, toolBusy == nil, AppActions.actionGrantable(tool), let app = model.app else { return }
        let kept = app.grantableGranted.filter { $0 != tool }
        let next = on ? kept + [tool] : kept
        toolBusy = tool
        let api = deps.appsApi
        let account = accountId
        Task {
            defer { toolBusy = nil }
            do {
                try await api.update(accountId: account, AppUpdateInput(id: app.id, grantedTools: next, headVersion: app.headVersion))
            } catch {
                toaster.error(FeedbackStates.Errors.title(verb: "save", thing: "the app"), description: error.trpcUserMessage)
            }
        }
    }

    /// A settings write; the synced row is the echo. A failure = the
    /// failed-action toast.
    private func patch(_ input: AppUpdateInput) {
        guard canManage else { return }
        let api = deps.appsApi
        let account = accountId
        Task {
            do {
                try await api.update(accountId: account, input)
            } catch {
                toaster.error(FeedbackStates.Errors.title(verb: "save", thing: "the app"), description: error.trpcUserMessage)
            }
        }
    }
}

/// The editor before its first message: the same
/// chrome the thread screen gives an editor root — the title, the band, a
/// pane covering the chat — over the empty chat.
private struct AppEditorShell<Title: View, Band: View, Chat: View>: View {
    let title: Title
    let band: Band
    let pane: AnyView?
    let trailing: AnyView?
    @ViewBuilder let chat: () -> Chat

    var body: some View {
        ZStack {
            AppBackground()
            chat()
                .opacity(pane == nil ? 1 : 0)
                .allowsHitTesting(pane == nil)
                .accessibilityHidden(pane != nil)
            if let pane {
                pane
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .workHeaderBand { band }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) { title }
            if let trailing {
                ToolbarItem(placement: .topBarTrailing) { trailing }
            }
        }
    }
}

/// THE floating composer for an editor with no session yet: text + files,
/// sent as `sessions.send({appId, text, deviceId})` (the server opens my
/// editor session, briefed). No device online = disabled, `copy.noDevice`.
private struct AppFirstMessageComposer: View {
    /// The editor's first-send draft (shared with a draft app's starters).
    let draft: AppFirstMessageDraft?
    let deviceId: String?
    /// VAPP-176: a DRAFT app: `copy.draftPlaceholder`, opened focused.
    var isDraftApp = false

    @Environment(\.toaster) private var toaster
    @Environment(\.motion) private var motion
    @State private var attach = SessionComposerAttach()
    @State private var expanded = false
    @State private var autoFocused = false

    private var placeholder: String {
        if deviceId == nil { return Apps.Copy.noDevice }
        return isDraftApp ? Apps.Copy.draftPlaceholder : Apps.Copy.composerPlaceholder
    }

    var body: some View {
        Group {
            if let draft {
                let open = deviceId != nil && (expanded || draft.hasContent || attach.busy)
                FloatingComposer(
                    placeholder: placeholder,
                    open: open,
                    collapsedLabel: placeholder,
                    collapsedIdentifier: "app-composer-collapsed",
                    onExpand: expand
                ) {
                    SessionComposerCard(
                        draft: draft,
                        placeholder: placeholder,
                        controls: nil,
                        attach: $attach,
                        sendEnabled: deviceId != nil && !draft.steerSending,
                        onSend: { Task { await draft.sendDraft() } },
                        fieldIdentifier: "app-composer-field"
                    )
                }
                .animation(motion.standard, value: open)
                .onChange(of: draft.sendError) { _, message in
                    guard let message else { return }
                    toaster.error(FeedbackStates.Errors.title(verb: "send", thing: "the message"), description: message)
                }
                .onChange(of: draft.draftEditor.isEditing) { _, editing in
                    guard expanded, !editing, !attach.busy, !draft.hasContent else { return }
                    withAnimation(motion.standard) { expanded = false }
                }
            } else {
                Color.clear.frame(height: 1)
            }
        }
        .frame(maxWidth: DesignTokens.Transcript.maxWidth)
        .frame(maxWidth: .infinity)
        .floatingBarEdge(!(expanded || draft?.hasContent == true))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.composer)
        .onChange(of: deviceId, initial: true) { _, next in
            draft?.deviceId = next
            focusDraftApp()
        }
        .onChange(of: draft == nil, initial: true) { _, _ in
            draft?.deviceId = deviceId
            focusDraftApp()
        }
    }

    /// VAPP-176: a draft app's composer opens FOCUSED, once, as soon as it
    /// can send (a device online).
    private func focusDraftApp() {
        guard isDraftApp, !autoFocused, draft != nil, deviceId != nil else { return }
        autoFocused = true
        expand()
    }

    private func expand() {
        guard deviceId != nil else { return }
        withAnimation(motion.standard) { expanded = true }
        DispatchQueue.main.async {
            guard let editor = draft?.draftEditor else { return }
            editor.setFocused(editor.blocks.first?.id)
        }
    }
}

/// The first message's draft: uploads go to the TEAM's session route (there
/// is no session yet), the send is `sessions.send({appId, deviceId, text,
/// attachmentIds})`.
@MainActor
@Observable
final class AppFirstMessageDraft: SessionComposerDraft {
    let appId: String
    var deviceId: String?
    /// The app's team (the upload route); set once the app row is read.
    var teamId: String
    let draftEditor = IssueEditorModel()
    var pendingImages: [PendingSteerImage] = []
    var pendingFiles: [PendingSteerFile] = []
    var steerImageError: String?
    var steerSending = false
    /// A refused send: the host toasts it (the draft stays put).
    var sendError: String?

    @ObservationIgnored private let accountId: String
    @ObservationIgnored private let sessionsApi: SessionsApi
    @ObservationIgnored private let attachmentsApi: AttachmentsApi

    init(appId: String, teamId: String = "", accountId: String, sessionsApi: SessionsApi, attachmentsApi: AttachmentsApi) {
        self.appId = appId
        self.teamId = teamId
        self.accountId = accountId
        self.sessionsApi = sessionsApi
        self.attachmentsApi = attachmentsApi
    }

    func uploadAttachment(data: Data, filename: String, contentType: String) async throws -> UploadedAttachment {
        try await attachmentsApi.uploadTeamSessionImage(
            accountId: accountId,
            teamId: teamId,
            data: data,
            filename: filename,
            contentType: contentType
        )
    }

    func sendMessage(_ text: String, attachmentIds: [String]?) async -> Bool {
        sendError = nil
        guard let deviceId else {
            sendError = Apps.Copy.noDevice
            return false
        }
        let holdsFlag = !steerSending
        if holdsFlag { steerSending = true }
        defer { if holdsFlag { steerSending = false } }
        do {
            _ = try await sessionsApi.send(
                accountId: accountId,
                .app(
                    appId: appId,
                    deviceId: deviceId,
                    text: text.isEmpty ? nil : text,
                    attachmentIds: attachmentIds?.isEmpty == false ? attachmentIds : nil
                )
            )
            return true
        } catch {
            sendError = error.userFacingMessage
            return false
        }
    }
}
