import ExpCore
import ExpUI
import SwiftUI

/// VAPP-110 (fixtures/apps.json): the Apps list = the band with `New app` as
/// its trailing pill, then one flat row per app (glyph, name, the version
/// pill, the `…` menu: Rename, Delete), sorted by name. Empty (no app of the
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
    @State private var creating = false
    @State private var renaming: AppListEntry?
    @State private var menuTarget: AppListEntry?
    /// The menu's pick, run once its sheet has dismissed.
    @State private var pendingMenu: (app: AppListEntry, action: AppMenuSheet.Action)?
    @State private var deleting: AppListEntry?

    private var teamId: String? { teamState.activeTeamId ?? teamState.teams.first?.id }

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    GlassSectionBand(Apps.Copy.title) {
                        GlassPill(Apps.Copy.newApp, icon: AppIcons.uiAdd, mode: .action {
                            creating = true
                        })
                        .accessibilityIdentifier(Apps.TestIds.newApp)
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
        .sheet(isPresented: $creating) {
            if let teamId {
                AppFormSheet(teamId: teamId, app: nil) { created in
                    pushRoute(.app(accountId: accountId, appId: created))
                }
            }
        }
        .sheet(item: $renaming) { app in
            AppFormSheet(teamId: app.teamId ?? teamId ?? "", app: app) { _ in }
        }
        .sheet(item: $menuTarget, onDismiss: runPendingMenu) { app in
            AppMenuSheet(title: app.name) { action in
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
            if model.canManage(app, userId: deps.auth.userId) {
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
        case .rename: renaming = pick.app
        case .delete: deleting = pick.app
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

/// THE version pill (`copy.version`): `v{n}`, or `No version yet` muted.
struct AppVersionPill: View {
    let app: AppListEntry

    var body: some View {
        GlassPill(app.pillLabel)
            .opacity(app.pillMuted ? 0.6 : 1)
            .monospacedDigit()
            .accessibilityIdentifier("app-version-pill")
    }
}

/// An app row's `…` — THE Menu in its sheet presentation: Rename · Delete app
/// (destructive). A pick hands the action back; the host acts once the sheet
/// has dismissed.
struct AppMenuSheet: View {
    enum Action { case rename, delete }

    let title: String
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: title) {
            VStack(spacing: 0) {
                Button { onPick(.rename) } label: {
                    GlassMenuItemLabel(Apps.Copy.Menu.rename, icon: AppIcons.uiEdit)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("app-menu-rename")
                Button { onPick(.delete) } label: {
                    GlassMenuItemLabel(Apps.Copy.Menu.delete, icon: AppIcons.uiDelete, destructive: true)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("app-menu-delete")
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("app-menu-sheet")
        }
    }
}

/// New app / Rename (fixtures/apps.json): the board form's identity row
/// (icon + colour + name) and the optional description; Create opens the
/// editor. An invalid name reads inline under the row, any other failure
/// toasts.
struct AppFormSheet: View {
    let teamId: String
    /// Rename mode: the app to edit (nil = New app).
    let app: AppListEntry?
    /// The created app's id (New app only).
    let onCreated: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var icon = AppFormSheet.defaultIcon
    @State private var color = AppFormSheet.defaultColor
    @State private var description = ""
    @State private var saving = false
    @State private var errorText: String?
    @State private var seeded = false

    /// Web `app-form-dialog.tsx` defaults.
    static let defaultIcon = "puzzle"
    static let defaultColor = "#6366f1"

    private var canSubmit: Bool {
        !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !saving
    }

    var body: some View {
        GlassSheetChrome(
            title: app == nil ? Apps.Copy.newApp : Apps.Copy.Menu.rename,
            content: {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 8) {
                        IconPicker(selection: $icon, tint: Color(hex: color))
                        ColorSwatchPicker(selection: $color)
                        GlassTextField(Apps.Copy.namePlaceholder, text: Binding(
                            get: { name },
                            set: {
                                name = String($0.prefix(Apps.nameMax))
                                errorText = nil
                            }
                        ), verticalPadding: 9, accessibilityIdentifier: "app-form-name")
                        .font(.subheadline)
                    }
                    // N-14: THE row host: an open icon grid drops under the
                    // whole identity row, so no sibling moves.
                    .inlinePickerRow()
                    if let errorText {
                        Text(errorText)
                            .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                            .foregroundStyle(DesignTokens.Semantic.red)
                    }
                    GlassTextField(
                        Apps.Copy.descriptionPlaceholder,
                        text: Binding(get: { description }, set: { description = String($0.prefix(Apps.descriptionMax)) }),
                        lines: 3...6,
                        accessibilityIdentifier: "app-form-description"
                    )
                    .font(.subheadline)
                }
                .padding(16)
            },
            primaryAction: {
                GlassSubmitButton(
                    app == nil ? Apps.Copy.create : Apps.Copy.rename,
                    enabled: canSubmit,
                    loading: saving
                ) {
                    Task { await submit() }
                }
                .accessibilityIdentifier("app-form-submit")
            }
        )
        .onAppear {
            guard !seeded, let app else { return }
            seeded = true
            name = app.name
            icon = BoardTypeDisplay.iconName(for: app.icon) ?? Self.defaultIcon
            color = app.color ?? Self.defaultColor
            description = app.description
        }
    }

    private func submit() async {
        guard canSubmit else { return }
        saving = true
        errorText = nil
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            if let app {
                try await deps.appsApi.update(
                    accountId: accountId,
                    AppUpdateInput(id: app.id, name: trimmed, icon: icon, color: color, description: description)
                )
                dismiss()
            } else {
                let body = description.trimmingCharacters(in: .whitespacesAndNewlines)
                let created = try await deps.appsApi.create(
                    accountId: accountId,
                    AppCreateInput(teamId: teamId, name: trimmed, icon: icon, color: color, description: body.isEmpty ? nil : body)
                )
                dismiss()
                onCreated(created.id)
            }
        } catch {
            if error.trpcErrorCode == "BAD_REQUEST" {
                errorText = error.trpcUserMessage
            } else {
                Toaster.shared.error(
                    FeedbackStates.Errors.title(verb: app == nil ? "create" : "rename", thing: "the app"),
                    description: error.trpcUserMessage
                )
            }
            saving = false
        }
    }
}
