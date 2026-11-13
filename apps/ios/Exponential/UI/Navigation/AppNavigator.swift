import CoreSpotlight
import Combine
import ExpUI
import ExpCore
import SwiftUI
import UserNotifications
import GRDB

enum AppRoute: Hashable {
    /// Search (EXP-686): a pushed detail off the board header's search
    /// button — no longer a tab of its own.
    case search
    case agents
    /// NEW-2: one prompt's page in Settings → Prompts (`id` nil = New
    /// prompt) — pushed from a Prompts row or a `prompt` entity ref.
    case prompt(accountId: String, teamId: String, id: String?)
    /// #EXP-1661: one skill's read-only page in Settings › Skills (phones
    /// never create or edit a skill).
    case skill(accountId: String, teamId: String, id: String)
    /// NEW-2/NEW-3: a team agent's page = the CALLER's DM with it (the
    /// definition's id; the conversation is resolved — or, none yet, opened
    /// by the first message — on the page).
    case agentChannel(accountId: String, agentId: String)
    /// VAPP-110: the team's Apps (fixtures/apps.json) — #EXP-1384: the
    /// Apps TAB's root (a link to it switches the tab).
    case apps(accountId: String)
    /// VAPP-110: one app's editor (`appId` = its row id). VAPP-242: only
    /// from a `…` › Edit app, New app and an import.
    case app(accountId: String, appId: String, screen: Apps.ScreenRequest? = nil)
    /// VAPP-242 (apps.json THE RUN VIEW): the app as a member uses it, its
    /// PUBLISHED package full screen (a list row tap); `screen` = an app
    /// screen link's screen.
    case appRun(accountId: String, appId: String, screen: Apps.ScreenRequest? = nil)
    /// The Inbox page (EXP-1373: pushed from the Chats header's inbox
    /// circle, notifications only — My issues and Drafts moved to the board).
    case myWork
    /// EXP-1291: the Chats tab (it replaced Reviews) — the agents, then
    /// the chat roots Needs you · Running · Recent.
    case chats
    /// #EXP-1388: the Chats list's Archived page (the Archived entry row).
    case archivedChats(accountId: String)
    /// #FEED-191 (needs-you.json): THE list, pushed from the Chats list's
    /// first row.
    case needsYou(accountId: String)
    case board(accountId: String, id: String)
    case issue(accountId: String, id: String)
    /// EXP-1241: an issue's page scrolled to one comment, which briefly
    /// highlights — a comment card tap (web `?comment=<id>`).
    case issueComment(accountId: String, id: String, commentId: String)
    /// EXP-1170: the New issue page — a DRAFT in the issue page's layout.
    /// Every opener mints `draftId` at TAP time (a fresh lowercase uuid; the
    /// Drafts list passes the row's own), so the route names the draft it
    /// autosaves to. `statusId` pre-picks a status; `parentId` files it as a
    /// sub-issue of that issue (the Sub-issues `+`, EXP-1097), which writes
    /// no draft row. Creating replaces this route with the issue it filed.
    /// `chatId` (a root session's id) files it INTO that chat.
    case issueDraft(
        accountId: String,
        draftId: String,
        boardId: String,
        statusId: String? = nil,
        parentId: String? = nil,
        chatId: String? = nil
    )
    /// EXP-1291: ONE session's raw work (`AgentSessionScreen` → today's
    /// steer view) — a session card's tap, a blocked-session push. A pushed
    /// destination (EXP-221), so it gets the native back + swipe-back.
    case agentSession(accountId: String, sessionId: String)
    /// EXP-1291: a CHAT = a root session (`chatId` = its id): a chat,
    /// or a team agent's channel — `ChatScreen`, its strip as the pager.
    /// `child` opens it on one page (nil = the Conversation; #EXP-1662
    /// `.plan(messageId)` = its plan tab on that plan).
    case chat(accountId: String, chatId: String, child: ChatPage? = nil)
    /// EXP-825: Home's composer pushed with a preselection (a play button's
    /// `seed`); an empty seed IS the Home tab.
    case agent(accountId: String, seed: AgentComposerSeed)
    case settings
    case serverDetail(accountId: String)
    /// NEW-67: the account's Settings → Voice page (`voice.json`).
    case voiceSettings(accountId: String)
    /// #EXP-1381 (settings-nav.json): a Team page row → that category's
    /// OWN page (Devices included: it left the tab bar for the Team page).
    case teamSettingsSection(accountId: String, teamId: String, section: TeamSettingsSection)
    /// #VAPP-185: Settings › MCP › Activity (settings-mcp.json `activityPage`).
    case mcpActivity(accountId: String, teamId: String)
    case invite(token: String)
    case syncDebug
    /// About (EXP-262): the app's version surface, pushed from Settings →
    /// General. Third-party licences are one push further so the notice blob
    /// never weighs down the settings screen.
    case about
    case thirdPartyLicenses

    /// The draft page draws its own Back (the discard guard).
    var isIssueDraft: Bool {
        if case .issueDraft = self { return true }
        return false
    }
}

extension AppRoute {
    /// #EXP-1456 (inbox-route.json `sessionTarget`, S-3): where a session
    /// opens — the viewer's own agent conversation root = the agent's page,
    /// a chat root or child = the chat (on the child's page), anything
    /// else (a teammate's conversation, a child under an agent, an unsynced
    /// id) = the session on its own screen.
    static func opening(_ target: InboxRoute.SessionTarget, accountId: String) -> AppRoute {
        switch target {
        case let .agent(agentId):
            .agentChannel(accountId: accountId, agentId: agentId)
        case let .chat(chatId, sessionId):
            .chat(accountId: accountId, chatId: chatId, child: sessionId.map { .session($0) })
        case let .session(id):
            .agentSession(accountId: accountId, sessionId: id)
        }
    }

    /// #EXP-1493 → #EXP-1662 (plan-page.json WAYS IN): an inbox row / push
    /// naming a session with a waiting plan of its own
    /// (`PlanPage.planPageFor`) opens its chat's plan tab; else
    /// `openingSession`.
    /// #PERF-5: these read the store off main (async), never on a tap.
    /// waiting-cards.json DEEP LINKS: a push's `card` (its `messageId`), else
    /// the session's own waiting question, opens its chat AT that card
    /// (`.card`, THE pending deep link: paged in, centred, rung).
    @MainActor static func openingInbox(
        _ sessionId: String, card: NotificationRouting.Card? = nil, accountId: String, deps: AppDependencies
    ) async -> AppRoute {
        if card == nil, let messageId = await planPageTarget(sessionId, accountId: accountId, deps: deps) {
            return await openingPlan(sessionId, messageId: messageId, accountId: accountId, deps: deps)
        }
        let viewerId = deps.auth.accounts.first { $0.id == accountId }?.userId
        let read = try? await deps.db.pool(forAccountId: accountId).read { db -> (InboxRoute.SessionTarget, String, String?)? in
            guard let session = try SessionEntity.fetchOne(db, key: sessionId) else { return nil }
            let target = try InboxRoute.sessionTarget(db: db, sessionId: sessionId, viewerId: viewerId)
            // The synced messages + the session's digest (its `waits`): the
            // messages sync on demand, the digest always (web's inbox reads it).
            let waiting = card == nil
                ? ChatRowAction.sessionWaitingCard(
                    sessionId: sessionId,
                    messages: try SessionQueries.cardRows(db: db, sessionId: sessionId).map(TreeMessage.init(messageRow:))
                        + Chats.digestMessages(sessionId: sessionId, digest: session.digest, rootId: session.rootId)
                )
                : nil
            return (target, session.rootId.isEmpty ? session.id : session.rootId, waiting)
        }
        // Not synced yet (a cold push tap): the push's own `rootId`.
        let landing = InboxRoute.landing(
            target: read?.0, rootId: read?.1 ?? card?.rootId,
            messageId: card?.messageId ?? read?.2, plan: card?.plan ?? false
        )
        switch landing {
        case let .plan(rootId, messageId):
            return .chat(accountId: accountId, chatId: rootId, child: .plan(messageId))
        case let .card(rootId, messageId):
            return .chat(accountId: accountId, chatId: rootId, child: .card(messageId))
        case .session:
            return await openingSession(sessionId, accountId: accountId, deps: deps)
        }
    }

    /// #EXP-1662: a plan opens as a tab of its session's root (a chat or an
    /// agent's conversation); an unsynced session opens by `openingSession`.
    @MainActor static func openingPlan(
        _ sessionId: String, messageId: String, accountId: String, deps: AppDependencies
    ) async -> AppRoute {
        let session = try? await deps.db.pool(forAccountId: accountId).read { db in
            try SessionEntity.fetchOne(db, key: sessionId)
        }
        guard let session else { return await openingSession(sessionId, accountId: accountId, deps: deps) }
        return .chat(
            accountId: accountId,
            chatId: session.rootId.isEmpty ? session.id : session.rootId,
            child: .plan(messageId)
        )
    }

    /// The session's waiting plan message (`PlanPage.planPageFor`), else nil.
    @MainActor static func planPageTarget(_ sessionId: String, accountId: String, deps: AppDependencies) async -> String? {
        let rows = (try? await deps.db.pool(forAccountId: accountId).read { db in
            try SessionMessage.fetchAll(db, sql: """
                \(SessionQueries.keyedMessagesSQL)
                AND "messages"."session_id" = ?
                AND json_extract("messages"."card", '$.kind') = 'plan'
                """, arguments: [sessionId])
        }) ?? []
        return PlanPage.planPageFor(sessionId, rows.map { row in
            PlanPage.Message(
                id: row.id, sessionId: row.sessionId, authorSessionId: row.authorSessionId,
                createdAt: SessionTree.ms(row.createdAt) ?? 0, card: TreeMessageCard.parse(row.card)
            )
        })
    }

    /// #FEED-191 (needs-you.json): where a row opens, by `opens`: a plan = its
    /// chat's Plan tab on that plan; a card = the Conversation scrolled to
    /// it; results = the Results tab; chat = the agent conversation.
    @MainActor static func openingNeedsYou(_ row: NeedsYou.Row, accountId: String, deps: AppDependencies) async -> AppRoute {
        switch row.opens {
        case .plan: .chat(accountId: accountId, chatId: row.rootId, child: .plan(row.messageId))
        case .card: .chat(accountId: accountId, chatId: row.rootId, child: .card(row.messageId))
        case .results: .chat(accountId: accountId, chatId: row.rootId, child: .results)
        case .chat: await openingSession(row.rootId, accountId: accountId, deps: deps)
        }
    }

    /// `opening(_:)` for a session id read from the account's store.
    @MainActor static func openingSession(_ sessionId: String, accountId: String, deps: AppDependencies) async -> AppRoute {
        let viewerId = deps.auth.accounts.first { $0.id == accountId }?.userId
        let target = (try? await deps.db.pool(forAccountId: accountId).read { db in
            try InboxRoute.sessionTarget(db: db, sessionId: sessionId, viewerId: viewerId)
        }) ?? .session(id: sessionId)
        return opening(target, accountId: accountId)
    }
}

