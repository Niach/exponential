import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-1295: the phone's ONE screen for a root session (fixture
/// `session-thread.json`) — a THREAD, or a team AGENT's conversation. S-3
/// CHAT MODE (chats.json, `SessionTree.threadChatMode`): ONE path; an
/// agent conversation (a root with `agentId`) is `bubbles` — its
/// Conversation is always the bubbles transcript (person right
/// `UserMessageBubble` max 80%, agent left `AgentChatBubble` max 94%, cards
/// in the agent column), never the root's raw work page — and a thread is
/// `transcript`. Everything else (header + strip, composer with the "+"
/// incl. Model, selection, the Dock, the jump button) is the same.
///
/// * NEW-6 (N-1): a thread with ONE session (the root alone) IS that
///   session: its page (raw work + composer; S-3: no header Stop) is the
///   main thing and the strip has no Conversation tab; a second
///   session brings the Conversation tab back.
/// * Header = the title (plain, never renamed — EXP-1316; a lock when
///   private; an agent's icon in its color before its name) and, for a
///   thread, THE strip — Conversation · one chip per linked issue · "+" ·
///   the session chips — as the pager (`FacePager`). Nothing else:
///   no caption, no device, no Show work.
/// * Conversation = `ThreadConversationView` over `buildSessionThread`, with
///   THE session composer box under it (`ThreadComposer` →
///   `SessionComposerCard`), sending
///   `sessions.send({sessionId: root})`.
/// * An issue tab = the issue page EMBEDDED (EXP-1316): no own header, no
///   Threads row, no Start; the bottom bar keeps properties + Comment.
/// * S-3 (session-thread.json): the Guide tab is gone. The thread PR
///   (`guidePullRequest`) keeps its Merge pill in the header's action slot
///   while it is open, left of ONE `…` (EXP-1307: THE Menu's sheet,
///   `guideMenuEntries` — Close PR (EXP-1338) behind the close-pr.json
///   confirm).
///
/// The "+" (left of Conversation, EXP-1334) opens THE Menu's sheet: Link an
/// issue (THE search list → `sessions.linkIssues`) · New issue (the New
/// issue page filing into this root); an issue chip's long press adds
/// Unlink (`sessions.unlinkIssue`). The strip never scrolls (EXP-1334): a
/// folded chip or a "+N" opens `ThreadFoldSheet`.
/// Failed actions toast (`Couldn't link the issue`, feedback-states).
///
/// EXP-1323: a suggested tasks card acts through `sessions.startSuggestion`
/// & co. (its ⋯ = THE Menu's sheet, `SuggestionOptionsSheet`); N-12: while
/// any item is open the card rides THE Dock and opens as its sheet (S-3: a
/// waiting plan or question never does; it stays inline); the
/// live root's relay state draws THE bottom strip (Tasks + status lines,
/// the session view's own) above the composer; opening the screen stamps
/// the viewer's read of the root (`sessions.markRead`).
struct ThreadScreen: View {
    let rootId: String
    let initialPage: ThreadPage
    /// VAPP-110: an app EDITOR hosting this channel as its Chat tab — its
    /// title replaces the header's, its band (the segmented control) the
    /// strip, and a non-nil `pane` (Preview, Versions) covers the
    /// Conversation, which stays mounted under it (its draft and scroll
    /// survive a tab switch).
    var editor: ThreadEditorSlot?

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
    /// THE Menu's sheet: the "+" (issueId nil), an issue tab's long press or
    /// the Guide's `…` (EXP-1307).
    @State private var menuTarget: ThreadMenuTarget?
    /// The sheet's pick, run once the sheet has dismissed.
    @State private var pendingMenuAction: (action: ThreadMenuSheet.Action, issueId: String?)?
    @State private var linkSheetOpen = false
    /// EXP-1334: a folded chip's subtree or a "+N" (THE Menu's sheet), and
    /// its pick, run once the sheet has dismissed.
    @State private var foldTarget: ThreadStripFold?
    @State private var pendingFoldPick: ThreadStripFold.Row?
    /// The question / plan answer in flight.
    @State private var answeringId: String?
    /// N-12: THE Dock's open sheet (a card id).
    @State private var dockOpenId: String?
    /// The Conversation follows its newest row (EXP-1316 1n).
    @State private var atBottom = true
    /// The composer expanded on a pinned reader: the keyboard's inset re-pins.
    @State private var pinOnInset = false
    /// The floating composer's measured height = the conversation's bottom
    /// inset; a pinned conversation re-pins whenever it changes.
    @State private var composerHeight: CGFloat = 0
    /// The composer's top edge and the newest row's bottom anchor (global):
    /// what the scroll view's inset leaves under the open composer while the
    /// keyboard is up is measured and padded out (`insetCorrection`).
    @State private var composerTop: CGFloat = 0
    @State private var anchorBottom: CGFloat = 0
    @State private var insetCorrection: CGFloat = 0
    // The thread PR (`guidePullRequest`)
    @State private var mergeState = WorkMergeState()
    /// EXP-1307: the thread PR awaiting the Close PR confirm.
    @State private var closePrTarget: PullRequestEntity?
    // EXP-1323: suggested tasks
    /// In-flight suggestion actions (`ThreadSuggestionActions.key`).
    @State private var suggestionPending: Set<String> = []
    /// EXP-1323: the live tail's claim on the root's socket owner
    /// (`attachSteerModel`, paired with `steerSessions.detach`).
    @State private var liveModel: AgentSessionModel?
    @State private var liveClaimId: String?
    /// EXP-1343: the shown session page's model (its embedded view's claim);
    /// nil off a session page.
    @State private var sessionPageModel: (id: String, model: AgentSessionModel)?
    /// EXP-1347: the innermost outlet for issue pills inside this thread.
    @State private var issueOutlet = ThreadIssueOutlet()
    /// EXP-1373 (chats.json MENUS): the header `…` = the chat's menu sheet
    /// (S-3: Delete, the Guide's Close PR; an agent chat's Compact · New
    /// conversation · Agent settings), its pick
    /// run once the sheet has dismissed.
    @State private var chatMenuOpen = false
    @State private var pendingChatPick: ChatMenuSheet.Entry?
    @State private var deleteTarget: String?
    @State private var isOwner = false
    @State private var agentSettingsOpen = false
    @Environment(\.dismiss) private var dismiss

    init(rootId: String, initialPage: ThreadPage = .conversation, editor: ThreadEditorSlot? = nil) {
        self.rootId = rootId
        self.initialPage = initialPage
        self.editor = editor
        // An issue or session tab may not be in the strip yet: hold it
        // (`pendingPage`).
        if initialPage != .conversation {
            _page = State(initialValue: .conversation)
            _pendingPage = State(initialValue: initialPage)
        } else {
            _page = State(initialValue: initialPage)
        }
    }

    private var rows: ThreadRows? { model?.rows }

