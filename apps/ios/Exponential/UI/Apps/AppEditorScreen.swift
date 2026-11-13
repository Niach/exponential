import ExpCore
import ExpUI
import GRDB
import ExponentialUI
import SwiftUI

/// VAPP-110 / VAPP-177 / #EXP-1732 (session-screen.json kind `app`): THE
/// EDITOR on a phone = THE session screen (`ChatScreen` with an editor
/// slot). Header = the app's glyph + name + the version pill over the
/// subtitle, then THE gear (the app's settings page, an opened item) and the
/// trailing `…` (`app-editor-menu`, THE Menu in its sheet presentation):
/// Publish · Export (once the app has a version) · Delete app (destructive;
/// the creator or an owner). Under the header THE tabs Chat · Preview · Data
/// · Versions · Sessions · Results:
///
/// * Chat = the member's EDITOR SESSION with the app (a root with `app_id` +
///   `user_id` = me) as THE chat screen's channel (`ChatScreen` with an
///   editor slot: ChannelMessageRow rows, cards answering in place, THE
///   composer). No session yet = the same pane, empty (`copy.chatEmpty`),
///   whose composer's first send is `sessions.send({appId, text, deviceId})`
///   on EDITOR DEVICE (#FEED-204, `AppEditorModel.settleDevice`); none online
///   = the composer disabled with `copy.noDevice`.
/// * Preview = the app host rendering `Apps.shownVersion`'s package's first
///   template (`AppPreview`; eApps V1 #VAPP-210: the head while my editor
///   session is live or nothing is published, else the published one); a
///   version previewed from the Versions sheet re-renders with the
///   `Viewing v{n}` banner + Publish (not on the published version) +
///   Restore (not on the head; a NEW head, toast `copy.restored`) + Current.
///
/// eApps V1: Publish is the menu's FIRST item while `Apps.canPublish` and I
/// may write the app: `apps.publish {id, number}`, toast `copy.publishedToast`.
/// * Data (VAPP-178, app-records.json `dataTab`) = the app's records
///   (`AppDataTab`): the kind picker + Add record, one grouped list, the
///   form as a sheet.
///
/// * Versions = the versions list (`AppVersionsList`); Settings = the
///   gear's page (`AppSettingsPage`).
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
    /// VAPP-218: an app screen link's screen: the editor opens on its
    /// Preview tab showing it (nil = the usual landing).
    var screen: Apps.ScreenRequest? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.dismiss) private var dismiss
    @Environment(\.pushRoute) private var pushRoute
    @State private var model = AppEditorModel()
    /// VAPP-234: the draft's "Start from a template" = the import sheet.
    @State private var importing = false
    @State private var tab: SessionScreen.Tab = .chat
    /// The version shown in the Preview instead of the head (nil = head).
    @State private var viewing: Int?
    @State private var viewingPackage: String?
    @State private var restoring = false
    /// eApps V1: the version being published (nil = none).
    @State private var publishing: Int?
    /// VAPP-177: the header's `…` (THE Menu's sheet).
    @State private var menuOpen = false
    /// The menu's (or a sheet's) pick, run once its sheet has dismissed.
    @State private var pendingMenu: AppMenuSheet.Action?
    /// #VAPP-256: the rollback the prompt asks about.
    @State private var rollbackOpen = false
    @State private var rollbackTarget = 0
    /// #EXP-1732: the gear's settings page on an editor with no session yet.
    @State private var settingsOpen = false
    /// VAPP-121: the Export sheet.
    @State private var exportOpen = false
    /// The delete confirm prompt.
    @State private var deleteOpen = false
    @State private var deleting = false
    /// The first message's draft (no session yet): the composer and a
    /// draft's starter pills share it, so both send the same way.
    @State private var firstDraft: AppFirstMessageDraft?
    /// VAPP-200: the viewer's app access (`appAccess.mine`; nil = write
    /// until known): at `read` the preview paints the app's own writing
    /// controls disabled.
    @State private var access: String?
    /// VAPP-218 (apps.json `screens`): the screen the preview is asked for
    /// (nil = the package's first): a link's, a bar tap's (its own params)
    /// or the package's `app.navigate`; kept in memory across repaints.
    @State private var screenRequest: Apps.ScreenRequest?
    /// The link's screen is applied once (a pop back here keeps the tab).
    @State private var screenLinkApplied = false

    var body: some View {
        Group {
            if let app = model.app {
                if let rootId = model.rootId {
                    ChatScreen(rootId: rootId, editor: slot(app))
                        .id(rootId)
                } else {
                    AppEditorShell(
                        title: title(app, status: SessionScreen.status([])),
                        tab: tab,
                        onSelect: { tab = $0 },
                        pane: tab == .chat ? nil : pane(app, tab),
                        trailing: menuButton(app),
                        onGear: {
                            UIApplication.endEditing()
                            settingsOpen = true
                        }
                    ) {
                        chatEmpty(app)
                    }
                    .navigationDestination(isPresented: $settingsOpen) { settingsPage }
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
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.editor)
        .onAppear {
            if !screenLinkApplied, let screen {
                screenLinkApplied = true
                screenRequest = screen
                tab = .preview
            }
            model.bind(accountId: accountId, appId: appId, userId: deps.auth.userId, db: deps.db)
            model.loadAccess(accountId: accountId, appId: appId, trpc: deps.trpc)
        }
        .onDisappear {
            endLiveSession()
            discardEmptyDraft()
            model.stop()
        }
        .task(id: appId) {
            access = try? await McpServersApi(trpc: deps.trpc).appAccessMine(accountId: accountId, appId: appId).level
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
            // #EXP-1729: the first send (composer or starter) carries the
            // Account and Model picks.
            firstDraft?.picks = { [model] in model.sendPicks }
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
                            canPublish: model.canWrite && Apps.canPublish(app.versions),
                            canExport: app.headVersion > 0,
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
                .sheet(isPresented: $importing) {
                    if let teamId = model.app?.teamId {
                        AppImportStep(teamId: teamId) { imported in
                            pushRoute(.app(accountId: accountId, appId: imported))
                        }
                    }
                }
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $rollbackOpen) {
                    GlassAlert(
                        title: Apps.Copy.rollbackTitle(rollbackTarget),
                        message: Apps.Copy.rollbackBody(rollbackTarget),
                        actions: [
                            GlassAlertAction("Cancel", role: .outline, isDefault: true, isCancel: true, id: "cancel") {},
                            GlassAlertAction(Apps.Copy.publish, role: .primary, isDefault: false, id: "app-rollback-confirm") {
                                if let app = model.app { publish(app, number: rollbackTarget) }
                            },
                        ]
                    )
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

    private func slot(_ app: AppListEntry) -> ChatEditorSlot {
        ChatEditorSlot(
            title: { status in AnyView(title(app, status: status)) },
            tab: $tab,
            pane: { tab in pane(app, tab) },
            settings: AnyView(settingsPage),
            trailing: menuButton(app),
            device: model.deviceMenu, appMove: model.deviceAppMove, picks: picks
        )
    }

    /// #EXP-1729: the composer's Account and Model rows (the launch
    /// composer's, seeded from the shown device).
    private var picks: ChatEditorPicks {
        let launch = model.launch
        return ChatEditorPicks(
            account: model.launchAccountMenu,
            agent: launch.agent,
            model: launch.model,
            onPickModel: { model.pickModel($0) },
            liveAccount: model.liveAccountFallback
        )
    }

    /// VAPP-177: the header's trailing `…` (`app-editor-menu`, THE chat
    /// header's ghost button), THE Menu in its sheet presentation (the Apps
    /// row's `…` alike); VAPP-235's pending-writes glyph before it.
    private func menuButton(_ app: AppListEntry) -> AnyView? {
        AnyView(
            HStack(spacing: 4) {
                AppOutboxPendingGlyph(appId: app.id)
                ChatHeaderGhostButton(icon: AppIcons.uiMore, label: "More", identifier: Apps.TestIds.menu) {
                    UIApplication.endEditing()
                    menuOpen = true
                }
            }
        )
    }

    /// #EXP-1732: the gear's opened item = the app's settings page; its
    /// Export / Delete hand back once the page has popped.
    private var settingsPage: some View {
        AppSettingsPage(model: model) { action in
            pendingMenu = action
            runPendingMenu()
        }
    }

    /// #EXP-1732 / #EXP-1729 (apps.json `editorHeader`): THE chat header's
    /// title block for an app: the BoardGlyph IN the lead disc, the name and
    /// the version pill right after it, over ONE subtitle line = the TREE's
    /// word (`SessionScreen.status`, never dropped) · `editorHeader`'s device
    /// part, in its tone.
    private func title(_ app: AppListEntry, status: String) -> some View {
        let header = model.header
        let tone: ChatHeaderTitle.Tone = switch header.tone {
        case .attention: .needsYou
        case .active: .working
        case .muted: .muted
        }
        return ChatHeaderTitle(
            title: app.name,
            subtitle: SessionScreen.subtitle(status: status, device: model.headerDevice, trigger: nil),
            subtitleTone: tone,
            agentIcon: BoardTypeDisplay.iconName(for: app.glyph) ?? SessionTree.ListCopy.agentDefaultIcon,
            agentColor: app.color ?? AppGlyph.neutralColor,
            titleAccessory: AnyView(AppVersionPill(app: app))
        )
    }

    /// The app tab's body (Preview, Data, Versions); nil = not the editor's.
    private func pane(_ app: AppListEntry, _ tab: SessionScreen.Tab) -> AnyView? {
        switch tab {
        case .preview:
            AnyView(preview(app))
        case .data:
            AnyView(AppDataTab(app: app))
        case .versions:
            AnyView(AppVersionsList(
                model: model,
                publishing: publishing,
                canWrite: model.canWrite,
                onPreview: { number in previewVersion(number) },
                onPublish: { number in
                    if let app = model.app { requestPublish(app, number: number) }
                }
            ))
        case .chat, .issues, .sessions, .results:
            nil
        }
    }

    /// A menu (or Settings sheet) pick, once its sheet is gone: one
    /// presentation at a time.
    private func runPendingMenu() {
        guard let action = pendingMenu else { return }
        pendingMenu = nil
        switch action {
        // The list's / the run view's menus only (the editor IS the editor).
        case .edit: break
        case .publish:
            if let app = model.app, model.canWrite, Apps.canPublish(app.versions) { publish(app, number: app.headVersion) }
        // #EXP-1732: Versions is a tab, Settings the gear (the menu lists neither).
        case .versions: tab = .versions
        case .settings: break
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
                deviceMenu: model.deviceMenu,
                picks: picks,
                teamId: app.teamId,
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
        // VAPP-234: never disabled (an import needs no device).
        let fromTemplate = GlassPill(AppTemplates.Copy.fromTemplate, size: .md, mode: .action {
            UIApplication.endEditing()
            importing = true
        })
        .accessibilityIdentifier(AppTemplates.TestIds.fromTemplate)
        return GeometryReader { geometry in
            ScrollView {
                EmptyState(title: Apps.Copy.starterTitle, icon: AppIcons.navApps, size: .page) {
                    ViewThatFits(in: .horizontal) {
                        VStack(spacing: 8) {
                            HStack(spacing: 8) { pills[0]; pills[1] }
                            HStack(spacing: 8) { pills[2]; pills[3] }
                            fromTemplate
                        }
                        VStack(spacing: 8) {
                            ForEach(pills.indices, id: \.self) { pills[$0] }
                            fromTemplate
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

    /// eApps V1: the version the preview shows (`Apps.shownVersion`).
    private func shown(_ app: AppListEntry) -> Int {
        Apps.shownVersion(app.versions, editing: model.editing, viewing: viewing)
    }

    /// The shown version's package: the head's and the published one's ride
    /// on the synced row, any other loads on demand.
    private func shownPackage(_ app: AppListEntry) -> String? {
        let number = shown(app)
        if number == 0 { return nil }
        if number == app.headVersion { return app.headPackage }
        if number == app.publishedVersion { return app.package }
        return viewingPackage
    }

    /// eApps R8 (apps.json `migrationCases`): previewing a version other than
    /// the published one: the published package's `version` its records are
    /// read through (the draft's migrations, copies only); nil = as stored.
    private func migrateFrom(_ app: AppListEntry, shown: Int) -> String? {
        guard app.publishedVersion > 0, shown != app.publishedVersion, let published = app.package else { return nil }
        return JSONValue.parse(published)["version"]?.string
    }

    private func preview(_ app: AppListEntry) -> some View {
        let canWrite = model.canWrite
        let number = shown(app)
        return ZStack {
            AppBackground()
            VStack(spacing: 12) {
                if let viewing {
                    HStack(spacing: 8) {
                        Text(Apps.Copy.viewing(viewing))
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.white)
                        Spacer(minLength: 8)
                        if canWrite, viewing != app.publishedVersion {
                            GlassPill(Apps.Copy.publish, mode: .action { requestPublish(app, number: viewing) }, primary: true, enabled: publishing == nil)
                                .accessibilityIdentifier("app-editor-viewing-publish")
                        }
                        if canWrite, viewing != app.headVersion {
                            GlassPill(Apps.Copy.restore, mode: .action { restore(app, number: viewing) }, enabled: !restoring)
                                .accessibilityIdentifier("app-editor-restore")
                        }
                        GlassPill(Apps.Copy.head, mode: .action { self.viewing = nil })
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassRow()
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier(Apps.TestIds.viewing)
                }
                // A grant is about the version it was made on (`grantVersion`):
                // any other previews asking for everything.
                AppPreview(packageJSON: shownPackage(app), appId: app.id, teamId: app.teamId, grants: number == Apps.grantVersion(app.versions), readOnly: access == "read", migrateFrom: migrateFrom(app, shown: number), screen: $screenRequest, glyph: app.glyph)
            }
            .padding(16)
        }
    }

    /// VAPP-177: a Versions row's Preview: the Preview tab shows that
    /// version (the preview's default = no banner).
    private func previewVersion(_ number: Int) {
        let defaultShown = model.app.map { Apps.shownVersion($0.versions, editing: model.editing, viewing: nil) }
        viewing = number == defaultShown ? nil : number
        tab = .preview
    }

    /// A picked version loads its package on demand (the head's and the
    /// published one's ride on the synced row).
    private func loadViewing(_ number: Int?) {
        viewingPackage = nil
        guard let number, let app = model.app else { return }
        guard number != app.headVersion, number != app.publishedVersion else { return }
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

    /// #VAPP-256 (fixture `rollbackCases`): a rollback past a migrated version
    /// asks first; every other publish goes through at once.
    private func requestPublish(_ app: AppListEntry, number: Int) {
        let rows = model.versions.map { (number: $0.number, migrationState: $0.migrationState) }
        guard Apps.rollbackWarns(published: app.publishedVersion, target: number, rows: rows) else {
            publish(app, number: number)
            return
        }
        rollbackTarget = number
        rollbackOpen = true
    }

    /// eApps V1 `apps.publish` of version `number` (the menu's head, a
    /// Versions row's or the banner's): toast `copy.publishedToast`, the
    /// preview back on its default; a failure = the failed-action toast.
    private func publish(_ app: AppListEntry, number: Int) {
        guard publishing == nil else { return }
        publishing = number
        Task {
            defer { publishing = nil }
            do {
                let result = try await deps.appsApi.publish(accountId: accountId, id: app.id, number: number)
                toaster.success(Apps.Copy.publishedToast(result.published))
                viewing = nil
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "publish", thing: "the version"),
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

/// #EXP-1732: the Versions TAB body (×4): ONE
/// grouped list newest first, each row the `v{n}` pill + the note (medium;
/// `copy.noNote` muted) over a muted `who · when`; eApps V1 (`versionRow`):
/// the published row = the `Published` badge, the head row ahead of it =
/// the `Current` badge + a Publish pill, every other row a ghost Preview + a
/// Publish pill (Publish needs write access to the app).
struct AppVersionsList: View {
    let model: AppEditorModel
    let publishing: Int?
    let canWrite: Bool
    let onPreview: (Int) -> Void
    let onPublish: (Int) -> Void

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                if model.versions.isEmpty {
                    EmptyState(title: Apps.Copy.versionsEmpty, icon: AppIcons.uiHistory, size: .list)
                        .padding(.vertical, 8)
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(model.versions.enumerated()), id: \.element.id) { index, version in
                            if index > 0 { GlassDivider() }
                            row(version, pointers: model.app?.versions ?? Apps.Versions(headVersion: 0, publishedVersion: 0))
                        }
                    }
                    .glassSection()
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Apps.TestIds.versions)
        }
        .scrollIndicators(.hidden)
    }

    private func row(_ version: AppVersionEntity, pointers: Apps.Versions) -> some View {
        let kind = Apps.versionRow(pointers, number: version.number)
        let author = version.createdBy.flatMap { id in model.users.first { $0.id == id } }
        let note = version.note?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let who = author?.name ?? Apps.Copy.byAgent
        let when = RelativeTime.long(wire: version.createdAt)
        // eApps R8 (apps.json `migrationCases`): the record sweep's line.
        let migration = Apps.versionMigration(state: version.migrationState, total: version.migrationTotal, done: version.migrationDone, error: version.migrationError)
        return HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    GlassPill(Apps.versionLabel(version.number))
                        .monospacedDigit()
                    Text(note.isEmpty ? Apps.Copy.noNote : note)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(note.isEmpty ? Color.white.opacity(TextOpacity.secondary) : Color.white)
                        .lineLimit(1)
                }
                Text([when.isEmpty ? who : "\(who)\(UserMessageCaption.separator)\(when)", migration].compactMap { $0 }.joined(separator: UserMessageCaption.separator))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    // The sweep's progress is the line's point: it wraps, never ellipsizes.
                    .lineLimit(migration == nil ? 1 : 2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if kind == .published {
                GlassPill(Apps.Copy.published)
            } else {
                if kind == .head {
                    GlassPill(Apps.Copy.head)
                } else {
                    Button { onPreview(version.number) } label: {
                        Text(Apps.Copy.preview)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.white)
                            .padding(.horizontal, GlassPillTokens.horizontalPaddingSm)
                            .frame(height: GlassPillTokens.heightSm)
                            .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(Apps.TestIds.versionPreview)
                }
                if canWrite {
                    GlassPill(Apps.Copy.publish, mode: .action { onPublish(version.number) }, enabled: publishing == nil)
                        .accessibilityIdentifier(Apps.TestIds.versionPublish)
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(Apps.TestIds.versionRow)-\(version.number)")
    }
}

// MARK: - Settings (VAPP-177)

/// #EXP-1732: the gear's Settings page (an opened item) = the wide clients'
/// Settings body, built
/// from the existing pieces: the board form's identity row (icon + colour +
/// name, the name saved when editing ends), the description (saved when
/// editing ends), the records line, the Tools group (a team OWNER flips the
/// grants: `apps.update {id, headVersion: grantVersion, grantedTools}`), then Export (a
/// version) and Delete app (the creator or an owner). Read-only for
/// everyone else. Export / Delete hand back to the editor, which runs them
/// once this page has popped.
struct AppSettingsPage: View {
    let model: AppEditorModel
    let onAction: (AppMenuSheet.Action) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.dismiss) private var dismiss
    @State private var seeded = false
    @State private var name = ""
    @State private var icon = "puzzle"
    @State private var color = AppSettingsPage.defaultColor
    @State private var description = ""
    @State private var toolBusy: String?
    /// #VAPP-243: the upstreams the app declares with my OWN connection, and
    /// the exposed tools with their switch (`appAccess.appTools`).
    @State private var accounts: [AppAccountWire] = []
    /// #VAPP-254: what an owner may pick for an upstream (empty for a member).
    @State private var servers: [AppServerWire] = []
    @State private var upstreamBusy: String?
    @State private var mcpTools: [AppMcpToolWire] = []
    @State private var mcpToolBusy: String?
    /// The app's team slug: Connect opens Settings › MCP on the web.
    @State private var teamSlug: String?
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var scenePhase
    /// #VAPP-220: the published package's automations (`appAccess.appTools`).
    @State private var automations: [AppAutomationWire] = []
    /// #VAPP-238: the published package's webhooks and `canToggle` (owner).
    @State private var hooks: [AppHookWire] = []
    @State private var canToggle = false
    /// A minted / rotated secret, by hook name: shown ONCE, in this sheet's
    /// memory only (never refetched; gone when the sheet closes).
    @State private var hookSecrets: [String: String] = [:]
    @State private var hookBusy: String?
    @State private var rotateTarget: String?
    @State private var showRotate = false
    /// VAPP-241: the published package's public links (`appAccess.appTools`).
    @State private var shares: [AppShareWire] = []
    @State private var shareBusy: String?
    @FocusState private var nameFocused: Bool
    @FocusState private var descriptionFocused: Bool

    /// The web settings tab's defaults for an app with no icon / colour.
    static let defaultColor = "#6366f1"

    private var canManage: Bool { model.canManage(userId: deps.auth.userId) }

    var body: some View {
        SessionScreenItemPage(title: SessionScreen.Copy.settings) {
            ScrollView {
                if let app = model.app {
                    content(app)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 24)
                }
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.settings)
        .onAppear { seed() }
        // #VAPP-220: refetched when the app or its published version changes.
        .task(id: automationsKey) { await loadAutomations() }
        // #VAPP-243: back from the browser's Connect = re-read my accounts.
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { Task { await loadAutomations() } }
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showRotate) {
                    GlassAlert(
                        prompt: Prompts.RotateWebhookSecret.copy(hook: rotateTarget ?? ""),
                        handlers: ["rotate": {
                            if let name = rotateTarget { rotateHook(name, verb: "rotate") }
                        }]
                    )
                }
        }
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
            policiesGroup(app)
            tools(app)
            shareLinksGroup
            accountsGroup
            mcpToolsGroup
            automationsGroup
            webhooksGroup
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

    /// #VAPP-243: `copy.policies` (shown when the running package declares
    /// records kinds): one read-only row per kind, the kind (mono) over its
    /// effective policy (`Apps.policyRows`).
    @ViewBuilder
    private func policiesGroup(_ app: AppListEntry) -> some View {
        let rows = Apps.policyRows(package: app.package ?? app.draftPackage)
        if !rows.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text(Apps.Copy.Policies.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 4)
                VStack(spacing: 0) {
                    ForEach(rows, id: \.kind) { row in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(row.kind)
                                .font(.expMono(.subheadline))
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            Text(row.caption)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .lineLimit(2)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier("\(Apps.TestIds.policyRow)-\(row.kind)")
                    }
                }
                .glassSection()
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Apps.TestIds.policies)
        }
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
                    .font(.expMono(.caption))
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

    /// #VAPP-243: `copy.accounts` (shown when the published package declares
    /// upstreams): one row per upstream, the declared name and my OWN state
    /// (`Apps.accountRow`): the muted text, or Connect / Reconnect. #VAPP-254
    /// (`upstreamPickerCases`): the real server on a second muted line when
    /// it differs; an owner's server picker before the trailing state.
    @ViewBuilder
    private var accountsGroup: some View {
        if !accounts.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text(Apps.Copy.Accounts.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 4)
                VStack(spacing: 0) {
                    ForEach(Array(accounts.enumerated()), id: \.offset) { _, account in
                        accountRow(account)
                    }
                }
                .glassSection()
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Apps.TestIds.accounts)
        }
    }

    private func accountRow(_ account: AppAccountWire) -> some View {
        let row = Apps.accountRow(account.row)
        let caption = Apps.accountCaption(name: account.name, serverName: account.serverName)
        // N-14: inside this sheet the picker's rows open under the whole row.
        return InlinePickerRow {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(account.name)
                        .font(.subheadline)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    if let caption {
                        Text(caption)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                UpstreamPickerPill(
                    name: account.name,
                    serverId: account.serverId,
                    servers: servers,
                    disabled: upstreamBusy != nil,
                    testId: "\(Apps.TestIds.accountRow)-\(account.name)-picker"
                ) { pickServer(upstream: account.name, value: $0) }
                if let label = row.actionLabel, let serverId = account.serverId {
                    GlassPill(label, icon: row.action == .reconnect ? AppIcons.uiWarning : AppIcons.uiSignIn,
                              mode: .action { connect(serverId) })
                        .fixedSize()
                        .accessibilityIdentifier("\(Apps.TestIds.accountRow)-\(account.name)-\(row.action?.rawValue ?? "")")
                } else if let label = row.label, row.status != "notSetUp" || servers.isEmpty {
                    Text(label)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize()
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(Apps.TestIds.accountRow)-\(account.name)")
    }

    /// #VAPP-254 `appAccess.setUpstreamMapping` at once (`none` = cleared),
    /// then the group re-reads; a failure = the failed-action toast and the
    /// old value (the row only ever shows what the server answered).
    private func pickServer(upstream: String, value: String) {
        guard upstreamBusy == nil, let app = model.app else { return }
        upstreamBusy = upstream
        let api = deps.appsApi
        let account = accountId
        Task {
            defer { upstreamBusy = nil }
            do {
                try await api.setUpstreamMapping(
                    accountId: account, appId: app.id, upstream: upstream,
                    serverId: value == Apps.upstreamPickerNone ? nil : value
                )
                await loadAutomations()
            } catch {
                toaster.error(FeedbackStates.Errors.title(verb: "save", thing: "the server"), description: error.trpcUserMessage)
            }
        }
    }

    /// My OWN account: Settings › MCP's connect on the web page in the
    /// browser (`?connect=<serverId>`), as Settings › MCP › Servers does.
    private func connect(_ serverId: String) {
        guard let slug = teamSlug,
              let instanceUrl = deps.auth.accounts.first(where: { $0.id == accountId })?.instanceUrl else { return }
        let base = instanceUrl.hasSuffix("/") ? String(instanceUrl.dropLast()) : instanceUrl
        guard let url = URL(string: "\(base)/t/\(slug)/settings/mcp?connect=\(serverId)") else { return }
        openURL(url)
    }

    /// #VAPP-243: `copy.mcpTools` (shown when the published package exposes
    /// tools): the title over the binding name, the Read / Write pill and the
    /// switch (an owner only; everyone else reads it).
    @ViewBuilder
    private var mcpToolsGroup: some View {
        if !mcpTools.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text(Apps.Copy.McpTools.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 4)
                VStack(spacing: 0) {
                    ForEach(Array(mcpTools.enumerated()), id: \.offset) { _, tool in
                        mcpToolRow(tool)
                    }
                }
                .glassSection()
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Apps.TestIds.mcpTools)
        }
    }

    private func mcpToolRow(_ tool: AppMcpToolWire) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(tool.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(tool.name)
                    .font(.expMono(.caption))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            GlassPill(tool.access == "read" ? Apps.Copy.McpTools.read : Apps.Copy.McpTools.write)
            GlassToggleRow(nil, isOn: Binding(
                get: { tool.enabled },
                set: { toggleMcpTool(tool.name, enabled: $0) }
            ))
            .disabled(!canToggle || mcpToolBusy != nil)
            .accessibilityLabel("\(Apps.Copy.McpTools.title): \(tool.title)")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(Apps.TestIds.mcpToolRow)-\(tool.name)")
    }

    /// `appAccess.setToolEnabled`: optimistic; a failure flips back with the
    /// failed-action toast.
    private func toggleMcpTool(_ name: String, enabled: Bool) {
        guard canToggle, mcpToolBusy == nil, let app = model.app else { return }
        let before = mcpTools
        mcpTools = mcpTools.map { $0.name == name ? $0.with(enabled: enabled) : $0 }
        mcpToolBusy = name
        let api = deps.appsApi
        let account = accountId
        Task {
            defer { mcpToolBusy = nil }
            do {
                try await api.setToolEnabled(accountId: account, appId: app.id, tool: name, enabled: enabled)
            } catch {
                mcpTools = before
                toaster.error(FeedbackStates.Errors.title(verb: "save", thing: "the tool"), description: error.trpcUserMessage)
            }
        }
    }

    /// #VAPP-220: `copy.automations` (shown when the published package
    /// declares any): one read-only row per automation, its name over the
    /// trigger (muted) and the status pill (`Apps.automationRow`).
    @ViewBuilder
    private var automationsGroup: some View {
        if !automations.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text(Apps.Copy.Automations.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 4)
                VStack(spacing: 0) {
                    ForEach(Array(automations.enumerated()), id: \.offset) { _, automation in
                        automationRow(automation)
                    }
                }
                .glassSection()
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Apps.TestIds.automations)
        }
    }

    private func automationRow(_ automation: AppAutomationWire) -> some View {
        let row = Apps.automationRow(automation.row)
        // #VAPP-243: who it runs as, or why it is paused.
        let sponsor = Apps.automationSponsor(paused: automation.paused, sponsorName: automation.sponsorName)
        // #VAPP-255: then the runs the rate cap dropped.
        let line = [row.trigger, sponsor, Apps.automationDropped(automation.droppedRuns)].compactMap { $0 }.joined(separator: " · ")
        return HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(automation.name)
                    .font(.expMono(.subheadline))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                // Why it is paused is the line's point: it wraps, never cut in the middle.
                Text(line)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            GlassPill(row.label)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.automationRow)
    }

    /// #VAPP-238: `copy.webhooks` (shown when the published package's
    /// automations listen on a webhook): one row per hook, the Automations
    /// row's layout (name mono over the URL, the status pill), then the URL
    /// copy pill and, for an owner (`canToggle`), Create / Rotate secret. A
    /// minted secret shows ONCE in a row right under its hook.
    @ViewBuilder
    private var webhooksGroup: some View {
        if !hooks.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text(Apps.Copy.Webhooks.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 4)
                VStack(spacing: 0) {
                    ForEach(Array(hooks.enumerated()), id: \.offset) { _, hook in
                        webhookRow(hook)
                        if let secret = hookSecrets[hook.name] {
                            webhookSecretRow(secret)
                        }
                    }
                }
                .glassSection()
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Apps.TestIds.webhooks)
        }
    }

    private func webhookRow(_ hook: AppHookWire) -> some View {
        let row = Apps.webhookRow(hook.row, isOwner: canToggle)
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(hook.name)
                        .font(.expMono(.subheadline))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    Text(hook.url)
                        .font(.expMono(.caption))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                GlassPill(row.label)
            }
            HStack(spacing: 8) {
                GlassPill(Apps.Copy.Webhooks.copyUrl, icon: AppIcons.uiCopy, mode: .action {
                    UIPasteboard.general.string = hook.url
                    toaster.success(Apps.Copy.Webhooks.copied)
                })
                .fixedSize()
                if let action = row.action, let label = row.actionLabel {
                    GlassPill(label, mode: .action {
                        switch action {
                        case .mint:
                            rotateHook(hook.name, verb: "create")
                        case .rotate:
                            rotateTarget = hook.name
                            showRotate = true
                        }
                    }, enabled: hookBusy == nil)
                    .fixedSize()
                    .accessibilityIdentifier("\(Apps.TestIds.webhookRow)-\(hook.name)-\(action.rawValue)")
                }
                Spacer(minLength: 0)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(Apps.TestIds.webhookRow)-\(hook.name)")
    }

    /// The shown-once secret: `copy.webhooks.secret` over the secret (mono,
    /// middle-truncated) and its copy pill.
    private func webhookSecretRow(_ secret: String) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(Apps.Copy.Webhooks.secret)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                Text(secret)
                    .font(.expMono(.subheadline))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            GlassPill(Apps.Copy.Webhooks.copySecret, icon: AppIcons.uiCopy, mode: .action {
                UIPasteboard.general.string = secret
                toaster.success(Apps.Copy.Webhooks.copied)
            })
            .fixedSize()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Apps.TestIds.webhookSecret)
    }

    /// VAPP-241 (app-share.json): `copy.title` (shown when the published
    /// package declares shares): one row per share, the name over
    /// `shareRowSubtitle`; an owner (`canToggle`) gets the switch (on =
    /// enable, off = revoke) and, while on, the `…` with Copy link / New link.
    @ViewBuilder
    private var shareLinksGroup: some View {
        if !shares.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text(AppShare.Copy.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 4)
                VStack(spacing: 0) {
                    ForEach(Array(shares.enumerated()), id: \.offset) { _, share in
                        shareRow(share)
                    }
                }
                .glassSection()
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(AppShare.TestIds.shareLinks)
        }
    }

    private func shareRow(_ share: AppShareWire) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(share.name)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(AppShare.shareRowSubtitle(share.mode, share.state, expiresAt: share.expiresAt))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if canToggle, share.state == .on, let url = share.url {
                GlassMenu {
                    GlassMenuItem(AppShare.Copy.copyLink, icon: AppIcons.uiCopy) {
                        UIPasteboard.general.string = url
                        toaster.success(Apps.Copy.Webhooks.copied)
                    }
                    GlassMenuItem(AppShare.Copy.newLink, icon: AppIcons.uiRefresh, enabled: shareBusy == nil) {
                        setShare(share.name, .rotate)
                    }
                    ForEach(AppShare.expiryChoices, id: \.label) { choice in
                        GlassMenuItem(choice.label, icon: AppIcons.uiClock, enabled: shareBusy == nil) {
                            setShare(share.name, .expire, days: choice.days)
                        }
                    }
                } label: {
                    GhostIconLabel(AppIcons.uiMore)
                        .accessibilityLabel(share.name)
                }
            }
            if canToggle {
                GlassToggleRow(nil, isOn: Binding(
                    get: { AppShare.shareSwitchOn(share.state) },
                    set: { setShare(share.name, $0 ? .enable : .revoke) }
                ))
                .disabled(shareBusy != nil)
                .accessibilityLabel("\(AppShare.Copy.title): \(share.name)")
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(AppShare.TestIds.shareRow)-\(share.name)")
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
                        handOff(.export)
                    })
                    .accessibilityIdentifier("app-export")
                }
                if canManage {
                    // EXP-1554: THE destructive button (web's filled red),
                    // beside Export, left-aligned (web).
                    DestructiveButton(Apps.Copy.Menu.delete) {
                        UIApplication.endEditing()
                        handOff(.delete)
                    }
                    .accessibilityIdentifier("app-delete")
                }
                Spacer(minLength: 0)
            }
            .padding(.top, 4)
        }
    }

    /// #EXP-1732: the page pops first, then the editor runs the action
    /// (its sheet / alert draws over the screen, not under this page).
    private func handOff(_ action: AppMenuSheet.Action) {
        dismiss()
        DispatchQueue.main.asyncAfter(deadline: .now() + DesignTokens.Motion.Duration.standard + 0.2) {
            onAction(action)
        }
    }

    // MARK: - Writes

    private var automationsKey: String? {
        model.app.map { "\($0.id):\($0.publishedVersion)" }
    }

    /// #VAPP-220: a failed read keeps the group hidden (nothing to act on).
    /// #VAPP-238: the same read carries the webhooks and `canToggle`.
    private func loadAutomations() async {
        guard let app = model.app else {
            accounts = []
            servers = []
            mcpTools = []
            automations = []
            hooks = []
            shares = []
            canToggle = false
            return
        }
        let api = deps.appsApi
        let account = accountId
        let summary = try? await api.tools(accountId: account, appId: app.id)
        guard !Task.isCancelled else { return }
        if teamSlug == nil, let teamId = app.teamId, let pool = try? deps.db.pool(forAccountId: account) {
            teamSlug = try? await pool.read { db in try TeamEntity.fetchOne(db, key: teamId)?.slug }
        }
        accounts = summary?.upstreams ?? []
        servers = summary?.servers ?? []
        mcpTools = summary?.tools ?? []
        automations = summary?.automations ?? []
        hooks = summary?.hooks ?? []
        shares = summary?.shares ?? []
        canToggle = summary?.canToggle ?? false
    }

    /// #VAPP-238 `appAccess.rotateHook`: mints (no secret yet) or rotates a
    /// hook's secret; the answer's secret is kept here only and the row
    /// turns `ready`. A failure = the failed-action toast.
    private func rotateHook(_ name: String, verb: String) {
        guard canToggle, hookBusy == nil, let app = model.app else { return }
        hookBusy = name
        let api = deps.appsApi
        let account = accountId
        Task {
            defer { hookBusy = nil }
            do {
                let minted = try await api.rotateHook(accountId: account, appId: app.id, name: name)
                hookSecrets[name] = minted.secret
                hooks = hooks.map { hook in
                    hook.name == name
                        ? AppHookWire(name: hook.name, url: hook.url, hasSecret: true, rotatedAt: hook.rotatedAt)
                        : hook
                }
            } catch {
                toaster.error(FeedbackStates.Errors.title(verb: verb, thing: "the secret"), description: error.trpcUserMessage)
            }
        }
    }

    /// VAPP-241 `appAccess.setShareLink`, then the rows reread (the state,
    /// expiry and URL are the server's). A failure = the failed-action toast.
    private func setShare(_ name: String, _ action: AppShareAction, days: Int? = nil) {
        guard canToggle, shareBusy == nil, let app = model.app else { return }
        shareBusy = name
        let api = deps.appsApi
        let account = accountId
        Task {
            defer { shareBusy = nil }
            do {
                try await api.setShareLink(accountId: account, appId: app.id, name: name, action: action, days: days)
                shares = try await api.tools(accountId: account, appId: app.id).shares
            } catch {
                toaster.error(FeedbackStates.Errors.title(verb: "save", thing: "the link"), description: error.trpcUserMessage)
            }
        }
    }

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
                try await api.update(accountId: account, AppUpdateInput(id: app.id, grantedTools: next, headVersion: Apps.grantVersion(app.versions)))
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

