import ExpCore
import ExpUI
import SwiftUI

/// VAPP-110 (fixtures/apps.json THE LIST): the Apps TAB = the band whose
/// `New app` pill opens THE NEW MENU (VAPP-242: New app, Import…), then one
/// flat row per app (glyph, name, the version pill, the `…` menu: Edit app,
/// Export, Delete app), sorted by name. VAPP-242: a row tap RUNS the app
/// (THE RUN VIEW, pushed); the editor sits behind `…` › Edit app.
/// VAPP-176: New app = `apps.create {teamId}` (a DRAFT) and its editor at
/// once, no dialog; Import… opens the import step as its own sheet. Empty
/// (no app of the team's; Exponential ships no built-in apps): the
/// list-size empty state under the band.
/// #EXP-1832 (sidebar-folders.json): the rows fold into the member's app
/// folders, read-only (no drag, no editing): a folder row (folder glyph,
/// name, count, chevron) folds on a tap, its apps indent one glyph.
struct AppsListScreen: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster
    @Environment(TeamState.self) private var teamState
    @State private var model = AppsListModel()
    @State private var layout = SidebarLayoutModel()
    /// #EXP-1832: folds flipped here (also written to UserDefaults).
    @State private var flipped: [String: Bool] = [:]
    @State private var creatingDraft = false
    @State private var importing = false
    /// VAPP-242: THE NEW MENU (the band pill's sheet) and its pick.
    @State private var newMenuOpen = false
    @State private var pendingNew: AppsNewMenuSheet.Action?
    @State private var menuTarget: AppListEntry?
    /// The menu's pick, run once its sheet has dismissed.
    @State private var pendingMenu: (app: AppListEntry, action: AppMenuSheet.Action)?
    @State private var deleting: AppListEntry?
    @State private var exporting: AppListEntry?

    private var teamId: String? { teamState.activeTeamId ?? teamState.teams.first?.id }

    var body: some View {
        host(ZStack {
            AppBackground()
            ScrollView {
                list
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .padding(.bottom, 24)
            }
            .scrollContentBackground(.hidden)
            .tabBarBottomInset()
        }
        // #EXP-1384: the Apps TAB's root wears the page header.
        .pageHeader(Chats.Copy.Headers.apps))
    }

    /// The band with the New app pill (THE NEW MENU) over one row per app.
    private var list: some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(Apps.Copy.title) {
                GlassPill(Apps.Copy.newApp, icon: AppIcons.uiAdd, mode: .action {
                    UIApplication.endEditing()
                    newMenuOpen = true
                }, enabled: !creatingDraft && teamId != nil)
                .accessibilityIdentifier(Apps.TestIds.newApp)
            }
            ForEach(folderRows) { entry in
                if entry.group {
                    AppsFolderRow(folder: entry) { toggle(entry.id) }
                } else if let app = appsById[entry.id] {
                    row(app, depth: entry.depth)
                }
            }
            if model.loaded, model.apps.isEmpty {
                EmptyState(title: Apps.Copy.empty, icon: AppIcons.navApps, size: .list)
                    .padding(.vertical, 8)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.list)
    }

    /// The sheets, the delete confirm and the observation both hosts share.
    private func host<Content: View>(_ content: Content) -> some View {
        content
        .background {
            Color.clear
                .sheet(isPresented: $newMenuOpen, onDismiss: runPendingNew) {
                    AppsNewMenuSheet { action in
                        pendingNew = action
                        newMenuOpen = false
                    }
                }
        }
        .sheet(isPresented: $importing) {
            if let teamId {
                AppImportStep(teamId: teamId) { imported in
                    pushRoute(.app(accountId: accountId, appId: imported))
                }
            }
        }
        .sheet(item: $exporting) { app in
            AppExportSheet(app: app)
        }
        .sheet(item: $menuTarget, onDismiss: runPendingMenu) { app in
            AppMenuSheet(
                title: app.name,
                canEdit: model.canManage(app, userId: deps.auth.userId),
                canExport: app.publishedVersion > 0,
                canManage: model.canManage(app, userId: deps.auth.userId)
            ) { action in
                pendingMenu = (app, action)
                menuTarget = nil
            }
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $deleting) { app in
                    GlassAlert(
                        title: Apps.Copy.deleteTitle(app.name),
                        message: Apps.Copy.deleteBody,
                        actions: [
                            GlassAlertAction("Cancel", role: .outline, isCancel: true, id: "cancel") {},
                            GlassAlertAction(Apps.Copy.delete, role: .destructive, id: "app-delete-confirm") {
                                delete(app)
                            },
                        ]
                    )
                }
        }
        .onAppear { bind() }
        .onChange(of: teamId) { _, _ in bind() }
        .onDisappear { model.stop() }
    }

    private func bind() {
        model.bind(accountId: accountId, teamId: teamId, userId: deps.auth.userId, db: deps.db)
        layout.bind(accountId: accountId, teamId: teamId, section: "app", db: deps.db)
    }

    private var appsById: [String: AppListEntry] {
        Dictionary(model.apps.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
    }

    /// The apps by name (the default order), folded by my folders.
    private var folderRows: [SidebarFolders.Row] {
        SidebarFolders.rows(
            SidebarFolders.tree(items: model.apps.map(\.id), entries: layout.entries),
            isOpen: isOpen
        )
    }

    private func isOpen(_ folderId: String) -> Bool {
        guard let teamId else { return true }
        return flipped[folderId] ?? SidebarFolders.isOpen(teamId: teamId, folderId: folderId)
    }

    private func toggle(_ folderId: String) {
        guard let teamId else { return }
        let open = !isOpen(folderId)
        flipped[folderId] = open
        SidebarFolders.setOpen(open, teamId: teamId, folderId: folderId)
    }

    private func row(_ app: AppListEntry, depth: Int = 0) -> some View {
        AppsListRow(
            app: app,
            depth: depth,
            // VAPP-121: any member exports an app that has a published version.
            showsMenu: app.publishedVersion > 0 || model.canManage(app, userId: deps.auth.userId),
            // VAPP-242: a tap RUNS the app (THE RUN VIEW), never the editor.
            onRun: { pushRoute(.appRun(accountId: accountId, appId: app.id)) },
            onMenu: {
                UIApplication.endEditing()
                menuTarget = app
            }
        )
    }

    private func runPendingMenu() {
        guard let pick = pendingMenu else { return }
        pendingMenu = nil
        switch pick.action {
        case .edit: pushRoute(.app(accountId: accountId, appId: pick.app.id))
        case .export: exporting = pick.app
        case .delete: deleting = pick.app
        // The editor's menu only (the row's menu offers neither).
        case .publish, .versions, .settings: break
        }
    }

    /// VAPP-242: THE NEW MENU's pick, once its sheet is gone.
    private func runPendingNew() {
        guard let action = pendingNew else { return }
        pendingNew = nil
        switch action {
        case .newApp: createDraft()
        case .importApp: importing = true
        }
    }

    /// VAPP-176 NEW APP: a DRAFT (`apps.create {teamId}`, no other field),
    /// then its editor at once; a failure = the failed-action toast.
    private func createDraft() {
        guard let teamId, !creatingDraft else { return }
        creatingDraft = true
        Task {
            defer { creatingDraft = false }
            do {
                let created = try await deps.appsApi.create(accountId: accountId, AppCreateInput(teamId: teamId))
                pushRoute(.app(accountId: accountId, appId: created.id))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "create", thing: "the app"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    private func delete(_ app: AppListEntry) {
        Task {
            do {
                try await deps.appsApi.delete(accountId: accountId, id: app.id)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "delete", thing: "the app"),
                    description: error.trpcUserMessage
                )
            }
        }
    }
}

/// One app row: glyph, name, the version pill, the `…` menu; a folder's app
/// (#EXP-1832, `depth` 1) indents one glyph + the row's gap.
struct AppsListRow: View {
    let app: AppListEntry
    var depth: Int = 0
    let showsMenu: Bool
    let onRun: () -> Void
    let onMenu: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Button(action: onRun) {
                HStack(spacing: 10) {
                    AgentRowIcon(icon: app.glyph, color: app.color ?? AppGlyph.neutralColor)
                    Text(app.name)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    AppVersionPill(app: app)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if showsMenu {
                Button(action: onMenu) {
                    AppIcon(AppIcons.uiMore, size: AppIcon.Size.small, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Menu for \(app.name)")
                .accessibilityIdentifier("\(Apps.TestIds.row)-menu-\(app.slug)")
            }
        }
        .padding(.leading, CGFloat(depth) * (AppIcon.Size.small + 10))
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .flatRow()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(Apps.TestIds.row)-\(app.slug)")
    }
}

/// #EXP-1832 (sidebar-folders.json): a folder row, read-only on phones:
/// folder glyph, name, its app count and the fold chevron; never navigable,
/// a tap folds it.
struct AppsFolderRow: View {
    let folder: SidebarFolders.Row
    let onToggle: () -> Void

    var body: some View {
        Button(action: onToggle) {
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiFolder, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(folder.name ?? SidebarFolders.defaultFolderName)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 8)
                FolderRowTrailing(count: folder.count, open: folder.open)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .flatRow()
        .accessibilityIdentifier("\(Apps.TestIds.row)-folder-\(folder.id)")
        .accessibilityValue(folder.open ? "Expanded" : "Collapsed")
    }
}

/// The glyph colour of an app with none: the row's muted ink.
enum AppGlyph {
    static let neutralColor = "#a1a1aa"
}

/// THE version pill (`copy.version`): `v{n}`, or the `Draft` badge muted.
struct AppVersionPill: View {
    let app: AppListEntry

    var body: some View {
        GlassPill(app.pillLabel)
            .opacity(app.pillMuted ? 0.6 : 1)
            .monospacedDigit()
            .accessibilityIdentifier("app-version-pill")
    }
}

/// An app row's `…` — THE Menu in its sheet presentation: Export (VAPP-121:
/// any member, once the app has a version) · Delete app (destructive; the
/// creator or an owner). No Rename (VAPP-176: the agent names an app). A pick hands the action back; the
/// host acts once the sheet has dismissed. VAPP-177: the phone editor's `…`
/// (`versionsCount` set) leads with Versions (the count as trailing meta)
/// and Settings (`editorTabs.menu`); eApps V1 (#VAPP-210): Publish comes
/// FIRST while `Apps.canPublish` and the viewer may write the app.
struct AppMenuSheet: View {
    enum Action { case edit, publish, versions, settings, export, delete }

    let title: String
    /// VAPP-242 (`run.menu`): Edit app FIRST (the list row's and the run
    /// view's menus; the creator or an owner) = THE EDITOR.
    var canEdit = false
    /// eApps V1: the editor's Publish (the head is ahead of the published one).
    var canPublish = false
    var canExport = false
    var canManage = true
    /// nil = the row's menu (no Versions / Settings).
    var versionsCount: Int? = nil
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: title) {
            VStack(spacing: 0) {
                if canEdit {
                    Button { onPick(.edit) } label: {
                        GlassMenuItemLabel(Apps.Copy.Menu.edit, icon: AppIcons.uiEdit)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-menu-edit")
                }
                if canPublish {
                    Button { onPick(.publish) } label: {
                        GlassMenuItemLabel(Apps.Copy.publish, icon: AppIcons.uiSubmit)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(Apps.TestIds.publish)
                }
                if let versionsCount {
                    Button { onPick(.versions) } label: {
                        GlassMenuItemLabel(Apps.Copy.Menu.versions, icon: AppIcons.uiHistory)
                            .overlay(alignment: .trailing) {
                                Text(String(versionsCount))
                                    .font(.system(size: DesignTokens.Typography.Size.sm).monospacedDigit())
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                    .padding(.trailing, GlassMenuTokens.itemHPadding)
                            }
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-menu-versions")
                    Button { onPick(.settings) } label: {
                        GlassMenuItemLabel(Apps.Copy.Menu.settings, icon: AppIcons.navSettings)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-menu-settings")
                }
                if canExport {
                    Button { onPick(.export) } label: {
                        GlassMenuItemLabel(AppExport.Copy.export, icon: AppIcons.uiShare)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(AppExport.TestIds.menuItem)
                }
                if canManage {
                    Button { onPick(.delete) } label: {
                        GlassMenuItemLabel(Apps.Copy.Menu.delete, icon: AppIcons.uiDelete, destructive: true)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-menu-delete")
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("app-menu-sheet")
        }
    }
}

/// VAPP-242 THE NEW MENU (`run.newMenu`, testId `newMenu`): the band's
/// `New app` pill opens THE Menu in its sheet presentation (the row's `…`
/// alike): New app (ui-add) · Import… (ui-file). A pick hands the action
/// back; the list acts once the sheet has dismissed.
struct AppsNewMenuSheet: View {
    enum Action { case newApp, importApp }

    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: Apps.Copy.newApp) {
            VStack(spacing: 0) {
                Button { onPick(.newApp) } label: {
                    GlassMenuItemLabel(Apps.Copy.newApp, icon: AppIcons.uiAdd)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("apps-new-menu-new")
                Button { onPick(.importApp) } label: {
                    GlassMenuItemLabel(Apps.Copy.import, icon: AppIcons.uiFile)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier(Apps.TestIds.importApp)
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Apps.TestIds.newMenu)
        }
    }
}