/// EXP-1210: the bottom bar's destinations — ROOT siblings on phones. The
/// selected one IS the navigation stack's root, so switching swaps the root
/// (no push animation, no back chevron, no swipe-back) and the path holds
/// only the pushes made inside it. `agent` = the chat arm's Home, the
/// screen the app opens on. A tab's pushes are parked while another tab is
/// up and handed back on return, EXCEPT Home's: it always lands on its root
/// (Android parity: `popUpTo(agent-tab)` never restores it).
///
/// #EXP-1384 (chats.json `shell.tabs`): the bar's tabs are Board · Chats ·
/// Apps · Team (Devices is a Team page row now); the agent home stays the
/// chat arm's root.
enum MainTab: Hashable {
    case agent, issues, chats, apps, team

    /// The bar tab this root lights (nil = the agent home).
    var shellTab: Chats.Shell.Tab? {
        switch self {
        case .agent: nil
        case .issues: .board
        case .chats: .chats
        case .apps: .apps
        case .team: .team
        }
    }

    init(_ tab: Chats.Shell.Tab) {
        switch tab {
        case .board: self = .issues
        case .chats: self = .chats
        case .apps: self = .apps
        case .team: self = .team
        }
    }

    /// Whether this tab's pushes are parked across a switch.
    var keepsPushes: Bool { self != .agent }
}

/// The board the Issues tab is currently showing. May belong to a
/// non-active account while the fallback resolve crosses servers — but a
/// switcher pick of another server's board activates that account
/// (EXP-400, Android parity), so the two normally coincide.
struct CurrentBoardRef: Hashable {
    let accountId: String
    let boardId: String
}

struct AppNavigator: View {
    @Environment(AppDependencies.self) private var deps
    // Web URL the app can't render (EXP-92) — presented in an in-app Safari
    // sheet. Lives at the root (not MainNavigator) so the fallback also works
    // while signed out / mid-onboarding.
    @State private var externalUrl: ExternalUrl?
    /// EXP-1031: the app's ONE toast stack, mounted here so signed-out,
    /// onboarding and main screens all reach it. The shared instance is also
    /// the environment default, so nothing can toast into a void.
    private let toaster = Toaster.shared
    /// The height of MainNavigator's top status banner while one is up
    /// (`ToastTopInsetKey`), so the top-hung stack sits below it; 0 else.
    @State private var toastBannerHeight: CGFloat = 0
    /// NEW-69: the app's one call.
    @State private var voiceCall = VoiceCallClient.shared
    /// #EXP-1642: every signed-in account's issues and chats in Spotlight;
    /// here (not MainNavigator) so a sign-out deletes its domain even when it
    /// was the last account.
    @State private var systemSearch: SystemSearchIndexer?

    private struct ExternalUrl: Identifiable {
        let url: URL
        var id: String { url.absoluteString }
    }

    var body: some View {
        Group {
            if let gatedAccountId = activeGatedAccountId {
                // Client-version gate (EXP-104): the ACTIVE account's server
                // 426'd this build, so its surfaces are blocked — its sync loops
                // have already stopped. Scoped to that one account (REV2-43):
                // other signed-in servers keep syncing, and the view offers
                // "Remove this server" so a misconfigured instance can never
                // strand the app.
                UpdateRequiredView(accountId: gatedAccountId)
            } else if deps.auth.accounts.isEmpty {
                // First launch — no accounts at all.
                InstanceView()
            } else if deps.auth.accounts.allSatisfy({ $0.token == nil }) {
                // Every account is signed out — show login for the most recent.
                LoginView()
            } else if deps.auth.isAuthenticated, deps.auth.needsOnboarding {
                // First-run wizard (web onboarding parity): the session read at
                // login explicitly reported no onboardingCompletedAt. Gated on
                // the server flag — never inferred locally from synced data.
                // The server owns the rule (lib/auth/onboarding.ts): it
                // backfills the flag for users who already have a board in a
                // non-public team, and OnboardingView re-reads the session
                // on appear so stale accounts dismiss themselves.
                OnboardingView()
                    .id(deps.auth.activeAccountId ?? "none")
            } else {
                MainNavigator()
                    .id(deps.auth.activeAccountId ?? "none")
            }
        }
        // EXP-621: steer sessions are app-scoped now, so an account switch or a
        // sign-out has to retire them explicitly — MainNavigator's
        // `.id(activeAccountId)` only recreates the SCREENS, and a socket left
        // dialing on the previous account's ticket (with its draft still in
        // memory) belongs to nobody.
        .onChange(of: deps.auth.activeAccountId) { _, _ in
            deps.steerSessions.removeAll()
            // NEW-69: a call belongs to the account that started it.
            voiceCall.end()
        }
        .onChange(of: deps.auth.authenticatedAccountIds) { old, new in
            deps.steerSessions.removeAll()
            // #PERF-9 STORED TAIL: signing out deletes the account's tails.
            SteerStoredTailStore.shared.removeAccounts(old.subtracting(new))
            systemSearch?.refresh()
        }
        .task {
            // Capture runs index nothing (the store is the seed's).
            guard systemSearch == nil, !ProcessInfo.processInfo.arguments.contains("-uiTesting") else { return }
            systemSearch = SystemSearchIndexer(auth: deps.auth, db: deps.db)
            systemSearch?.refresh()
            // #EXP-1640: the agents the "Ask <agent>" phrase knows.
            AppIntentsHost.refreshShortcutParameters()
        }
        // #EXP-1642: a Spotlight result opens like a push tap.
        .onContinueUserActivity(CSSearchableItemActionType) { activity in
            SystemSearchIndexer.open(activity, deps: deps)
        }
        // URL handling lives at the ROOT view (mounted from first render), so a
        // cold launch via exponential:// lands in the bus even before
        // MainNavigator exists; MainNavigator drains the bus when it appears.
        .onOpenURL { url in
            handleDeepLink(url)
        }
        .onChange(of: deps.deepLinkBus.pendingExternalUrl) { _, url in
            if let url {
                externalUrl = ExternalUrl(url: url)
                _ = deps.deepLinkBus.consumeExternalUrl()
            }
        }
        .sheet(item: $externalUrl) { external in
            SafariView(url: external.url)
                .ignoresSafeArea()
        }
        .transaction { $0.animation = nil }
        // NEW-69: THE call surface (voice-call.json), full screen above every
        // screen so a call outlives the chat that started it; EXP-1681: a
        // minimized call is the floating pill over every screen instead.
        .background {
            let shown = voiceCall.view != nil && !voiceCall.minimized
            Color.clear
                .fullScreenCover(isPresented: Binding(get: { shown }, set: { if !$0 { voiceCall.minimize() } })) {
                    VoiceCallScreen(client: voiceCall)
                }
        }
        .overlay {
            if voiceCall.view != nil, voiceCall.minimized {
                VoiceCallPillOverlay(client: voiceCall)
            }
        }
        .onPreferenceChange(ToastTopInsetKey.self) { height in
            MainActor.assumeIsolated { toastBannerHeight = height }
        }
        .environment(\.toaster, toaster)
        // #EXP-1645: ExpUI's team mark fetches through the signed-in loader.
        .environment(\.teamImageLoader, .signedIn(deps))
        // EXP-1275 (D8 BLOCKING): ONE overlay over the whole window, above
        // the `.id(activeAccountId)` screens so an account switch or a
        // sign-out never tears it down mid-wait.
        .blockingOverlayHost()
        // EXP-1275: the styleguide suite's Feedback states screen (capture
        // only, `-uiTesting -uiTestingStyleguide feedback-states`).
        .fullScreenCover(isPresented: .constant(UITestingStyleguide.requested == "feedback-states")) {
            FeedbackStatesScreen()
        }
        // EXP-1259 D12-D14: the chat cards (capture only,
        // `-uiTestingStyleguide work-card|script-card|comment-call-card`).
        .fullScreenCover(isPresented: .constant(
            UITestingStyleguide.requested.map(ChatCardsScreen.kinds.contains) == true
        )) {
            ChatCardsScreen(kind: UITestingStyleguide.requested ?? "work-card")
        }
        // #EXP-1662: the `plan-page` specimen (capture only).
        .fullScreenCover(isPresented: .constant(
            UITestingStyleguide.requested.map(PlanPageSpecimenScreen.kinds.contains) == true
        )) {
            PlanPageSpecimenScreen()
        }
        // EXP-1635: the share extension's pages + the attach menu (capture only).
        .fullScreenCover(isPresented: .constant(
            UITestingStyleguide.requested.map(ShareSpecimens.kinds.contains) == true
        )) {
            ShareSpecimens(kind: UITestingStyleguide.requested ?? "share-to")
        }
        // #EXP-1639: the Live Activity's views (capture only).
        .fullScreenCover(isPresented: .constant(
            UITestingStyleguide.requested.map(LiveActivitySpecimens.kinds.contains) == true
        )) {
            LiveActivitySpecimens(kind: UITestingStyleguide.requested ?? "os-live-activity")
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-cards-screen")
        }
        // #EXP-1641: the expanded notification's actions (capture only, a mock).
        .fullScreenCover(isPresented: .constant(
            UITestingStyleguide.requested.map(NotificationActionsSpecimen.kinds.contains) == true
        )) {
            NotificationActionsSpecimen()
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-cards-screen")
        }
        .toastHost(toaster, bannerHeight: toastBannerHeight)
    }

    /// The active account when ITS server has rejected this build (EXP-104).
    /// Nil for every other case — including a gate left over from an account
    /// that has since been removed.
    private var activeGatedAccountId: String? {
        guard let id = deps.auth.activeAccountId,
              deps.auth.accounts.contains(where: { $0.id == id }),
              UpdateGate.shared.upgrade(forAccountId: id) != nil
        else { return nil }
        return id
    }

