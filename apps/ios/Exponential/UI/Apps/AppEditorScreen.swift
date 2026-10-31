import ExpCore
import ExpUI
import SwiftUI

/// VAPP-110 (fixtures/apps.json): THE EDITOR on a phone. Header = the app's
/// glyph + name + the version pill; under it THE segmented control Chat |
/// Preview | Versions and ONE pane:
///
/// * Chat = the member's EDITOR SESSION with the app (a root with `app_id` +
///   `user_id` = me) as THE thread screen's channel (`ThreadScreen` with an
///   editor slot: ChannelMessageRow rows, cards answering in place, THE
///   composer). No session yet = the same pane, empty (`copy.chatEmpty`),
///   whose composer's first send is `sessions.send({appId, text, deviceId})`
///   on my default (else first online) device; none online = the composer
///   disabled with `copy.noDevice`.
/// * Preview = the app host rendering the head package's first template
///   (`AppPreview`); a picked older version re-renders with the `Viewing
///   v{n}` banner + Restore (a NEW head, toast `copy.restored`) + Current.
/// * Versions = newest first: `v{n}`, the note (`copy.noNote`), `Current` on
///   the head, who (a person, or `copy.byAgent`) and the time.
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
    @State private var model = AppEditorModel()
    @State private var tab: Tab = .chat
    /// The version shown in the Preview instead of the head (nil = head).
    @State private var viewing: Int?
    @State private var viewingPackage: String?
    @State private var restoring = false
    /// VAPP-121: the Export sheet (the header's trailing button).
    @State private var exportOpen = false
    /// The first message's draft (no session yet): the composer and a
    /// draft's starter pills share it, so both send the same way.
    @State private var firstDraft: AppFirstMessageDraft?

    enum Tab: Hashable, CaseIterable {
        case chat, preview, versions

        var label: String {
            switch self {
            case .chat: Apps.Copy.Tabs.chat
            case .preview: Apps.Copy.Tabs.preview
            case .versions: Apps.Copy.Tabs.versions
            }
        }
    }

    var body: some View {
        Group {
            if let app = model.app {
                if let rootId = model.rootId {
                    ThreadScreen(rootId: rootId, editor: slot(app))
                        .id(rootId)
                } else {
                    AppEditorShell(title: title(app), band: band(app), pane: pane(app), trailing: exportButton(app)) {
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
        .sheet(isPresented: $exportOpen) {
            if let app = model.app {
                AppExportSheet(app: app)
            }
        }
    }

    // MARK: - Chrome

    private func slot(_ app: AppListEntry) -> ThreadEditorSlot {
        ThreadEditorSlot(title: AnyView(title(app)), band: AnyView(band(app)), pane: pane(app), trailing: exportButton(app))
    }

    /// VAPP-121: Export = the header's trailing round button (`ui-share`),
    /// once the app has a version.
    private func exportButton(_ app: AppListEntry) -> AnyView? {
        guard app.headVersion > 0 else { return nil }
        return AnyView(
            Button {
                UIApplication.endEditing()
                exportOpen = true
            } label: {
                PageHeaderGlyph(icon: AppIcons.uiShare, dot: false)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(AppExport.Copy.export)
            .accessibilityIdentifier(AppExport.TestIds.editorButton)
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
        case .versions:
            return AnyView(versionsPane(app))
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
                        GlassPill(Apps.Copy.restore, mode: .action { restore(app) }, primary: true, enabled: !restoring)
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

    private func restore(_ app: AppListEntry) {
        guard let number = viewing, !restoring else { return }
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

    // MARK: - Versions

    private func versionsPane(_ app: AppListEntry) -> some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(spacing: 0) {
                    if model.versions.isEmpty {
                        EmptyState(title: Apps.Copy.versionsEmpty, icon: AppIcons.uiHistory, size: .list)
                            .padding(.vertical, 8)
                    }
                    ForEach(model.versions) { version in
                        versionRow(version, head: app.headVersion)
                    }
                }
                .padding(16)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier(Apps.TestIds.versions)
            }
            .scrollContentBackground(.hidden)
        }
    }

    private func versionRow(_ version: AppVersionEntity, head: Int) -> some View {
        let author = version.createdBy.flatMap { id in model.users.first { $0.id == id } }
        let active = viewing == nil ? version.number == head : viewing == version.number
        let note = version.note?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return Button {
            viewing = version.number == head ? nil : version.number
            tab = .preview
        } label: {
            HStack(spacing: 10) {
                Text(Apps.versionLabel(version.number))
                    .font(.subheadline.weight(.medium).monospacedDigit())
                    .foregroundStyle(.white)
                    .frame(width: 40, alignment: .leading)
                Text(note.isEmpty ? Apps.Copy.noNote : note)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if version.number == head {
                    GlassPill(Apps.Copy.head)
                }
                HStack(spacing: 6) {
                    if let author {
                        UserAvatar(user: author, id: author.id, size: 16)
                    }
                    Text(author?.name ?? Apps.Copy.byAgent)
                        .lineLimit(1)
                        .frame(maxWidth: 96, alignment: .leading)
                        .fixedSize(horizontal: true, vertical: false)
                    Text(UserMessageCaption.separator.trimmingCharacters(in: .whitespaces))
                    Text(UserMessageCaption.text(name: nil, at: WireTimestamps.parse(version.createdAt), queued: false))
                }
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .flatRow(isActive: active)
        .accessibilityIdentifier("\(Apps.TestIds.versionRow)-\(version.number)")
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