    /// The pager's pages: the strip's (an app editor's chat has the
    /// Conversation alone: its band replaces the strip). NEW-46: on a
    /// `transcript` thread with ONE session the Conversation tab IS that
    /// session's page (one page, not two); `bubbles` keeps the Conversation.
    private func pages(_ rows: ThreadRows, _ view: SessionThreadView) -> [ThreadPage] {
        guard editor == nil else { return [.conversation] }
        let main = rows.chatMode == .transcript ? view.mainSessionId : nil
        // EXP-1334: a folded sub-issue is a page too (its chip's subtree);
        // EXP-1343: so is every session chip and every session it folds, in
        // strip order (`+` is never a page).
        return view.tabs.flatMap { tab -> [ThreadPage] in
            switch tab {
            case .conversation: [main.map { .session($0) } ?? .conversation]
            case let .issue(issueId, _, nested): [.issue(issueId)] + nested.map { .issue($0.issueId) }
            case let .session(sessionId, _, _, nested):
                [.session(sessionId)] + nested.map { .session($0.sessionId) }
            case .add: []
            }
        }
    }

    private var currentPages: [ThreadPage] {
        guard let rows, let view = model?.view else { return [.conversation] }
        return pages(rows, view)
    }

    /// The page a thread lands on: its Conversation, or (NEW-6 N-1) the ONE
    /// session's page of a single-session thread.
    private var defaultPage: ThreadPage { resolved(.conversation) }

    /// NEW-46: the page a tab names: on a single-session `transcript`
    /// thread the Conversation tab's page is that session's.
    private func resolved(_ page: ThreadPage) -> ThreadPage {
        guard page == .conversation, editor == nil, let rows, rows.chatMode == .transcript,
              let main = model?.view?.mainSessionId else { return page }
        return .session(main)
    }