    private func handleDeepLink(_ url: URL) {
        // Universal links (EXP-92): https app.exponential.at issue/invite URLs
        // land here too (SwiftUI lifecycle delivers them to onOpenURL).
        if url.scheme == "https" || url.scheme == "http" {
            handleWebLink(url)
            return
        }
        // exponential://github-connected[?error=<code>] — the GitHub App install flow
        // finished (fired by the server's post-install page). The in-app install
        // surface (ASWebAuthenticationSession) normally consumes this as its
        // callback; this path covers installs that finish in an external browser.
        // The repo picker listens and re-queries — and surfaces the error slug
        // (EXP-390: dropping it made every failed connect a silent no-op).
        if url.host == "github-connected" {
            let userInfo = GithubConnect.errorSlug(from: url).map { ["error": $0] }
            NotificationCenter.default.post(name: .githubConnected, object: nil, userInfo: userInfo)
        }
        // exponential://oauth-return?linked=<providerId> (EXP-1126) — a
        // link-mode handoff that finished OUTSIDE its auth sheet (an external
        // browser). The in-app SignInLinkSession normally consumes it; this
        // path just tells the Sign-in methods section to refetch.
        if url.host == "oauth-return", case .linked = OAuthReturn.parse(url) {
            NotificationCenter.default.post(name: .signInMethodsChanged, object: nil)
        }
        // exponential://issue/<issueId>
        if url.host == "issue", let issueId = url.pathComponents.dropFirst().first {
            deps.deepLinkBus.navigateToIssue(String(issueId))
        }
        // exponential://session/<sessionId>[?userId=<id>]: Android's session
        // link, the same route as a session push tap.
        if url.host == "session", let sessionId = url.pathComponents.dropFirst().first {
            let userId = URLComponents(url: url, resolvingAgainstBaseURL: false)?
                .queryItems?.first { $0.name == "userId" }?.value
            deps.deepLinkBus.navigateToSession(String(sessionId), userId: userId)
        }
        // exponential://invite/<token>
        if url.host == "invite", let token = url.pathComponents.dropFirst().first {
            deps.deepLinkBus.navigateToInvite(String(token))
        }
    }

    /// A universal link (EXP-92): THE opener (#EXP-1674).
    private func handleWebLink(_ url: URL) {
        LinkOpener.openWebLink(url, deps: deps)
    }
}

