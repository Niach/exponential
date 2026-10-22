import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-1295: the phone's ONE screen for a root session (fixture
/// `session-thread.json`) — a THREAD, or a team AGENT's channel (the same
/// Conversation, Slack-like, its started threads as session cards).
///
/// * NEW-6 (N-1): a thread with ONE session (the root alone) IS that
///   session: its page (raw work + composer, Stop in the header while live)
///   is the main thing and the strip has no Conversation tab; a second
///   session brings the Conversation tab back.
/// * Header = the title (plain, never renamed — EXP-1316; a lock when
///   private; an agent's icon in its color before its name) and, for a
///   thread, THE strip — Conversation · one chip per linked issue · "+" ·
///   the session chips · Guide — as the pager (`FacePager`). Nothing else:
///   no caption, no Stop, no device, no Show work.
/// * Conversation = `ThreadConversationView` over `buildSessionThread`, with
///   THE session composer box under it (`ThreadComposer` →
///   `SessionComposerCard`), sending
///   `sessions.send({sessionId: root})`.
/// * An issue tab = the issue page EMBEDDED (EXP-1316): no own header, no
///   Threads row, no Start; the bottom bar keeps properties + Comment.
/// * Guide = the existing Guide face over the root's `guide` (the PR body),
///   counting the thread PR's files, with its Merge capsule while it is open
///   and ONE `…` in the header's action slot (EXP-1307: THE Menu's sheet,
///   `guideMenuEntries` — Close PR (EXP-1338) behind the close-pr.json
///   confirm). EXP-1306: while that PR
///   sits in a linear open stack, a `Stack` band + THE `StackRail` lead it
///   (a row opens that PR's thread, a hold offers Merge through here).
///
/// The "+" (left of Conversation, EXP-1334) opens THE Menu's sheet: Link an
/// issue (THE search list → `sessions.linkIssues`) · New issue (the New
/// issue page filing into this root); an issue chip's long press adds
/// Unlink (`sessions.unlinkIssue`). The strip never scrolls (EXP-1334): a
/// folded chip or a "+N" opens `ThreadFoldSheet`.
/// Failed actions toast (`Couldn't link the issue`, feedback-states).
///
/// EXP-1323: a suggested tasks card acts through `sessions.startSuggestion`
/// & co. (its Options = THE Menu's sheet, `SuggestionOptionsSheet`); the
/// live root's relay state draws THE bottom strip (Tasks + status lines,
/// the session view's own) above the composer; opening the screen stamps
/// the viewer's read of the root (`sessions.markRead`).
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
    // Guide
    @State private var guideSection: GuideSectionKey?
    @State private var focusPath: String?
    @State private var mergeState = WorkMergeState()
    /// EXP-1306: a rail member's Merge through here, awaiting its confirm.
    @State private var mergeThrough: PullRequestStack.Confirm?
    @State private var prChanges: ChangesViewModel?
    /// EXP-1307: the Guide PR awaiting the Close PR confirm.
    @State private var closePrTarget: PullRequestEntity?
    // EXP-1323: suggested tasks
    /// In-flight suggestion actions (`ThreadSuggestionActions.key`).
    @State private var suggestionPending: Set<String> = []
    /// The item whose Options sheet is up.
    @State private var suggestionOptions: SuggestionOptionsTarget?
    /// The Options sheet's Start on… rows (loaded as it opens).
    @State private var suggestionDevices: [SuggestionOptionsSheet.Device]?
    /// The sheet's pick, run once the sheet has dismissed.
    @State private var pendingSuggestionAction: (target: SuggestionOptionsTarget, action: SuggestionOptionsSheet.Action)?
    /// EXP-1323: the live tail's claim on the root's socket owner
    /// (`attachSteerModel`, paired with `steerSessions.detach`).
    @State private var liveModel: AgentSessionModel?
    @State private var liveClaimId: String?
    /// EXP-1343: the shown session page's model (its embedded view's claim),
    /// for the header's Stop; nil off a session page.
    @State private var sessionPageModel: (id: String, model: AgentSessionModel)?
    @State private var sessionStopConfirm = false
    /// EXP-1347: the innermost outlet for issue pills inside this thread.
    @State private var issueOutlet = ThreadIssueOutlet()

    init(rootId: String, initialPage: ThreadPage = .conversation) {
        self.rootId = rootId
        self.initialPage = initialPage
        // An issue or session tab may not be in the strip yet: hold it
        // (`pendingPage`).
        if initialPage != .conversation, initialPage != .guide {
            _page = State(initialValue: .conversation)
            _pendingPage = State(initialValue: initialPage)
        } else {
            _page = State(initialValue: initialPage)
        }
    }

    private var rows: ThreadRows? { model?.rows }

    /// The pager's pages: the strip's (an agent channel has the
    /// Conversation alone). NEW-46: on a thread with ONE session the
    /// Conversation tab IS that session's page (one page, not two).
    private func pages(_ rows: ThreadRows, _ view: SessionThreadView) -> [ThreadPage] {
        guard !rows.isAgent else { return [.conversation] }
        // EXP-1334: a folded sub-issue is a page too (its chip's subtree);
        // EXP-1343: so is every session chip and every session it folds, in
        // strip order (`+` is never a page).
        return view.tabs.flatMap { tab -> [ThreadPage] in
            switch tab {
            case .conversation: [view.mainSessionId.map { .session($0) } ?? .conversation]
            case let .issue(issueId, _, nested): [.issue(issueId)] + nested.map { .issue($0.issueId) }
            case let .session(sessionId, _, _, nested):
                [.session(sessionId)] + nested.map { .session($0.sessionId) }
            case .guide: [.guide]
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

    /// NEW-46: the page a tab names: on a single-session thread the
    /// Conversation tab's page is that session's.
    private func resolved(_ page: ThreadPage) -> ThreadPage {
        guard page == .conversation, let rows, !rows.isAgent,
              let main = model?.view?.mainSessionId else { return page }
        return .session(main)
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
                .environment(\.threadIssueOutlet, issueOutlet)
                // EXP-1316 (detail-chrome.json): THE detail chrome band on
                // every tab — the nav row (‹ · title + lock · …) over the
                // strip, content scrolling under its scrim + edge fade.
                .workHeaderBand {
                    header(rows: rows, view: view)
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
        .toolbar {
            // EXP-1316: the title, ONE line, not tappable, the lock beside it.
            ToolbarItem(placement: .principal) {
                if let rows, let view = model?.view {
                    if rows.isAgent {
                        // NEW-2: the agent DEFINITION's icon (coloured) + name.
                        ThreadHeaderTitle(
                            title: rows.agentName.isEmpty ? view.title : rows.agentName,
                            isPrivate: view.isPrivate,
                            agentIcon: rows.agentIcon,
                            agentColor: rows.agentColor
                        )
                    } else {
                        ThreadHeaderTitle(title: view.title, isPrivate: view.isPrivate)
                    }
                }
            }
            // EXP-1307/1316: ONE `…` rides the header's action slot on every
            // tab while it has an entry — Close PR (EXP-1338: no GitHub
            // entry), in THE Menu's sheet.
            // EXP-1343: a session page's Stop rides the ONE header's
            // trailing slot while that session is live (always mounted,
            // EXP-942; draws nothing otherwise).
            ToolbarItem(placement: .topBarTrailing) { sessionStopButton }
            ToolbarItem(placement: .topBarTrailing) {
                if !guideMenuEntries.isEmpty {
                    Button { openGuideMenu() } label: {
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
        // EXP-1343: the session page's Stop confirm (`Prompts.StopRun`).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $sessionStopConfirm) {
                    GlassAlert(prompt: Prompts.StopRun.copy(), handlers: [
                        "stop": { sessionPageModel?.model.stop() },
                    ])
                }
        }
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
                shownIssueId: { if case let .issue(id) = page { id } else { nil } }()
            ) { row in
                pendingFoldPick = row
                foldTarget = nil
            }
        }
        .sheet(item: $suggestionOptions, onDismiss: runPendingSuggestionAction) { target in
            SuggestionOptionsSheet(title: target.title, devices: suggestionDevices) { action in
                pendingSuggestionAction = (target, action)
                suggestionOptions = nil
            }
            .task { await loadSuggestionDevices(teamId: rows?.root.teamId) }
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
        .onChange(of: rows?.guidePullRequest?.id, initial: true) { _, _ in syncPrChanges() }
        // EXP-1323: the live tail follows the root while it is not ended.
        .onChange(of: model?.view?.liveSessionId, initial: true) { _, _ in syncLiveTail() }
        // A newer agent message read here is read: re-stamp the root.
        .onChange(of: newestAgentMessageAt) { _, newest in
            if newest != nil { markRead() }
        }
    }

    // MARK: - Header

    /// The band under the nav row: a thread's strip (the pager); an agent
    /// channel has none.
    @ViewBuilder
    private func header(rows: ThreadRows, view: SessionThreadView) -> some View {
        if !rows.isAgent {
            ThreadStripTabs(
                tabs: view.tabs,
                // NEW-46: the ONE session's page reads as the Conversation tab.
                selection: view.mainSessionId.map { page == .session($0) } == true ? .conversation : page,
                issue: { rows.issue(id: $0) },
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
        guideSection = nil
        selectPage(.issue(issueId))
        return true
    }

    /// EXP-1343: Stop for the shown session page while it is live and this
    /// viewer may stop it (the session view's own rule).
    @ViewBuilder
    private var sessionStopButton: some View {
        if case let .session(shown) = page, let held = sessionPageModel, held.id == shown,
           held.model.canStop, !held.model.isOver {
            Button(role: .destructive) { sessionStopConfirm = true } label: {
                Text("Stop").fontWeight(.medium)
            }
            .tint(DesignTokens.Semantic.red)
            .accessibilityLabel("Stop the session")
            .accessibilityIdentifier("session-stop")
        }
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
        case .guide:
            guidePage(rows: rows)
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
                    .background(
                        GeometryReader { content in
                            Color.clear.preference(
                                key: FeedBottomOverflowKey.self,
                                value: content.frame(in: .named(Self.coordSpace)).maxY - geo.size.height
                            )
                        }
                    )
                }
                .coordinateSpace(name: Self.coordSpace)
                .scrollDismissesKeyboard(.interactively)
                .modifier(FollowPinTracker(
                    atBottom: $atBottom,
                    slack: AgentSessionLayout.followSlack,
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
                .safeAreaInset(edge: .bottom) {
                    // N-12: THE Dock over the Conversation's bottom edge: the
                    // tree's waiting cards, the live root's Queue, queued
                    // messages and banners, then THE composer (always).
                    let dockRows = conversationDockRows(rows: rows, view: view)
                    DockView(
                        rows: dockRows,
                        openId: $dockOpenId,
                        renderSheet: { row, close in conversationDockSheet(row, rows: rows, view: view, close: close) },
                        renderAction: { row in
                            row.id == "banner:closed" ? liveModel.map(SessionDock.reconnect) : nil
                        }
                    ) {
                        // EXP-1211: the agent's quick replies, directly above
                        // the composer.
                        SessionComposerReplies(sessionId: view.composer.sessionId)
                        ThreadComposer(
                            sessionId: view.composer.sessionId,
                            root: rows.root,
                            device: rows.rootDevice,
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
                    // AUTO-OPEN: a plan or question that arrives while the
                    // Conversation is in front opens its sheet.
                    .dockAutoOpen(dockRows) { dockOpenId = $0 }
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
                    .overlay(alignment: .top) {
                        ZStack {
                            if !atBottom {
                                JumpToBottomButton {
                                    atBottom = true
                                    withAnimation(motion.standard) {
                                        scroller.scrollTo(Self.bottomId, anchor: .bottom)
                                    }
                                }
                                .jumpToBottomTransition(motion)
                                .accessibilityIdentifier("thread-jump-to-bottom")
                            }
                        }
                        .alignmentGuide(.top) { $0[.bottom] + JumpToBottomButton.bottomGap }
                        .animation(motion.standard, value: atBottom)
                    }
                }
            }
        }
    }

    private static let coordSpace = "thread-conversation-scroll"

    /// N-12: the Conversation's DockCards: every card of the tree still
    /// waiting on this reader, then the live root's own (`SessionDock`).
    private func conversationDockRows(rows: ThreadRows, view: SessionThreadView) -> [Dock.Row] {
        guard canEdit else { return [] }
        let asks = AskDock.threadPendingAsks(view.rows, sessionName: AskDock.threadSessionNames(view.tabs))
        let live = liveModel.map { SessionDock.sources($0, session: rows.root) } ?? []
        return Dock.rows(asks.map(\.dockSource) + live)
    }

    /// A card's sheet: an ask = THE plan / question sheet, the root's Queue
    /// and queued messages their lists.
    private func conversationDockSheet(
        _ row: Dock.Row, rows: ThreadRows, view: SessionThreadView, close: @escaping () -> Void
    ) -> AnyView? {
        if let card = view.rows.first(where: { $0.id == row.id }) {
            return card.dockSheet(
                row,
                canAnswer: canEdit,
                busy: answeringId == card.id,
                markdownContext: askMarkdownContext(rows: rows),
                // The answered card leaves the Dock, and its sheet with it.
                onSubmit: submitAnswer,
                onClose: close
            )
        }
        return liveModel.flatMap { SessionDock.sheet(row, model: $0, close: close) }
    }

    /// The Conversation's markdown context (attachments through the
    /// instance, `#EXP-1` refs against the root's team) for the dock's cards.
    private func askMarkdownContext(rows: ThreadRows) -> AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            issueRefs: AgentIssueRefContext(
                teamId: rows.root.teamId,
                db: deps.db,
                onOpen: { [deps, accountId, issueOutlet] issueId in
                    openIssueInnermost(issueId, outlet: issueOutlet, deps: deps, accountId: accountId)
                }
            )
        )
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
            stack: guideStack(rows: rows),
            guideRaw: rows.root.guide,
            showsMerge: openPullRequest != nil
        ) {
            if let openPullRequest {
                WorkMergePill(
                    pullRequest: openPullRequest,
                    issueCount: rows.pullRequestIssueIds[openPullRequest.id]?.count ?? 0,
                    stackConfirm: rows.guideStackConfirm,
                    state: $mergeState,
                    // EXP-1307: the thread Guide's capsule (`testIds.guideMerge`).
                    identifier: SessionTree.ThreadCopy.guideMergeTestId
                )
                .id(openPullRequest.id)
            }
        }
    }

    /// EXP-1306: the Guide PR's stack block (desktop `stack_card`): the
    /// `Stack` band over THE `StackRail`, top first, the Guide PR's row in the
    /// current wash, down to the base branch. nil = no linear open stack.
    private func guideStack(rows: ThreadRows) -> AnyView? {
        guard let view = rows.guideStackView else { return nil }
        let members = view.rows.map {
            StackRailMember(id: $0.pullRequestId, identifier: $0.identifier, title: $0.title, current: $0.isCurrent)
        }
        let roots = Dictionary(
            view.rows.compactMap { row in row.rootSessionId.map { (row.pullRequestId, $0) } },
            uniquingKeysWith: { first, _ in first }
        )
        return AnyView(
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(SessionTree.ThreadCopy.stack) {
                    AppIcon(AppIcons.prStack, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                } trailing: {
                    EmptyView()
                }
                StackRail(
                    members: members,
                    baseBranch: view.baseBranch ?? "",
                    onOpen: { member in
                        // The thread on screen is already open.
                        guard let rootId = roots[member.id], rootId != self.rootId else { return }
                        pushRoute(.thread(accountId: accountId, threadId: rootId))
                    },
                    onMergeThrough: canEdit && !mergeState.merging
                        ? { member in mergeThrough = rows.mergeThroughConfirm(pullRequestId: member.id) }
                        : nil
                )
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(SessionTree.ThreadCopy.guideStackTestId)
            // THE stack confirm, on a zero-size node of its own (EXP-240).
            .background {
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .glassAlert(item: $mergeThrough) { confirm in
                        GlassAlert(
                            title: confirm.title,
                            message: confirm.body,
                            actions: [
                                GlassAlertAction(
                                    PullRequestStack.cancelLabel, role: .outline, isCancel: true, id: "cancel"
                                ) {},
                                GlassAlertAction(confirm.title, role: .primary, id: "merge-through") {
                                    merge(through: confirm)
                                },
                            ]
                        )
                    }
            }
        )
    }

    /// Merge through a rail member: `pullRequests.merge({stack: true})` on
    /// it. Shares the capsule's in-flight state, so both spin together.
    private func merge(through confirm: PullRequestStack.Confirm) {
        mergeState.merging = true
        let api = deps.pullRequestsApi
        let accountId = accountId
        Task {
            do {
                let result = try await api.merge(
                    accountId: accountId, pullRequestId: confirm.throughId, stack: true
                )
                if !result.merged, result.queued != true {
                    toaster.error(
                        FeedbackStates.Errors.title(verb: "merge", thing: "the stack"),
                        description: result.error
                    )
                }
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "merge", thing: "the stack"),
                    description: error.userFacingMessage
                )
            }
            mergeState.merging = false
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
            onHere: { rowId, itemId in startSuggestion(rowId, itemId, where: .here) },
            onDismiss: { rowId, itemId in dismissSuggestion(rowId, itemId) },
            onOptions: { rowId, item in
                UIApplication.endEditing()
                suggestionOptions = SuggestionOptionsTarget(rowId: rowId, itemId: item.id, title: item.title)
            },
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

    private func runPendingSuggestionAction() {
        guard let pending = pendingSuggestionAction else { return }
        pendingSuggestionAction = nil
        let target = pending.target
        switch pending.action {
        case let .startOn(deviceId):
            startSuggestion(target.rowId, target.itemId, where: .child, deviceId: deviceId)
        case let .startWith(model):
            startSuggestion(target.rowId, target.itemId, where: .child, model: model)
        case .file:
            runSuggestion(ThreadSuggestionActions.key(target.rowId, target.itemId), verb: "file", thing: "the issue") { api, accountId in
                try await api.fileSuggestion(accountId: accountId, messageId: target.rowId, itemId: target.itemId)
            }
        case .dismiss:
            dismissSuggestion(target.rowId, target.itemId)
        }
    }

    /// Start on…'s rows: the machines a start can reach right now with an
    /// agent to run (the composer's pool).
    private func loadSuggestionDevices(teamId: String?) async {
        let devices = await DeviceQueries.onlineStartTargets(
            db: deps.db, accountId: accountId, teamId: teamId, userId: deps.auth.userId
        )
        suggestionDevices = devices.filter(\.hasRunnableAgent).map {
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
        prChanges?.startObserving()
        syncIssueModels()
        syncLiveTail()
        markRead()
    }

    private func disappear() {
        UIApplication.endEditing()
        model?.stop()
        prChanges?.stopObserving()
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
        guideSection = nil
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

/// EXP-1323: the suggested task whose Options sheet is up.
struct SuggestionOptionsTarget: Identifiable {
    let rowId: String
    let itemId: String
    let title: String
    var id: String { ThreadSuggestionActions.key(rowId, itemId) }
}

/// What the strip's menu sheet is open for — an issue tab (`issueId`), the
/// "+" (nil) or the Guide's `…` (`guide`, EXP-1307).
struct ThreadMenuTarget: Identifiable {
    let issueId: String?
    var guide = false
    var id: String { guide ? "guide" : issueId ?? "add" }
}

/// EXP-1295: a Conversation's draft for THE session composer box
/// (`SessionComposerCard`, its controls fed by the root's live session model)
/// — `sessions.send({sessionId, text, attachmentIds})` into the root:
/// an idle session resumes, an ended thread continues server-side. Picks
/// upload to that session first, each once.
@MainActor
@Observable
final class ThreadComposerDraft: SessionComposerDraft {
    let sessionId: String
    let draftEditor = IssueEditorModel()
    /// NEW-7: Message an agent ›'s pick (composer-menu.json): the send goes
    /// into the caller's DM with that agent (`sessions.send({agentId})`),
    /// never into this thread; nil = a message here.
    var agentId: String?
    /// The agent a send went to — the host opens that DM and consumes it.
    var landedAgentId: String?
    /// The thread's team (an agent message's uploads use the team route).
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

    init(sessionId: String, accountId: String, sessionsApi: SessionsApi, attachmentsApi: AttachmentsApi) {
        self.sessionId = sessionId
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
        if let agentId {
            do {
                _ = try await sessionsApi.send(
                    accountId: accountId,
                    .agent(
                        agentId: agentId,
                        text: text.isEmpty ? nil : text,
                        attachmentIds: attachmentIds?.isEmpty == false ? attachmentIds : nil
                    )
                )
                self.agentId = nil
                landedAgentId = agentId
                return true
            } catch {
                sendError = error.userFacingMessage
                return false
            }
        }
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

/// EXP-1316: THE floating composer (ExpUI `FloatingComposer`) under a
/// Conversation for one session: the `Reply…` capsule floating over the
/// content, a tap expands it into THE session composer box (text, `+`
/// images/files, send, Stop while the tree works), blur folds it back when
/// nothing would be lost (the session view's rule).
///
/// The open card carries the session view's Controls for the ROOT (fed by
/// its live `AgentSessionModel`, the same relay source the session view
/// reads): the model pill (`/model <alias>`), the usage ring, the `/` keys
/// with THE `SlashCommandMenu`, Stop while busy — and no plan chip on a
/// dispatcher root. The collapsed capsule is unchanged.
struct ThreadComposer: View {
    let sessionId: String
    /// The root's synced row: the live model attaches to it, and its
    /// `dispatch` hides the plan chip.
    var root: SessionEntity?
    /// EXP-1334: the root's device (the Computer use row needs its cap).
    var device: DeviceEntity?
    let placeholder: String
    /// The tree's live sessions working a turn — Stop interrupts them.
    var busySessionIds: [String] = []
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
    // EXP-1334: the "+" (composer-menu.json) and its pickers.
    @State private var showPlusMenu = false
    @State private var plusPick: ComposerMenu.RowId?
    @State private var showEffortPicker = false
    @State private var showMcpPicker = false
    @State private var showAgentPicker = false
    @State private var linkSheetOpen = false
    @State private var mcpServers: [McpServerRow] = []
    /// NEW-7: the team's agents (Message an agent ›'s rows).
    @State private var teamAgents: [AgentDefinitionEntity] = []
    @Environment(\.pushRoute) private var pushRoute
    /// The picks sent mid-run, shown until the synced row agrees (the MCP
    /// list is server-only: what this composer last sent).
    @State private var picks = ThreadComposerPicks()

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
                        VStack(spacing: 6) {
                        if let agent = pickedAgent(draft) {
                            agentChip(agent, draft)
                        }
                        SessionComposerCard(
                            draft: draft,
                            placeholder: placeholder,
                            // N-12: THE composer row ×4: the device glyph,
                            // the model, Plan and the ring ride IN the card;
                            // NEW-7: a message to an agent runs on ITS device
                            // and login, so the root's picks hide.
                            controls: steerModel.flatMap { draft.agentId == nil ? controls($0) : nil },
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
                            fieldIdentifier: "thread-composer-field",
                            onPlus: openPlusMenu
                        )
                        }
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
                // NEW-7: an agent message opens the caller's DM with it.
                .onChange(of: draft.landedAgentId) { _, agentId in
                    guard let agentId else { return }
                    draft.landedAgentId = nil
                    pushRoute(.agentChannel(accountId: accountId, agentId: agentId))
                }
                .onChange(of: steerModel?.moveError) { _, message in
                    guard let message else { return }
                    toaster.error(SessionDeviceMove.failureToast, description: message)
                }
                .onChange(of: draft.draftEditor.isEditing) { _, editing in
                    if editing { focusPending = false }
                    guard expanded, !editing, !focusPending else { return }
                    guard !attach.busy, !draft.hasContent, slashConfirm == nil, !plusBusy else { return }
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
                    accountId: accountId,
                    sessionsApi: deps.sessionsApi,
                    attachmentsApi: deps.attachmentsApi
                )
            }
            draft?.teamId = root?.teamId
        }
        .sheet(isPresented: $showPlusMenu, onDismiss: promotePlusPick) {
            ThreadComposerPlusMenu(
                conditions: plusConditions,
                agent: agent,
                effortValue: effortValue,
                mcpValue: McpServers.pickedValue(picks.mcpServerIds ?? []),
                agentValue: draft.flatMap { pickedAgent($0)?.name },
                ultracode: Binding(get: { ultracodeOn }, set: { configure(.init(sessionId: sessionId, ultracode: $0)) }),
                computerUse: Binding(get: { computerUseOn }, set: { configure(.init(sessionId: sessionId, computerUse: $0)) }),
                addFileEnabled: draft.map { !$0.attachFull && !$0.steerSending } ?? false,
                rowsDisabled: ended
            ) { row in
                plusPick = row
                showPlusMenu = false
            }
        }
        .sheet(isPresented: $linkSheetOpen) {
            if let root {
                ThreadLinkIssueSheet(teamId: root.teamId, excluded: []) { issueId in
                    linkSheetOpen = false
                    link(issueId)
                }
            }
        }
        .background { effortPicker }
        .background { mcpPicker }
        .background { agentPicker }
        .task(id: root?.teamId) { await loadMcpServers() }
        .task(id: root?.teamId) { await loadAgents() }
        .task(id: root?.teamId) {
            slashPrompts.bind(accountId: accountId, teamId: root?.teamId, db: deps.db)
        }
        // A sync past the configure settles its pick: whenever the row reads
        // what a pick says, the pick goes (web `dropSettled`).
        .onChange(of: settledRow) { _, row in
            if let row { picks.settle(row) }
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

    private func controls(_ model: AgentSessionModel) -> SessionComposerFooter.Controls {
        let row = root ?? model.session
        return SessionComposerFooter.Controls(
            planModeActive: model.planModeActive,
            modelValue: model.composerModelValue(row: root),
            agent: model.catalogAgent,
            // EXP-877: a model switch is a PLAIN MESSAGE into the root — never
            // into an ENDED one (the pin is a label then, `ended`).
            onPickModel: { alias in
                guard let draft, row?.status != DomainContract.sessionStatusEnded else { return }
                Task { await draft.sendMessage("/model \(alias)", attachmentIds: nil) }
            },
            usageFraction: model.sessionUsage?.percent.map { Double($0) / 100 },
            usageSeverity: AgentUsagePresentation.severity(model.sessionUsage?.percent.map(Double.init)),
            onUsage: { showUsageSheet = true },
            showsPlan: row?.dispatch != true,
            ended: row?.status == DomainContract.sessionStatusEnded,
            planModeFallback: model.sessionConfig == nil && row?.planMode == true,
            // EXP-1252: the root's Device pin (move to another device).
            devicePin: model.devicePin,
            // N-12: the Plan switch rides the card's row (`row?.dispatch`
            // roots have none).
            plan: row?.dispatch == true ? nil : planBinding
        )
    }

    // MARK: - EXP-1334: the "+" and the mid-run picks (`sessions.configure`)

    private var agent: String { steerModel?.catalogAgent ?? root?.cli ?? "claude" }

    /// A sheet or picker of the "+" is up: the blur fold stands down.
    private var plusBusy: Bool {
        showPlusMenu || plusPick != nil || showEffortPicker || showMcpPicker || showAgentPicker || linkSheetOpen
    }

    /// An ENDED root takes no `sessions.configure` (the server refuses it):
    /// every option row stands down, like web's.
    private var ended: Bool { root?.status == DomainContract.sessionStatusEnded }

    private var plusConditions: ComposerMenu.Conditions {
        ComposerMenu.Conditions(
            subagentModel: false,
            ultracode: agent == "claude" && !ended,
            mcp: !mcpServers.isEmpty && !ended,
            computerUse: Self.advertises(device, cap: ComposerMenu.computerUseRunCap) && !ended,
            // NEW-7: Message an agent › while the team has agents, like the
            // launch composer (composer-menu.json).
            agents: !teamAgents.isEmpty
        )
    }

    /// The synced row's option fields a pick settles against; nil for a
    /// dispatcher root (its picks are work picks the row never echoes).
    private var settledRow: ThreadComposerPicks.Row? {
        guard let root, !root.dispatch else { return nil }
        return ThreadComposerPicks.Row(
            planMode: steerModel?.planModeActive ?? root.planMode,
            effort: root.effort,
            ultracode: root.ultracode,
            computerUse: root.computerUse
        )
    }

    static func advertises(_ device: DeviceEntity?, cap: String) -> Bool {
        guard let data = device?.caps?.data(using: .utf8),
              let caps = try? JSONSerialization.jsonObject(with: data) as? [String]
        else { return false }
        return caps.contains(cap)
    }

    private var planOn: Bool { picks.planMode ?? steerModel?.planModeActive ?? root?.planMode ?? false }
    private var ultracodeOn: Bool { picks.ultracode ?? root?.ultracode ?? false }
    private var computerUseOn: Bool { picks.computerUse ?? root?.computerUse ?? false }
    private var effort: String { LaunchVocabulary.effortChoice(picks.effort ?? root?.effort) }

    private var effortValue: String {
        effort == LaunchVocabulary.cliDefault ? "CLI default" : LaunchVocabulary.effortLabel(effort)
    }

    private var planBinding: Binding<Bool>? {
        guard LaunchVocabulary.supportsPlanMode(agent), !ended else { return nil }
        return Binding(get: { planOn }, set: { configure(.init(sessionId: sessionId, planMode: $0)) })
    }

    private func openPlusMenu() {
        UIApplication.endEditing()
        showPlusMenu = true
    }

    /// The row the "+" closed on opens its picker now the sheet is gone.
    private func promotePlusPick() {
        guard let pick = plusPick else { return }
        plusPick = nil
        switch pick {
        case .implementIssue: linkSheetOpen = true
        case .addFile:
            var transaction = Transaction()
            transaction.disablesAnimations = true
            withTransaction(transaction) { attach.menu = true }
        case .effort: showEffortPicker = true
        case .mcpServers: showMcpPicker = true
        case .messageAgent: showAgentPicker = true
        case .subagents, .ultracode, .computerUse: break
        }
    }

    /// Send one mid-run pick; it shows at once and rolls back on a refusal.
    private func configure(_ input: SessionConfigureInput) {
        let before = picks
        picks.apply(input)
        let api = deps.sessionsApi
        let accountId = accountId
        Task {
            do {
                try await api.configure(accountId: accountId, input)
                if let row = settledRow { picks.settle(row) }
            } catch {
                picks = before
                toaster.error(
                    FeedbackStates.Errors.title(verb: "change", thing: "the session"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    private var effortPicker: some View {
        let values = [LaunchVocabulary.cliDefault] + LaunchVocabulary.effortValues(for: agent)
        return GlassPicker(
            items: values.map { value in
                PickerItem(
                    value: value,
                    label: value == LaunchVocabulary.cliDefault ? "CLI default" : LaunchVocabulary.effortLabel(value)
                )
            },
            mode: .single,
            value: [effort],
            onChange: { picked in
                if let value = picked.first {
                    configure(.init(sessionId: sessionId, effort: LaunchVocabulary.wireEffort(value)))
                }
            },
            title: LaunchVocabulary.effortTitle(for: agent),
            open: $showEffortPicker,
            hideTrigger: true,
            sheetIdentifier: "thread-effort-picker",
            trigger: { EmptyView() }
        )
    }

    private var mcpPicker: some View {
        McpServerPicker(
            servers: mcpServers,
            value: picks.mcpServerIds ?? [],
            onChange: { configure(.init(sessionId: sessionId, mcpServerIds: $0)) },
            open: $showMcpPicker,
            hideTrigger: true,
            trigger: { EmptyView() }
        )
    }

    /// NEW-7: Message an agent › — THE Picker rows of the team's agents
    /// (icon in its colour + name, single pick); the pick is the subject.
    private var agentPicker: some View {
        GlassPicker(
            items: teamAgents.map { agent in
                PickerItem(
                    value: agent.id,
                    label: agent.name,
                    icon: BoardTypeDisplay.iconName(for: agent.icon) ?? SessionTree.ListCopy.agentDefaultIcon,
                    color: Color(hex: agent.color ?? SessionTree.ListCopy.agentDefaultColor)
                )
            },
            mode: .single,
            value: Set([draft?.agentId].compactMap { $0 }),
            onChange: { picked in
                guard let id = picked.first else { return }
                draft?.agentId = id
                expanded = true
            },
            title: ComposerMenu.allRows.first { $0.id == .messageAgent }?.label,
            open: $showAgentPicker,
            hideTrigger: true,
            sheetIdentifier: "thread-agent-picker",
            trigger: { EmptyView() }
        )
    }

    private func pickedAgent(_ draft: ThreadComposerDraft) -> AgentDefinitionEntity? {
        guard let id = draft.agentId else { return nil }
        return teamAgents.first { $0.id == id }
    }

    /// The ONE agent chip over the card (icon in its colour · name · ✕);
    /// the ✕ turns the composer back into this thread's.
    private func agentChip(_ agent: AgentDefinitionEntity, _ draft: ThreadComposerDraft) -> some View {
        HStack(spacing: 6) {
            AgentRowIcon(icon: agent.icon ?? "", color: agent.color ?? "")
            Text(agent.name)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
            Button {
                draft.agentId = nil
            } label: {
                AppIcon(AppIcons.uiClose, size: 10)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .frame(width: 24, height: 24)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Remove \(agent.name)")
            .accessibilityIdentifier("agent-composer-chip-agent-remove")
        }
        .padding(.leading, 10)
        .padding(.trailing, 2)
        .frame(height: 28)
        .background(GlassTokens.fillCard, in: Capsule())
        .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 4)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer-chip-agent")
    }

    private func loadAgents() async {
        guard let teamId = root?.teamId, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        teamAgents = (try? await pool.read { db in try SessionQueries.agents(db: db, teamId: teamId) }) ?? []
    }

    private func loadMcpServers() async {
        guard let teamId = root?.teamId, !ended else { return }
        let rows = (try? await McpServersApi(trpc: deps.trpc).list(accountId: accountId, teamId: teamId)) ?? []
        mcpServers = rows
        guard picks.mcpServerIds == nil else { return }
        // The session's own pick (server-only); a dispatcher's work picks
        // never read back, so its row seeds EMPTY like web's (no preselect:
        // the row shows what the server has, nothing it never got).
        picks.mcpServerIds = root?.dispatch == true
            ? []
            : ((try? await deps.sessionsApi.mcpPicks(accountId: accountId, sessionId: sessionId)) ?? [])
    }

    /// Implement issue = link it into this thread (the dispatcher wakes).
    private func link(_ issueId: String) {
        guard let rootId = root?.id else { return }
        let api = deps.sessionsApi
        let accountId = accountId
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
                for id in ids { try await api.stop(accountId: accountId, sessionId: id) }
            } catch {
                toaster.error(FeedbackStates.Errors.title(verb: "stop", thing: "the session"), description: error.userFacingMessage)
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
