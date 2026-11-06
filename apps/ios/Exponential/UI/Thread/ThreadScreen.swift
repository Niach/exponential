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
///   no caption, no device, no Show work. #EXP-1662: a thread holding a plan
///   adds the plan chip + page right after Conversation (`PlanPageBody`).
/// * Conversation = `ThreadConversationView` over `buildSessionThread`, with
///   THE session composer box under it (`ThreadComposer` →
///   `SessionComposerCard`), sending
///   `sessions.send({sessionId: root})`.
/// * An issue tab = the issue page EMBEDDED (EXP-1316): no own header, no
///   Threads row, no Start; the bottom bar keeps properties + Comment.
/// * S-3 (session-thread.json): the Guide tab is gone. The thread PR
///   (`guidePullRequest`) keeps (#EXP-1710) the merge train's STATUS pill
///   in the header's action slot, left of ONE `…` (EXP-1307: THE Menu's
///   sheet, `mergeMenuEntries` — Checks, Merge now, Hold | Resume
///   auto-merge, Close PR (EXP-1338) behind the close-pr.json confirm).
///
/// The "+" (left of Conversation, EXP-1334) opens THE Menu's sheet: Link an
/// issue (THE search list → `sessions.linkIssues`) · New issue (the New
/// issue page filing into this root); an issue chip's long press adds
/// Unlink (`sessions.unlinkIssue`). The strip never scrolls (#EXP-1433):
/// past one issue (or any session) ONE group tab opens
/// `ThreadStripGroupSheet`.
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
    /// #EXP-1662: the plan a way in named (`.plan(id)`), while the tab shows
    /// it; nil = `PlanPage.planTabTarget`.
    @State private var planPick: String?
    /// #FEED-191 (plan-page.json `kept`): the plan the tab last showed during
    /// THIS visit of the chat (its target, or a plan whose row was pressed);
    /// it lives as long as the screen does, so leaving the chat clears it and
    /// a tab switch inside it keeps it.
    @State private var planKept: String?
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
    /// #EXP-1433: the strip group's Menu sheet, and its pick, run once the
    /// sheet has dismissed.
    @State private var groupOpen = false
    @State private var pendingGroupPick: ThreadStripGroupSheet.Pick?
    /// #FEED-191 (needs-you.json): the card a way in named, focused once the
    /// thread has loaded.
    @State private var pendingCard: String?
    /// #EXP-1562: the waiting button's Menu sheet, its pick (run once the
    /// sheet has dismissed), the scroll it asks for and the lit card.
    @State private var waitingOpen = false
    @State private var pendingWaitingPick: String?
    @State private var waitingFocus: WaitingFocus?
    @State private var highlightId: String?
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
    /// #EXP-1710: the thread PR awaiting THE `merge-now-pr` alert.
    @State private var mergeNowTarget: MergeNowTarget?
    /// #EXP-1710: the status pill's tap opens the menu sheet ON its Checks.
    @State private var chatMenuChecks = false
    // EXP-1323: suggested tasks
    /// In-flight suggestion actions (`ThreadSuggestionActions.key`).
    @State private var suggestionPending: Set<String> = []
    /// EXP-1323: the live tail's claim on the root's socket owner
    /// (`attachSteerModel`, paired with `steerSessions.detach`).
    @State private var liveModel: AgentSessionModel?
    @State private var liveClaimId: String?
    /// #EXP-1545: the claims on the tree's Live children's socket owners
    /// (their per-session relay slots: THE Queue for the thread's ONE tasks
    /// card), keyed by session id; released once a child ends or the screen
    /// goes.
    @State private var childModels: [String: AgentSessionModel] = [:]
    /// #EXP-1623: when each child claim was taken: a child whose Queue is
    /// still unread `childProbeDeadline` later reads closed for the probe,
    /// so an unreachable host never pins it; the tick re-asks the probe then.
    @State private var childAttachedAt: [String: Date] = [:]
    @State private var childProbeTick = 0
    /// #EXP-1623: claims are taken only while the screen is on screen; a
    /// covered screen still observes its inputs.
    @State private var onScreen = false
    private static let childProbeDeadline = TimeInterval(Dock.probeDeadlineMs) / 1000
    /// EXP-1343: the shown session page's model (its embedded view's claim);
    /// nil off a session page.
    @State private var sessionPageModel: (id: String, model: AgentSessionModel)?
    /// EXP-1347: the innermost outlet for issue pills inside this thread.
    @State private var issueOutlet = ThreadIssueOutlet()
    /// #EXP-1662: a plan's ways in inside this thread land on its tab.
    @State private var planOutlet = ThreadPlanOutlet()
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
        if case let .plan(messageId) = initialPage {
            _page = State(initialValue: .conversation)
            _pendingPage = State(initialValue: .plan(nil))
            _planPick = State(initialValue: messageId)
        } else if case let .card(messageId) = initialPage {
            _page = State(initialValue: .conversation)
            _pendingCard = State(initialValue: messageId)
        } else if initialPage != .conversation, initialPage != .results {
            _page = State(initialValue: .conversation)
            _pendingPage = State(initialValue: initialPage)
        } else {
            _page = State(initialValue: initialPage)
        }
    }

    private var rows: ThreadRows? { model?.rows }

    /// #EXP-1662 (plan-page.json): the plan the tab shows: the one a way in
    /// named, else `planTabTarget` over the thread's plan cards; nil = no tab.
    /// An app editor's chat has no strip to hold it.
    private func shownPlan(_ view: SessionThreadView) -> ThreadPlanRow? {
        guard editor == nil else { return nil }
        let plans = view.rows.compactMap { row -> ThreadPlanRow? in
            if case let .plan(card) = row { return card }
            return nil
        }
        if let planPick, let picked = plans.first(where: { $0.id == planPick }) { return picked }
        let target = PlanPage.planTabTarget(plans.map { (id: $0.id, decision: $0.decision) }, kept: planKept)
        return plans.first { $0.id == target }
    }

    /// #FEED-191: the shown plan's id (`kept` follows it whenever it is set).
    private var shownPlanId: String? {
        guard let view = model?.view else { return nil }
        return shownPlan(view)?.id
    }

    /// chats.json AGENT CHAT: an agent's chat's pages: the Conversation, and
    /// the plan tab while it holds a plan.
    private func agentChatPages(_ view: SessionThreadView) -> [ThreadPage] {
        Chats.agentChatTabs(holdsPlan: shownPlan(view) != nil).contains("plan") ? [.conversation, .plan(nil)] : [.conversation]
    }

    /// EVERY page the strip reaches (the Conversation, the plan, each issue of
    /// the tree, each session under the root, Results): what a pill, a landing
    /// or a pick may open in place. An app editor's chat has the
    /// Conversation alone (its band replaces the strip); an agent's chat that
    /// and the plan it holds (chats.json AGENT CHAT). NEW-46: on a
    /// `transcript` thread with ONE session the Conversation tab IS that
    /// session's page (one page, not two); `bubbles` keeps the Conversation.
    private func reachable(_ rows: ThreadRows, _ view: SessionThreadView) -> [ThreadPage] {
        guard editor == nil else { return [.conversation] }
        if rows.isAgent { return agentChatPages(view) }
        let main = rows.chatMode == .transcript ? view.mainSessionId : nil
        let plan: [ThreadPage] = shownPlan(view) == nil ? [] : [.plan(nil)]
        return plan + view.tabs.flatMap { tab -> [ThreadPage] in
            switch tab {
            case .conversation: [main.map { .session($0) } ?? .conversation]
            case let .issue(issueId, _, nested): [.issue(issueId)] + nested.map { .issue($0.issueId) }
            case let .session(sessionId, _, _, nested):
                [.session(sessionId)] + nested.map { .session($0.sessionId) }
            case .add: []
            }
        } + [.results]
    }

    /// EXP-1343 / #EXP-1433: the pager's pages = Conversation · (#EXP-1662:
    /// the plan) · (chips: the one issue | group: the item opened from the group, only while it is
    /// the shown page) · Results; `+` and the group tab are never pages.
    private func pages(_ rows: ThreadRows, _ view: SessionThreadView) -> [ThreadPage] {
        guard editor == nil else { return [.conversation] }
        if rows.isAgent { return agentChatPages(view) }
        let conversation = resolved(.conversation)
        let group = ThreadStripGroup(tabs: view.tabs)
        let middle: [ThreadPage] = if let single = group.single {
            [.issue(single.issueId)]
        } else if page != conversation, group.holds(page) {
            [page]
        } else {
            []
        }
        let plan: [ThreadPage] = shownPlan(view) == nil ? [] : [.plan(nil)]
        return [conversation] + plan + middle + [.results]
    }

    private var currentPages: [ThreadPage] {
        guard let rows, let view = model?.view else { return [.conversation] }
        return pages(rows, view)
    }

    private var reachablePages: [ThreadPage] {
        guard let rows, let view = model?.view else { return [.conversation] }
        return reachable(rows, view)
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
                        // chats.json AGENT CHAT: only a page of its tabs, whatever a route named.
                        selection: Binding(
                            get: { rows.isAgent && !agentChatPages(view).contains(page) ? ThreadPage.conversation : page },
                            set: { switchPage($0) }
                        )
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
                .environment(\.threadPlanOutlet, planOutlet)
                .environment(\.waitingFocus, waitingFocus)
                // EXP-1316 (detail-chrome.json): THE detail chrome band on
                // every tab — #EXP-1688: THE chat header over the strip,
                // content scrolling under its scrim + edge fade.
                .workHeaderBand {
                    if let editor {
                        editor.band
                    } else {
                        chatHeader(rows: rows, view: view)
                        header(rows: rows, view: view)
                    }
                }
            } else if model?.missing == true {
                bareBand(
                    EmptyState(.threadNotFound)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .accessibilityIdentifier("thread-not-found")
                )
            } else {
                // EXP-1275 (D8): a first load is skeleton rows.
                bareBand(
                    SkeletonRows()
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                )
            }
        }
        // #EXP-1688: a chat draws THE chat header itself; only an app editor
        // keeps the navigation bar.
        .toolbar(editor == nil ? .hidden : .automatic, for: .navigationBar)
        .navigationBarTitleDisplayMode(.inline)
        // The header band draws the bar's material — the nav bar's own
        // background and divider stand down (EXP-1150).
        .toolbarBackground(.hidden, for: .navigationBar)
        // An app editor's `…`: a bare glyph too (no shared glass).
        .ownChromeToolbarItem(placement: .topBarTrailing, enabled: editor?.trailing != nil) {
            editor?.trailing
        }
        .toolbar {
            // EXP-1316: the title, ONE line, not tappable, the lock beside it.
            if let editor {
                ToolbarItem(placement: .principal) { editor.title }
            }
        }
        // Each presentation on its OWN node (EXP-240).
        .background {
            Color.clear
                .sheet(isPresented: $chatMenuOpen, onDismiss: runPendingChatPick) {
                    ChatMenuSheet(
                        title: chatMenuTitle,
                        entries: chatMenuEntries,
                        checksSummary: rows.flatMap { PrChecks.checksSummary($0.guideChecks) },
                        checkRows: rows.map { PrChecks.checksList($0.guideChecks, now: Date()) } ?? [],
                        startOnChecks: chatMenuChecks
                    ) { entry in
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
                        guideEntries: target.guide ? guideMenuEntries.filter { $0 != .checks } : nil
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
                    .bareBackButton()
            }
        }
        .task(id: rows?.root.teamId) { await resolveOwner() }
        // #EXP-1710: THE shared Merge now alert (Merge destructive, Cancel focused).
        .mergeNowConfirm(target: $mergeNowTarget) { target in
            mergeTrainActions.mergeNow(pullRequestId: target.pullRequestId)
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
        .sheet(isPresented: $groupOpen, onDismiss: runPendingGroupPick) {
            if let view = model?.view {
                ThreadStripGroupSheet(
                    group: ThreadStripGroup(tabs: view.tabs),
                    issue: { rows?.issue(id: $0) },
                    issueStatus: { rows?.issueStatus(id: $0) },
                    shown: page
                ) { pick in
                    pendingGroupPick = pick
                    groupOpen = false
                }
            }
        }
        .sheet(isPresented: $waitingOpen, onDismiss: runPendingWaitingPick) {
            ThreadWaitingSheet(chips: waitingChips.chips) { id in
                pendingWaitingPick = id
                waitingOpen = false
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
        .onChange(of: reachablePages) { _, pages in
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
        .onChange(of: threadLoaded, initial: true) { _, _ in focusPendingCard() }
        // #FEED-191: `kept` = the tab's plan whenever there is one, so a plan
        // decided during this visit keeps its chip until the person leaves.
        .onChange(of: shownPlanId, initial: true) { _, id in
            if let id { planKept = id }
        }
        .onChange(of: threadLanding.request) { _, request in
            guard let request, request.threadId == rootId else { return }
            land(on: request.page)
        }
        // EXP-1154: a new thread PR starts with no merge in flight.
        .onChange(of: rows?.guidePullRequest?.id) { _, _ in mergeState = WorkMergeState() }
        // EXP-1323: the live tail follows the root while it is not ended.
        .onChange(of: model?.view?.liveSessionId, initial: true) { _, _ in syncLiveTail() }
        // #EXP-1545/#EXP-1623: the ONE Live child's relay slots follow the
        // tree and what is known of each child's Queue (its own scope: the
        // probe reads the attached child's slots).
        .background {
            BodySlice {
                Color.clear.onChange(of: childProbeInput, initial: true) { _, _ in syncChildModels() }
            }
        }
        // A newer agent message read here is read: re-stamp the root.
        .onChange(of: newestAgentMessageAt) { _, newest in
            if newest != nil { markRead() }
        }
    }

    // MARK: - Header

    /// #EXP-1688: back · the lead disc · title over the subtitle · the
    /// trailing actions in web's order: the thread PR's Merge pill, then
    /// (NEW-69, voice-call.json) an agent chat's telephone, then ONE `…`
    /// (EXP-1307/1316: Close PR in THE Menu's sheet).
    private func chatHeader(rows: ThreadRows, view: SessionThreadView) -> some View {
        ChatHeader {
            ChatHeaderTitle(
                title: rows.isAgent && !rows.agentName.isEmpty ? rows.agentName : view.title,
                subtitle: ChatHeaderTitle.subtitle(rows: rows),
                subtitleTone: ChatHeaderTitle.tone(rows: rows),
                isPrivate: view.isPrivate,
                agentIcon: rows.isAgent ? rows.agentIcon : nil,
                agentColor: rows.isAgent ? rows.agentColor : nil,
                assistant: rows.isAgent && rows.app == nil && rows.agent?.isAssistant == true,
                cli: rows.root.cli,
                prStates: rows.pullRequests.map(\.state),
                mark: SessionTree.threadRowMarkState(
                    needsYou: rows.sessions.contains { $0.needsInput && $0.status != DomainContract.sessionStatusEnded },
                    busy: rows.sessions.contains { $0.busy && $0.status != DomainContract.sessionStatusEnded }
                )
            )
        } trailing: {
            headerMergePill
            if rows.root.isAgent, rows.root.appId == nil, let agentId = rows.root.agentId {
                AgentCallButton(
                    agentId: agentId, agentName: rows.agentName,
                    agentIcon: rows.agent?.isAssistant == true ? nil : BoardTypeDisplay.iconName(for: rows.agent?.icon),
                    agentColor: rows.agent?.color
                )
                .buttonStyle(.plain)
            }
            if !chatMenuEntries.isEmpty {
                ChatHeaderGhostButton(
                    icon: AppIcons.uiMore, label: "More", identifier: SessionTree.ThreadCopy.guideMenuTestId
                ) {
                    openChatMenu()
                }
            }
        }
    }

    /// #EXP-1688: no chat yet (first load, not found): the header's back alone,
    /// the bar being hidden; an app editor keeps its toolbar.
    @ViewBuilder
    private func bareBand<Content: View>(_ content: Content) -> some View {
        if editor == nil {
            content.workHeaderBand { ChatHeader() }
        } else {
            content
        }
    }

    /// The band under the chat header: THE strip (the pager) on a thread; an
    /// agent's chat has none but while it holds a plan (chats.json AGENT CHAT):
    /// Conversation and the plan tab.
    @ViewBuilder
    private func header(rows: ThreadRows, view: SessionThreadView) -> some View {
        if !rows.isAgent || agentChatPages(view).count > 1 {
            threadStrip(rows: rows, view: view)
        }
    }

    private func threadStrip(rows: ThreadRows, view: SessionThreadView) -> some View {
        ThreadStripTabs(
            tabs: view.tabs,
            // NEW-46: the ONE session's page reads as the Conversation tab.
            selection: view.mainSessionId.map { page == .session($0) } == true ? .conversation : page,
            issue: { rows.issue(id: $0) },
            issueStatus: { rows.issueStatus(id: $0) },
            isFresh: { model?.isFresh(issueId: $0) ?? false },
            onSelect: { next in
                pendingPage = nil
                // #EXP-1662: the chip = the tab's own plan, never an older pick.
                if case .plan = next { planPick = nil }
                selectPage(next)
            },
            onIssueMenu: { issueId in openMenu(issueId: issueId) },
            onAdd: canEdit && !rows.isAgent ? { openMenu(issueId: nil) } : nil,
            onOpenGroup: {
                UIApplication.endEditing()
                groupOpen = true
            },
            showsResults: !rows.isAgent,
            waiting: rows.isAgent ? nil : waitingChips,
            onOpenWaiting: {
                UIApplication.endEditing()
                waitingOpen = true
            },
            planWaiting: shownPlan(view).map { $0.decision == nil },
            planGlyph: PlanPage.planTabGlyph(shownPlan(view)?.decision)
        )
    }

    /// #EXP-1562 (waiting-cards.json CHIPS): the Conversation's WAITING
    /// cards (`threadPendingAsks`); a phone = ONE button. #EXP-1662: a plan's
    /// chip is the plan tab; these are the questions.
    private var waitingChips: WaitingCards.Chips {
        let rows = model?.view?.rows ?? []
        return WaitingCards.chips(AskDock.threadPendingAsks(rows, sessionName: { _ in nil }).filter { $0.kind != .plan }, phone: true)
    }

    /// #EXP-1562: a waiting row's pick shows the Conversation (if another
    /// page is up), scrolls that card to centre and lights it.
    private func runPendingWaitingPick() {
        guard let id = pendingWaitingPick else { return }
        pendingWaitingPick = nil
        pendingPage = nil
        if page != defaultPage { selectPage(.conversation) }
        waitingFocus = WaitingFocus(id: id, at: Date())
    }

    private var threadLoaded: Bool { model?.view != nil }

    /// #FEED-191 (needs-you.json): a Needs you row's `card`: the Conversation,
    /// scrolled to the card, once the thread is there (the waiting pick's).
    private func focusPendingCard() {
        guard let id = pendingCard, model?.view != nil else { return }
        pendingCard = nil
        pendingPage = nil
        if page != defaultPage { selectPage(.conversation) }
        waitingFocus = WaitingFocus(id: id, at: Date())
    }

    /// #EXP-1433: the group sheet's pick opens that issue / session IN
    /// PLACE (its page).
    private func runPendingGroupPick() {
        guard let pick = pendingGroupPick else { return }
        pendingGroupPick = nil
        pendingPage = nil
        switch pick {
        case let .issue(issueId): selectPage(.issue(issueId))
        case let .session(sessionId): openSession(sessionId)
        }
    }

    /// EXP-1343: a session of this thread's strip shows as its page; one
    /// the strip does not hold (another root's) pushes its own screen.
    /// #EXP-1464: a child of this thread opens in place; any other session
    /// by inbox-route.json `sessionTarget` (`AppRoute.openingSession`).
    private func openSession(_ sessionId: String) {
        if reachablePages.contains(.session(sessionId)) {
            selectPage(.session(sessionId))
        } else {
            let route = AppRoute.openingSession(sessionId, accountId: accountId, deps: deps)
            if case let .thread(_, threadId, _) = route, threadId == (rows?.root.id ?? rootId) {
                // This very thread (a page the strip does not hold).
                pushRoute(.agentSession(accountId: accountId, sessionId: sessionId))
            } else {
                pushRoute(route)
            }
        }
    }

    /// EXP-1347: an issue pill inside this thread whose issue is one of its
    /// tabs selects that tab; false = not ours (the caller pushes).
    private func openIssueTab(_ issueId: String) -> Bool {
        guard reachablePages.contains(.issue(issueId)) else { return false }
        pendingPage = nil
        selectPage(.issue(issueId))
        return true
    }

    /// #EXP-1662: a plan of this thread shows in its tab; false = not ours
    /// (the caller pushes that plan's thread).
    private func openPlanTab(_ messageId: String) -> Bool {
        guard editor == nil, let view = model?.view,
              view.rows.contains(where: { row in
                  if case let .plan(card) = row { return card.id == messageId }
                  return false
              })
        else { return false }
        pendingPage = nil
        selectPage(.plan(messageId))
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
        case .plan:
            if let plan = shownPlan(view) {
                PlanPageBody(
                    card: plan,
                    pictureSizes: TreeMessageCard.parse(
                        rows.messages.first { $0.id == plan.id }?.card
                    )?.pictures ?? [],
                    canAnswer: canEdit,
                    busy: answeringId == plan.id,
                    onSubmit: submitAnswer
                )
                .id(plan.id)
            }
        case .results:
            ThreadResultsPage(
                rootId: rows.root.id,
                rows: rows,
                canEdit: canEdit,
                mergeState: $mergeState
            )
        case .card:
            EmptyView()
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
                            onOpenXref: { xref in pushRoute(rows.xrefRoute(xref, accountId: accountId)) },
                            onSubmit: submitAnswer,
                            onUnqueue: unqueue,
                            viewerId: deps.auth.userId,
                            answeringId: answeringId,
                            canAnswer: canEdit,
                            highlightId: highlightId,
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
                // #EXP-1562: a waiting pick scrolls here and lights the card.
                .modifier(WaitingFocusScroll(
                    focus: waitingFocus, proxy: scroller, highlight: $highlightId, onScroll: { atBottom = false }
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
                    // #EXP-1623: THE Dock is its own observation scope. Built
                    // inline, every live model's slots it reads (the root's
                    // and the Live child's) were tracked by this screen's
                    // body, so their frames re-rendered the whole thread.
                    BodySlice {
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
                    }
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
    /// waiting plan or question is never a DockCard; it stays
    /// inline in the transcript, answerable.
    private func conversationDockRows(rows: ThreadRows, view: SessionThreadView) -> [Dock.Row] {
        guard canEdit else { return [] }
        let live = liveModel.map { SessionDock.sources($0, session: rows.root, tasks: false) } ?? []
        // #EXP-1545: the ONE tasks card, the root's else a Live child's.
        let tasks = threadQueue(rows: rows, view: view).map { pick -> Dock.Source in
            .tasks(pick.model.taskQueue(now: Date()).map(Dock.QueueSummary.init), sessionName: pick.sessionName)
        }
        // #EXP-1383: the agent chat's "Instructions changed" banner.
        let hint = SessionDock.agentHint(root: rows.root, agent: rows.agent).map { [$0] } ?? []
        return Dock.rows(Dock.suggestionSources(view.rows) + live + (tasks.map { [$0] } ?? []) + hint)
    }

    /// #EXP-1545 (dock.json THREAD QUEUE): whose Queue the thread's Dock
    /// shows (`Dock.threadDockQueue`) over the root's and every Live child's
    /// relay slots (LOCAL FIRST: the store seeds a fresh model with the
    /// last-known ones); its model draws the card and its sheet's Trace.
    private func threadQueue(
        rows: ThreadRows, view: SessionThreadView
    ) -> (model: AgentSessionModel, sessionName: String?)? {
        let names = Dictionary(
            ThreadStripGroup(tabs: view.tabs).sessions.map { ($0.sessionId, $0.name) },
            uniquingKeysWith: { first, _ in first }
        )
        let now = Date()
        var models: [String: AgentSessionModel] = childModels
        if let liveModel, let liveClaimId { models[liveClaimId] = liveModel }
        let candidates = models.compactMap { id, model -> Dock.ThreadQueueCandidate? in
            guard let session = rows.session(id: id) else { return nil }
            return Dock.ThreadQueueCandidate(
                sessionId: id,
                name: names[id] ?? session.title ?? "",
                root: session.parentId == nil,
                ended: session.status == DomainContract.sessionStatusEnded || model.isOver,
                activeAt: SessionTree.ms(session.lastTurnAt) ?? SessionTree.ms(session.startedAt),
                open: !(model.taskQueue(now: now)?.todo.items.isEmpty ?? true)
            )
        }
        guard let pick = Dock.threadDockQueue(candidates), let model = models[pick.sessionId] else { return nil }
        return (model, pick.sessionName)
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
        // #EXP-1545: the tasks card opens ITS session's Trace.
        if row.kind == .tasks, let pick = threadQueue(rows: rows, view: view) {
            return SessionDock.sheet(row, model: pick.model, close: close)
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

    // MARK: - The thread PR's status pill

    /// #EXP-1710 (merge-train.json STATUS PILL): THE status pill over the
    /// thread PR (open, or merged: `Merged`); its tap opens the `…` sheet ON
    /// the Checks submenu; `Needs approval` is the only action.
    @ViewBuilder
    private var headerMergePill: some View {
        if editor == nil, canEdit, let rows,
           let pullRequest = rows.guidePullRequest, pullRequest.state != DomainContract.prStateClosed {
            WorkMergePill(
                pullRequest: pullRequest,
                issueCount: rows.pullRequestIssueIds[pullRequest.id]?.count ?? 0,
                stackConfirm: rows.guideStackConfirm,
                checks: rows.guideChecks,
                batchSize: rows.guideBatchSize,
                onChecks: { openChecks() },
                state: $mergeState,
                identifier: MergeTrain.TestIds.statusPill,
                style: .toolbar
            )
            .id(pullRequest.id)
        }
    }

    private func openChecks() {
        UIApplication.endEditing()
        chatMenuChecks = true
        chatMenuOpen = true
    }

    private var mergeTrainActions: MergeTrainActions {
        MergeTrainActions(deps: deps, accountId: accountId, toaster: toaster)
    }

    /// #EXP-1710: Merge now's alert over the thread PR (`titleStack` on an
    /// open-stack member).
    private func askMergeNow() {
        guard let rows, let pullRequest = rows.guidePullRequest, pullRequest.isOpen else { return }
        mergeNowTarget = MergeNowTarget(
            pullRequestId: pullRequest.id, number: pullRequest.number, stack: rows.guideStackConfirm != nil
        )
    }

    private func holdGuide(_ hold: Bool) {
        guard let pullRequest = rows?.guidePullRequest, pullRequest.isOpen else { return }
        mergeTrainActions.hold(pullRequestId: pullRequest.id, hold: hold)
    }

    // MARK: - Cards (answered in place)

    private func submitAnswer(_ card: ThreadCardAnswer) {
        guard answeringId == nil else { return }
        answeringId = card.messageId
        let api = deps.sessionsApi
        let accountId = accountId
        // plan-page.json (#FEED-191): a decision taken ON the Plan tab returns
        // to the Conversation, where the agent works on; one from there stays.
        var returnsToConversation = false
        if case .plan = page, card.decision != nil { returnsToConversation = true }
        Task {
            do {
                if card.dismissed {
                    try await api.dismissCard(accountId: accountId, messageId: card.messageId)
                } else {
                    try await api.answer(
                        accountId: accountId, messageId: card.messageId, answer: card.answer, decision: card.decision,
                        fresh: card.fresh, feedback: card.feedback, answers: card.answers, surfaceData: card.surfaceData
                    )
                }
                if returnsToConversation, case .plan = page { selectPage(.conversation) }
            } catch {
                toaster.error(
                    card.dismissed
                        ? FeedbackStates.Errors.title(verb: "dismiss", thing: "the card")
                        : FeedbackStates.Errors.title(
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

    /// #EXP-1545: the tree's not-ended children (any depth).
    private var liveChildren: [SessionEntity] {
        (rows?.sessions ?? [])
            .filter { $0.parentId != nil && $0.status != DomainContract.sessionStatusEnded }
            .sorted { $0.id < $1.id }
    }

    /// #EXP-1623: each Live child as the probe sees it: when it was last
    /// active and what is known of its Queue without opening its stream.
    private var childProbes: [Dock.ThreadQueueProbe] {
        liveChildren.map { session in
            let activeAt = SessionTree.ms(session.lastTurnAt) ?? SessionTree.ms(session.startedAt)
            return Dock.ThreadQueueProbe(
                sessionId: session.id, activeAt: activeAt,
                state: childQueueState(session.id, activeAt: activeAt)
            )
        }
    }

    private struct ChildProbeInput: Equatable {
        let rootOpen: Bool
        let probes: [Dock.ThreadQueueProbe]
        let tick: Int
    }

    private var childProbeInput: ChildProbeInput {
        ChildProbeInput(rootOpen: liveModel.map(Self.queueOpen) ?? false, probes: childProbes, tick: childProbeTick)
    }

    private func childQueueState(_ id: String, activeAt: Double?) -> Dock.ChildQueueState {
        let store = deps.steerSessions
        let model = childModels[id] ?? store.peek(accountId: accountId, sessionId: id)
        if let model, Self.queueOpen(model) { return .open }
        if store.queueReadClosed(accountId: accountId, sessionId: id, activeAt: activeAt) { return .closed }
        if let model {
            // No device feeds it (`starting`): nothing to read there yet, so
            // the next child in line is; it is read again once it turns.
            if model.phase == .starting { return .closed }
            if case .closed = model.phase { return .closed }
            if model.replayed || model.isOver { return .closed }
            if let at = childAttachedAt[id], Date().timeIntervalSince(at) >= Self.childProbeDeadline {
                return .closed
            }
            return .unknown
        }
        return .unknown
    }

    private static func queueOpen(_ model: AgentSessionModel) -> Bool {
        !(model.taskQueue(now: Date())?.todo.items.isEmpty ?? true)
    }

    /// #EXP-1623 (was #EXP-1545's one claim per Live child: a stream and a
    /// full join replay for every child on open): ONE claim, on the child
    /// `Dock.threadQueueProbe` names; a child read closed is released and
    /// remembered as closed until it is active again.
    private func syncChildModels() {
        guard onScreen else { return }
        let input = childProbeInput
        let probes = input.probes
        let wanted = Dock.threadQueueProbe(rootOpen: input.rootOpen, children: probes)
        for probe in probes where probe.state == .closed {
            deps.steerSessions.noteQueueClosed(accountId: accountId, sessionId: probe.sessionId, activeAt: probe.activeAt)
        }
        for id in childModels.keys where id != wanted {
            childModels[id] = nil
            childAttachedAt[id] = nil
            deps.steerSessions.detach(accountId: accountId, sessionId: id)
        }
        if let wanted, childModels[wanted] == nil, let session = rows?.session(id: wanted) {
            childModels[wanted] = deps.attachSteerModel(accountId: accountId, session: session)
            childAttachedAt[wanted] = Date()
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(Self.childProbeDeadline))
                childProbeTick &+= 1
            }
        }
    }

    private func releaseChildModels() {
        for id in childModels.keys {
            deps.steerSessions.detach(accountId: accountId, sessionId: id)
        }
        childModels = [:]
        childAttachedAt = [:]
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
        return own + guideMenuEntries.map { entry -> ChatMenuSheet.Entry in
            switch entry {
            case .checks: .checks
            case .mergeNow: .mergeNow
            case .hold: .hold
            case .resume: .resume
            case .closePr: .closePr
            }
        }
    }

    private func openChatMenu() {
        UIApplication.endEditing()
        chatMenuOpen = true
    }

    private func runPendingChatPick() {
        chatMenuChecks = false
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
        case .checks:
            // The sheet pushes its own Checks page.
            break
        case .mergeNow: askMergeNow()
        case .hold: holdGuide(true)
        case .resume: holdGuide(false)
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

    /// #EXP-1710: the Guide header's `…` entries (merge-train.json `menu`,
    /// `mergeMenuEntries` ×4) over the Guide's PR; empty = no trigger.
    private var guideMenuEntries: [MergeTrain.MenuEntry] {
        let pullRequest = rows?.guidePullRequest
        return MergeTrain.mergeMenuEntries(
            state: pullRequest?.state,
            member: canEdit,
            owner: isOwner,
            checks: rows?.guideChecks.count ?? 0,
            mergeState: pullRequest?.mergeState
        )
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
        case .mergeNow: askMergeNow()
        case .hold: holdGuide(true)
        case .resume: holdGuide(false)
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
        planOutlet.setHandler { messageId in openPlanTab(messageId) }
        if model == nil {
            model = ThreadScreenModel(rootId: rootId, accountId: accountId, db: deps.db)
        }
        model?.start()
        syncIssueModels()
        syncLiveTail()
        onScreen = true
        syncChildModels()
        markRead()
    }

    private func disappear() {
        UIApplication.endEditing()
        model?.stop()
        stopIssueModel()
        releaseLiveTail()
        onScreen = false
        releaseChildModels()
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
        if case let .card(messageId) = request {
            menuTarget = nil
            pendingCard = messageId
            focusPendingCard()
            return
        }
        var target = resolved(request)
        if case let .plan(messageId) = target {
            planPick = messageId
            target = .plan(nil)
        }
        menuTarget = nil
        if reachablePages.contains(target) {
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
        var next = resolved(request)
        if case let .plan(messageId) = next {
            if let messageId { planPick = messageId }
            next = .plan(nil)
        } else if case .plan = page, next != page {
            planPick = nil
        }
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
                Group {
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
                // Web's TypeaheadMenu `placement="above"`: the `/` menu floats
                // 4pt over the card, over what stands above it (the quick
                // replies), and takes no room; nothing moves.
                .overlay(alignment: .top) {
                    if open, slashMenuVisible(draft) {
                        Color.clear.frame(height: 0).overlay(alignment: .bottom) {
                            SlashCommandMenu(
                                candidates: slashMatches(draft),
                                highlighted: slashHighlight
                            ) { candidate in
                                applySlashCandidate(candidate, draft)
                            }
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.horizontal, 16)
                            .padding(.bottom, 4)
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
                .onChange(of: steerModel?.moveError) { _, message in
                    guard let message else { return }
                    toaster.error(SessionDeviceMove.failureToast, description: message)
                }
                .onChange(of: steerModel?.switchError) { _, message in
                    guard let message else { return }
                    toaster.error(FeedbackStates.Errors.title(verb: "switch", thing: "the account"), description: message)
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
            teamId: draftTeamId,
            // #EXP-1434: the "+" Prompts page = the `/` menu's prompts; a
            // pick inserts the body at the caret.
            prompts: slashPrompts.prompts,
            onPickPrompt: { prompt in
                Task {
                    guard let body = await TeamSlashPrompts.loadBody(prompt, accountId: accountId, deps: deps),
                          let editor = draft?.draftEditor
                    else { return }
                    editor.insertTextAtCaret(body)
                    focus()
                }
            },
            // #EXP-1535: the "+" Account page = the usage wall's session
            // account switch on the root (its live model's rules).
            account: steerModel?.composerAccountMenu
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