struct MainNavigator: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.motion) private var motion
    /// EXP-1210: the selected bottom-bar destination = the stack's ROOT. The
    /// app LANDS on the Agent tab, so a cold start and an account switch
    /// (`.id(activeAccountId)` recreates this view) both open there.
    @State private var tab: MainTab = .agent
    // Typed path (not NavigationPath) so the tab bar can inspect the top route.
    // Only the pushes made INSIDE the current tab (issue, run, board list, the
    // New issue page…); a tab switch parks it in `savedPaths`.
    @State private var path: [AppRoute] = []
    /// Each OTHER tab's pushes, parked while it is not selected and handed
    /// back when it is (Android `restoreState` parity); never the Agent
    /// tab's (`MainTab.keepsPushes`).
    @State private var savedPaths: [MainTab: [AppRoute]] = [:]
    @State private var teamState = TeamState()
    /// EXP-698 r5: the bulk-selection bar takes the tab bar's slot, so the
    /// list tells the bar to stand down while a selection is live.
    @State private var tabBarChrome = TabBarChrome()
    /// ONE box for the whole navigator's lifetime — its handler is wired in
    /// `onAppear`. Rebuilding it per body pass would invalidate every reader
    /// of `\.pushRoute` on every render.
    @State private var pushRouteAction = PushRouteAction()
    /// EXP-1284 (T11/T-N9): lands a chat page already on the stack.
    @State private var chatLanding = ChatLanding()
    /// EXP-1212: a New issue page with content holds every path change below.
    @State private var draftLeaveGuard = IssueDraftLeaveGuard()
    @State private var boardLoader: MultiAccountBoardLoader?
    /// EXP-1635: the share extension's mirror and the direct-share donations.
    @State private var shareTargets: ShareTargetsPublisher?
    @State private var observationTasks: [Task<Void, Never>] = []
    @State private var syncing = false
    /// EXP-1373: the team of every unread notification the inbox can render
    /// (nil = a team-less row): the page header's Inbox count reads the
    /// ACTIVE team's (#FEED-232).
    @State private var unreadTeamIds: [String?] = []
    /// EXP-1272: the Inbox tab's dot also lights while a draft resolves: the
    /// SAME rows the inbox's Drafts segment lists (account-wide, like it).
    /// The team id of every resolvable draft (one entry per draft); the
    /// Inbox dot counts the active team's (EXP-1272, web parity).
    @State private var draftTeamIds: [String] = []
    /// EXP-1291: the session tree's lists — ONE observation for the Chats
    /// tab and its dot, Home's Running band, the launcher dot and the board
    /// switcher's per-team dots.
    @State private var sessionLists = SessionListsModel()
    @State private var currentBoard: CurrentBoardRef?
    // EXP-400: the current board resolved before the boards observation
    // delivered rows, so the active-team alignment couldn't look up its
    // team yet — re-run it on the next boards emission.
    @State private var pendingTeamAlign = false
    /// Home hides the floating bar while the keyboard is up.
    @State private var keyboardVisible = false
    /// #EXP-1384: the board menu's New board row → the create-board sheet.
    @State private var createBoardTarget: CreateBoardTarget?
    /// #EXP-1674: every read-only markdown render's link tap = THE opener. One
    /// closure for the navigator's lifetime, like `pushRouteAction`.
    @State private var markdownLinks: MarkdownLinkHandler?
    /// #EXP-1645: the Team tab's team menu (every server's teams, New team).
    @State private var teamLoader: MultiAccountTeamLoader?
    @State private var showTeamSetup = false
    /// #EXP-1645: a pick of another server's team rebuilds this navigator
    /// (`.id(activeAccountId)`); the new one lands on the Team page.
    @MainActor private static var landOnTeamPage = false
    /// #NEW-12: regular width (iPad full screen, the ⅔ split) = the split
    /// layout (`TabletShell`); compact = the phone's.
    @Environment(\.horizontalSizeClass) private var sizeClass
    /// #NEW-12: the sidebar's open entry (follows `tab` on a landing).
    @State private var sidebar: SidebarItem = .agent
    /// #NEW-12: a list column row's open REPLACES the detail.
    @State private var listPushAction = PushRouteAction()
    /// #NEW-12: every sidebar pick (an overlaid sidebar folds on one).
    @State private var sidebarPicks = 0

    /// The size class mirrored into state: the push handlers are closures
    /// set once, and read it after the body that set them.
    @State private var splitActive = false

    private var isSplit: Bool { sizeClass == .regular }

    private struct CreateBoardTarget: Identifiable {
        let accountId: String
        let teamId: String
        var id: String { "\(accountId)/\(teamId)" }
    }
    var body: some View {
        ZStack {
            AppBackground()

            if isSplit {
                tabletShell
            } else {
                NavigationStack(path: $path) {
                    tabRoot
                        // Every root states its bar visible explicitly — the
                        // roots swap inside ONE stack (EXP-1358: Chats wears the
                        // same inline bar as the others again).
                        .toolbar(.visible, for: .navigationBar)
                        // EXP-1554: every pushed page's back = the bare arrow;
                        // the draft page draws its own (the discard guard).
                        .navigationDestination(for: AppRoute.self) { route in
                            destination(for: route).bareBackButton(enabled: !route.isIssueDraft)
                        }
                }
            }
        }
        .background { if isSplit { tabletShortcuts } }
        .onChange(of: isSplit, initial: true) { _, split in splitActive = split }
        .onChange(of: tab) { _, target in
            // #NEW-12: a landing on a phone root lights its sidebar entry.
            if sidebar.tab != target { sidebar = SidebarItem(target, board: currentBoard) }
        }
        .onChange(of: currentBoard) { _, board in
            if case .board = sidebar, let board { sidebar = .board(board.boardId) }
        }
        .environment(teamState)
        .environment(sessionLists)
        .environment(tabBarChrome)
        // EXP-698 r5: the issue list's rows are plain Buttons now (a
        // NavigationLink in a List row draws the system disclosure OUTSIDE
        // the glass card), so they push through this instead.
        .environment(\.pushRoute, pushRouteAction)
        .environment(\.markdownLinkHandler, markdownLinks)
        .environment(\.chatLanding, chatLanding)
        .environment(\.issueDraftLeaveGuard, draftLeaveGuard)
        .environment(\.accountId, deps.auth.activeAccountId ?? "")
        // #EXP-1384: every page header's inbox count (#FEED-232).
        .environment(\.inboxUnreadCount, activeInboxUnreadCount)
        // #FEED-232 (nav-badges.json `appIconCases`): the OS app icon = every
        // team's needs-you rows summed; 0 clears it.
        .onChange(of: appIconBadgeCount, initial: true) { _, count in
            UNUserNotificationCenter.current().setBadgeCount(count)
        }
        .sheet(item: $createBoardTarget) { target in
            CreateBoardSheet(
                accountId: target.accountId,
                teamId: target.teamId,
                onCreated: { boardId in selectBoard(accountId: target.accountId, boardId: boardId) }
            )
        }
        .sheet(isPresented: $showTeamSetup) {
            TeamSetupSheet()
        }
        .onAppear {
            if markdownLinks == nil {
                markdownLinks = LinkOpener.surfaceHandler(accountId: nil, baseURL: nil, teamId: nil, deps: deps)
            }
            if Self.landOnTeamPage {
                Self.landOnTeamPage = false
                tab = .team
            }
            listPushAction.setHandler { route in
                if let item = sidebarItem(for: route) {
                    selectSidebar(item)
                } else {
                    navigate { path = [route] }
                }
            }
            pushRouteAction.setHandler { route in
                // #NEW-12: the split layout's entries are sidebar picks.
                if splitActive, let item = sidebarItem(for: route) {
                    selectSidebar(item)
                    return
                }
                // EXP-1210: a bar destination is never pushed — a link to one
                // (an entity chip, a getting-started card) switches to it.
                if let target = Self.tab(for: route) {
                    selectTab(target)
                } else if case let .chat(_, chatId, child) = route,
                          let index = Self.chatIndex(chatId, in: path) {
                    // EXP-1284 (T-N9): never a second page of the same
                    // chat — pop back to it and land the child there.
                    navigate { landOnChat(at: index, chatId: chatId, page: child) }
                } else {
                    navigate { path.append(route) }
                }
            }
            if boardLoader == nil {
                boardLoader = MultiAccountBoardLoader(auth: deps.auth, db: deps.db)
                shareTargets = ShareTargetsPublisher(auth: deps.auth, db: deps.db)
                shareTargets?.refresh()
            }
            if teamLoader == nil {
                teamLoader = MultiAccountTeamLoader(auth: deps.auth, db: deps.db)
            }
            startObserving()
            resolveCurrentBoard()
            if teamState.teams.isEmpty {
                syncing = true
                Task {
                    await deps.syncManager.initialSync()
                    syncing = false
                }
            }
        }
        .onChange(of: deps.auth.accounts) { _, _ in
            boardLoader?.refresh()
            teamLoader?.refresh()
            shareTargets?.refresh()
        }
        // Defense-in-depth against a split binding. `.id(activeAccountId)` on
        // this navigator (AppNavigator) normally recreates the whole view on an
        // account switch, resetting @State and re-running startObserving(). If
        // that recreation is ever skipped (e.g. an account activated while a
        // cover is presented), the environment accountId + tRPC re-reads flip to
        // the new account while these observations keep streaming the OLD
        // account's pool — the "wrong account's data" bug. Rebind explicitly:
        // cancel, clear state, re-observe the new active pool, re-resolve.
        .onChange(of: deps.auth.activeAccountId) { _, _ in
            stopObserving()
            teamState.teams = []
            teamState.boards = []
            teamState.activeTeamId = nil
            currentBoard = nil
            startObserving()
            resolveCurrentBoard()
        }
        // Any change to the available (signed-in) boards re-validates the
        // Issues tab's current board.
        .onChange(of: availableBoardKeys) { _, _ in
            resolveCurrentBoard()
        }
        .onDisappear { stopObserving() }
        .onChange(of: deps.deepLinkBus.pendingIssueId) { _, issueId in
            if let issueId {
                // Universal links (EXP-92) resolve the account directly (URL
                // host match); push taps only know the recipient's userId.
                let accountId = deps.deepLinkBus.pendingIssueAccountId
                    ?? issueAccountId(forUserId: deps.deepLinkBus.pendingIssueUserId)
                let replacesTop = deps.deepLinkBus.pendingIssueReplacesTop
                let commentId = deps.deepLinkBus.pendingIssueCommentId
                _ = deps.deepLinkBus.consume()
                navigate {
                    appendIssueRoute(
                        accountId: accountId, issueId: issueId, replacingTop: replacesTop,
                        commentId: commentId
                    )
                }
            }
        }
        .onChange(of: deps.deepLinkBus.pendingInviteToken) { _, token in
            if let token {
                _ = deps.deepLinkBus.consumeInvite()
                navigate { path.append(AppRoute.invite(token: token)) }
            }
        }
        // An agent_message push tap (EXP-801): open My Work → Inbox under the
        // recipient's account (the row renders nowhere else).
        .onChange(of: deps.deepLinkBus.pendingInbox) { _, pending in
            if pending {
                navigate({ openInboxFromPush() }, dropped: { _ = deps.deepLinkBus.consumeInbox() })
            }
        }
        // A push tap naming a session (EXP-980 wall, #EXP-1456 an agent's
        // decision or a mention, whatever its type): open it by the
        // inbox-route rule, under the recipient's account.
        .onChange(of: deps.deepLinkBus.pendingSessionId) { _, sessionId in
            if let sessionId {
                let accountId = issueAccountId(forUserId: deps.deepLinkBus.pendingSessionUserId)
                let card = deps.deepLinkBus.pendingSessionCard
                _ = deps.deepLinkBus.consumeSession()
                navigate {
                    Task { @MainActor in
                        await activateTeam(forSession: sessionId, accountId: accountId)
                        path.append(await AppRoute.openingInbox(sessionId, card: card, accountId: accountId, deps: deps))
                    }
                }
            }
        }
        // #EXP-1534: a signed-out login (inbox row or push) opens that
        // device's settings on the Devices page.
        .onChange(of: deps.deepLinkBus.pendingDeviceRowId) { _, rowId in
            if rowId != nil {
                navigate({ openDeviceFromLink() }, dropped: { _ = deps.deepLinkBus.consumeDevice() })
            }
        }
        // #EXP-1674: a link that resolved to a screen (`LinkOpener`).
        .onChange(of: deps.deepLinkBus.pendingLinkLanding) { _, landing in
            if landing != nil {
                navigate({ openLinkLanding() }, dropped: { _ = deps.deepLinkBus.consumeLinkLanding() })
            }
        }
        // #EXP-1638: a pick from the app icon's long-press menu.
        .onChange(of: deps.deepLinkBus.pendingShortcut) { _, target in
            if target != nil {
                navigate({ openShortcut() }, dropped: { _ = deps.deepLinkBus.consumeShortcut() })
            }
        }
        // A cold-launch New issue waits for the boards to arrive.
        .onChange(of: teamState.boards.map(\.id)) { _, _ in
            if deps.deepLinkBus.pendingShortcut == .newIssue { openShortcut() }
        }
        // A team was deleted in-app (EXP-43): back to the landing tab (Home)
        // so no pushed view (team settings, server detail) still targets it.
        .onReceive(NotificationCenter.default.publisher(for: .teamDeleted)) { _ in
            savedPaths = [:]
            tab = .agent
            path = []
        }
        // Drain links that arrived before this navigator mounted (cold launch).
        // They push ON TOP of Home, where the app lands, so Back returns
        // there — deep links are the only cold-launch navigation.
        .task {
            let pendingAccountId = deps.deepLinkBus.pendingIssueAccountId
            let userId = deps.deepLinkBus.pendingIssueUserId
            if let issueId = deps.deepLinkBus.consume() {
                let accountId = pendingAccountId ?? issueAccountId(forUserId: userId)
                appendIssueRoute(accountId: accountId, issueId: issueId)
            }
            if let token = deps.deepLinkBus.consumeInvite() {
                path.append(AppRoute.invite(token: token))
            }
            let sessionUserId = deps.deepLinkBus.pendingSessionUserId
            let sessionCard = deps.deepLinkBus.pendingSessionCard
            if let sessionId = deps.deepLinkBus.consumeSession() {
                let accountId = issueAccountId(forUserId: sessionUserId)
                await activateTeam(forSession: sessionId, accountId: accountId)
                path.append(await AppRoute.openingInbox(sessionId, card: sessionCard, accountId: accountId, deps: deps))
            }
            if deps.deepLinkBus.pendingInbox { openInboxFromPush() }
            if deps.deepLinkBus.pendingDeviceRowId != nil { openDeviceFromLink() }
            if deps.deepLinkBus.pendingLinkLanding != nil { openLinkLanding() }
            if deps.deepLinkBus.pendingShortcut != nil { openShortcut() }
        }
        // A stack, not a safeAreaInset: an inset never moves the pushed pages'
        // navigation bars, so a banner covered their headers.
        .modifier(TopBannerStack {
            syncBanner
                // EXP-1031: the top-hung toast stack sits below the banner.
                .background(GeometryReader { proxy in
                    Color.clear.preference(key: ToastTopInsetKey.self, value: proxy.size.height)
                })
        })
        // Attached as an OVERLAY, not a safeAreaInset (EXP-36): an ancestor
        // inset outside the NavigationStack never reliably reaches the pushed
        // scrollables' content insets, so each bar-visible scrollable reserves
        // its own clearance via `.tabBarBottomInset()` instead — one source of
        // truth, no double-inset.
        .overlay(alignment: .bottom) {
            if showsTabBar && !tabBarChrome.suppressed && !isSplit {
                MobileTabBar(
                    selected: barSelection,
                    agentActive: tab == .agent,
                    board: activeBoard,
                    // The board view = the Board tab's root (its menu).
                    onBoardView: tab == .issues && path.isEmpty,
                    // The Team page itself; a pushed page's tap returns to it.
                    onTeamPage: tab == .team && path.isEmpty,
                    team: teamState.activeTeam,
                    chatsBadge: sessionLists.chatsBadge(teamId: teamState.activeTeamId),
                    // The launcher capsule (chat | new issue) rides every
                    // bar-visible surface (EXP-827/EXP-973); only a team with
                    // no board leaves the New-issue arm inert.
                    composeEnabled: composeTarget != nil,
                    boards: teamState.filteredBoards,
                    onNewBoard: newBoardAction,
                    teams: teamMenuRows,
                    onNewTeam: { showTeamSetup = true },
                    // EXP-1210: every entry SWITCHES the root (a re-tap pops
                    // back to it); EXP-1212: through the draft hold like
                    // every other path change.
                    onSelect: { selectTab(MainTab($0)) },
                    onSelectBoard: { board in
                        guard let accountId = deps.auth.activeAccountId else { return }
                        selectBoard(accountId: accountId, boardId: board.id)
                    },
                    onSelectTeam: { row in switchTeam(accountId: row.accountId, teamId: row.team.id) },
                    onCompose: {
                        // EXP-1170: the draft id is minted HERE, at tap time.
                        guard let target = composeTarget else { return }
                        navigate {
                            path.append(.issueDraft(
                                accountId: target.accountId,
                                draftId: UUID().uuidString.lowercased(),
                                boardId: target.boardId
                            ))
                        }
                    },
                    // EXP-825: the chat arm SWITCHES to Home (the empty-seed
                    // page, a bar-visible root) — never a push.
                    onChat: { selectTab(.agent) }
                )
                // Slides out of the way when a screen claims its slot, so the
                // bulk bar arrives in the space the bar just left rather than
                // popping in on top of it.
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(motion.standard, value: tabBarChrome.suppressed)
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in
            keyboardVisible = true
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in
            keyboardVisible = false
        }
    }

    // MARK: - Tab bar

    /// The lit tab: the root's (Devices is pushed over the Team root, so the
    /// Team tab stays lit there, #EXP-1384).
    private var barSelection: Chats.Shell.Tab? {
        tab.shellTab
    }

    /// The Board tab's board: the current one, while it is the active team's.
    private var activeBoard: BoardEntity? {
        guard let current = currentBoard else { return nil }
        return teamState.boards.first { $0.id == current.boardId }
    }

    /// The board menu's New board row (the active team's create-board sheet).
    private var newBoardAction: (() -> Void)? {
        guard let accountId = deps.auth.activeAccountId, let teamId = teamState.activeTeam?.id else { return nil }
        return { createBoardTarget = CreateBoardTarget(accountId: accountId, teamId: teamId) }
    }

    /// The team menu's rows: the Team page's list, every server's teams.
    private var teamMenuRows: [MobileTabBar.TeamRow] {
        let groups = teamLoader?.groups ?? []
        return groups.flatMap { group in
            group.teams.map {
                MobileTabBar.TeamRow(
                    team: $0,
                    accountId: group.accountId,
                    server: groups.count > 1 ? group.hostname : nil
                )
            }
        }
    }

    /// #FEED-232: the page header's Inbox count: the unread rows of the
    /// ACTIVE team (or team-less ones).
    private var activeInboxUnreadCount: Int {
        unreadTeamIds.filter { $0 == nil || $0 == teamState.activeTeamId }.count
    }

    /// #FEED-232: the app icon's number (`NavBadges.appIconBadge` over every
    /// team's needs-you rows).
    private var appIconBadgeCount: Int {
        NavBadges.appIconBadge(teamState.teams.map {
            sessionLists.needsYou(teamId: $0.id, me: deps.auth.userId, now: Date()).count
        })
    }

    /// The bar floats only over the tab ROOTS (EXP-1210) and pushed board
    /// lists; detail and settings screens — Search among them since EXP-686,
    /// and a SEEDED Home pushed by a play button — get the full height
    /// back. On the Home root the bar also stands down while the keyboard is
    /// up, so it never rides above the keyboard over the composer.
    private var showsTabBar: Bool {
        guard let top = path.last else {
            return tab == .agent ? !keyboardVisible : true
        }
        if case .board = top { return true }
        // #EXP-1384: Devices (a Team page row) keeps the bar, Team lit.
        if case .teamSettingsSection(_, _, .devices) = top { return true }
        // #EXP-1388: the Archived page (pushed over Chats) keeps it too.
        if case .archivedChats = top { return true }
        return false
    }

    /// EXP-1210: the root a bar entry (or a link to its destination) lands
    /// on. Re-selecting the current tab pops back to its root; a switch
    /// replaces the root, parking the old tab's pushes and handing the
    /// target's back. Through the draft hold (EXP-1212) like every other
    /// path change, so a New issue page is never parked.
    private func selectTab(_ target: MainTab) {
        if tab == target {
            if !path.isEmpty { navigate { path = [] } }
            return
        }
        navigate {
            // A root SWAP, never a pop/push transition.
            var swap = Transaction()
            swap.disablesAnimations = true
            withTransaction(swap) {
                park()
                tab = target
                path = savedPaths.removeValue(forKey: target) ?? []
            }
        }
    }

    /// A link's landing (a push tap, a universal link): the tab's ROOT, its
    /// parked pushes dropped; the tab being left is parked as usual.
    private func landOnRoot(of target: MainTab) {
        if tab != target { park() }
        savedPaths[target] = nil
        tab = target
        path = []
    }

    /// Parks the current tab's pushes before a switch away from it; Home
    /// drops them (it always lands on its root).
    private func park() {
        savedPaths[tab] = tab.keepsPushes ? path : nil
    }

    /// The bar destination a route NAMES, if any — those routes never push
    /// (EXP-1210); an empty-seed Home is the Home root itself.
    private static func tab(for route: AppRoute) -> MainTab? {
        switch route {
        case .chats: return .chats
        case .apps: return .apps
        case .settings: return .team
        case let .agent(_, seed) where seed == .empty: return .agent
        default: return nil
        }
    }

    /// The selected tab's screen — the navigation stack's ROOT.
    @ViewBuilder
    private var tabRoot: some View {
        let accountId = deps.auth.activeAccountId ?? ""
        switch tab {
        case .agent:
            AgentPageView(seed: .empty, isTabRoot: true)
                .environment(\.accountId, accountId)
        case .issues:
            IssuesHomeView(
                syncing: syncing,
                currentBoard: currentBoard,
                boardLoader: boardLoader,
                onSelectBoard: { accountId, boardId in
                    selectBoard(accountId: accountId, boardId: boardId)
                }
            )
        case .team:
            TeamTabView(
                onSwitchTeam: { accountId, teamId in switchTeam(accountId: accountId, teamId: teamId) }
            )
                .environment(\.accountId, accountId)
        case .apps:
            AppsListScreen()
                .environment(\.accountId, accountId)
        case .chats:
            ChatsView()
                .environment(\.accountId, accountId)
        }
    }

    /// Compose targets the board in view: a pushed board list wins, and every
    /// other bar-visible surface (Issues root, Devices, Inbox,
    /// Chats) falls back to the CURRENT board — the one the Issues
    /// tab is pointed at, resolved from the last-used board, else the first of
    /// the active team (EXP-973). Filing an issue is never route-dependent;
    /// only a team with no board at all leaves the arm with nowhere to go.
    private var composeTarget: (accountId: String, boardId: String)? {
        if case let .board(accountId, id)? = path.last {
            return (accountId, id)
        }
        if let current = currentBoard {
            return (current.accountId, current.boardId)
        }
        return nil
    }

    /// EXP-1212: every navigator-level path change runs through here. A New
    /// issue page WITH content on top HOLDS it and asks (`IssueDraftPage.Leave`);
    /// an answer that leaves pops the draft first, then runs `change`, so a
    /// push lands where the draft was. `dropped` runs when the user stays (a
    /// link left pending on the bus is cleared). The page's OWN exits
    /// (`onCreated`, `onClose`) never come through here.
    private func navigate(_ change: @escaping () -> Void, dropped: @escaping () -> Void = {}) {
        if case let .issueDraft(_, draftId, _, _, _, _)? = path.last {
            let held = IssueDraftLeaveGuard.Held(
                proceed: {
                    if case let .issueDraft(_, top, _, _, _, _)? = path.last, top == draftId {
                        path.removeLast()
                    }
                    change()
                },
                dropped: dropped
            )
            if draftLeaveGuard.holds(topDraftId: draftId, held) { return }
        }
        change()
    }

    /// Land on what was just filed (EXP-596) by REPLACING the compose page —
    /// Back from the issue returns to the board, not to an empty draft. One
    /// mutation, so the stack animates as a single push.
    private func replaceTopRoute(with route: AppRoute) {
        // Only the compose page is swapped out. `create()` is async, so a
        // notification tap or a link can push something else on top meanwhile;
        // that must not be clobbered.
        if case .issueDraft = path.last {
            path[path.count - 1] = route
        } else {
            path.append(route)
        }
    }

    /// EXP-1284: the LAST stack position showing `chatId`, if any.
    /// #EXP-1674 (app-link.json): an app's run view opened from another one
    /// stays on the stack above it.
    private static func opensOverRun(_ route: AppRoute, on path: [AppRoute]) -> Bool {
        guard case .appRun = route, case .appRun = path.last else { return false }
        return true
    }

    private static func chatIndex(_ chatId: String, in path: [AppRoute]) -> Int? {
        path.lastIndex { route in
            if case let .chat(_, id, _) = route { return id == chatId }
            return false
        }
    }

    /// EXP-1284 (T11/T-N9): pop everything above the chat page at `index`
    /// and land it on `page` (nil = its Conversation).
    private func landOnChat(at index: Int, chatId: String, page: ChatPage?) {
        if index + 1 < path.count {
            path.removeSubrange((index + 1)...)
        }
        chatLanding.land(chatId: chatId, page: page ?? .conversation)
    }

    /// EXP-1284 (T11/T-N7): the New issue page filed into a chat — it
    /// leaves the stack (its OWN exit, never held), and the chat shows the
    /// new issue's tab: the page below when it is on the stack, else a fresh
    /// chat page.
    private func landAfterChatCreate(accountId: String, chatId: String, issueId: String) {
        if case .issueDraft = path.last { path.removeLast() }
        if let index = Self.chatIndex(chatId, in: path) {
            landOnChat(at: index, chatId: chatId, page: .issue(issueId))
        } else {
            path.append(.chat(accountId: accountId, chatId: chatId, child: .issue(issueId)))
        }
    }

    /// Thin status banners: a background account the server has version-gated
    /// (EXP-104/REV2-43), then the active account's live-sync health.
    @ViewBuilder
    private var syncBanner: some View {
        VStack(spacing: 0) {
            updateGateBanner
            healthBanner
        }
    }

    /// Signed-in accounts (other than the active one) whose server rejected this
    /// build. Their pipelines are stopped, so the app has to say so somewhere —
    /// the active account is unaffected and keeps working.
    private var gatedBackgroundAccounts: [ServerAccount] {
        let gated = UpdateGate.shared.gatedAccountIds
        guard !gated.isEmpty else { return [] }
        return deps.auth.accounts.filter {
            $0.id != deps.auth.activeAccountId && gated.contains($0.id)
        }
    }

    /// Non-blocking counterpart to UpdateRequiredView: taps through to the
    /// server's detail screen, where it can be signed out of or removed.
    @ViewBuilder
    private var updateGateBanner: some View {
        let gated = gatedBackgroundAccounts
        if let first = gated.first {
            Button {
                navigate { path.append(.serverDetail(accountId: first.id)) }
            } label: {
                HStack(spacing: 6) {
                    AppIcon(AppIcons.uiUpdate, size: 11)
                    Text(gated.count > 1
                        ? "\(gated.count) servers need a newer app version. Their sync is paused."
                        : "\(first.displayName) needs a newer app version. Its sync is paused.")
                        .font(.caption2)
                }
                .foregroundStyle(.white.opacity(0.9))
                .padding(.horizontal, 12)
                .padding(.vertical, 5)
                .frame(maxWidth: .infinity)
                .background(.orange.opacity(0.35))
                .background(.ultraThinMaterial)
            }
            .buttonStyle(.plain)
        }
    }

    /// EXP-1358: its own view, so the sync stats it observes re-render
    /// this banner alone, never the navigator around it.
    private var healthBanner: some View {
        SyncHealthBanner(accountId: deps.auth.activeAccountId)
    }
}