/// #EXP-1732: the editor before its first message: THE session screen's
/// chrome (kind `app`: the header with the gear and the `…`, THE tabs) over
/// the empty chat; Sessions and Results have nothing yet.
private struct AppEditorShell<Title: View, Chat: View>: View {
    let title: Title
    let tab: SessionScreen.Tab
    let onSelect: (SessionScreen.Tab) -> Void
    /// The app tab's body covering the chat (nil = the Chat tab).
    let pane: AnyView?
    let trailing: AnyView?
    let onGear: () -> Void
    @ViewBuilder let chat: () -> Chat

    private var screen: SessionScreen.Screen {
        SessionScreen.screen(kind: .app, requested: tab, open: nil, counts: SessionScreen.Counts())
    }

    var body: some View {
        let shown = screen
        ZStack {
            AppBackground()
            chat()
                .opacity(shown.active == .chat ? 1 : 0)
                .allowsHitTesting(shown.active == .chat)
                .accessibilityHidden(shown.active != .chat)
            if shown.active == .results {
                EmptyState(title: ResultsTab.Tab.Copy.empty, icon: AppIcons.chatResults, size: .list)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if shown.active != .chat, let pane {
                pane
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .workHeaderBand {
            ChatHeader {
                title
                    .frame(maxWidth: .infinity, alignment: .leading)
            } trailing: {
                SessionScreenGear(action: onGear)
                if let trailing { trailing }
            }
            SessionScreenTabs(tabs: shown.tabs, selection: shown.active, onSelect: onSelect)
        }
        .toolbar(.hidden, for: .navigationBar)
    }
}

/// THE floating composer for an editor with no session yet: text + files,
/// sent as `sessions.send({appId, text, deviceId})` (the server opens my
/// editor session, briefed). No device online = disabled, `copy.noDevice`.
private struct AppFirstMessageComposer: View {
    /// The editor's first-send draft (shared with a draft app's starters).
    let draft: AppFirstMessageDraft?
    let deviceId: String?
    /// #FEED-204: the "+" Device row (apps.json EDITOR DEVICE).
    let deviceMenu: SessionComposerFooter.DeviceMenu
    /// #EXP-1729: the "+" Account and Model rows (the launch picks).
    let picks: ChatEditorPicks
    /// The team whose issues the "+" Issue page searches.
    let teamId: String?
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
                // apps.json: a DRAFT app's composer is the open card (web),
                // never folded to the capsule, keyboard up or down.
                let open = deviceId != nil && (isDraftApp || expanded || draft.hasContent || attach.busy)
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
                        controls: controls,
                        attach: $attach,
                        sendEnabled: deviceId != nil && !draft.steerSending,
                        onSend: { Task { await draft.sendDraft() } },
                        fieldIdentifier: "app-composer-field"
                    )
                }
                .animation(motion.standard, value: open)
                // #FEED-219: the app editor's draft before its first session.
                .composerDraft(
                    .app(draft.appId),
                    teamId: draft.teamId.isEmpty ? nil : draft.teamId,
                    revision: draft.draftText,
                    read: { draft.draftText },
                    apply: { draft.draftText = $0 }
                )
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
        .floatingBarEdge(!((isDraftApp && deviceId != nil) || expanded || draft?.hasContent == true))
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

    /// #FEED-204 / #EXP-1729: THE "+" sheet before the session exists:
    /// Photo or file, Issue, Device, Account and Model (the launch picks the
    /// first send carries; no usage = no ring).
    private var controls: SessionComposerFooter.Controls {
        SessionComposerFooter.Controls(
            modelValue: picks.model,
            agent: picks.agent,
            onPickModel: picks.onPickModel,
            usageFraction: nil,
            usageSeverity: AgentUsagePresentation.severity(Double?.none),
            onUsage: {},
            teamId: teamId,
            account: picks.account,
            device: deviceMenu,
            editor: true
        )
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
    /// #EXP-1729: the editor composer's Account and Model picks, read at
    /// send time.
    @ObservationIgnored var picks: @MainActor () -> (cli: String?, account: String?, model: String?) = { (nil, nil, nil) }

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
        let picked = picks()
        do {
            _ = try await sessionsApi.send(
                accountId: accountId,
                .app(
                    appId: appId,
                    deviceId: deviceId,
                    text: text.isEmpty ? nil : text,
                    attachmentIds: attachmentIds?.isEmpty == false ? attachmentIds : nil,
                    cli: picked.cli,
                    account: picked.account,
                    model: picked.model
                )
            )
            return true
        } catch {
            sendError = error.userFacingMessage
            return false
        }
    }
}
