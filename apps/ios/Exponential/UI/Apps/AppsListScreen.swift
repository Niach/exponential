import ExpCore
import ExpUI
import SwiftUI

/// VAPP-110 (fixtures/apps.json): the Apps list = the band with `Import…`
/// and `New app` as its trailing pills, then one flat row per app (glyph,
/// name, the version pill, the `…` menu: Export, Delete), sorted by name.
/// VAPP-176: New app = `apps.create {teamId}` (a DRAFT) and its editor at
/// once, no dialog; Import opens the import step as its own sheet. Empty (no app of the
/// team's; Exponential ships no built-in apps): the list-size
/// empty state under the band. The active team's apps (web
/// `t/$teamSlug/apps`).
struct AppsListScreen: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster
    @Environment(TeamState.self) private var teamState
    @State private var model = AppsListModel()
    @State private var creatingDraft = false
    @State private var importing = false
    @State private var menuTarget: AppListEntry?
    /// The menu's pick, run once its sheet has dismissed.
    @State private var pendingMenu: (app: AppListEntry, action: AppMenuSheet.Action)?
    @State private var deleting: AppListEntry?
    @State private var exporting: AppListEntry?

    private var teamId: String? { teamState.activeTeamId ?? teamState.teams.first?.id }

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    GlassSectionBand(Apps.Copy.title) {
                        HStack(spacing: 8) {
                            GlassPill(Apps.Copy.import, mode: .action {
                                UIApplication.endEditing()
                                importing = true
                            })
                            .accessibilityIdentifier(Apps.TestIds.importApp)
                            GlassPill(Apps.Copy.newApp, icon: AppIcons.uiAdd, mode: .action {
                                createDraft()
                            }, enabled: !creatingDraft && teamId != nil)
                            .accessibilityIdentifier(Apps.TestIds.newApp)
                        }
                    }
                    ForEach(model.apps) { app in
                        row(app)
                    }
                    if model.loaded, model.apps.isEmpty {
                        EmptyState(title: Apps.Copy.empty, icon: AppIcons.navApps, size: .list)
                            .padding(.vertical, 8)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 24)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier(Apps.TestIds.list)
            }
            .scrollContentBackground(.hidden)
            .tabBarBottomInset()
        }
        // #EXP-1384: the Apps TAB's root wears the page header.
        .pageHeader(Chats.Copy.Headers.apps)
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
                canExport: app.headVersion > 0,
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
    }

    private func row(_ app: AppListEntry) -> some View {
        HStack(spacing: 10) {
            Button {
                pushRoute(.app(accountId: accountId, appId: app.id))
            } label: {
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
            // VAPP-121: any member exports an app that has a version.
            if app.headVersion > 0 || model.canManage(app, userId: deps.auth.userId) {
                Button {
                    UIApplication.endEditing()
                    menuTarget = app
                } label: {
                    AppIcon(AppIcons.uiMore, size: AppIcon.Size.small, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Menu for \(app.name)")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .flatRow()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(Apps.TestIds.row)-\(app.slug)")
    }

    private func runPendingMenu() {
        guard let pick = pendingMenu else { return }
        pendingMenu = nil
        switch pick.action {
        case .export: exporting = pick.app
        case .delete: deleting = pick.app
        // The editor's menu only (the row's menu offers neither).
        case .versions, .settings: break
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
/// and Settings (`editorTabs.menu`).
struct AppMenuSheet: View {
    enum Action { case versions, settings, export, delete }

    let title: String
    var canExport = false
    var canManage = true
    /// nil = the row's menu (no Versions / Settings).
    var versionsCount: Int? = nil
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: title) {
            VStack(spacing: 0) {
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