/// A banner above the screen: everything under it, its navigation bars too,
/// lays out below it. The banner coming or going moves the navigation stack,
/// but UIKit leaves the stacked pages' safe areas where they were until each
/// page's next layout pass: a page that changes nothing then (an app's run
/// view, the Apps list) kept the old top inset under a shorter one, an empty
/// band between its header and its page. So every page lays out again.
private struct TopBannerStack<Banner: View>: ViewModifier {
    @ViewBuilder let banner: () -> Banner

    func body(content: Content) -> some View {
        VStack(spacing: 0) {
            banner()
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { _ in
                    // After this pass has moved the stack.
                    DispatchQueue.main.async { UIViewController.relayoutWindowControllers() }
                }
            content.frame(maxHeight: .infinity)
        }
    }
}

/// The active account's live-sync health banner (offline / session expired).
/// Only the ACTIVE account's health — a signed-out/failing OTHER account
/// must never flash the banner while the active account syncs fine.
struct SyncHealthBanner: View {
    let accountId: String?

    var body: some View {
        let health = accountId.flatMap { SyncDebug.shared.health[$0] } ?? .ok
        if health != .ok {
            HStack(spacing: 6) {
                AppIcon(health == .unauthorized ? AppIcons.uiWarning : AppIcons.uiOffline, size: 11)
                Text(health == .unauthorized
                    ? "Session expired. Sign in again to keep syncing."
                    : "Can't reach the server, showing cached data")
                    .font(.caption2)
            }
            .foregroundStyle(.white.opacity(0.9))
            .padding(.horizontal, 12)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity)
            .background(.orange.opacity(0.35))
            .background(.ultraThinMaterial)
        }
    }
}

extension MainNavigator {
    // MARK: - #NEW-12 split layout

    private var tabletShell: some View {
        TabletShell(
            picks: sidebarPicks,
            sidebar: AnyView(TabletSidebar(
                selection: sidebar,
                currentBoardId: currentBoard?.boardId,
                draftCount: draftTeamIds.filter { $0 == teamState.activeTeamId }.count,
                onSelect: { selectSidebar($0) },
                onSearch: { openSearch() },
                onNewIssue: newIssueAction
            )),
            listColumn: tabletListColumn,
            detailRoot: tabletDetailRoot,
            placeholderIcon: sidebar.placeholderIcon,
            path: $path,
            listPush: listPushAction,
            destination: { route, pushed in
                pushed
                    ? AnyView(destination(for: route).bareBackButton(enabled: !route.isIssueDraft))
                    : AnyView(destination(for: route))
            }
        )
    }