    var body: some View {
        ZStack {
            AppBackground()
            if let model, let rows = model.rows, let view = model.view {
                ZStack {
                    FacePager(
                        pages: pages(rows, view),
                        selection: Binding(get: { page }, set: { switchPage($0) })
                    ) { page in
                        pageView(page, rows: rows, view: view)
                    }
                    .opacity(editor?.pane == nil ? 1 : 0)
                    .allowsHitTesting(editor?.pane == nil)
                    .accessibilityHidden(editor?.pane != nil)
                    if let pane = editor?.pane {
                        pane
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .environment(\.threadIssueOutlet, issueOutlet)
                // EXP-1316 (detail-chrome.json): THE detail chrome band on
                // every tab — the nav row (‹ · title + lock · …) over the
                // strip, content scrolling under its scrim + edge fade.
                .workHeaderBand {
                    if let editor {
                        editor.band
                    } else {
                        header(rows: rows, view: view)
                    }
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
        // The header band draws the bar's material — the nav bar's own
        // background and divider stand down (EXP-1150).
        .toolbarBackground(.hidden, for: .navigationBar)
        // EXP-1373: back · the lead disc · title over the subtitle, outside
        // the toolbar builder (its own chrome, iOS 26: no shared glass).
        .ownChromeToolbarItem(placement: .principal, enabled: editor == nil) {
            if let rows, let view = model?.view {
                ChatHeaderTitle(
                    title: rows.isAgent && !rows.agentName.isEmpty ? rows.agentName : view.title,
                    subtitle: ChatHeaderTitle.subtitle(rows: rows),
                    subtitleTone: ChatHeaderTitle.tone(rows: rows),
                    isPrivate: view.isPrivate,
                    agentIcon: rows.isAgent ? rows.agentIcon : nil,
                    agentColor: rows.isAgent ? rows.agentColor : nil,
                    cli: rows.root.cli,
                    mark: SessionTree.threadRowMarkState(
                        needsYou: rows.sessions.contains { $0.needsInput && $0.status != DomainContract.sessionStatusEnded },
                        busy: rows.sessions.contains { $0.busy && $0.status != DomainContract.sessionStatusEnded }
                    )
                )
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .toolbar {
            // EXP-1316: the title, ONE line, not tappable, the lock beside it.
            if let editor {
                ToolbarItem(placement: .principal) { editor.title }
                if let trailing = editor.trailing {
                    ToolbarItem(placement: .topBarTrailing) { trailing }
                }
            }
            // EXP-1307/1316: ONE `…` rides the header's action slot on every
            // tab while it has an entry — Close PR (EXP-1338: no GitHub
            // entry), in THE Menu's sheet.
            // NEW-69 (voice-call.json): an agent chat's telephone, left of `…`.
            ToolbarItem(placement: .topBarTrailing) {
                if editor == nil, let rows, rows.root.isAgent, rows.root.appId == nil,
                   let agentId = rows.root.agentId {
                    AgentCallButton(agentId: agentId, agentName: rows.agentName)
                }
            }
            // S-3 (session-thread.json): the thread PR's Merge pill, left of
            // `…`, on every page while the PR is open (the phones' bottom
            // capsule left with the Guide tab).
            ToolbarItem(placement: .topBarTrailing) { headerMergePill }
            ToolbarItem(placement: .topBarTrailing) {
                if editor == nil, !chatMenuEntries.isEmpty {
                    Button { openChatMenu() } label: {
                        AppIcon(AppIcons.uiMore, size: AppIcon.Size.medium, weight: .medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                    .accessibilityLabel("More")
                    .accessibilityIdentifier(SessionTree.ThreadCopy.guideMenuTestId)
                }
            }
        }
        // Each presentation on its OWN node (EXP-240).
        .background {
            Color.clear
                .sheet(isPresented: $chatMenuOpen, onDismiss: runPendingChatPick) {
                    ChatMenuSheet(title: chatMenuTitle, entries: chatMenuEntries) { entry in
                        pendingChatPick = entry
                        chatMenuOpen = false
                    }
                }
        }
        .background {
            Color.clear
                .sheet(item: $menuTarget, onDismiss: runPendingMenuAction) { target in
                    ThreadMenuSheet(
                        identifier: target.issueId.flatMap { rows?.issue(id: $0)?.identifier },
                        guideEntries: target.guide ? guideMenuEntries : nil
                    ) { action in
                        pendingMenuAction = (action, target.issueId)
                        menuTarget = nil
                    }
                }
        }
        .chatDeleteConfirm(target: $deleteTarget) { id in
            chatActions.delete(rootId: id) { deleted in
                // chats.json: after a delete the chat's screen closes.
                if deleted { dismiss() }
            }
        }
        .navigationDestination(isPresented: $agentSettingsOpen) {
            if let rows, let agentId = rows.root.agentId {
                AgentSettingsPage(teamId: rows.root.teamId, agentId: agentId, isOwner: isOwner)
                    .environment(\.accountId, accountId)
            }
        }
        .task(id: rows?.root.teamId) { await resolveOwner() }
        // EXP-1307: close-pr.json's confirm (the app's own card, EXP-240).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $closePrTarget) { pullRequest in
                    GlassAlert(
                        title: ClosePrCopy.title,
                        message: ClosePrCopy.message(
                            otherLinkedIssues: max((rows?.pullRequestIssueIds[pullRequest.id]?.count ?? 0) - 1, 0)
                        ),
                        actions: [
                            GlassAlertAction(
                                PullRequestStack.cancelLabel, role: .outline, isCancel: true, id: "cancel"
                            ) {},
                            GlassAlertAction(ClosePrCopy.confirm, role: .destructive, id: "close-pr") {
                                closePr(pullRequest)
                            },
                        ]
                    )
                }
        }
        .sheet(item: $foldTarget, onDismiss: runPendingFoldPick) { fold in
            ThreadFoldSheet(
                fold: fold,
                issue: { rows?.issue(id: $0) },
                issueStatus: { rows?.issueStatus(id: $0) },
                shownIssueId: { if case let .issue(id) = page { id } else { nil } }()
            ) { row in
                pendingFoldPick = row
                foldTarget = nil
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
                page = defaultPage
            }
            syncIssueModels()
        }
        // The navigator popped back to THIS page for a filed issue: land on
        // the tab it names.
        .onChange(of: threadLanding.request) { _, request in
            guard let request, request.threadId == rootId else { return }
            land(on: request.page)
        }
        // EXP-1154: a new thread PR starts with no merge in flight.
        .onChange(of: rows?.guidePullRequest?.id) { _, _ in mergeState = WorkMergeState() }
        // EXP-1323: the live tail follows the root while it is not ended.
        .onChange(of: model?.view?.liveSessionId, initial: true) { _, _ in syncLiveTail() }
        // A newer agent message read here is read: re-stamp the root.
        .onChange(of: newestAgentMessageAt) { _, newest in
            if newest != nil { markRead() }
        }
    }

    // MARK: - Header

    /// The band under the nav row: THE strip (the pager), both chat modes.
    @ViewBuilder
    private func header(rows: ThreadRows, view: SessionThreadView) -> some View {
        ThreadStripTabs(
            tabs: view.tabs,
            // NEW-46: the ONE session's page reads as the Conversation tab.
            selection: view.mainSessionId.map { page == .session($0) } == true ? .conversation : page,
            issue: { rows.issue(id: $0) },
            issueStatus: { rows.issueStatus(id: $0) },
            isFresh: { model?.isFresh(issueId: $0) ?? false },
            onSelect: { next in
                pendingPage = nil
                selectPage(next)
            },
            onIssueMenu: { issueId in openMenu(issueId: issueId) },
            onAdd: canEdit ? { openMenu(issueId: nil) } : nil,
            // EXP-1343: a session chip is a page of this pager.
            onOpenSession: { sessionId in
                pendingPage = nil
                openSession(sessionId)
            },
            onOpenFold: { fold in
                UIApplication.endEditing()
                foldTarget = fold
            }
        )
    }

    /// EXP-1334: a fold sheet's pick — an issue shows its page, a session
    /// pushes its screen.
    private func runPendingFoldPick() {
        guard let pick = pendingFoldPick else { return }
        pendingFoldPick = nil
        switch pick {
        case let .issue(issueId, _, _):
            pendingPage = nil
            selectPage(.issue(issueId))
        case let .session(sessionId, _, _, _):
            pendingPage = nil
            openSession(sessionId)
        }
    }

    /// EXP-1343: a session of this thread's strip shows as its page; one
    /// the strip does not hold (another root's) pushes its own screen.
    private func openSession(_ sessionId: String) {
        if currentPages.contains(.session(sessionId)) {
            selectPage(.session(sessionId))
        } else {
            pushRoute(.agentSession(accountId: accountId, sessionId: sessionId))
        }
    }

    /// EXP-1347: an issue pill inside this thread whose issue is one of its
    /// tabs selects that tab; false = not ours (the caller pushes).
    private func openIssueTab(_ issueId: String) -> Bool {
        guard currentPages.contains(.issue(issueId)) else { return false }
        pendingPage = nil
        selectPage(.issue(issueId))
        return true
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
        case let .session(sessionId):
            sessionPage(sessionId, rows: rows)
        }
    }

    /// EXP-1316 (1n): the Conversation scrolls like the session feed — it
    /// opens on the newest row, stays pinned while rows land unless the
    /// reader scrolled up, and THE jump-to-bottom button re-pins (the
    /// session view's `FollowPinTracker`).
    private func conversationPage(rows: ThreadRows, view: SessionThreadView) -> some View {
        GeometryReader { geo in
            ScrollViewReader { scroller in
                ScrollView {
                    VStack(spacing: 0) {
                        ThreadConversationView(
                            rows: rows,
                            view: view,
                            onOpenSession: { sessionId in openSession(sessionId) },
                            onOpenThread: { threadId in
                                pushRoute(.thread(accountId: accountId, threadId: threadId))
                            },
                            onOpenXref: { xref in pushRoute(rows.xrefRoute(xref, accountId: accountId)) },
                            onSubmit: submitAnswer,
                            onUnqueue: unqueue,
                            viewerId: deps.auth.userId,
                            answeringId: answeringId,
                            canAnswer: canEdit,
                            suggestions: suggestionActions,
                            // EXP-1323/1324: THE Queue + the status line off
                            // the live root, as the Conversation's last row.
                            liveTail: liveModel.map { live in
                                ThreadLiveTail(
                                    model: live,
                                    agent: view.liveSessionId.flatMap { rows.session(id: $0)?.cli }
                                )
                            },
                            onReply: { text in reply(text, sessionId: view.composer.sessionId) },
                            onLoadEarlier: { anchor in
                                // The widened window lays out first; then the
                                // row the reader saw on top stays on top.
                                DispatchQueue.main.async { scroller.scrollTo(anchor, anchor: .top) }
                            }
                        )
                        .padding(.vertical, 12)
                        .transcriptColumn()
                        Color.clear
                            .frame(height: insetCorrection)
                        Color.clear
                            .frame(height: 1)
                            .id(Self.bottomId)
                            .onGeometryChange(for: CGFloat.self, of: { $0.frame(in: .global).maxY }) { bottom in
                                anchorBottom = bottom
                            }
                    }
                    // #EXP-1380: the content-frame preference only feeds the
                    // pre-iOS-18 pin path; on 18+ the scroll geometry does,
                    // and the lazy rows are never measured as a whole.
                    .modifier(LegacyFeedOverflow(viewport: geo.size.height, space: Self.coordSpace))
                }
                .coordinateSpace(name: Self.coordSpace)
                .scrollDismissesKeyboard(.interactively)
                .modifier(FollowPinTracker(
                    atBottom: $atBottom,
                    // chats.json FOLLOW ×4: `transcript.followSlack`.
                    slack: CGFloat(Chats.Transcript.followSlack),
                    repin: { scroller.scrollTo(Self.bottomId, anchor: .bottom) }
                ))
                .onAppear {
                    scroller.scrollTo(Self.bottomId, anchor: .bottom)
                    // Lazy rows can still be sizing on the first pass.
                    Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(50))
                        if atBottom { scroller.scrollTo(Self.bottomId, anchor: .bottom) }
                    }
                }
                .onChange(of: view.rows.count) { _, _ in
                    guard atBottom else { return }
                    withAnimation(motion.standard) { scroller.scrollTo(Self.bottomId, anchor: .bottom) }
                }
                // EXP-1316: the composer expanding and the keyboard rising
                // shrink the viewport from below — a reader on the newest row
                // stays on it (Android parity), re-pinned once the inset lands.
                .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardDidShowNotification)) { _ in
                    guard pinOnInset || atBottom else { return }
                    pinOnInset = false
                    atBottom = true
                    repin(scroller)
                }
                .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardDidHideNotification)) { _ in
                    insetCorrection = 0
                    guard atBottom else { return }
                    withAnimation(motion.standard) { scroller.scrollTo(Self.bottomId, anchor: .bottom) }
                }
                // #EXP-1383 (owner 2026-10-24: "exactly as in sessions"):
                // the run view's jump button, placed the run view's way
                // (AgentSessionView.feedBody): an overlay on the SCROLL VIEW
                // inside the Dock's safe-area inset (spacing 0), centred 12pt
                // above the Dock's top edge, only while scrolled away from
                // the newest row of a non-empty transcript.
                .overlay(alignment: .bottom) {
                    ZStack {
                        if !atBottom, !view.rows.isEmpty {
                            JumpToBottomButton {
                                atBottom = true
                                withAnimation(motion.standard) {
                                    scroller.scrollTo(Self.bottomId, anchor: .bottom)
                                }
                            }
                            .jumpToBottomTransition(motion)
                            .padding(.bottom, JumpToBottomButton.bottomGap)
                            .accessibilityIdentifier(Chats.TestIds.jumpToBottom)
                        }
                    }
                    .animation(motion.standard, value: atBottom)
                }
                .safeAreaInset(edge: .bottom, spacing: 0) {
                    // N-12: THE Dock over the Conversation's bottom edge: the
                    // suggested tasks, the live root's Queue, queued
                    // messages and banners, then THE composer (always).
                    let dockRows = conversationDockRows(rows: rows, view: view)
                    DockView(
                        rows: dockRows,
                        openId: $dockOpenId,
                        renderSheet: { row, close in conversationDockSheet(row, rows: rows, view: view, close: close) },
                        renderAction: { row in
                            row.id == "banner:closed"
                                ? liveModel.map(SessionDock.reconnect)
                                : SessionDock.agentHintAction(row) { text in reply(text, sessionId: rows.root.id) }
                        }
                    ) {
                        // EXP-1211: the agent's quick replies, directly above
                        // the composer.
                        SessionComposerReplies(sessionId: view.composer.sessionId)
                        ThreadComposer(
                            sessionId: view.composer.sessionId,
                            root: rows.root,
                            placeholder: rows.composerPlaceholder(view),
                            busySessionIds: rows.sessions
                                .filter { $0.busy && $0.status != DomainContract.sessionStatusEnded }
                                .map(\.id),
                            onExpand: {
                                // Remember the pin BEFORE the inset moves it.
                                pinOnInset = atBottom
                            }
                        )
                    }
                    .floatingBarEdge(!dockRows.isEmpty)
                    // EXP-1316 (Android 247c806b33): the floating composer's
                    // MEASURED height (capsule or open card, + its edge) is
                    // the conversation's bottom inset; a pinned conversation
                    // re-pins whenever it changes — expansion, keyboard,
                    // late layout — so the newest row never hides under it.
                    .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
                        composerTop = frame.minY
                        guard frame.height != composerHeight else { return }
                        if frame.height < composerHeight { insetCorrection = 0 }
                        composerHeight = frame.height
                        guard pinOnInset || atBottom else { return }
                        atBottom = true
                        repin(scroller)
                    }
                }
            }
        }
    }

    private static let coordSpace = "thread-conversation-scroll"

    /// N-12: the Conversation's DockCards: the suggested tasks with an open
    /// item, then the live root's own (`SessionDock`). S-3 (dock.json): a
    /// waiting plan or question is a story, never a DockCard; it stays
    /// inline in the transcript, answerable.
    private func conversationDockRows(rows: ThreadRows, view: SessionThreadView) -> [Dock.Row] {
        guard canEdit else { return [] }
        let live = liveModel.map { SessionDock.sources($0, session: rows.root) } ?? []
        // #EXP-1383: the agent chat's "Instructions changed" banner.
        let hint = SessionDock.agentHint(root: rows.root, agent: rows.agent).map { [$0] } ?? []
        return Dock.rows(Dock.suggestionSources(view.rows) + live + hint)
    }

    /// A card's sheet: the suggested tasks' sheet, the root's Queue and
    /// queued messages their lists.
    private func conversationDockSheet(
        _ row: Dock.Row, rows: ThreadRows, view: SessionThreadView, close: @escaping () -> Void
    ) -> AnyView? {
        if case let .suggestions(id, _, _, startAll, items)? = view.rows.first(where: { $0.id == row.id }) {
            return AnyView(ThreadSuggestionsSheet(
                row: row,
                rowId: id,
                startAll: startAll,
                items: items,
                actions: suggestionActions,
                onOpenSession: { sessionId in
                    close()
                    openSession(sessionId)
                },
                onOpenIssue: { [deps, accountId, issueOutlet] issueId in
                    close()
                    openIssueInnermost(issueId, outlet: issueOutlet, deps: deps, accountId: accountId)
                },
                onClose: close
            ))
        }
        return liveModel.flatMap { SessionDock.sheet(row, model: $0, close: close) }
    }

    /// EXP-1316: pin the newest row above the floating composer — now, after
    /// the next layout pass (the safe-area inset lands one pass late) and
    /// once the card's open animation and the keyboard have settled.
    private func repin(_ scroller: ScrollViewProxy) {
        scroller.scrollTo(Self.bottomId, anchor: .bottom)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
            guard atBottom else { return }
            scroller.scrollTo(Self.bottomId, anchor: .bottom)
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + DesignTokens.Motion.Duration.standard + 0.2) {
            guard atBottom else { return }
            scroller.scrollTo(Self.bottomId, anchor: .bottom)
            // Settled: with the keyboard up the scroll view's inset falls
            // short of the composer by the bottom safe area it swallowed
            // (34pt on a home-indicator phone). The anchor always lands on
            // the scroll view's own bottom, so the shortfall is measured
            // against it once and padded out above the anchor; a later pass
            // measures the same gap and changes nothing.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                let shortfall = anchorBottom - composerTop - insetCorrection
                guard atBottom, composerTop > 0, shortfall > 0.5, shortfall < 200 else { return }
                insetCorrection += shortfall
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                    scroller.scrollTo(Self.bottomId, anchor: .bottom)
                }
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
                hostThreadId: rootId,
                embedded: true
            )
        } else {
            SkeletonRows()
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
    }

    /// EXP-1343: a session page = that session's raw work + its composer
    /// (the session view's body, no header of its own) under the thread's
    /// ONE header. Only the SHOWN page holds a live model: a neighbour
    /// renders cheap skeleton rows, and a page left unmounts its view (the
    /// claim goes with it).
    @ViewBuilder
    private func sessionPage(_ sessionId: String, rows: ThreadRows) -> some View {
        if page == .session(sessionId), let session = rows.session(id: sessionId) {
            AgentSessionView(
                accountId: accountId,
                session: session,
                embedded: true,
                onModel: { id, model in
                    if let model {
                        sessionPageModel = (id, model)
                    } else if sessionPageModel?.id == id {
                        sessionPageModel = nil
                    }
                }
            )
            .id(sessionId)
        } else {
            SkeletonRows(rows: 3, rowHeight: 28, dividers: false)
                .transcriptColumn()
                .padding(.top, 8)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
    }

    // MARK: - The thread PR's Merge pill

    /// S-3 (session-thread.json): the header's Merge pill over the thread PR
    /// (`guidePullRequest`) while it is open (EXP-1307 `testIds.guideMerge`);
    /// on a member of an open stack it merges the stack (`guideStackConfirm`).
    @ViewBuilder
    private var headerMergePill: some View {
        if editor == nil, canEdit, let rows,
           let pullRequest = rows.guidePullRequest, pullRequest.isOpen {
            WorkMergePill(
                pullRequest: pullRequest,
                issueCount: rows.pullRequestIssueIds[pullRequest.id]?.count ?? 0,
                stackConfirm: rows.guideStackConfirm,
                state: $mergeState,
                identifier: SessionTree.ThreadCopy.guideMergeTestId,
                style: .toolbar
            )
            .id(pullRequest.id)
        }
    }

    // MARK: - Cards (answered in place)

    private func submitAnswer(_ card: ThreadCardAnswer) {
        guard answeringId == nil else { return }
        answeringId = card.messageId
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                try await api.answer(
                    accountId: accountId, messageId: card.messageId, answer: card.answer, decision: card.decision,
                    fresh: card.fresh, feedback: card.feedback, answers: card.answers, surfaceData: card.surfaceData
                )
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(
                        verb: "answer", thing: card.decision == nil ? "the question" : "the plan"
                    ),
                    description: error.userFacingMessage
                )
            }
            answeringId = nil
        }
    }

    /// EXP-1317: revoke the viewer's own queued message (`sessions.unqueue`);
    /// the Electric echo drops the row, a refusal toasts.
    private func unqueue(_ messageId: String) {
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                try await api.unqueue(accountId: accountId, messageId: messageId)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "remove", thing: "the message"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    // MARK: - Suggested tasks (EXP-1323)

    private var suggestionActions: ThreadSuggestionActions {
        ThreadSuggestionActions(
            onStart: { rowId, itemId in startSuggestion(rowId, itemId, where: .child) },
            onStartAll: { rowId in
                runSuggestion(ThreadSuggestionActions.key(rowId), verb: "start", thing: "the tasks") { api, accountId in
                    try await api.startAllSuggestions(accountId: accountId, messageId: rowId)
                }
            },
            onMenu: { rowId, itemId, action in runSuggestionMenu(rowId, itemId, action) },
            devices: { [teamId = rows?.root.teamId] in await loadSuggestionDevices(teamId: teamId) },
            pending: suggestionPending
        )
    }

    private func startSuggestion(
        _ rowId: String, _ itemId: String, where target: SuggestionStartWhere,
        deviceId: String? = nil, model: String? = nil
    ) {
        runSuggestion(ThreadSuggestionActions.key(rowId, itemId), verb: "start", thing: "the task") { api, accountId in
            try await api.startSuggestion(
                accountId: accountId, messageId: rowId, itemId: itemId, where: target,
                deviceId: deviceId, model: model
            )
        }
    }

    private func dismissSuggestion(_ rowId: String, _ itemId: String) {
        runSuggestion(ThreadSuggestionActions.key(rowId, itemId), verb: "dismiss", thing: "the task") { api, accountId in
            try await api.dismissSuggestion(accountId: accountId, messageId: rowId, itemId: itemId)
        }
    }

    /// One suggestion call: its key waits while it is out (the Electric echo
    /// moves the item), a refusal toasts `Couldn't <verb> <thing>`.
    private func runSuggestion(
        _ key: String, verb: String, thing: String,
        _ call: @escaping @Sendable (SessionsApi, String) async throws -> Void
    ) {
        guard !suggestionPending.contains(key) else { return }
        suggestionPending.insert(key)
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                try await call(api, accountId)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: verb, thing: thing),
                    description: error.userFacingMessage
                )
            }
            suggestionPending.remove(key)
        }
    }

    /// A pick of the item's ⋯ menu (run once its sheet has dismissed).
    private func runSuggestionMenu(_ rowId: String, _ itemId: String, _ action: SuggestionOptionsSheet.Action) {
        switch action {
        case .here:
            startSuggestion(rowId, itemId, where: .here)
        case let .startOn(deviceId):
            startSuggestion(rowId, itemId, where: .child, deviceId: deviceId)
        case let .startWith(model):
            startSuggestion(rowId, itemId, where: .child, model: model)
        case .file:
            runSuggestion(ThreadSuggestionActions.key(rowId, itemId), verb: "file", thing: "the issue") { api, accountId in
                try await api.fileSuggestion(accountId: accountId, messageId: rowId, itemId: itemId)
            }
        case .dismiss:
            dismissSuggestion(rowId, itemId)
        }
    }

    /// Start on…'s rows: the machines a start can reach right now with an
    /// agent to run (the composer's pool).
    private func loadSuggestionDevices(teamId: String?) async -> [SuggestionOptionsSheet.Device] {
        let devices = await DeviceQueries.onlineStartTargets(
            db: deps.db, accountId: accountId, teamId: teamId, userId: deps.auth.userId
        )
        return devices.filter(\.hasRunnableAgent).map {
            SuggestionOptionsSheet.Device(
                id: $0.deviceId, name: $0.deviceLabel, icon: DeviceIconDisplay.iconName(for: $0)
            )
        }
    }

    // MARK: - Live tail + read marks (EXP-1323)

    /// Claim the live root's socket owner while it is not ended; release it
    /// once it ends or the screen goes.
    private func syncLiveTail() {
        let wanted = model?.view?.liveSessionId
        guard wanted != liveClaimId else { return }
        releaseLiveTail()
        guard let wanted, let root = rows?.session(id: wanted) else { return }
        liveClaimId = wanted
        liveModel = deps.attachSteerModel(accountId: accountId, session: root)
    }

    private func releaseLiveTail() {
        guard let claimed = liveClaimId else { return }
        liveClaimId = nil
        liveModel = nil
        deps.steerSessions.detach(accountId: accountId, sessionId: claimed)
    }

    /// The tree's newest agent-authored message (the unread rule's input).
    private var newestAgentMessageAt: String? {
        rows?.messages
            .filter { $0.authorKind == DomainContract.messageAuthorKindAgent }
            .map(\.createdAt)
            .max()
    }

    /// `sessions.markRead` on the ROOT; a background failure stays silent.
    private func markRead() {
        let api = deps.sessionsApi
        let accountId = accountId
        let rootId = rows?.root.id ?? rootId
        Task { try? await api.markRead(accountId: accountId, sessionId: rootId) }
    }

    // MARK: - The chat's menu (EXP-1373)

    private var chatActions: ChatActions {
        ChatActions(deps: deps, accountId: accountId, toaster: toaster)
    }

    /// The starter or a team owner steers the chat.
    /// web `canSteerSession`: a shared chat, or the viewer's own.
    private var canSteer: Bool {
        guard let rows else { return false }
        return rows.root.shared || rows.root.userId == deps.auth.userId
    }

    /// The starter or a team owner deletes a chat (`sessions.delete`).
    private var canDelete: Bool {
        guard let rows else { return false }
        return isOwner || rows.root.userId == deps.auth.userId
    }

    private var chatMenuTitle: String? {
        guard let rows, let view = model?.view else { return nil }
        return rows.isAgent && !rows.agentName.isEmpty ? rows.agentName : view.title
    }

    /// The header `…`: an agent chat's `agentMenuEntries`; a thread's
    /// `threadMenuEntries`, then the Guide's Close PR (EXP-1307).
    private var chatMenuEntries: [ChatMenuSheet.Entry] {
        guard let rows else { return [] }
        if rows.isAgent {
            guard rows.root.appId == nil else { return [] }
            return Chats.agentMenuEntries(busy: rows.root.busy, isOwner: isOwner).map { .agent($0) }
        }
        let own = Chats.threadMenuEntries(rootStatus: rows.root.status, canSteer: canSteer, canDelete: canDelete).map { ChatMenuSheet.Entry.thread($0) }
        return own + guideMenuEntries.map { _ in .closePr }
    }

    private func openChatMenu() {
        UIApplication.endEditing()
        chatMenuOpen = true
    }

    private func runPendingChatPick() {
        guard let pick = pendingChatPick, let rows else { return }
        pendingChatPick = nil
        switch pick {
        case .thread(.delete):
            deleteTarget = rows.root.id
        case let .agent(entry):
            switch entry {
            case .compact, .newConversation:
                if let text = Chats.agentMenuSends[entry] { reply(text, sessionId: rows.root.id) }
            case .settings:
                agentSettingsOpen = true
            }
        case .closePr:
            closePrTarget = rows.guidePullRequest.flatMap { $0.isOpen ? $0 : nil }
        }
    }

    /// A surface card's reply / an agent menu's builtin line: the person's
    /// next message, through the one send path.
    private func reply(_ text: String, sessionId: String) {
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                _ = try await api.send(accountId: accountId, .message(sessionId: sessionId, text: text))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "send", thing: "the message"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    private func resolveOwner() async {
        guard let teamId = rows?.root.teamId, let userId = deps.auth.userId,
              let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        isOwner = (try? await pool.read { db in
            try TeamMemberEntity
                .filter(Column("team_id") == teamId && Column("user_id") == userId)
                .fetchOne(db)?.role == "owner"
        }) ?? false
    }

    // MARK: - THE Menu

    private func openMenu(issueId: String?) {
        guard canEdit else { return }
        UIApplication.endEditing()
        menuTarget = ThreadMenuTarget(issueId: issueId)
    }

    /// EXP-1307: the Guide header's `…` entries (`guideMenuEntries`, fixture
    /// `guideMenuCases` ×4) over the Guide's PR; empty = no trigger.
    private var guideMenuEntries: [SessionTree.GuideMenuEntry] {
        SessionTree.guideMenuEntries(state: rows?.guidePullRequest?.state, member: canEdit)
    }

    private func openGuideMenu() {
        UIApplication.endEditing()
        menuTarget = ThreadMenuTarget(issueId: nil, guide: true)
    }

    private func runPendingMenuAction() {
        guard let pending = pendingMenuAction else { return }
        pendingMenuAction = nil
        switch pending.action {
        case .linkIssue: linkSheetOpen = true
        case .newIssue: newIssue()
        case .unlink:
            if let issueId = pending.issueId { unlink(issueId) }
        case .closePr:
            closePrTarget = rows?.guidePullRequest.flatMap { $0.isOpen ? $0 : nil }
        }
    }

    /// EXP-1307: `pullRequests.close` on the Guide's open PR; the Electric
    /// echo flips its state (the entry goes), a refusal toasts.
    private func closePr(_ pullRequest: PullRequestEntity) {
        let api = deps.pullRequestsApi
        let accountId = accountId
        Task {
            do {
                try await api.close(accountId: accountId, pullRequestId: pullRequest.id)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "close", thing: "the PR"),
                    description: error.userFacingMessage
                )
            }
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
        if page == .issue(issueId) { selectPage(defaultPage) }
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
        issueOutlet.setHandler { issueId in openIssueTab(issueId) }
        if model == nil {
            model = ThreadScreenModel(rootId: rootId, accountId: accountId, db: deps.db)
        }
        model?.start()
        syncIssueModels()
        syncLiveTail()
        markRead()
    }

    private func disappear() {
        UIApplication.endEditing()
        model?.stop()
        stopIssueModel()
        releaseLiveTail()
    }

    /// EXP-1288: ONE issue model observes at a time — the shown issue tab's,
    /// created the first time its page is reached and kept (stopped) once
    /// the page is left. An unlinked issue's model is dropped. EXP-1343: the
    /// shown page's NEIGHBOURS get their (idle) model built too, so a swipe
    /// lands on the page, not a skeleton flash.
    private func syncIssueModels() {
        guard let rows else { return }
        let ids = Set(rows.issues.map(\.id))
        for id in issueVMs.keys where !ids.contains(id) {
            if observingIssueId == id { stopIssueModel() }
            issueVMs[id] = nil
        }
        let pages = currentPages
        if let at = pages.firstIndex(of: page) {
            for neighbour in [at - 1, at + 1] where pages.indices.contains(neighbour) {
                if case let .issue(id) = pages[neighbour], ids.contains(id), issueVMs[id] == nil {
                    let vm = makeIssueModel(id)
                    vm.preload()
                    issueVMs[id] = vm
                }
            }
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
    private func land(on request: ThreadPage) {
        let target = resolved(request)
        menuTarget = nil
        if currentPages.contains(target) {
            pendingPage = nil
            selectPage(target)
        } else {
            pendingPage = target
            selectPage(defaultPage)
        }
    }

    private func selectPage(_ next: ThreadPage) {
        withAnimation(motion.standard) { switchPage(next) }
    }

    private func switchPage(_ request: ThreadPage) {
        let next = resolved(request)
        guard next != page else { return }
        UIApplication.endEditing()
        page = next
        syncIssueModels()
    }
}

/// What the strip's menu sheet is open for — an issue tab (`issueId`), the
/// "+" (nil) or the Guide's `…` (`guide`, EXP-1307).
/// #EXP-1380: the pre-iOS-18 `FollowPinTracker` input (the content's
/// overflow below the viewport); iOS 18+ reads the scroll geometry and
/// skips the GeometryReader entirely.
private struct LegacyFeedOverflow: ViewModifier {
    let viewport: CGFloat
    let space: String

    func body(content: Content) -> some View {
        if #available(iOS 18.0, *) {
            content
        } else {
            content.background(
                GeometryReader { frame in
                    Color.clear.preference(
                        key: FeedBottomOverflowKey.self,
                        value: frame.frame(in: .named(space)).maxY - viewport
                    )
                }
            )
        }
    }
}

