import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: the phone's ONE screen for a root session (fixture
/// `session-thread.json`) — a THREAD, or a team AGENT's channel (the same
/// Conversation, Slack-like, its started threads as session cards).
///
/// * Header = the title (a lock when private; an agent's icon in its color
///   before its name) and, for a thread, THE strip — Conversation · one tab
///   per linked issue · "+" · Guide — as the pager (`FacePager`). Nothing
///   else: no caption, no Stop, no device, no Show work.
/// * Conversation = `ThreadConversationView` over `buildSessionThread`, with
///   THE session composer box under it (`ThreadComposer` →
///   `SessionComposerCard`), sending
///   `sessions.send({sessionId: root})`.
/// * An issue tab = the existing issue page (`IssueFaceView`) of that issue.
/// * Guide = the existing Guide face over the root's `guide` (the PR body),
///   counting the thread PR's files, with its Merge capsule while it is open.
///
/// The "+" opens THE Menu's sheet: Link an issue (THE search list →
/// `sessions.linkIssues`) · New issue (the New issue page filing into this
/// root); an issue tab's long press offers Unlink (`sessions.unlinkIssue`).
/// Failed actions toast (`Couldn't link the issue`, feedback-states).
struct ThreadScreen: View {
    let rootId: String
    let initialPage: ThreadPage

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.motion) private var motion
    @Environment(\.toaster) private var toaster
    @Environment(TeamState.self) private var teamState
    @Environment(\.threadLanding) private var threadLanding
    @State private var page: ThreadPage
    /// A requested issue tab that has not synced into the strip yet (a
    /// just-filed issue): the page shows the Conversation and switches the
    /// moment the tab arrives; a tab tap drops the request.
    @State private var pendingPage: ThreadPage?
    @State private var model: ThreadScreenModel?
    /// The issue page models of the issue tabs VISITED so far (EXP-1288):
    /// only the SHOWN page's model observes (`observingIssueId`).
    @State private var issueVMs: [String: IssueDetailViewModel] = [:]
    @State private var observingIssueId: String?
    /// THE Menu's sheet: the "+" (issueId nil) or an issue tab's long press.
    @State private var menuTarget: ThreadMenuTarget?
    /// The sheet's pick, run once the sheet has dismissed.
    @State private var pendingMenuAction: (action: ThreadMenuSheet.Action, issueId: String?)?
    @State private var linkSheetOpen = false
    /// The question / plan answer in flight.
    @State private var answeringId: String?
    // Guide
    @State private var guideSection: GuideSectionKey?
    @State private var focusPath: String?
    @State private var mergeState = WorkMergeState()
    @State private var prChanges: ChangesViewModel?

    init(rootId: String, initialPage: ThreadPage = .conversation) {
        self.rootId = rootId
        self.initialPage = initialPage
        // An issue tab may not be in the strip yet: hold it (`pendingPage`).
        if case .issue = initialPage {
            _page = State(initialValue: .conversation)
            _pendingPage = State(initialValue: initialPage)
        } else {
            _page = State(initialValue: initialPage)
        }
    }

    private var rows: ThreadRows? { model?.rows }

    /// The pager's pages: the strip's (an agent channel has the
    /// Conversation alone).
    private func pages(_ rows: ThreadRows, _ view: SessionThreadView) -> [ThreadPage] {
        guard !rows.isAgent else { return [.conversation] }
        return view.tabs.compactMap { tab in
            switch tab {
            case .conversation: .conversation
            case let .issue(issueId, _): .issue(issueId)
            case .guide: .guide
            case .add: nil
            }
        }
    }

    private var currentPages: [ThreadPage] {
        guard let rows, let view = model?.view else { return [.conversation] }
        return pages(rows, view)
    }

    var body: some View {
        ZStack {
            AppBackground()
            if let model, let rows = model.rows, let view = model.view {
                FacePager(
                    pages: pages(rows, view),
                    selection: Binding(get: { page }, set: { switchPage($0) })
                ) { page in
                    pageView(page, rows: rows, view: view)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .safeAreaInset(edge: .top, spacing: 0) {
                    header(rows: rows, view: view)
                        .frame(maxWidth: .infinity)
                        .headerEdgeChrome(ground: page == .conversation)
                }
            } else if model?.missing == true {
                EmptyState(.threadNotFound)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .accessibilityIdentifier("thread-not-found")
            } else {
                // EXP-1275 (D8): a first load is skeleton rows.
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        // Each presentation on its OWN node (EXP-240).
        .background {
            Color.clear
                .sheet(item: $menuTarget, onDismiss: runPendingMenuAction) { target in
                    ThreadMenuSheet(
                        identifier: target.issueId.flatMap { rows?.issue(id: $0)?.identifier }
                    ) { action in
                        pendingMenuAction = (action, target.issueId)
                        menuTarget = nil
                    }
                }
        }
        .sheet(isPresented: $linkSheetOpen) {
            if let rows {
                ThreadLinkIssueSheet(
                    teamId: rows.root.teamId,
                    excluded: Set(rows.issues.map(\.id)),
                    onPick: { issueId in
                        linkSheetOpen = false
                        link(issueId)
                    }
                )
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-screen")
        .onAppear(perform: appear)
        .onDisappear(perform: disappear)
        // A held tab that arrived is shown; a page that vanished (an
        // unlinked issue) lands on the Conversation.
        .onChange(of: currentPages) { _, pages in
            if let pending = pendingPage, pages.contains(pending) {
                pendingPage = nil
                selectPage(pending)
            } else if !pages.contains(page) {
                page = .conversation
            }
            syncIssueModels()
        }
        // The navigator popped back to THIS page for a filed issue: land on
        // the tab it names.
        .onChange(of: threadLanding.request) { _, request in
            guard let request, request.threadId == rootId else { return }
            land(on: request.page)
        }
        .onChange(of: rows?.guidePullRequest?.id, initial: true) { _, _ in syncPrChanges() }
    }

    // MARK: - Header

    @ViewBuilder
    private func header(rows: ThreadRows, view: SessionThreadView) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            if rows.isAgent {
                ThreadHeaderTitle(
                    title: rows.root.name ?? view.title,
                    isPrivate: view.isPrivate,
                    agentIcon: BoardTypeDisplay.iconName(for: rows.root.icon)
                        ?? SessionTree.ListCopy.agentDefaultIcon,
                    agentColor: rows.root.color ?? SessionTree.ListCopy.agentDefaultColor
                )
                .padding(.bottom, 8)
            } else {
                ThreadHeaderTitle(title: view.title, isPrivate: view.isPrivate)
                ThreadStripTabs(
                    tabs: view.tabs,
                    selection: page,
                    issue: { rows.issue(id: $0) },
                    isFresh: { model?.isFresh(issueId: $0) ?? false },
                    onSelect: { next in
                        pendingPage = nil
                        selectPage(next)
                    },
                    onIssueMenu: { issueId in openMenu(issueId: issueId) },
                    onAdd: canEdit ? { openMenu(issueId: nil) } : nil
                )
            }
        }
        .padding(.top, 4)
    }

    /// A member may link and file here (`resolveTeamAccess`: every member
    /// comments and creates issues).
    private var canEdit: Bool {
        guard let rows else { return false }
        return teamState.teams.contains { $0.id == rows.root.teamId }
    }

    // MARK: - Pages

    @ViewBuilder
    private func pageView(_ page: ThreadPage, rows: ThreadRows, view: SessionThreadView) -> some View {
        switch page {
        case .conversation:
            conversationPage(rows: rows, view: view)
        case let .issue(issueId):
            issuePage(issueId)
        case .guide:
            guidePage(rows: rows)
        }
    }

    private func conversationPage(rows: ThreadRows, view: SessionThreadView) -> some View {
        ScrollViewReader { scroller in
            ScrollView {
                ThreadConversationView(
                    rows: rows,
                    view: view,
                    onOpenSession: { sessionId in
                        pushRoute(.agentSession(accountId: accountId, sessionId: sessionId))
                    },
                    onOpenThread: { threadId in
                        pushRoute(.thread(accountId: accountId, threadId: threadId))
                    },
                    onAnswer: { messageId, option in answer(messageId, answer: option) },
                    onDecide: { messageId, decision, feedback in
                        answer(messageId, decision: decision, feedback: feedback)
                    },
                    answeringId: answeringId,
                    canAnswer: canEdit
                )
                .padding(.vertical, 12)
                .transcriptColumn()
                Color.clear
                    .frame(height: 1)
                    .id(Self.bottomId)
            }
            .defaultScrollAnchor(.bottom)
            .scrollDismissesKeyboard(.interactively)
            .onChange(of: view.rows.count) { _, _ in
                withAnimation(motion.standard) { scroller.scrollTo(Self.bottomId, anchor: .bottom) }
            }
            .safeAreaInset(edge: .bottom) {
                ThreadComposer(
                    sessionId: view.composer.sessionId,
                    placeholder: view.composer.placeholder
                )
            }
        }
    }

    private static let bottomId = "thread-conversation-bottom"

    @ViewBuilder
    private func issuePage(_ issueId: String) -> some View {
        if let vm = issueVMs[issueId], let issue = vm.issue {
            IssueFaceView(
                vm: vm,
                issue: issue,
                barTrailing: .hidden,
                onStartCoding: {},
                // Its `Threads` row leaves THIS thread out.
                hostThreadId: rootId
            )
        } else {
            SkeletonRows()
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
    }

    // MARK: - Guide (the root's Guide = the PR body)

    private func guidePage(rows: ThreadRows) -> some View {
        let pullRequest = rows.guidePullRequest
        let openPullRequest = pullRequest.flatMap { $0.isOpen ? $0 : nil }
        return GuideFace(
            groups: parseSessionResultGroups(rows.root.guide),
            files: prChanges?.loadedFiles,
            diffStatus: prDiffStatus,
            teamId: rows.root.teamId,
            section: $guideSection,
            focusPath: $focusPath,
            showsMerge: openPullRequest != nil
        ) {
            if let openPullRequest {
                WorkMergePill(
                    pullRequest: openPullRequest,
                    issueCount: rows.pullRequestIssueIds[openPullRequest.id]?.count ?? 0,
                    state: $mergeState
                )
                .id(openPullRequest.id)
            }
        }
    }

    private var prDiffStatus: GuideDiffStatus {
        guard let prChanges else { return .ready }
        switch prChanges.load {
        case .loading: return .loading
        case let .failed(message): return .failed(message)
        case .loaded: return .ready
        }
    }

    /// The Guide counts the thread PR's files (open, else merged).
    private func syncPrChanges() {
        let wanted = rows?.guidePullRequest?.id
        guard prChanges?.pullRequestId != wanted else { return }
        prChanges?.stopObserving()
        prChanges = nil
        mergeState = WorkMergeState()
        guard let wanted else { return }
        let changes = ChangesViewModel(
            accountId: accountId,
            pullRequestId: wanted,
            db: deps.db,
            pullRequestsApi: deps.pullRequestsApi
        )
        prChanges = changes
        changes.startObserving()
    }

    // MARK: - Cards (answered in place)

    private func answer(
        _ messageId: String, answer: String? = nil, decision: PlanDecision? = nil, feedback: String? = nil
    ) {
        guard answeringId == nil else { return }
        answeringId = messageId
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                try await api.answer(
                    accountId: accountId, messageId: messageId, answer: answer, decision: decision, feedback: feedback
                )
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "send", thing: "the answer"),
                    description: error.userFacingMessage
                )
            }
            answeringId = nil
        }
    }

    // MARK: - THE Menu

    private func openMenu(issueId: String?) {
        guard canEdit else { return }
        UIApplication.endEditing()
        menuTarget = ThreadMenuTarget(issueId: issueId)
    }

    private func runPendingMenuAction() {
        guard let pending = pendingMenuAction else { return }
        pendingMenuAction = nil
        switch pending.action {
        case .linkIssue: linkSheetOpen = true
        case .newIssue: newIssue()
        case .unlink:
            if let issueId = pending.issueId { unlink(issueId) }
        }
    }

    private func link(_ issueId: String) {
        let api = deps.sessionsApi
        let accountId = accountId
        let rootId = rootId
        Task {
            do {
                try await api.linkIssues(accountId: accountId, sessionId: rootId, issueIds: [issueId])
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "link", thing: "the issue"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    private func unlink(_ issueId: String) {
        if page == .issue(issueId) { selectPage(.conversation) }
        let api = deps.sessionsApi
        let accountId = accountId
        let rootId = rootId
        Task {
            do {
                try await api.unlinkIssue(accountId: accountId, sessionId: rootId, issueId: issueId)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "unlink", thing: "the issue"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    /// The New issue page filing into this root, on its first issue's board,
    /// else the team's first board.
    private func newIssue() {
        guard let rows else { return }
        let boardId = rows.issues.first?.boardId
            ?? teamState.boards
                .filter { $0.teamId == rows.root.teamId }
                .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
                .first?.id
        guard let boardId else {
            toaster.error(FeedbackStates.Errors.title(verb: "file", thing: "the issue"), description: "The team has no board yet.")
            return
        }
        // EXP-1170: the draft id is minted HERE, at tap time.
        pushRoute(.issueDraft(
            accountId: accountId,
            draftId: UUID().uuidString.lowercased(),
            boardId: boardId,
            threadId: rootId
        ))
    }

    // MARK: - Lifecycle

    private func appear() {
        if model == nil {
            model = ThreadScreenModel(rootId: rootId, accountId: accountId, db: deps.db)
        }
        model?.start()
        prChanges?.startObserving()
        syncIssueModels()
    }

    private func disappear() {
        UIApplication.endEditing()
        model?.stop()
        prChanges?.stopObserving()
        stopIssueModel()
    }

    /// EXP-1288: ONE issue model observes at a time — the shown issue tab's,
    /// created the first time its page is reached and kept (stopped) once
    /// the page is left. An unlinked issue's model is dropped.
    private func syncIssueModels() {
        guard let rows else { return }
        let ids = Set(rows.issues.map(\.id))
        for id in issueVMs.keys where !ids.contains(id) {
            if observingIssueId == id { stopIssueModel() }
            issueVMs[id] = nil
        }
        guard case let .issue(id) = page, ids.contains(id) else {
            stopIssueModel()
            return
        }
        guard observingIssueId != id else { return }
        stopIssueModel()
        let vm = issueVMs[id] ?? makeIssueModel(id)
        issueVMs[id] = vm
        vm.startObserving()
        observingIssueId = id
    }

    /// Stop the observing issue model, saving what its page left unsaved.
    private func stopIssueModel() {
        guard let id = observingIssueId else { return }
        observingIssueId = nil
        guard let vm = issueVMs[id] else { return }
        vm.stopObserving()
        Task {
            await vm.saveTitle()
            await vm.commitDescription()
        }
    }

    private func makeIssueModel(_ id: String) -> IssueDetailViewModel {
        IssueDetailViewModel(
            accountId: accountId,
            issueId: id,
            db: deps.db,
            issuesApi: deps.issuesApi,
            attachmentsApi: deps.attachmentsApi,
            labelsApi: deps.labelsApi,
            relationsApi: deps.relationsApi,
            widgetsApi: deps.widgetsApi,
            auth: deps.auth
        )
    }

    /// Show `target` now, or hold it until it reaches the strip.
    private func land(on target: ThreadPage) {
        menuTarget = nil
        guideSection = nil
        if currentPages.contains(target) {
            pendingPage = nil
            selectPage(target)
        } else {
            pendingPage = target
            selectPage(.conversation)
        }
    }

    private func selectPage(_ next: ThreadPage) {
        withAnimation(motion.standard) { switchPage(next) }
    }

    private func switchPage(_ next: ThreadPage) {
        guard next != page else { return }
        UIApplication.endEditing()
        page = next
        syncIssueModels()
    }
}

/// What the strip's menu sheet is open for — an issue tab (`issueId`) or
/// the "+" (nil).
struct ThreadMenuTarget: Identifiable {
    let issueId: String?
    var id: String { issueId ?? "add" }
}

/// EXP-1295: a Conversation's draft for THE session composer box
/// (`SessionComposerCard`, mounted bare: no plan chip, model pill or usage
/// ring) — `sessions.send({sessionId, text, attachmentIds})` into the root:
/// an idle session resumes, an ended thread continues server-side. Picks
/// upload to that session first, each once.
@MainActor
@Observable
final class ThreadComposerDraft: SessionComposerDraft {
    let sessionId: String
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

    init(sessionId: String, accountId: String, sessionsApi: SessionsApi, attachmentsApi: AttachmentsApi) {
        self.sessionId = sessionId
        self.accountId = accountId
        self.sessionsApi = sessionsApi
        self.attachmentsApi = attachmentsApi
    }

    func uploadAttachment(data: Data, filename: String, contentType: String) async throws -> UploadedAttachment {
        try await attachmentsApi.uploadSessionImage(
            accountId: accountId,
            sessionId: sessionId,
            data: data,
            filename: filename,
            contentType: contentType
        )
    }

    func sendMessage(_ text: String, attachmentIds: [String]?) async -> Bool {
        sendError = nil
        let holdsFlag = !steerSending
        if holdsFlag { steerSending = true }
        defer { if holdsFlag { steerSending = false } }
        do {
            try await sessionsApi.send(
                accountId: accountId,
                .message(
                    sessionId: sessionId,
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

/// Mounts THE session composer box under a Conversation for one session.
struct ThreadComposer: View {
    let sessionId: String
    let placeholder: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var draft: ThreadComposerDraft?
    @State private var attach = SessionComposerAttach()

    var body: some View {
        Group {
            if let draft, draft.sessionId == sessionId {
                SessionComposerCard(
                    draft: draft,
                    placeholder: placeholder,
                    attach: $attach,
                    sendEnabled: !draft.steerSending,
                    fieldIdentifier: "thread-composer-field"
                )
                .onChange(of: draft.sendError) { _, message in
                    guard let message else { return }
                    toaster.error(FeedbackStates.Errors.title(verb: "send", thing: "the message"), description: message)
                }
            } else {
                Color.clear.frame(height: 1)
            }
        }
        .transcriptColumn()
        .padding(.bottom, 8)
        .floatingBarEdge()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ThreadCopy.composerTestId)
        // The composed session can move (a resumed child): a new draft.
        .task(id: sessionId) {
            if draft?.sessionId != sessionId {
                draft = ThreadComposerDraft(
                    sessionId: sessionId,
                    accountId: accountId,
                    sessionsApi: deps.sessionsApi,
                    attachmentsApi: deps.attachmentsApi
                )
            }
        }
    }
}

private struct ThreadLandingKey: EnvironmentKey {
    /// No navigator above: a landing goes nowhere.
    static let defaultValue = ThreadLanding()
}

extension EnvironmentValues {
    /// EXP-1284: the navigator's landing box (`ThreadLanding`).
    var threadLanding: ThreadLanding {
        get { self[ThreadLandingKey.self] }
        set { self[ThreadLandingKey.self] = newValue }
    }
}
