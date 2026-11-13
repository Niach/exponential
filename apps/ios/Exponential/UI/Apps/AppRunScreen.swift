import ExpCore
import ExpUI
import SwiftUI

/// VAPP-242 (fixtures/apps.json THE RUN VIEW, web `app-run-view.tsx`): an
/// app as a member uses it, FULL SCREEN, pushed over the Apps tab by a row
/// tap or an app screen link. It runs the PUBLISHED package (`apps.package`,
/// never the draft) through THE SAME app host as the editor's preview
/// (`AppPreview` unframed: records with `me` = the viewer, the published
/// grants, read-only access, the HostBanner, the consent card, the screens
/// bar), edge to edge with `preview.padding` inside. No toolbar, no frame.
/// The header = the detail header: back, the app's glyph + name and the
/// `…` (`runMenu`, THE Menu's sheet, `run.menu`): Edit app (the creator or
/// an owner) → THE EDITOR, Export (published > 0), Delete app (the creator
/// or an owner) → the delete prompt. Nothing published = the page-size
/// EmptyState `copy.runEmpty` with the app's glyph; a package the host refuses
/// (#VAPP-251) = `copy.needsUpdate`, `AppPreview`'s. `app.navigate` keeps
/// the screen in memory; a new published version keeps it when it exists.
struct AppRunScreen: View {
    let appId: String
    /// An app screen link's screen (nil = the package's first).
    var screen: Apps.ScreenRequest? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.dismiss) private var dismiss
    @Environment(\.pushRoute) private var pushRoute
    /// The editor's live model (the app row, owner, access); the run view
    /// starts and ends no editor session.
    @State private var model = AppEditorModel()
    @State private var screenRequest: Apps.ScreenRequest?
    @State private var screenLinkApplied = false
    @State private var menuOpen = false
    @State private var pendingMenu: AppMenuSheet.Action?
    @State private var exportOpen = false
    @State private var deleteOpen = false
    @State private var deleting = false

    var body: some View {
        Group {
            if let app = model.app {
                ZStack {
                    AppBackground()
                    AppPreview(
                        packageJSON: app.publishedVersion > 0 ? app.package : nil,
                        appId: app.id,
                        teamId: app.teamId,
                        grants: true,
                        readOnly: model.access == "read",
                        screen: $screenRequest,
                        framed: false,
                        emptyTitle: Apps.Copy.runEmpty,
                        emptyIcon: app.glyph,
                        glyph: app.glyph
                    )
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                // An empty band: the page's own ground, no material over it.
                .workHeaderBand(ground: true) { EmptyView() }
                .navigationBarTitleDisplayMode(.inline)
                .toolbarBackground(.hidden, for: .navigationBar)
                .ownChromeToolbarItem(placement: .topBarTrailing, enabled: hasMenu(app)) { menuButton }
                .toolbar {
                    ToolbarItem(placement: .principal) { title(app) }
                }
            } else if model.missing {
                ZStack {
                    AppBackground()
                    EmptyState(.chatNotFound)
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
        .onAppear {
            if !screenLinkApplied, let screen {
                screenLinkApplied = true
                screenRequest = screen
            }
            model.bind(accountId: accountId, appId: appId, userId: deps.auth.userId, db: deps.db)
            model.loadAccess(accountId: accountId, appId: appId, trpc: deps.trpc)
        }
        .onDisappear { model.stop() }
        // Each presentation on its OWN node (EXP-240).
        .background {
            Color.clear
                .sheet(isPresented: $menuOpen, onDismiss: runPendingMenu) {
                    if let app = model.app {
                        AppMenuSheet(
                            title: app.name,
                            canEdit: model.canManage(userId: deps.auth.userId),
                            canExport: app.publishedVersion > 0,
                            canManage: model.canManage(userId: deps.auth.userId)
                        ) { action in
                            pendingMenu = action
                            menuOpen = false
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

    private func hasMenu(_ app: AppListEntry) -> Bool {
        app.publishedVersion > 0 || model.canManage(userId: deps.auth.userId)
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
            AppOutboxPendingGlyph(appId: app.id)
        }
    }

    /// The header's trailing `…` round button (`app-run-menu`), THE Menu in
    /// its sheet presentation (the editor's `…` alike).
    private var menuButton: some View {
        Button {
            menuOpen = true
        } label: {
            PageHeaderGlyph(icon: AppIcons.uiMore)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("More")
        .accessibilityIdentifier(Apps.TestIds.runMenu)
    }

    private func runPendingMenu() {
        guard let action = pendingMenu else { return }
        pendingMenu = nil
        switch action {
        case .edit:
            if model.canManage(userId: deps.auth.userId) { pushRoute(.app(accountId: accountId, appId: appId)) }
        case .export: exportOpen = model.app.map { $0.publishedVersion > 0 } ?? false
        case .delete: deleteOpen = model.canManage(userId: deps.auth.userId)
        // The editor's menu only.
        case .publish, .versions, .settings: break
        }
    }

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
}