struct ThreadMenuTarget: Identifiable {
    let issueId: String?
    var guide = false
    var id: String { guide ? "guide" : issueId ?? "add" }
}

/// EXP-1295: a Conversation's draft for THE session composer box
/// (`SessionComposerCard`, its controls fed by the root's live session model)
/// — `sessions.send({sessionId, text, attachmentIds})` into the root:
/// an idle session resumes, an ended thread continues server-side. Picks
/// upload to that session first, each once. S-3: an agent WITHOUT a
/// conversation yet (`agentId`) sends `sessions.send({agentId, text,
/// attachmentIds})` instead, which opens the viewer's conversation (its
/// uploads ride the team route: there is no session yet).
@MainActor
@Observable
final class ThreadComposerDraft: SessionComposerDraft {
    let sessionId: String
    let draftEditor = IssueEditorModel()
    /// S-3: the agent whose conversation this first send opens; nil = a
    /// message into `sessionId`.
    let agentId: String?
    /// The team (an agent's first message uploads through the team route).
    var teamId: String?
    var pendingImages: [PendingSteerImage] = []
    var pendingFiles: [PendingSteerFile] = []
    var steerImageError: String?
    var steerSending = false
    /// A refused send: the host toasts it (the draft stays put).
    var sendError: String?

    @ObservationIgnored private let accountId: String
    @ObservationIgnored private let sessionsApi: SessionsApi
    @ObservationIgnored private let attachmentsApi: AttachmentsApi

