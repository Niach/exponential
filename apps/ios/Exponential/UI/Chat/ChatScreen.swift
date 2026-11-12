import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// #EXP-1732 (session-screen.json): THE SESSION SCREEN on a phone — ONE
/// screen for every kind (`SessionScreen.kind`): a chat (a root session), a
/// team agent's or the Assistant's conversation and (VAPP-110, `editor`) an
/// app's editor session. Phones are NARROW: the layout is `chat`, `tab` (the
/// tab's body replaces the chat) or `item` (an opened issue, session, plan
/// or settings as a page pushed over the tab, its `‹` back = the pushed
/// page's).
///
/// * HEADER (THE chat header, `ChatHeader`): back · the lead disc (an
///   agent's disc, the Assistant's brand mark, an app's glyph + version
///   pill) · the title over ONE subtitle line (`SessionScreen.subtitle`:
///   status · device · an agent's first enabled trigger), then the trailing
///   slot: the chat PR's merge status pill (#EXP-1710), an agent's
///   telephone, THE gear (agents, the Assistant, apps) and the `…`.
/// * Under it on the same band THE segmented control (`SessionScreenTabs`):
///   Chat · Issues · Sessions · Results (an app: Chat · Preview · Data ·
///   Versions · Sessions · Results), each count muted after its label.
/// * Chat = the ROOT's own session view (`AgentSessionView`, embedded):
///   its raw-work feed (#EXP-1736's transcript with the agent's traces), its
///   cards inline, THE Dock and THE composer posting to the root — the same
///   view a Sessions row pushes for a child. A plan opens the plan page as
///   an opened item.
/// * Issues = the tree's issue rows led by the `+` (THE Menu's sheet: Link
///   an issue → `sessions.linkIssues` · New issue filing into this root); a
///   row opens the issue page EMBEDDED (EXP-1316), a long press adds Unlink
///   (`sessions.unlinkIssue`). Sessions = THE tree rows; a row opens that
///   session's raw work + composer. Results = `ChatResultsPage`.
/// * The chat PR (`guidePullRequest`) keeps the merge train's STATUS pill
///   in the header, left of ONE `…` (EXP-1307: THE Menu's sheet,
///   `mergeMenuEntries` — Checks, Merge now, Hold | Resume auto-merge, Close
///   PR (EXP-1338) behind the close-pr.json confirm).
///
/// Failed actions toast (`Couldn't link the issue`, feedback-states).
///
/// Opening the screen stamps the viewer's read of the root
/// (`sessions.markRead`).
struct ChatScreen: View {
    let rootId: String
    let initialPage: ChatPage
    /// VAPP-110 / #EXP-1732: an app EDITOR's session — its title leads the
    /// header, its tab binding drives the tabs, its bodies fill Preview,
    /// Data and Versions and its settings page is the gear's item.
    var editor: ChatEditorSlot?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.motion) private var motion
    @Environment(\.toaster) private var toaster
    @Environment(TeamState.self) private var teamState
    @Environment(\.chatLanding) private var chatLanding
    /// The active tab (an app editor's lives in the editor: `editor.tab`).
    @State private var ownTab: SessionScreen.Tab
    /// The item opened over the tab (layout `item`: a pushed page).
    @State private var opened: ChatOpenedItem?
    /// #FEED-191 (plan-page.json `kept`): the plan the plan page last showed
    /// during THIS visit of the chat (its target, or a plan whose row was
    /// pressed); it lives as long as the screen does.
    @State private var planKept: String?
    /// A requested issue / session / plan that has not synced into the tree
    /// yet (a just-filed issue): the screen shows its tab and opens the item
    /// the moment it arrives; a tab tap drops the request.
    @State private var pendingPage: ChatPage?
    @State private var model: ChatScreenModel?
    /// The issue page models of the issues OPENED so far (EXP-1288): only
    /// the open item's model observes (`observingIssueId`).
    @State private var issueVMs: [String: IssueDetailViewModel] = [:]
    @State private var observingIssueId: String?
    /// THE Menu's sheet: the Issues tab's "+" (issueId nil) or a row's long
    /// press.
    @State private var menuTarget: ChatIssueMenuTarget?
    /// The sheet's pick, run once the sheet has dismissed.
    @State private var pendingMenuAction: (action: ChatIssueMenuSheet.Action, issueId: String?)?
    @State private var linkSheetOpen = false
    /// #FEED-191 (needs-you.json): the card a way in named, focused once the
    /// chat has loaded.
    @State private var pendingCard: String?
    /// #EXP-1562: the scroll a waiting card asks for (the root's session
    /// view scrolls to it and lights it).
    @State private var waitingFocus: WaitingFocus?
    /// #EXP-1785: the waiting button's Menu sheet, its pick (run once the
    /// sheet has dismissed).
    @State private var waitingOpen = false
    @State private var pendingWaitingPick: String?
    /// The question / plan answer in flight.
    @State private var answeringId: String?
    // The chat PR (`guidePullRequest`)
    @State private var mergeState = WorkMergeState()
    /// EXP-1307: the chat PR awaiting the Close PR confirm.
    @State private var closePrTarget: PullRequestEntity?
    /// #EXP-1710: the chat PR awaiting THE `merge-now-pr` alert.
    @State private var mergeNowTarget: MergeNowTarget?
    /// #EXP-1710: the status pill's tap opens the menu sheet ON its Checks.
    @State private var chatMenuChecks = false
    /// EXP-1347: the innermost outlet for issue pills inside this chat.
    @State private var issueOutlet = ChatIssueOutlet()
    /// #EXP-1662: a plan's ways in inside this chat open its plan page.
    @State private var planOutlet = ChatPlanOutlet()
    /// EXP-1373 (chats.json MENUS): the header `…` = the chat's menu sheet
    /// (S-3: Delete, the Guide's Close PR; an agent chat's Compact · New
    /// conversation · Agent settings), its pick
    /// run once the sheet has dismissed.
    @State private var chatMenuOpen = false
    @State private var pendingChatPick: ChatMenuSheet.Entry?
    @State private var deleteTarget: String?
    @State private var isOwner = false
    @Environment(\.dismiss) private var dismiss

    init(rootId: String, initialPage: ChatPage = .conversation, editor: ChatEditorSlot? = nil) {
        self.rootId = rootId
        self.initialPage = initialPage
        self.editor = editor
        switch initialPage {
        case .results:
            _ownTab = State(initialValue: .results)
        case let .card(messageId):
            _ownTab = State(initialValue: .chat)
            _pendingCard = State(initialValue: messageId)
        case .issue:
            // An item may not be in the tree yet: hold it (`pendingPage`).
            _ownTab = State(initialValue: .issues)
            _pendingPage = State(initialValue: initialPage)
        case .session:
            _ownTab = State(initialValue: .sessions)
            _pendingPage = State(initialValue: initialPage)
        case .plan:
            _ownTab = State(initialValue: .chat)
            _pendingPage = State(initialValue: initialPage)
        case .conversation:
            _ownTab = State(initialValue: .chat)
        }
    }

    private var rows: ChatRows? { model?.rows }

    /// The active tab: the editor's (an app) or the screen's own.
    private var tab: SessionScreen.Tab { editor?.tab.wrappedValue ?? ownTab }

    private func setTab(_ next: SessionScreen.Tab) {
        if let editor { editor.tab.wrappedValue = next } else { ownTab = next }
    }

    // MARK: - The screen (session-screen.json)

    private func kind(_ rows: ChatRows) -> SessionScreen.Kind {
        SessionScreen.kind(agentId: rows.root.agentId, appId: rows.root.appId, assistant: rows.agent?.isAssistant == true)
    }

    /// #EXP-1732: the tree's issues and sessions (the RAW tree, never the
    /// view's visible chips; an agent's chat has none).
    private func items(_ view: SessionChatView) -> ChatTreeItems {
        guard let rows else { return ChatTreeItems(tabs: view.tabs) }
        return ChatTreeItems(rows: rows, now: Date())
    }

    /// THE screen value: the tabs with their counts, the active tab, the
    /// gear and the layout (phones are never wide).
    private func screen(_ rows: ChatRows, _ view: SessionChatView) -> SessionScreen.Screen {
        let tree = items(view)
        return SessionScreen.screen(
            kind: kind(rows),
            requested: tab,
            open: opened?.open,
            counts: SessionScreen.Counts(
                issues: tree.issues.count,
                sessions: tree.sessions.count,
                results: ChatResultsPage.results(rootId: rows.root.id, rows: rows, now: Date()).count
            )
        )
    }

    /// The plans of the Conversation, in order.
    private func plans(_ view: SessionChatView) -> [ChatPlanRow] {
        view.rows.compactMap { row -> ChatPlanRow? in
            if case let .plan(card) = row { return card }
            return nil
        }
    }

    /// #EXP-1662 (plan-page.json): the plan an opened plan item shows: the
    /// one it names, else `planTabTarget` over the chat's plan cards.
    private func plan(_ messageId: String?, _ view: SessionChatView) -> ChatPlanRow? {
        let cards = plans(view)
        if let messageId { return cards.first { $0.id == messageId } }
        let target = PlanPage.planTabTarget(cards.map { (id: $0.id, decision: $0.decision) }, kept: planKept)
        return cards.first { $0.id == target }
    }

    /// #FEED-191: the open plan's id (`kept` follows it whenever it is set).
    private var shownPlanId: String? {
        guard case let .plan(messageId) = opened, let view = model?.view else { return nil }
        return plan(messageId, view)?.id
    }

    /// EVERY item the screen opens in place: each issue of the tree, each
    /// session under the root, each plan of the Conversation (a landing or a
    /// pill may name one).
    private var reachable: Set<String> {
        guard let view = model?.view else { return [] }
        let tree = items(view)
        return Set(tree.issues.map { "i:\($0.issueId)" } + tree.sessions.map { "s:\($0.sessionId)" }
            + plans(view).map { "p:\($0.id)" })
    }

    private func reaches(_ item: ChatOpenedItem) -> Bool {
        switch item {
        case let .issue(id): reachable.contains("i:\(id)")
        case let .session(id): reachable.contains("s:\(id)")
        case let .plan(id): id.map { reachable.contains("p:\($0)") } ?? true
        case .settings: true
        }
    }

    var body: some View {
        ZStack {
            AppBackground()
            if let model, let rows = model.rows, let view = model.view {
                let shown = screen(rows, view)
                ZStack {
                    // The Chat stays mounted under every other tab (its draft
                    // and scroll survive a tab switch).
                    chatPage(rows: rows)
                        .opacity(shown.active == .chat ? 1 : 0)
                        .allowsHitTesting(shown.active == .chat)
                        .accessibilityHidden(shown.active != .chat)
                    if shown.active != .chat {
                        tabBody(shown.active, rows: rows, view: view)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .environment(\.chatIssueOutlet, issueOutlet)
                .environment(\.chatPlanOutlet, planOutlet)
                .environment(\.waitingFocus, waitingFocus)
                // EXP-1316 (detail-chrome.json): THE detail chrome band on
                // every tab — #EXP-1688: THE chat header over THE tabs,
                // content scrolling under its scrim + edge fade.
                .workHeaderBand {
                    chatHeader(rows: rows, view: view, gear: shown.gear)
                    SessionScreenTabs(tabs: shown.tabs, selection: shown.active) { next in
                        selectTab(next)
                    }
                }
            } else if model?.missing == true {
                EmptyState(.chatNotFound)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .accessibilityIdentifier("chat-not-found")
                    .workHeaderBand { ChatHeader() }
            } else {
                // EXP-1275 (D8): a first load is skeleton rows.
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .workHeaderBand { ChatHeader() }
            }
        }
        // #EXP-1688: THE session screen draws THE chat header itself, for
        // every kind (an app editor's too).
        .toolbar(.hidden, for: .navigationBar)
        // #EXP-1732: an opened item = a page pushed over the tab.
        .navigationDestination(item: $opened) { item in
            openedPage(item)
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
                .sheet(isPresented: $waitingOpen, onDismiss: runPendingWaitingPick) {
                    ChatWaitingSheet(chips: waitingChips.chips) { id in
                        pendingWaitingPick = id
                        waitingOpen = false
                    }
                }
        }
        .background {
            Color.clear
                .sheet(item: $menuTarget, onDismiss: runPendingMenuAction) { target in
                    ChatIssueMenuSheet(
                        identifier: target.issueId.flatMap { rows?.issue(id: $0)?.identifier }
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
        .sheet(isPresented: $linkSheetOpen) {
            if let rows {
                ChatLinkIssueSheet(
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
        .accessibilityIdentifier("chat-screen")
        .onAppear(perform: appear)
        .onDisappear(perform: disappear)
        // A held item that arrived opens; an open item that vanished (an
        // unlinked issue) closes.
        .onChange(of: reachable) { _, _ in
            if let pending = pendingPage {
                land(on: pending)
            } else if let opened, !reaches(opened) {
                self.opened = nil
            }
            syncIssueModels()
        }
        .onChange(of: opened) { _, _ in syncIssueModels() }
        // The navigator popped back to THIS page for a filed issue: land on
        // the tab it names.
        .onChange(of: chatLoaded, initial: true) { _, _ in focusPendingCard() }
        // #FEED-191: `kept` = the open plan whenever there is one, so a plan
        // decided during this visit stays reachable until the person leaves.
        .onChange(of: shownPlanId, initial: true) { _, id in
            if let id { planKept = id }
        }
        .onChange(of: chatLanding.request) { _, request in
            guard let request, request.chatId == rootId else { return }
            land(on: request.page)
        }
        // EXP-1154: a new chat PR starts with no merge in flight.
        .onChange(of: rows?.guidePullRequest?.id) { _, _ in mergeState = WorkMergeState() }
        // A newer agent message read here is read: re-stamp the root.
        .onChange(of: newestAgentMessageAt) { _, newest in
            if newest != nil { markRead() }
        }
    }

    // MARK: - Header

    /// #EXP-1688 / #EXP-1732: back · the lead disc · title over the
    /// subtitle · the trailing actions in web's order: the chat PR's Merge
    /// pill, (NEW-69, voice-call.json) an agent chat's telephone, THE gear,
    /// then ONE `…` (EXP-1307/1316: Close PR in THE Menu's sheet; an app's =
    /// the editor's Export · Delete).
    private func chatHeader(rows: ChatRows, view: SessionChatView, gear: Bool) -> some View {
        ChatHeader {
            if let editor {
                // #EXP-1729: the editor's title block carries its own
                // subtitle (apps.json `editorHeader`: why nothing starts).
                editor.title(ChatHeaderTitle.status(rows: rows))
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
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
                    mark: SessionTree.chatRowMarkState(
                        needsYou: rows.sessions.contains { $0.needsInput && $0.status != DomainContract.sessionStatusEnded },
                        busy: rows.sessions.contains { $0.busy && $0.status != DomainContract.sessionStatusEnded }
                    )
                )
            }
        } trailing: {
            // #EXP-1785: THE waiting button leads the trailing slot (not an
            // app editor's).
            if editor == nil, waitingChips.layout == .button {
                ChatWaitingButton(count: waitingChips.count) {
                    UIApplication.endEditing()
                    waitingOpen = true
                }
            }
            headerMergePill
            if rows.root.isAgent, rows.root.appId == nil, let agentId = rows.root.agentId {
                AgentCallButton(
                    agentId: agentId, agentName: rows.agentName,
                    agentIcon: rows.agent?.isAssistant == true ? nil : BoardTypeDisplay.iconName(for: rows.agent?.icon),
                    agentColor: rows.agent?.color
                )
                .buttonStyle(.plain)
            }
            if gear {
                SessionScreenGear { openItem(.settings) }
            }
            if let trailing = editor?.trailing {
                trailing
            } else if !chatMenuEntries.isEmpty {
                ChatHeaderGhostButton(
                    icon: AppIcons.uiMore, label: "More", identifier: SessionTree.ChatCopy.guideMenuTestId
                ) {
                    openChatMenu()
                }
            }
        }
    }

    private var chatLoaded: Bool { model?.view != nil }

    /// #EXP-1785 (waiting-cards.json): THE TREE's waiting plans and
    /// questions (ask-dock `chatPendingAsks` over the chat view's rows, the
    /// children's included), in Conversation order.
    private var waitingCards: [(card: AskDock.Pending, sessionId: String)] {
        guard let view = model?.view else { return [] }
        let sessions = Dictionary(view.rows.compactMap { row -> (String, String)? in
            switch row {
            case let .plan(card): (card.id, card.sessionId)
            case let .question(card): (card.id, card.sessionId)
            default: nil
            }
        }, uniquingKeysWith: { first, _ in first })
        return AskDock.chatPendingAsks(view.rows, sessionName: { _ in nil }).map {
            ($0, sessions[$0.id] ?? rootId)
        }
    }

    private var waitingChips: WaitingCards.Chips {
        WaitingCards.chips(waitingCards.map(\.card), phone: true)
    }

    /// #EXP-1785 `waitingTarget`: a plan = its plan page; the root's
    /// question = the Chat tab scrolled to its card + the ring; a child's =
    /// that session opened in place, scrolled to its card + the ring.
    private func runPendingWaitingPick() {
        guard let id = pendingWaitingPick else { return }
        pendingWaitingPick = nil
        guard let picked = waitingCards.first(where: { $0.card.id == id }) else { return }
        let target = WaitingCards.waitingTarget(
            card: WaitingCards.TargetCard(kind: picked.card.kind, sessionId: picked.sessionId),
            rootId: rows?.root.id ?? rootId
        )
        pendingPage = nil
        switch target.open {
        case .plan:
            setTab(.chat)
            openItem(.plan(id))
        case .card:
            opened = nil
            setTab(.chat)
            waitingFocus = WaitingFocus(id: id, at: Date())
        case .session:
            waitingFocus = WaitingFocus(id: id, at: Date())
            openSession(target.sessionId)
        }
    }

    /// #FEED-191 (needs-you.json): a Needs you row's `card`: the Chat,
    /// scrolled to the card, once the chat is there.
    private func focusPendingCard() {
        guard let id = pendingCard, model?.view != nil else { return }
        pendingCard = nil
        pendingPage = nil
        opened = nil
        setTab(.chat)
        waitingFocus = WaitingFocus(id: id, at: Date())
    }

    /// EXP-1343 / #EXP-1732: a session under this root opens in place (the
    /// Sessions tab, its page pushed over it); any other session by
    /// inbox-route.json `sessionTarget` (`AppRoute.openingSession`).
    private func openSession(_ sessionId: String) {
        if reaches(.session(sessionId)) {
            pendingPage = nil
            setTab(.sessions)
            openItem(.session(sessionId))
        } else {
            let route = AppRoute.openingSession(sessionId, accountId: accountId, deps: deps)
            if case let .chat(_, chatId, _) = route, chatId == (rows?.root.id ?? rootId) {
                // This very chat (a session the tree does not hold).
                pushRoute(.agentSession(accountId: accountId, sessionId: sessionId))
            } else {
                pushRoute(route)
            }
        }
    }

    /// EXP-1347: an issue pill inside this chat whose issue is one of its
    /// tree's opens it in place; false = not ours (the caller pushes).
    private func openIssueTab(_ issueId: String) -> Bool {
        guard reaches(.issue(issueId)) else { return false }
        pendingPage = nil
        setTab(.issues)
        openItem(.issue(issueId))
        return true
    }

    /// #EXP-1662: a plan of this chat opens its plan page; false = not ours
    /// (the caller pushes that plan's chat).
    private func openPlanTab(_ messageId: String) -> Bool {
        guard reaches(.plan(messageId)), model?.view != nil else { return false }
        pendingPage = nil
        openItem(.plan(messageId))
        return true
    }

    /// A member may link and file here (`resolveTeamAccess`: every member
    /// comments and creates issues). #EXP-1714: the viewer's own Assistant
    /// conversation stays editable while its team (the personal one) never
    /// syncs.
    private var canEdit: Bool {
        guard let rows else { return false }
        if AgentSettings.isMyAssistant(rows.agent, me: deps.auth.userId) { return true }
        return teamState.teams.contains { $0.id == rows.root.teamId }
    }

    // MARK: - Tabs and opened items

    /// A non-chat tab's body (layout `tab`: it replaces the chat).
    @ViewBuilder
    private func tabBody(_ tab: SessionScreen.Tab, rows: ChatRows, view: SessionChatView) -> some View {
        switch tab {
        case .chat:
            EmptyView()
        case .issues:
            ChatIssuesTab(
                issues: items(view).issues,
                issue: { rows.issue(id: $0) },
                issueStatus: { rows.issueStatus(id: $0) },
                onAdd: canEdit ? { openMenu(issueId: nil) } : nil,
                onOpen: { issueId in openItem(.issue(issueId)) },
                onMenu: canEdit ? { issueId in openMenu(issueId: issueId) } : nil
            )
        case .sessions:
            ChatSessionsTab(sessions: items(view).sessions) { sessionId in
                openItem(.session(sessionId))
            }
        case .results:
            ChatResultsPage(
                rootId: rows.root.id,
                rows: rows,
                canEdit: canEdit,
                mergeState: $mergeState
            )
        case .preview, .data, .versions:
            if let pane = editor?.pane(tab) {
                pane
            }
        }
    }

    /// An opened item's page (layout `item`, pushed over the tab): an issue
    /// EMBEDDED (EXP-1316), a session's raw work + composer, a plan page, or
    /// the gear's settings (an agent's / the Assistant's page, an app's).
    @ViewBuilder
    private func openedPage(_ item: ChatOpenedItem) -> some View {
        switch item {
        case let .issue(issueId):
            SessionScreenItemPage(title: rows?.issue(id: issueId)?.identifier ?? "") {
                issuePage(issueId)
            }
            .environment(\.chatIssueOutlet, issueOutlet)
            .environment(\.chatPlanOutlet, planOutlet)
        case let .session(sessionId):
            SessionScreenItemPage(title: rows.flatMap { $0.session(id: sessionId) }.map(sessionName) ?? "") {
                if let rows { sessionPage(sessionId, rows: rows) }
            }
            // #EXP-1785: a child's waiting question scrolls its page too.
            .environment(\.waitingFocus, waitingFocus)
            .environment(\.chatIssueOutlet, issueOutlet)
            .environment(\.chatPlanOutlet, planOutlet)
        case let .plan(messageId):
            SessionScreenItemPage(title: PlanPage.Copy.title) {
                if let rows, let view = model?.view, let plan = plan(messageId, view) {
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
            }
        case .settings:
            if let editor {
                editor.settings
                    .environment(\.detailColumnRoot, false)
            } else if let rows, let agentId = rows.root.agentId {
                AgentSettingsPage(teamId: rows.root.teamId, agentId: agentId, isOwner: isOwner)
                    .environment(\.accountId, accountId)
                    .environment(\.detailColumnRoot, false)
                    .bareBackButton()
            }
        }
    }

    /// A tree row's name for the pushed session page's header.
    private func sessionName(_ session: SessionEntity) -> String {
        model?.view.flatMap { view in items(view).sessions.first { $0.sessionId == session.id }?.name }
            ?? session.title ?? SessionTitle.Copy.untitled
    }

    /// #EXP-1732 (the owner's correction): THE Chat tab = the ROOT's own
    /// session view, every kind — its raw-work feed (#EXP-1736's transcript
    /// with the agent's traces), its cards inline, THE Dock and THE composer
    /// posting to the root (an app editor's: the "+" Device row too). The
    /// same view a Sessions row pushes for a child.
    private func chatPage(rows: ChatRows) -> some View {
        AgentSessionView(
            accountId: accountId,
            session: rows.root,
            embedded: true,
            editorDevice: editor?.device,
            editorMove: editor?.appMove,
            editorPicks: editor?.picks
        )
        .id(rows.root.id)
    }

    @ViewBuilder
    private func issuePage(_ issueId: String) -> some View {
        if let vm = issueVMs[issueId] {
            ChatIssueItem(vm: vm, hostChatId: rootId)
        } else {
            SkeletonRows()
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
    }

    /// EXP-1343: a session item = that session's raw work + its composer
    /// (the session view's body, no header of its own) under the pushed
    /// page's header. Only the OPEN item holds a live model; a page left
    /// unmounts its view (the claim goes with it).
    @ViewBuilder
    private func sessionPage(_ sessionId: String, rows: ChatRows) -> some View {
        if opened == .session(sessionId), let session = rows.session(id: sessionId) {
            AgentSessionView(
                accountId: accountId,
                session: session,
                embedded: true
            )
            .id(sessionId)
        } else {
            SkeletonRows(rows: 3, rowHeight: 28, dividers: false)
                .transcriptColumn()
                .padding(.top, 8)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
    }

    // MARK: - The chat PR's status pill

    /// #EXP-1710 (merge-train.json STATUS PILL): THE status pill over the
    /// chat PR for ANY viewer (a closed PR = none); its tap opens the
    /// check rows directly as the menu; `Needs approval` (members) is the
    /// only action.
    @ViewBuilder
    private var headerMergePill: some View {
        if editor == nil, let rows, let pullRequest = rows.guidePullRequest,
           MergeTrain.pillState(prState: pullRequest.state, mergeState: pullRequest.mergeState) != nil {
            WorkMergePill(
                pullRequest: pullRequest,
                issueCount: rows.pullRequestIssueIds[pullRequest.id]?.count ?? 0,
                stackConfirm: rows.guideStackConfirm,
                checks: rows.guideChecks,
                batchSize: rows.guideBatchSize,
                canApprove: canEdit,
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

    /// #EXP-1710: Merge now's alert over the chat PR (`titleStack` on an
    /// open-stack member).
    private func askMergeNow() {
        guard let target = rows?.guideMergeNowTarget else { return }
        mergeNowTarget = target
    }

    private func holdGuide(_ hold: Bool) {
        guard let pullRequest = rows?.guidePullRequest, pullRequest.isOpen else { return }
        mergeTrainActions.hold(pullRequestId: pullRequest.id, hold: hold)
    }

    // MARK: - Cards (answered in place)

    private func submitAnswer(_ card: ChatCardAnswer) {
        guard answeringId == nil else { return }
        answeringId = card.messageId
        let api = deps.sessionsApi
        let accountId = accountId
        // plan-page.json (#FEED-191): a decision taken ON the plan page returns
        // to the Chat, where the agent works on; one from there stays.
        var returnsToConversation = false
        if case .plan = opened, card.decision != nil { returnsToConversation = true }
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
                if returnsToConversation, case .plan = opened { opened = nil }
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

    /// The header `…`: an agent chat's `agentMenuEntries`; a chat's
    /// `chatMenuEntries`, then the Guide's Close PR (EXP-1307).
    private var chatMenuEntries: [ChatMenuSheet.Entry] {
        guard let rows else { return [] }
        if rows.isAgent {
            guard rows.root.appId == nil else { return [] }
            return Chats.agentMenuEntries(busy: rows.root.busy, isOwner: isOwner).map { .agent($0) }
        }
        let own = Chats.chatMenuEntries(rootStatus: rows.root.status, canSteer: canSteer, canDelete: canDelete).map { ChatMenuSheet.Entry.chat($0) }
        return own + prMenuEntries.map { entry -> ChatMenuSheet.Entry in
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
        case .chat(.delete):
            deleteTarget = rows.root.id
        case let .agent(entry):
            switch entry {
            case .compact, .newConversation:
                if let text = Chats.agentMenuSends[entry] { reply(text, sessionId: rows.root.id) }
            case .settings:
                openItem(.settings)
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
        menuTarget = ChatIssueMenuTarget(issueId: issueId)
    }

    /// #EXP-1710: the Guide header's `…` entries (merge-train.json `menu`,
    /// `mergeMenuEntries` ×4) over the Guide's PR; empty = no trigger.
    private var prMenuEntries: [MergeTrain.MenuEntry] {
        let pullRequest = rows?.guidePullRequest
        return MergeTrain.mergeMenuEntries(
            state: pullRequest?.state,
            member: canEdit,
            owner: isOwner,
            checks: rows?.guideChecks.count ?? 0,
            mergeState: pullRequest?.mergeState
        )
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
        if opened == .issue(issueId) { opened = nil }
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
            chatId: rootId
        ))
    }

    // MARK: - Lifecycle

    private func appear() {
        issueOutlet.setHandler { issueId in openIssueTab(issueId) }
        planOutlet.setHandler { messageId in openPlanTab(messageId) }
        if model == nil {
            model = ChatScreenModel(rootId: rootId, accountId: accountId, db: deps.db)
        }
        model?.start()
        syncIssueModels()
        markRead()
    }

    private func disappear() {
        UIApplication.endEditing()
        model?.stop()
        stopIssueModel()
    }

    /// EXP-1288: ONE issue model observes at a time — the open issue
    /// item's, created the first time it is opened and kept (stopped) once
    /// it closes. An unlinked issue's model is dropped.
    private func syncIssueModels() {
        guard let rows else { return }
        let ids = Set(rows.issues.map(\.id))
        for id in issueVMs.keys where !ids.contains(id) {
            if observingIssueId == id { stopIssueModel() }
            issueVMs[id] = nil
        }
        guard case let .issue(id) = opened, ids.contains(id) else {
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

    /// A way in (`ChatPage`): its tab now, its item now or once it reaches
    /// the tree (`pendingPage`).
    private func land(on request: ChatPage) {
        menuTarget = nil
        switch request {
        case let .card(messageId):
            pendingCard = messageId
            focusPendingCard()
        case .conversation:
            pendingPage = nil
            opened = nil
            setTab(.chat)
        case .results:
            pendingPage = nil
            opened = nil
            setTab(.results)
        case let .issue(id):
            setTab(.issues)
            hold(request, item: .issue(id))
        case let .session(id):
            setTab(.sessions)
            hold(request, item: .session(id))
        case let .plan(messageId):
            setTab(.chat)
            hold(request, item: .plan(messageId))
        }
    }

    /// Open `item` if the tree holds it, else hold the request.
    private func hold(_ request: ChatPage, item: ChatOpenedItem) {
        guard model?.view != nil, reaches(item) else {
            pendingPage = request
            return
        }
        pendingPage = nil
        openItem(item)
    }

    /// A tab tap: the tab, nothing opened over it, a held request dropped.
    private func selectTab(_ next: SessionScreen.Tab) {
        pendingPage = nil
        UIApplication.endEditing()
        withAnimation(motion.standard) {
            opened = nil
            setTab(next)
        }
    }

    /// Open an item over the active tab (a pushed page on a phone). An
    /// issue's model exists before the page is pushed (the page observes it).
    private func openItem(_ item: ChatOpenedItem) {
        UIApplication.endEditing()
        if case let .issue(id) = item, issueVMs[id] == nil {
            issueVMs[id] = makeIssueModel(id)
        }
        opened = item
    }
}

/// EXP-1316: an opened issue = the issue page EMBEDDED (no own header, no
/// Chats row, no Start; the bottom bar keeps properties + Comment), its own
/// observation scope over the issue's model.
private struct ChatIssueItem: View {
    let vm: IssueDetailViewModel
    let hostChatId: String

    var body: some View {
        if let issue = vm.issue {
            IssueFaceView(
                vm: vm,
                issue: issue,
                barTrailing: .hidden,
                onStartCoding: {},
                hostChatId: hostChatId,
                embedded: true
            )
        } else {
            SkeletonRows()
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
    }
}

/// #EXP-1732 (session-screen.json `open`): the item opened over a tab.
enum ChatOpenedItem: Hashable {
    case issue(String)
    case session(String)
    /// nil = `PlanPage.planTabTarget`.
    case plan(String?)
    case settings

    var open: SessionScreen.Open {
        switch self {
        case .issue: .issue
        case .session: .session
        case .plan: .plan
        case .settings: .settings
        }
    }
}

/// What THE Menu's issue sheet is open for — an issue row (`issueId`) or
/// the Issues tab's "+" (nil).
struct ChatIssueMenuTarget: Identifiable {
    let issueId: String?
    var id: String { issueId ?? "add" }
}

/// EXP-1295: a Conversation's draft for THE session composer box
/// (`SessionComposerCard`, its controls fed by the root's live session model)
/// — `sessions.send({sessionId, text, attachmentIds})` into the root:
/// an idle session resumes, an ended chat continues server-side. Picks
/// upload to that session first, each once. S-3: an agent WITHOUT a
/// conversation yet (`agentId`) sends `sessions.send({agentId, text,
/// attachmentIds})` instead, which opens the viewer's conversation (its
/// uploads ride the team route: there is no session yet).
@MainActor
@Observable
final class ChatComposerDraft: SessionComposerDraft {
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
    /// #EXP-1715: the Assistant's Teams pick every send carries
    /// (`scopeTeamIds`); nil = not the viewer's Assistant root (never sent).
    var scope: TeamsScope?
    /// #EXP-1793: the "+" MCP servers pick of the agent's conversation this
    /// first send opens (seeded from the agent's `mcpServerIds`).
    var mcpPick: AgentChatMcpPick?

    @ObservationIgnored private let accountId: String
    @ObservationIgnored private let sessionsApi: SessionsApi
    @ObservationIgnored private let attachmentsApi: AttachmentsApi

    /// #FEED-204: an ended editor root's next send, to another device:
    /// `sessions.send({appId, deviceId})` revives the root on that device
    /// (`AgentSessionModel.sendAppMove`, the editor slot's `appMove`).
    struct AppMove: Equatable {
        let appId: String
        let deviceId: String
        var cli: String?
        var account: String?
        var model: String?
    }

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
                    .agent(
                        agentId: agentId, text: text, attachmentIds: attachmentIds, scope: scope,
                        mcpServerIds: mcpPick?.outgoing
                    )
                )
                mcpPick?.sent()
            } else {
                try await sessionsApi.send(
                    accountId: accountId,
                    .message(sessionId: sessionId, text: text, attachmentIds: attachmentIds, scope: scope)
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
struct ChatComposer: View {
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
    /// #EXP-1715: the conversation's agent DEFINITION (the root's, or
    /// `agentId`'s before the first send): the viewer's own Assistant gets
    /// the Teams pick.
    var agentRow: AgentDefinitionEntity?
    /// The capsule was tapped open (the host keeps its feed pinned).
    var onExpand: () -> Void = {}

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.motion) private var motion
    @State private var draft: ChatComposerDraft?
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
    /// #EXP-1714: optional, a specimen may mount the composer without one.
    @Environment(TeamState.self) private var teamState: TeamState?
    /// #EXP-1715: the Teams pick (nil = every team), seeded from the root's
    /// synced `scope_team_ids` and re-seeded when another client moves it.
    @State private var teamsPicked: [String]?
    /// #EXP-1793: the agent's team's MCP servers (`mcpServers.list`) for the
    /// "+" MCP servers page of a first message to an agent.
    @State private var mcpServers: [McpServerRow] = []

    /// #EXP-1793 (composer-menu.json `mcpPick`): an agent's first message
    /// (no conversation yet) picks the servers its conversation opens with,
    /// seeded from the agent's `mcpServerIds`.
    private var mcpMenu: SessionComposerFooter.McpMenu? {
        guard let draft, let pick = draft.mcpPick, draft.agentId != nil else { return nil }
        return SessionComposerFooter.McpMenu(
            servers: mcpServers,
            picked: pick.picked,
            onChange: { draft.mcpPick?.picked = $0 }
        )
    }

    /// #EXP-1793: an agent's first message starts from the agent's servers
    /// (an untouched pick follows a change of them).
    private func seedMcpPick() {
        guard let draft, let agentId = draft.agentId else { return }
        let seed = agentRow?.mcpServerIds ?? []
        if draft.mcpPick == nil {
            draft.mcpPick = AgentChatMcpPick(agentId: agentId, seed: seed)
        } else {
            draft.mcpPick?.reseed(seed)
        }
    }

    /// #EXP-1715 (composer-menu.json `teamsPick`): the Teams pick while this
    /// composer writes into the viewer's OWN Assistant conversation ROOT (or
    /// opens it); nil anywhere else, a child included (the server refuses a
    /// scope there).
    private var teamsPick: ComposerTeamsBinding? {
        guard AgentSettings.isMyAssistant(agentRow, me: deps.auth.userId),
              sessionId.isEmpty || sessionId == root?.id
        else { return nil }
        return ComposerTeamsBinding(
            teams: ComposerTeamsBinding.teams(teamState?.teams ?? []),
            picked: teamsPicked,
            onChange: { teamsPicked = $0 }
        )
    }

    /// The upload / send team: the root's (the agent's before the first send).
    private var draftTeamId: String? { root?.teamId ?? teamId }
    /// #EXP-1714 (`lookupTeamId`): what the `/` prompts and the "+" issue
    /// search read: the draft's team while synced, else the active team
    /// (the global Assistant's personal team never syncs).
    private var lookupTeamId: String? {
        AgentSettings.lookupTeamId(
            draftTeamId, teamIds: teamState?.teams.map(\.id) ?? [], activeTeamId: teamState?.activeTeam?.id
        )
    }

    var body: some View {
        Group {
            if let draft, draft.sessionId == sessionId {
                let open = composerOpen
                Group {
                    FloatingComposer(
                        placeholder: placeholder,
                        open: open,
                        collapsedLabel: placeholder,
                        collapsedIdentifier: "chat-composer-collapsed",
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
                            fieldIdentifier: "chat-composer-field"
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
        // The reading column; #EXP-1782: no edge fade behind the bar.
        .frame(maxWidth: DesignTokens.Transcript.maxWidth)
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ChatCopy.composerTestId)
        // The composed session can move (a resumed child): a new draft.
        .task(id: sessionId) {
            if draft?.sessionId != sessionId {
                draft = ChatComposerDraft(
                    sessionId: sessionId,
                    agentId: sessionId.isEmpty ? agentId : nil,
                    accountId: accountId,
                    sessionsApi: deps.sessionsApi,
                    attachmentsApi: deps.attachmentsApi
                )
            }
            draft?.teamId = draftTeamId
            draft?.scope = teamsPick?.scope
            seedMcpPick()
        }
        // #EXP-1793: the agent's servers seed the first message's pick.
        .onChange(of: agentRow?.mcpServerIds) { _, _ in seedMcpPick() }
        .task(id: agentId == nil || !sessionId.isEmpty ? nil : draftTeamId) {
            guard agentId != nil, sessionId.isEmpty, let teamId = draftTeamId else { mcpServers = []; return }
            mcpServers = (try? await McpServersApi(trpc: deps.trpc).list(accountId: accountId, teamId: teamId)) ?? []
        }
        .task(id: root?.scopeTeamIds) { teamsPicked = root?.scopeTeamIds }
        .onChange(of: teamsPick?.scope) { _, scope in draft?.scope = scope }
        .task(id: lookupTeamId) {
            slashPrompts.bind(accountId: accountId, teamId: lookupTeamId, db: deps.db)
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
            teamId: lookupTeamId,
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
            account: steerModel?.composerAccountMenu,
            // #EXP-1715: the viewer's Assistant chat's Teams pick.
            teams: teamsPick,
            mcp: mcpMenu
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
    private func slashMatches(_ draft: ChatComposerDraft) -> [SlashCandidate] {
        SlashCommands.candidates(draft: draft.draftText, agent: catalogAgent, prompts: slashPrompts.prompts)
    }

    /// A command rewrites the draft; a prompt's body (`prompts.get`) replaces
    /// the `/name` draft (web `session-composer.tsx`). Never sends.
    private func applySlashCandidate(_ candidate: SlashCandidate, _ draft: ChatComposerDraft) {
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

    /// EXP-1676: the menu follows the DRAFT, not the keyboard (web, Android).
    private func slashMenuVisible(_ draft: ChatComposerDraft) -> Bool {
        !slashMatches(draft).isEmpty
            && slashDismissedFor != draft.draftText
    }

    /// Accepting a row REWRITES the draft and never sends.
    private func applySlashCommand(_ command: SlashCommand, _ draft: ChatComposerDraft) {
        draft.draftText = command.insertion
        slashDismissedFor = command.insertion
        slashHighlight = 0
        focus()
    }

    private func moveSlashHighlight(_ delta: Int, _ draft: ChatComposerDraft) {
        let count = slashMatches(draft).count
        guard count > 0 else { return }
        slashHighlight = ((slashHighlight + delta) % count + count) % count
    }

    /// Return accepts the highlighted `/` row while the menu is up, else sends.
    private func handleReturn(_ draft: ChatComposerDraft) {
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
    private func send(_ draft: ChatComposerDraft) {
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

// MARK: - #EXP-1732 an agent's route into THE session screen

/// NEW-2/NEW-3: a team agent's page = the CALLER's conversation with it
/// (web `t/$teamSlug/agents/$agentId`) as THE session screen (kind `agent` /
/// `assistant`). The route names the agent DEFINITION (`agents` row); the
/// conversation is the root with that `agent_id` whose `user_id` is me.
/// Before the first message there is no root yet: THE screen's chrome (the
/// agent's header with the telephone and the gear, THE tabs) over an empty
/// Chat whose composer's first send is `sessions.send({agentId, text,
/// attachmentIds})`; the screen switches to the conversation the moment its
/// row syncs.
struct AgentChatRoute: View {
    let agentId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var model = AgentChannelModel()

    var body: some View {
        Group {
            if let conversationId = model.conversationId {
                ChatScreen(rootId: conversationId)
                    .id(conversationId)
            } else if let agent = model.agent {
                AgentChatEmpty(agent: agent)
            } else if model.missing {
                ZStack {
                    AppBackground()
                    EmptyState(.chatNotFound)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .accessibilityIdentifier("agent-channel-not-found")
                        .workHeaderBand { ChatHeader() }
                }
            } else {
                ZStack {
                    AppBackground()
                    // EXP-1275 (D8): a first load is skeleton rows.
                    SkeletonRows()
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                        .workHeaderBand { ChatHeader() }
                }
            }
        }
        // #EXP-1688: THE chat header replaces the bar, loading or not.
        .toolbar(.hidden, for: .navigationBar)
        .onAppear {
            model.bind(accountId: accountId, agentId: agentId, userId: deps.auth.userId, db: deps.db)
        }
        .onDisappear { model.stop() }
    }
}

/// The agent's definition row and the caller's conversation with it, live
/// off GRDB. A definition that never arrives turns `missing` after a grace
/// period (web `SYNC_GRACE_MS`).
@MainActor
@Observable
final class AgentChannelModel {
    private(set) var agent: AgentDefinitionEntity?
    private(set) var conversationId: String?
    private(set) var missing = false

    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var graceTask: Task<Void, Never>?

    nonisolated init() {}

    func bind(accountId: String, agentId: String, userId: String?, db: DatabaseManager) {
        stop()
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db -> (AgentDefinitionEntity?, String?) in
            let agent = try AgentDefinitionEntity.fetchOne(db, key: agentId)
            let conversation = try SessionEntity
                .filter(Column("agent_id") == agentId)
                .filter(Column("user_id") == (userId ?? ""))
                .filter(Column("parent_id") == nil)
                .order(Column("created_at"), Column("id"))
                .fetchOne(db)
            return (agent, conversation?.id)
        }
        task = Task { [weak self] in
            do {
                for try await (agent, conversationId) in observation.values(in: pool) {
                    guard let self else { return }
                    self.agent = agent
                    self.conversationId = conversationId
                    if agent != nil || conversationId != nil { self.missing = false }
                }
            } catch {}
        }
        graceTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled, let self, self.agent == nil, self.conversationId == nil else { return }
            self.missing = true
        }
    }

    func stop() {
        task?.cancel()
        task = nil
        graceTask?.cancel()
        graceTask = nil
    }
}

/// The conversation before its first message: THE session screen's chrome
/// (the agent's disc — #EXP-1448: the Assistant's = the brand mark — over
/// the subtitle; NEW-69 the telephone, THE gear; no `…` before the first
/// message), THE tabs (nothing counted yet), an empty Chat and THE chat
/// composer (`Message <name>`) whose first send opens the conversation.
private struct AgentChatEmpty: View {
    let agent: AgentDefinitionEntity

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var tab: SessionScreen.Tab = .chat
    @State private var settingsOpen = false
    @State private var isOwner = false

    private var screen: SessionScreen.Screen {
        SessionScreen.screen(
            kind: SessionScreen.kind(agentId: agent.id, appId: nil, assistant: agent.isAssistant),
            requested: tab, open: nil, counts: SessionScreen.Counts()
        )
    }

    private var subtitle: String {
        SessionScreen.subtitle(
            status: SessionScreen.status([]),
            device: nil,
            trigger: AgentTrigger.parseList(agent.triggers).first(where: \.enabled).map { AgentSettings.triggerSummary($0) }
        )
    }

    var body: some View {
        let shown = screen
        ZStack {
            AppBackground()
            if shown.active == .results {
                EmptyState(title: ResultsTab.Tab.Copy.empty, icon: AppIcons.chatResults, size: .list)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Color.clear
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .contentShape(Rectangle())
                    .onTapGesture { UIApplication.endEditing() }
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if shown.active == .chat {
                ChatComposer(
                    sessionId: "",
                    placeholder: "Message \(agent.name)",
                    agentId: agent.id,
                    teamId: agent.teamId,
                    agentRow: agent
                )
            }
        }
        .workHeaderBand {
            ChatHeader {
                ChatHeaderTitle(
                    title: agent.name,
                    subtitle: subtitle,
                    agentIcon: BoardTypeDisplay.iconName(for: agent.icon) ?? SessionTree.ListCopy.agentDefaultIcon,
                    agentColor: agent.color ?? SessionTree.ListCopy.agentDefaultColor,
                    assistant: agent.isAssistant
                )
            } trailing: {
                AgentCallButton(
                    agentId: agent.id, agentName: agent.name,
                    agentIcon: agent.isAssistant ? nil : BoardTypeDisplay.iconName(for: agent.icon), agentColor: agent.color
                )
                .buttonStyle(.plain)
                if shown.gear {
                    SessionScreenGear {
                        UIApplication.endEditing()
                        settingsOpen = true
                    }
                }
            }
            SessionScreenTabs(tabs: shown.tabs, selection: shown.active) { next in
                UIApplication.endEditing()
                tab = next
            }
        }
        .navigationDestination(isPresented: $settingsOpen) {
            AgentSettingsPage(teamId: agent.teamId, agentId: agent.id, isOwner: isOwner)
                .environment(\.accountId, accountId)
                .environment(\.detailColumnRoot, false)
                .bareBackButton()
        }
        .task(id: agent.teamId) { await resolveOwner() }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-channel-page")
    }

    private func resolveOwner() async {
        guard let userId = deps.auth.userId, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let teamId = agent.teamId
        isOwner = (try? await pool.read { db in
            try TeamMemberEntity
                .filter(Column("team_id") == teamId && Column("user_id") == userId)
                .fetchOne(db)?.role == "owner"
        }) ?? false
    }
}

private struct ChatLandingKey: EnvironmentKey {
    /// No navigator above: a landing goes nowhere.
    static let defaultValue = ChatLanding()
}

extension EnvironmentValues {
    /// EXP-1284: the navigator's landing box (`ChatLanding`).
    var chatLanding: ChatLanding {
        get { self[ChatLandingKey.self] }
        set { self[ChatLandingKey.self] = newValue }
    }
}