    /// The hardware keyboard's ⌘N (New issue), ⌘K (Search) and ⌘F (find:
    /// the desktop binds it to the same search).
    private var tabletShortcuts: some View {
        Group {
            Button("New issue") { newIssueAction?() }
                .keyboardShortcut("n", modifiers: .command)
            Button("Search") { openSearch() }
                .keyboardShortcut("k", modifiers: .command)
            Button("Find") { openSearch() }
                .keyboardShortcut("f", modifiers: .command)
        }
        .opacity(0)
        .frame(width: 0, height: 0)
        .accessibilityHidden(true)
    }

    /// The New issue page on the board in view (the tab bar's compose arm).
    private var newIssueAction: (() -> Void)? {
        guard let target = composeTarget else { return nil }
        return {
            navigate {
                path.append(.issueDraft(
                    accountId: target.accountId,
                    draftId: UUID().uuidString.lowercased(),
                    boardId: target.boardId
                ))
            }
        }
    }

    private func openSearch() {
        if case .search? = path.last { return }
        navigate { path.append(.search) }
    }

    /// The sidebar entry a route NAMES (it selects, never pushes).
    private func sidebarItem(for route: AppRoute) -> SidebarItem? {
        switch route {
        case .chats: .chats
        case .apps: .apps
        case .settings: .settings
        case let .agent(_, seed) where seed == .empty: .agent
        case .needsYou: .needsYou
        case let .board(_, id): .board(id)
        default: nil
        }
    }

    /// A sidebar pick: the entry's columns, the detail cleared (through the
    /// draft hold like every other path change).
    private func selectSidebar(_ item: SidebarItem) {
        navigate {
            if case let .board(boardId) = item, let accountId = deps.auth.activeAccountId,
               currentBoard?.boardId != boardId {
                selectBoard(accountId: accountId, boardId: boardId)
            }
            var swap = Transaction()
            swap.disablesAnimations = true
            withTransaction(swap) {
                sidebar = item
                tab = item.tab
                path = []
            }
            sidebarPicks += 1
        }
    }

    /// The entry's list column (nil = the entry is one page).
    private var tabletListColumn: AnyView? {
        let accountId = deps.auth.activeAccountId ?? ""
        switch sidebar {
        case .needsYou:
            return AnyView(NeedsYouPage().environment(\.accountId, accountId))
        case .chats:
            return AnyView(ChatsView().environment(\.accountId, accountId))
        case .drafts:
            return AnyView(TabletDraftsColumn().environment(\.accountId, accountId))
        case .apps:
            return AnyView(AppsListScreen().environment(\.accountId, accountId))
        case .settings:
            return AnyView(TeamTabView(
                onSwitchTeam: { accountId, teamId in switchTeam(accountId: accountId, teamId: teamId) }
            ).environment(\.accountId, accountId))
        case .board:
            return AnyView(IssuesHomeView(
                syncing: syncing,
                currentBoard: currentBoard,
                boardLoader: boardLoader,
                onSelectBoard: { accountId, boardId in selectBoard(accountId: accountId, boardId: boardId) }
            ))
        case .agent, .devices, .agentChannel, .app, .chat:
            return nil
        }
    }

    /// A one-page entry's page, the detail stack's root.
    private var tabletDetailRoot: AnyView? {
        let accountId = deps.auth.activeAccountId ?? ""
        switch sidebar {
        case .agent:
            return AnyView(AgentPageView(seed: .empty).environment(\.accountId, accountId))
        case .devices:
            return AnyView(AgentsView().environment(\.accountId, accountId))
        case let .agentChannel(agentId):
            return AnyView(AgentChatRoute(agentId: agentId).id(agentId).environment(\.accountId, accountId))
        case let .app(appId):
            return AnyView(AppRunScreen(appId: appId).id(appId).environment(\.accountId, accountId))
        case let .chat(rootId):
            return AnyView(ChatScreen(rootId: rootId, initialPage: .conversation).id(rootId)
                .environment(\.accountId, accountId))
        case .needsYou, .chats, .drafts, .apps, .settings, .board:
            return nil
        }
    }

    @ViewBuilder
    private func destination(for route: AppRoute) -> some View {
        switch route {
        case .search:
            SearchView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case .agents:
            AgentsView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case let .prompt(accountId, teamId, id):
            PromptSettingsPage(teamId: teamId, promptId: id)
                .environment(\.accountId, accountId)
        case let .skill(accountId, _, id):
            SkillSettingsPage(skillId: id)
                .environment(\.accountId, accountId)
        case let .agentChannel(accountId, agentId):
            AgentChatRoute(agentId: agentId)
                .id(agentId)
                .environment(\.accountId, accountId)
        case let .apps(accountId):
            AppsListScreen()
                .environment(\.accountId, accountId)
        case let .app(accountId, appId, screen):
            AppEditorScreen(appId: appId, screen: screen)
                .id(appId)
                .environment(\.accountId, accountId)
        case let .appRun(accountId, appId, screen):
            AppRunScreen(appId: appId, screen: screen)
                .id(appId)
                .environment(\.accountId, accountId)
        case .myWork:
            InboxPage()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case .chats:
            ChatsView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case let .archivedChats(accountId):
            ChatsView(view: .archived)
                .environment(\.accountId, accountId)
        case let .needsYou(accountId):
            NeedsYouPage()
                .environment(\.accountId, accountId)
        case let .board(accountId, id):
            IssueListView(boardId: id)
                .environment(\.accountId, accountId)
        case let .issue(accountId, id):
            // `.id(id)`: a card swaps the top route IN PLACE, and the screen
            // must rebuild for the other issue, not keep its state.
            IssueScreen(issueId: id)
                .id(id)
                .environment(\.accountId, accountId)
        case let .issueComment(accountId, id, commentId):
            // EXP-1241: the issue page scrolled to one comment, which flashes.
            IssueScreen(issueId: id, focusCommentId: commentId)
                .id(id)
                .environment(\.accountId, accountId)
        case let .issueDraft(accountId, draftId, boardId, statusId, parentId, chatId):
            IssueDraftPageView(
                draftId: draftId,
                boardId: boardId,
                statusId: statusId,
                parentId: parentId,
                chatId: chatId,
                onCreated: { createdId in
                    if let chatId {
                        // EXP-1284 (T11/T-N7): filed INTO a chat = back on
                        // that chat with the new issue's tab selected.
                        landAfterChatCreate(accountId: accountId, chatId: chatId, issueId: createdId)
                    } else {
                        replaceTopRoute(with: .issue(accountId: accountId, id: createdId))
                    }
                },
                onClose: {
                    // Only THIS page: a link may have pushed over it.
                    if case let .issueDraft(_, top, _, _, _, _)? = path.last, top == draftId {
                        path.removeLast()
                    }
                }
            )
            .environment(\.accountId, accountId)
        case let .agentSession(accountId, sessionId):
            AgentSessionScreen(sessionId: sessionId)
                .id(sessionId)
                .environment(\.accountId, accountId)
        case let .chat(accountId, chatId, child):
            // `.id(chatId)`: a root swapped in place rebuilds rather than
            // keeping the old one's state.
            ChatScreen(rootId: chatId, initialPage: child ?? .conversation)
                .id(chatId)
                .environment(\.accountId, accountId)
        case let .agent(accountId, seed):
            // Always a pushed detail (a play button's seed): Home is the
            // stack's root (`tabRoot`, EXP-1210).
            AgentPageView(seed: seed)
                .environment(\.accountId, accountId)
        case .settings:
            TeamTabView(
                onSwitchTeam: { accountId, teamId in switchTeam(accountId: accountId, teamId: teamId) }
            )
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case let .serverDetail(accountId):
            ServerDetailView(accountId: accountId)
        case let .voiceSettings(accountId):
            VoiceSettingsPage(accountId: accountId)
                .environment(\.accountId, accountId)
        case let .teamSettingsSection(accountId, teamId, section):
            TeamSettingsPage(teamId: teamId, section: section)
                .environment(\.accountId, accountId)
        case let .mcpActivity(accountId, teamId):
            McpActivityPage(teamId: teamId)
                .environment(\.accountId, accountId)
        case let .invite(token):
            InviteAcceptView(token: token)
        case .syncDebug:
            SyncDebugView()
        case .about:
            AboutView()
        case .thirdPartyLicenses:
            ThirdPartyLicensesView()
        }
    }