    init(
        sessionId: String,
        agentId: String? = nil,
        accountId: String,
        sessionsApi: SessionsApi,
        attachmentsApi: AttachmentsApi
    ) {
        self.sessionId = sessionId
        self.agentId = agentId
        self.accountId = accountId
        self.sessionsApi = sessionsApi
        self.attachmentsApi = attachmentsApi
    }

    func uploadAttachment(data: Data, filename: String, contentType: String) async throws -> UploadedAttachment {
        if agentId != nil, let teamId {
            return try await attachmentsApi.uploadTeamSessionImage(
                accountId: accountId,
                teamId: teamId,
                data: data,
                filename: filename,
                contentType: contentType
            )
        }
        return try await attachmentsApi.uploadSessionImage(
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
        let text = text.isEmpty ? nil : text
        let attachmentIds = attachmentIds?.isEmpty == false ? attachmentIds : nil
        do {
            if let agentId {
                _ = try await sessionsApi.send(
                    accountId: accountId,
                    .agent(agentId: agentId, text: text, attachmentIds: attachmentIds)
                )
            } else {
                try await sessionsApi.send(
                    accountId: accountId,
                    .message(sessionId: sessionId, text: text, attachmentIds: attachmentIds)
                )
            }
            return true
        } catch {
            sendError = error.userFacingMessage
            return false
        }
    }
}

/// EXP-1316: THE floating composer (ExpUI `FloatingComposer`) under a
/// Conversation for one session: the `Reply…` capsule floating over the
/// content, a tap expands it into THE session composer box (text, the `+`,
/// send, Stop while the tree works), blur folds it back when nothing would
/// be lost (the session view's rule).
///
/// The open card carries the ROOT's controls (fed by its live
/// `AgentSessionModel`, the same relay source the session view reads): S-3
/// (composer-menu.json) the `+` is the ONLY tool — THE "+" sheet with Photo
/// or file, Issue › (`#IDENT` at the caret) and, while the agent switches
/// models, Model › (`/model <alias>`), surface `agent` on an agent chat,
/// `session` otherwise — the usage ring, the `/` keys with THE
/// `SlashCommandMenu`, Stop while busy. The same composer opens an agent's
/// conversation (`agentId`, no root yet): its first send is
/// `sessions.send({agentId})`.
struct ThreadComposer: View {
    let sessionId: String
    /// The root's synced row: the live model attaches to it.
    var root: SessionEntity?
    let placeholder: String
    /// The tree's live sessions working a turn — Stop interrupts them.
    var busySessionIds: [String] = []
    /// S-3: an agent with no conversation yet — the first send opens it
    /// (`sessionId` is empty then), `teamId` = the agent's team.
    var agentId: String?
    var teamId: String?
    /// The capsule was tapped open (the host keeps its feed pinned).
    var onExpand: () -> Void = {}

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.motion) private var motion
    @State private var draft: ThreadComposerDraft?
    @State private var attach = SessionComposerAttach()
    @State private var expanded = false
    /// Set by `expand`, cleared once the editor takes focus (or the retry
    /// window passes): the blur fold stands down meanwhile.
    @State private var focusPending = false
    /// The root's live session model (EXP-621's ref-counted store): the
    /// config/usage the Controls read. `claimedId` = the one claim held.
    @State private var steerModel: AgentSessionModel?
    @State private var claimedId: String?
    @State private var showUsageSheet = false
    /// EXP-724: the draft the `/` menu was dismissed at; the keyboard row.
    @State private var slashDismissedFor: String?
    @State private var slashHighlight = 0
    /// A confirm-gated command waiting on its dialog (`/clear`).
    @State private var slashConfirm: SlashCommand?
    /// NEW-2: the team's prompts, after the builtins in the `/` menu.
    @State private var slashPrompts = TeamSlashPrompts()

    private var draftTeamId: String? { root?.teamId ?? teamId }

    var body: some View {
        Group {
            if let draft, draft.sessionId == sessionId {
                let open = composerOpen
                VStack(spacing: 8) {
                    if open, slashMenuVisible(draft) {
                        SlashCommandMenu(
                            candidates: slashMatches(draft),
                            highlighted: slashHighlight
                        ) { candidate in
                            applySlashCandidate(candidate, draft)
                        }
                        .padding(.horizontal, 16)
                    }
                    FloatingComposer(
                        placeholder: placeholder,
                        open: open,
                        collapsedLabel: placeholder,
                        collapsedIdentifier: "thread-composer-collapsed",
                        onExpand: expand
                    ) {
                        SessionComposerCard(
                            draft: draft,
                            placeholder: placeholder,
                            // S-3: the "+" sheet (Model on an agent chat /
                            // a model-switching session) and the ring.
                            controls: controls,
                            attach: $attach,
                            sendEnabled: !draft.steerSending,
                            showsStop: !busySessionIds.isEmpty && !draft.hasContent,
                            canStop: !busySessionIds.isEmpty,
                            onStop: stop,
                            onReturn: { handleReturn(draft) },
                            onSend: { send(draft) },
                            onArrow: { delta in
                                guard slashMenuVisible(draft) else { return false }
                                moveSlashHighlight(delta, draft)
                                return true
                            },
                            onEscape: {
                                guard slashMenuVisible(draft) else { return false }
                                slashDismissedFor = draft.draftText
                                return true
                            },
                            fieldIdentifier: "thread-composer-field"
                        )
                    }
                }
                .animation(motion.standard, value: open)
                .onChange(of: draft.draftText) { _, _ in
                    slashHighlight = 0
                }
                .onChange(of: draft.sendError) { _, message in
                    guard let message else { return }
                    toaster.error(FeedbackStates.Errors.title(verb: "send", thing: "the message"), description: message)
                }
                .onChange(of: steerModel?.moveError) { _, message in
                    guard let message else { return }
                    toaster.error(SessionDeviceMove.failureToast, description: message)
                }
                .onChange(of: draft.draftEditor.isEditing) { _, editing in
                    if editing { focusPending = false }
                    guard expanded, !editing, !focusPending else { return }
                    guard !attach.busy, !draft.hasContent, slashConfirm == nil else { return }
                    withAnimation(motion.standard) { expanded = false }
                }
            } else {
                Color.clear.frame(height: 1)
            }
        }
        // The reading column, the bottom edge fade behind the whole bar
        // (never behind an open card, which is opaque).
        .frame(maxWidth: DesignTokens.Transcript.maxWidth)
        .frame(maxWidth: .infinity)
        .floatingBarEdge(!composerOpen)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ThreadCopy.composerTestId)
        // The composed session can move (a resumed child): a new draft.
        .task(id: sessionId) {
            if draft?.sessionId != sessionId {
                draft = ThreadComposerDraft(
                    sessionId: sessionId,
                    agentId: sessionId.isEmpty ? agentId : nil,
                    accountId: accountId,
                    sessionsApi: deps.sessionsApi,
                    attachmentsApi: deps.attachmentsApi
                )
            }
            draft?.teamId = draftTeamId
        }
        .task(id: draftTeamId) {
            slashPrompts.bind(accountId: accountId, teamId: draftTeamId, db: deps.db)
        }
        .onAppear { claimRootModel() }
        .onChange(of: root?.id) { _, _ in claimRootModel() }
        .onDisappear { releaseRootModel() }
        .sheet(isPresented: $showUsageSheet) {
            if let model = steerModel {
                AgentUsageSheet(
                    agent: model.session?.cli,
                    runAccount: model.runAccount,
                    account: model.agentAccount,
                    usage: model.runUsage,
                    sessionUsage: model.sessionUsage,
                    contextLayout: model.sessionContextLayout,
                    onOpen: { model.requestUsageRefresh() }
                )
            }
        }
        // EXP-724: `/clear` confirms before it goes out (copy ×4).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $slashConfirm) { command in
                    GlassAlert(
                        title: SlashCommands.confirmTitle(command),
                        message: SlashCommands.confirmBody,
                        actions: [
                            GlassAlertAction("Cancel", role: .outline, isDefault: true, isCancel: true, id: "cancel") {},
                            GlassAlertAction(SlashCommands.confirmButton(command), role: .destructive, id: "confirm") {
                                if let draft { Task { await draft.sendDraft() } }
                            },
                        ]
                    )
                }
        }
    }

    // MARK: - The root's controls

    /// The root's agent (its models; codex words Effort as Reasoning).
    private var agent: String { steerModel?.catalogAgent ?? root?.cli ?? "claude" }

    /// The card's controls: the "+" sheet's surface, team and Model row,
    /// and the usage ring. No root yet (an agent's first message) = nothing
    /// to switch: no Model row.
    private var controls: SessionComposerFooter.Controls {
        let row = root ?? steerModel?.session
        let ended = row == nil || row?.status == DomainContract.sessionStatusEnded
        let percent = steerModel?.sessionUsage?.percent
        return SessionComposerFooter.Controls(
            modelValue: steerModel?.composerModelValue(row: root) ?? root?.model,
            agent: agent,
            // EXP-877: a model switch is a PLAIN MESSAGE into the root — never
            // into an ENDED one.
            onPickModel: { alias in
                guard let draft, !ended else { return }
                Task { await draft.sendMessage("/model \(alias)", attachmentIds: nil) }
            },
            usageFraction: percent.map { Double($0) / 100 },
            usageSeverity: AgentUsagePresentation.severity(percent.map(Double.init)),
            onUsage: { showUsageSheet = true },
            ended: ended,
            surface: (root?.agentId ?? agentId).map { $0.isEmpty ? .session : .agent } ?? .session,
            teamId: draftTeamId
        )
    }

    /// ONE claim on the root's model (`attachSteerModel` / `detach` pair).
    private func claimRootModel() {
        guard let root, claimedId != root.id else { return }
        releaseRootModel()
        steerModel = deps.attachSteerModel(accountId: accountId, session: root)
        claimedId = root.id
    }

    private func releaseRootModel() {
        guard let claimedId else { return }
        deps.steerSessions.detach(accountId: accountId, sessionId: claimedId)
        self.claimedId = nil
        steerModel = nil
    }

    // MARK: - Slash commands (EXP-724)

    private var catalogAgent: String? {
        steerModel?.catalogAgent ?? root?.cli
    }

    /// NEW-2: the builtins this agent runs, then the team's prompts.
    private func slashMatches(_ draft: ThreadComposerDraft) -> [SlashCandidate] {
        SlashCommands.candidates(draft: draft.draftText, agent: catalogAgent, prompts: slashPrompts.prompts)
    }

    /// A command rewrites the draft; a prompt's body (`prompts.get`) replaces
    /// the `/name` draft (web `session-composer.tsx`). Never sends.
    private func applySlashCandidate(_ candidate: SlashCandidate, _ draft: ThreadComposerDraft) {
        switch candidate {
        case let .command(command):
            applySlashCommand(command, draft)
        case let .prompt(prompt):
            slashDismissedFor = draft.draftText
            slashHighlight = 0
            Task {
                guard let body = await TeamSlashPrompts.loadBody(prompt, accountId: accountId, deps: deps),
                      SlashCommands.isSlashDraft(draft.draftText)
                else { return }
                draft.draftText = body
                focus()
            }
        }
    }

    private func slashMenuVisible(_ draft: ThreadComposerDraft) -> Bool {
        draft.draftEditor.isEditing && !slashMatches(draft).isEmpty
            && slashDismissedFor != draft.draftText
    }

    /// Accepting a row REWRITES the draft and never sends.
    private func applySlashCommand(_ command: SlashCommand, _ draft: ThreadComposerDraft) {
        draft.draftText = command.insertion
        slashDismissedFor = command.insertion
        slashHighlight = 0
        focus()
    }

    private func moveSlashHighlight(_ delta: Int, _ draft: ThreadComposerDraft) {
        let count = slashMatches(draft).count
        guard count > 0 else { return }
        slashHighlight = ((slashHighlight + delta) % count + count) % count
    }

    /// Return accepts the highlighted `/` row while the menu is up, else sends.
    private func handleReturn(_ draft: ThreadComposerDraft) {
        if slashMenuVisible(draft) {
            let matches = slashMatches(draft)
            let candidate = matches.indices.contains(slashHighlight) ? matches[slashHighlight] : matches.first
            if let candidate { applySlashCandidate(candidate, draft) }
            return
        }
        send(draft)
    }

    /// A slash command is the WHOLE message: no attachments around it (the
    /// toast refuses, web parity), and `/clear` asks first; everything else
    /// goes out as typed.
    private func send(_ draft: ThreadComposerDraft) {
        guard !draft.steerSending else { return }
        if let command = SlashCommands.command(
            for: draft.draftText, agent: catalogAgent
        ) {
            if draft.hasPendingAttachments {
                toaster.error(SlashCommands.attachmentsRefusal)
                return
            }
            if command.confirm {
                slashConfirm = command
                return
            }
        }
        Task { await draft.sendDraft() }
    }

    // MARK: - Open / fold

    /// The card shows: tapped open, or a draft / pick / open picker keeps it.
    private var composerOpen: Bool {
        guard let draft, draft.sessionId == sessionId else { return false }
        return expanded || draft.hasContent || attach.busy
    }

    /// Expand + focus (one runloop hop, a 150ms retry — the session view's
    /// `expandComposer`).
    private func expand() {
        onExpand()
        withAnimation(motion.standard) { expanded = true }
        focusPending = true
        DispatchQueue.main.async { focus() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
            if expanded, draft?.draftEditor.isEditing == false { focus() }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { focusPending = false }
    }

    private func focus() {
        guard let editor = draft?.draftEditor else { return }
        editor.setFocused(editor.blocks.first?.id)
    }

    /// Stop = interrupt every working session of the tree.
    private func stop() {
        let api = deps.sessionsApi
        let accountId = accountId
        let ids = busySessionIds
        Task {
            do {
                var undelivered = false
                for id in ids {
                    let result = try await api.stop(accountId: accountId, sessionId: id)
                    if result.delivered == false { undelivered = true }
                }
                // chats.json `copy.stopTurn`: a stop no device got toasts.
                if undelivered { toaster.warning(Chats.Copy.StopTurn.notDelivered) }
            } catch {
                toaster.error(Chats.Copy.StopTurn.failed, description: error.userFacingMessage)
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