    private func startObserving() {
        stopObserving()

        guard let pool = try? deps.db.pool(forAccountId: deps.auth.activeAccountId ?? "") else { return }

        let wsObs = ValueObservation.tracking { db in
            try TeamEntity.fetchAll(db)
        }
        let projObs = ValueObservation.tracking { db in
            try BoardEntity.fetchAll(db)
        }

        let wsTask = Task { @MainActor in
            do {
                for try await ws in wsObs.removeDuplicates().values(in: pool) {
                    teamState.teams = ws
                    // Re-resolve a dangling selection (EXP-43): after a
                    // team delete syncs out, an activeTeamId
                    // pointing at a vanished row must not stick around.
                    // Non-empty emissions only — "Resync now" wipes every
                    // table before relaunching the pipeline, so this
                    // observation emits a transient []; nilling there would
                    // silently re-point a still-valid selection at the
                    // arbitrary first row once the refetch lands. A real
                    // delete still heals: its 409 refetch replaces rows in
                    // one transaction, so the emission is non-empty (or
                    // becomes non-empty via the personal-team heal).
                    if !ws.isEmpty,
                       let active = teamState.activeTeamId,
                       !ws.contains(where: { $0.id == active }) {
                        teamState.activeTeamId = nil
                    }
                    // EXP-1373: start on the account's remembered team; a
                    // remembered team that is gone is forgotten (and the
                    // board re-resolved without it).
                    let accountId = deps.auth.activeAccountId ?? ""
                    let remembered = SharedBoardMirror.readActiveTeam(accountId: accountId)
                    if !ws.isEmpty, let remembered, !ws.contains(where: { $0.id == remembered }) {
                        SharedBoardMirror.clearActiveTeam(accountId: accountId)
                        resolveCurrentBoard()
                    }
                    if teamState.activeTeamId == nil {
                        if let remembered, let team = ws.first(where: { $0.id == remembered }) {
                            teamState.activeTeamId = team.id
                        } else if let first = ws.first {
                            teamState.activeTeamId = first.id
                        }
                    }
                }
            } catch {}
        }
        let projTask = Task { @MainActor in
            do {
                for try await proj in projObs.removeDuplicates().values(in: pool) {
                    teamState.boards = proj
                    if pendingTeamAlign { alignActiveTeam() }
                }
            } catch {}
        }
        // Unread notifications drive the tab bar's inbox dot. The synced issue
        // ids ride along because the dot must count ONLY rows the inbox can
        // render and clear (InboxViewModel.isRenderable, REV-15): delivered
        // notification rows outlive membership, so after leaving a team its
        // unread rows keep syncing while their issues drop out of the local
        // store — a raw count would light the dot forever over an inbox that
        // shows "All caught up".
        let notifObs = ValueObservation.tracking { db -> [String?] in
            // `InboxViewModel.isRenderable` in SQL: an issue row needs its
            // issue synced; issue-less rows are agent messages and blocked
            // runs. The team = the row's own, else its issue's board's.
            try Row.fetchAll(db, sql: """
                SELECT COALESCE("n"."team_id", "b"."team_id") AS "team_id"
                FROM "notifications" AS "n"
                LEFT JOIN "issues" AS "i" ON "i"."id" = "n"."issue_id"
                LEFT JOIN "boards" AS "b" ON "b"."id" = "i"."board_id"
                WHERE "n"."read_at" IS NULL
                  AND (("n"."issue_id" IS NULL AND "n"."type" IN (?, ?, ?)) OR "i"."id" IS NOT NULL)
                """, arguments: [
                    DomainContract.notificationTypeAgentMessage,
                    DomainContract.notificationTypeSessionBlocked,
                    // #EXP-1534: team-less (`team_id` NULL = every team's dot).
                    DomainContract.notificationTypeAgentSignedOut,
                ]).map { $0["team_id"] as String? }
        }
        let notifTask = Task { @MainActor in
            do {
                for try await teamIds in notifObs.values(in: pool) {
                    unreadTeamIds = teamIds
                }
            } catch {}
        }
        // EXP-1272: drafts light the Inbox dot too. The same resolved rows
        // as the Drafts segment (`DraftsViewModel`), so the dot and the
        // segment agree: a draft whose board is gone lights neither. Only
        // the ACTIVE team's drafts count (web `useDraftEntries(teamId)`),
        // so the rows' team ids are kept and the dot reads them against
        // `teamState.activeTeamId` (a team switch moves the dot at once).
        let draftsObs = ValueObservation.tracking { db in
            try IssueDraftQueries.resolved(db: db).map(\.draft.teamId)
        }
        let draftsTask = Task { @MainActor in
            do {
                for try await teamIds in draftsObs.removeDuplicates().values(in: pool) {
                    draftTeamIds = teamIds
                }
            } catch {}
        }
        // EXP-1291: the session tree's lists (the Chats tab + its dot,
        // Home's Running band, the launcher and board switcher dots).
        sessionLists.bind(
            accountId: deps.auth.activeAccountId ?? "", userId: deps.auth.userId, db: deps.db
        )
        observationTasks = [wsTask, projTask, notifTask, draftsTask]
    }

    // MARK: - Current board (Issues tab)

    /// Every selectable board across all signed-in servers, as
    /// `accountId/boardId` keys. `MultiAccountBoardLoader` already limits
    /// this to boards of signed-in accounts, so key membership doubles as
    /// validity.
    private var availableBoardKeys: [String] {
        (boardLoader?.groups ?? []).flatMap { group in
            group.teamBlocks.flatMap { block in
                block.boards.map { "\(group.accountId)/\($0.id)" }
            }
        }
    }

    /// Resolution order: keep a still-valid selection → last-used board →
    /// first board of the first team (active account sorts first) →
    /// none (empty state, switcher disabled). A changed resolution also
    /// re-points the active team at the board's team (EXP-400).
    private func resolveCurrentBoard() {
        let available = Set(availableBoardKeys)
        if let current = currentBoard,
           available.contains("\(current.accountId)/\(current.boardId)") {
            return
        }
        // EXP-1373: the active account's remembered team wins over a
        // last-used board of another team (which would pull the shell back
        // to that board's team); a board-less one leaves no board. An
        // unknown team (teams not synced yet) counts until the teams
        // observation drops a stale one.
        if let accountId = deps.auth.activeAccountId,
           let teamId = SharedBoardMirror.readActiveTeam(accountId: accountId),
           teamState.teams.isEmpty || teamState.teams.contains(where: { $0.id == teamId }) {
            guard let boards = boardLoader?.groups.first(where: { $0.accountId == accountId })?
                .teamBlocks.first(where: { $0.team.id == teamId })?.boards else {
                currentBoard = nil
                return
            }
            let last = SharedBoardMirror.readLastUsed()
            if !(last?.accountId == accountId && boards.contains { $0.id == last?.boardId }),
               let board = boards.first {
                currentBoard = CurrentBoardRef(accountId: accountId, boardId: board.id)
                alignActiveTeam()
                return
            }
        }
        if let last = SharedBoardMirror.readLastUsed(),
           available.contains("\(last.accountId)/\(last.boardId)") {
            currentBoard = CurrentBoardRef(accountId: last.accountId, boardId: last.boardId)
            alignActiveTeam()
            return
        }
        if let group = boardLoader?.groups.first,
           let board = group.teamBlocks.first?.boards.first {
            currentBoard = CurrentBoardRef(accountId: group.accountId, boardId: board.id)
            alignActiveTeam()
            return
        }
        currentBoard = nil
    }

    private func selectBoard(accountId: String, boardId: String) {
        // Remember the choice so the Share Extension defaults its picker to it
        // and the next launch lands back in it.
        SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: boardId)
        if let teamId = boardLoader?.groups.first(where: { $0.accountId == accountId })?
            .teamBlocks.first(where: { $0.boards.contains { $0.id == boardId } })?.team.id {
            SharedBoardMirror.writeActiveTeam(accountId: accountId, teamId: teamId)
        }
        // EXP-1275 (D8 BLOCKING): a pick that switches ACCOUNT or TEAM covers
        // the window with the mark and one line while it lands.
        if accountId != (deps.auth.activeAccountId ?? "") {
            let account = deps.auth.accounts.first { $0.id == accountId }
            let name = account?.userEmail ?? account?.displayName ?? ""
            BlockingFeedback.shared.show(FeedbackStates.Blocking.switchAccount(name)) {
                applyBoardSelection(accountId: accountId, boardId: boardId)
            }
            return
        }
        if let teamId = teamState.boards.first(where: { $0.id == boardId })?.teamId,
           teamId != teamState.activeTeamId,
           let team = teamState.teams.first(where: { $0.id == teamId }) {
            BlockingFeedback.shared.show(FeedbackStates.Blocking.switchTeam(team.name)) {
                applyBoardSelection(accountId: accountId, boardId: boardId)
            }
            return
        }
        applyBoardSelection(accountId: accountId, boardId: boardId)
    }

    private func applyBoardSelection(accountId: String, boardId: String) {
        currentBoard = CurrentBoardRef(accountId: accountId, boardId: boardId)
        // EXP-400: the tab bar's team-scoped surfaces (Home's composer)
        // key off the active team — without a
        // re-point here a cross-team pick left them all serving the previous
        // team. A cross-server pick activates the picked account instead
        // (Android parity): `.id(activeAccountId)` recreates this navigator
        // and the fresh resolve lands on the just-written last-used board,
        // aligning the team on the way.
        if accountId != (deps.auth.activeAccountId ?? "") {
            deps.auth.switchAccount(id: accountId)
            return
        }
        alignActiveTeam()
    }

    /// Points the active team at the current board's team (EXP-400). Called
    /// only when the current board actually CHANGES — a team picked elsewhere
    /// (the team switcher) deliberately differs from the current board's, and
    /// that choice must survive unrelated re-renders and sync deltas.
    private func alignActiveTeam() {
        guard let current = currentBoard,
              current.accountId == (deps.auth.activeAccountId ?? "") else {
            // A foreign-account board can't be looked up in the active
            // account's pool — nothing to align (and nothing pending).
            pendingTeamAlign = false
            return
        }
        guard let teamId = teamState.boards.first(where: { $0.id == current.boardId })?.teamId else {
            // Boards not observed yet (cold start / fresh account switch) —
            // retry on the next boards emission.
            pendingTeamAlign = true
            return
        }
        pendingTeamAlign = false
        if teamState.activeTeamId != teamId {
            teamState.activeTeamId = teamId
        }
        SharedBoardMirror.writeActiveTeam(accountId: current.accountId, teamId: teamId)
    }

    /// Push the issue detail from a deep-link/push tap. Shared by both drain
    /// paths (EXP-172). No explicit sync kick: IssueDetailView observes GRDB
    /// live, and a just-created issue (e.g. a fresh widget submission) arrives over
    /// the running Electric long-poll — on a cold start the initial sync is
    /// already in flight by the time this route lands, so there is nothing
    /// useful to await here (initialSync only passively polls the active
    /// account's teams table; it starts no shape fetch).
    ///
    /// `replacingTop` swaps an issue page on top in place, so Back returns
    /// to where it was opened from.
    private func appendIssueRoute(
        accountId: String, issueId: String, replacingTop: Bool = false, commentId: String? = nil
    ) {
        // EXP-1373: ONE active team: a link into another team switches it
        // first, silently.
        // The local `issues` row has no `team_id`: resolve through its board.
        // #PERF-5: off main; the switch lands just after the push.
        Task { @MainActor in
            await activateTeam(accountId: accountId) { db in
                try String.fetchOne(db, sql: """
                    SELECT "b"."team_id" FROM "issues" AS "i"
                    JOIN "boards" AS "b" ON "b"."id" = "i"."board_id"
                    WHERE "i"."id" = ?
                    """, arguments: [issueId])
            }
        }
        let route: AppRoute = if let commentId {
            .issueComment(accountId: accountId, id: issueId, commentId: commentId)
        } else {
            .issue(accountId: accountId, id: issueId)
        }
        switch path.last {
        case .issue?, .issueComment?:
            if replacingTop {
                path[path.count - 1] = route
                return
            }
        default:
            break
        }
        path.append(route)
    }

    /// EXP-801: land on My Work's Inbox segment for the push's recipient —
    /// My Work renders the ACTIVE account, so a push for another signed-in
    /// account switches first (the issue routes carry their account instead).
    private func openInboxFromPush() {
        let (_, userId, pushTeamId) = deps.deepLinkBus.consumeInbox()
        let accountId = issueAccountId(forUserId: userId)
        if !accountId.isEmpty, accountId != deps.auth.activeAccountId {
            // EXP-1373: the rebuilt navigator opens on the row's team.
            if let pushTeamId {
                let block = boardLoader?.groups.first { $0.accountId == accountId }?
                    .teamBlocks.first { $0.team.id == pushTeamId }
                if let board = block?.boards.first {
                    SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: board.id)
                }
                SharedBoardMirror.writeActiveTeam(accountId: accountId, teamId: pushTeamId)
            }
            deps.auth.switchAccount(id: accountId)
        } else {
            // EXP-1373: the inbox lists the ACTIVE team's rows only, so the
            // tapped row's team becomes active first: the payload's, else
            // the newest unread row's (the one the push announced).
            // #PERF-5: off main; the switch lands just after the push.
            Task { @MainActor in
                await activateTeam(accountId: accountId) { db in
                    if let pushTeamId { return pushTeamId }
                    return try String.fetchOne(db, sql: """
                        SELECT COALESCE("n"."team_id", "b"."team_id") FROM "notifications" AS "n"
                        LEFT JOIN "issues" AS "i" ON "i"."id" = "n"."issue_id"
                        LEFT JOIN "boards" AS "b" ON "b"."id" = "i"."board_id"
                        WHERE "n"."read_at" IS NULL
                        ORDER BY "n"."created_at" DESC LIMIT 1
                        """)
                }
            }
        }
        // EXP-1373: the Inbox is a page pushed over the Chats tab.
        landOnRoot(of: .chats)
        path.append(.myWork)
    }

    /// #EXP-1534 (inbox-route.json): land on the Devices page (the Team
    /// tab's Devices row, the same page) and hand it the devices ROW id whose
    /// settings sheet it opens. A push for another signed-in account switches
    /// first and leaves the tap PENDING: `.id(activeAccountId)` recreates this
    /// navigator and its cold-launch `.task` drains it under that account.
    private func openDeviceFromLink() {
        let bus = deps.deepLinkBus
        guard let rowId = bus.pendingDeviceRowId else { return }
        let accountId = bus.pendingDeviceAccountId ?? issueAccountId(forUserId: bus.pendingDeviceUserId)
        if !accountId.isEmpty, accountId != deps.auth.activeAccountId {
            deps.auth.switchAccount(id: accountId)
            return
        }
        _ = bus.consumeDevice()
        bus.pendingDeviceSettingsRowId = rowId
        // Already on the Devices page: it opens the sheet in place.
        if case .teamSettingsSection(_, _, .devices)? = path.last { return }
        landOnRoot(of: .team)
        path.append(.teamSettingsSection(
            accountId: accountId, teamId: teamState.activeTeamId ?? "", section: .devices
        ))
    }

    /// #EXP-1638 (app-shortcuts.json): open what an app-icon shortcut names,
    /// through the openers the app already has: the tab bar's New-issue arm,
    /// its chat arm (the Agent tab's composer), the header's Search push, and
    /// a session push tap. A chat that is gone opens nothing (the start
    /// screen). New issue opens on the current board, else the active
    /// team's first (the page's board picker moves it); with no board synced
    /// yet it waits for the boards, at most `shortcutBoardWait`, then drops.
    private func openShortcut() {
        let bus = deps.deepLinkBus
        guard let target = bus.pendingShortcut else { return }
        if case .newIssue = target, shortcutBoard == nil {
            Task { @MainActor in
                try? await Task.sleep(for: Self.shortcutBoardWait)
                if bus.pendingShortcut == .newIssue { _ = bus.consumeShortcut() }
            }
            return
        }
        _ = bus.consumeShortcut()
        switch target {
        case .newIssue:
            guard let board = shortcutBoard else { return }
            path.append(.issueDraft(
                accountId: board.accountId,
                draftId: UUID().uuidString.lowercased(),
                boardId: board.boardId
            ))
        case .newChat:
            landOnRoot(of: .agent)
            bus.pendingComposerFocus = true
        case .search:
            path.append(.search)
        case let .chat(sessionId):
            let accountId = deps.auth.activeAccountId ?? ""
            Task { @MainActor in
                let exists = (try? await deps.db.pool(forAccountId: accountId).read { db in
                    try Bool.fetchOne(db, sql: "SELECT EXISTS(SELECT 1 FROM \"sessions\" WHERE \"id\" = ?)", arguments: [sessionId])
                }) ?? nil
                guard exists == true else { return }
                await activateTeam(forSession: sessionId, accountId: accountId)
                path.append(await AppRoute.openingInbox(sessionId, accountId: accountId, deps: deps))
            }
        }
    }

    private static let shortcutBoardWait: Duration = .seconds(15)

    /// The New issue shortcut's board: the tab bar's compose target, else the
    /// active team's first board, else the account's first.
    private var shortcutBoard: (accountId: String, boardId: String)? {
        if let composeTarget { return composeTarget }
        guard let accountId = deps.auth.activeAccountId else { return nil }
        let board = teamState.filteredBoards.first
            ?? teamState.boards.first { $0.teamId == teamState.activeTeamId }
            ?? teamState.boards.first
        return board.map { (accountId, $0.id) }
    }

    /// #EXP-1674: land where a link resolved to. Another signed-in account
    /// switches first and leaves the landing PENDING (`.id(activeAccountId)`
    /// recreates this navigator and its cold-launch `.task` drains it); a link
    /// into another team switches it silently, then opens.
    private func openLinkLanding() {
        let bus = deps.deepLinkBus
        guard let landing = bus.pendingLinkLanding else { return }
        if !landing.accountId.isEmpty, landing.accountId != deps.auth.activeAccountId {
            if let teamId = landing.teamId {
                SharedBoardMirror.writeActiveTeam(accountId: landing.accountId, teamId: teamId)
            }
            deps.auth.switchAccount(id: landing.accountId)
            return
        }
        _ = bus.consumeLinkLanding()
        if let teamId = landing.teamId, teamId != teamState.activeTeamId {
            if teamState.teams.contains(where: { $0.id == teamId }) {
                applyTeam(teamId)
            } else {
                // Cold launch: the teams observation opens on the remembered team.
                SharedBoardMirror.writeActiveTeam(accountId: landing.accountId, teamId: teamId)
            }
        }
        switch landing.destination {
        case .home:
            landOnRoot(of: .agent)
        case let .tab(target):
            landOnRoot(of: target)
        case let .board(boardId):
            SharedBoardMirror.writeLastUsed(accountId: landing.accountId, boardId: boardId)
            applyBoardSelection(accountId: landing.accountId, boardId: boardId)
            landOnRoot(of: .issues)
        case let .push(route, target):
            // #EXP-1674: a link from a running app pushes: back returns to it.
            if let target, !Self.opensOverRun(route, on: path) { landOnRoot(of: target) }
            // EXP-1288: a chat already on the stack is popped back to.
            if case let .chat(_, chatId, child) = route, let index = Self.chatIndex(chatId, in: path) {
                landOnChat(at: index, chatId: chatId, page: child)
            } else {
                path.append(route)
            }
        }
    }

    /// EXP-1373 (chats.json `shell`): the phone shows ONE active team; a
    /// push or link naming another team of the active account switches it
    /// first, silently (no prompt), re-pointing the board too.
    /// #PERF-5: the team reads off main (async).
    private func activateTeam(accountId: String, teamOf query: @escaping @Sendable (Database) throws -> String?) async {
        guard accountId == (deps.auth.activeAccountId ?? ""),
              let pool = try? deps.db.pool(forAccountId: accountId),
              let teamId = (try? await pool.read { try query($0) }) ?? nil,
              teamId != teamState.activeTeamId,
              teamState.teams.contains(where: { $0.id == teamId })
        else { return }
        applyTeam(teamId)
    }

    private func activateTeam(forSession sessionId: String, accountId: String) async {
        await activateTeam(accountId: accountId) { db in
            try String.fetchOne(db, sql: "SELECT \"team_id\" FROM \"sessions\" WHERE \"id\" = ?", arguments: [sessionId])
        }
    }

    /// The Team tab's pick: another server's team activates that account
    /// first (its navigator rebuilds on the team's board); this server's
    /// covers the window while the team lands (EXP-1275 D8).
    private func switchTeam(accountId: String, teamId: String) {
        if accountId != (deps.auth.activeAccountId ?? "") {
            let block = boardLoader?.groups.first { $0.accountId == accountId }?
                .teamBlocks.first { $0.team.id == teamId }
            if let board = block?.boards.first {
                SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: board.id)
            }
            // A pick on the Team tab lands on that team's Team page.
            Self.landOnTeamPage = tab == .team
            // The rebuilt navigator opens on this team, board or not.
            SharedBoardMirror.writeActiveTeam(accountId: accountId, teamId: teamId)
            let account = deps.auth.accounts.first { $0.id == accountId }
            BlockingFeedback.shared.show(
                FeedbackStates.Blocking.switchAccount(account?.userEmail ?? account?.displayName ?? "")
            ) {
                deps.auth.switchAccount(id: accountId)
            }
            return
        }
        guard teamId != teamState.activeTeamId,
              let team = teamState.teams.first(where: { $0.id == teamId }) else { return }
        BlockingFeedback.shared.show(FeedbackStates.Blocking.switchTeam(team.name)) {
            applyTeam(teamId)
        }
    }

    /// Points the active team (and the board in view) at `teamId`.
    private func applyTeam(_ teamId: String) {
        teamState.activeTeamId = teamId
        guard let accountId = deps.auth.activeAccountId else { return }
        // EXP-1373: remembered per account, so the pick survives relaunch
        // even when the team has no board to be the last-used one.
        SharedBoardMirror.writeActiveTeam(accountId: accountId, teamId: teamId)
        if let current = currentBoard,
           teamState.boards.first(where: { $0.id == current.boardId })?.teamId == teamId {
            return
        }
        let boards = boardLoader?.groups.first { $0.accountId == accountId }?
            .teamBlocks.first { $0.team.id == teamId }?.boards ?? []
        if let board = boards.first {
            currentBoard = CurrentBoardRef(accountId: accountId, boardId: board.id)
            SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: board.id)
        } else {
            // A board-less team: the Board tab must not keep showing (and
            // filing into) the previous team's board.
            currentBoard = nil
        }
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
        sessionLists.stop()
    }

    // Pushes carry the recipient's server user id: on a multi-account device
    // the tapped issue must open under the signed-in account that received
    // it — the active account's database may not contain the issue at all.
    // Plain URL links (no user id) keep the active-account behavior.
    private func issueAccountId(forUserId userId: String?) -> String {
        if let userId,
           let match = deps.auth.accounts.first(where: { $0.userId == userId && $0.token != nil }) {
            return match.id
        }
        return deps.auth.activeAccountId ?? ""
    }
}

extension Notification.Name {
    /// `exponential://github-connected` arrived — a GitHub App install just
    /// completed.
    static let githubConnected = Notification.Name("githubConnected")
    /// A sign-in method was linked outside the in-app auth sheet (EXP-1126)
    /// — the account's Sign-in methods section refetches.
    static let signInMethodsChanged = Notification.Name("signInMethodsChanged")
    /// A team was deleted in-app (EXP-43) — MainNavigator pops to root so
    /// no pushed view still targets the deleted team.
    static let teamDeleted = Notification.Name("teamDeleted")
}
