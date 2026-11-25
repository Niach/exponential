import CoreSpotlight
import Combine
import ExpUI
import ExpCore
import SwiftUI
import UserNotifications
import GRDB

enum AppRoute: Hashable {
    /// Search (EXP-686): a pushed detail off a root page's search button.
    case search
    /// #EXP-2109 (sidebar-tree.json `phone`): a team's settings page (its
    /// row's `…`).
    case teamSettings(accountId: String, teamId: String)
    /// The Devices page (Settings › Devices).
    case devices(accountId: String)
    /// NEW-2: one prompt's page in Settings → Prompts (`id` nil = New
    /// prompt) — pushed from a Prompts row or a `prompt` entity ref.
    case prompt(accountId: String, teamId: String, id: String?)
    /// #EXP-1661: one skill's read-only page in Settings › Skills (phones
    /// never create or edit a skill).
    case skill(accountId: String, teamId: String, id: String)
    /// NEW-2/NEW-3: a team agent's page = the CALLER's DM with it (the
    /// definition's id; the conversation is resolved — or, none yet, opened
    /// by the first message — on the page). #FEED-239: `settings` = the
    /// agent's settings opened over it (an agent row's `…` › settings).
    case agentChannel(accountId: String, agentId: String, settings: Bool = false)
    /// VAPP-110: one app's editor (`appId` = its row id). VAPP-242: only
    /// from a `…` › Edit app, New app and an import.
    case app(accountId: String, appId: String, screen: Apps.ScreenRequest? = nil)
    /// VAPP-242 (apps.json THE RUN VIEW): the app as a member uses it, its
    /// PUBLISHED package full screen (a list row tap); `screen` = an app
    /// screen link's screen.
    case appRun(accountId: String, appId: String, screen: Apps.ScreenRequest? = nil)
    /// #EXP-1617 / #EXP-1804 (ci-run.json): THE run page of our own CI (a
    /// check's `details_url`, `/t/<team>/ci/<runId>?job=`); `job` = the
    /// requested job (nil = the default one).
    case ciRun(accountId: String, runId: String, job: String? = nil)
    /// The Inbox page (EXP-1373: pushed from a root page's inbox circle,
    /// notifications only — My issues and Drafts moved to the board).
    case myWork
    /// EXP-1291: the Chats tab.
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
    /// #EXP-1943 (session-card.json `callView`): a call card's tap = THE
    /// call view of that call session (its summary, then its spoken turns).
    case callView(accountId: String, sessionId: String)
    /// EXP-1291: a CHAT = a root session (`chatId` = its id): a chat,
    /// or a team agent's channel — `ChatScreen`, its strip as the pager.
    /// `child` opens it on one page (nil = the Conversation; #EXP-1662
    /// `.plan(messageId)` = its plan tab on that plan).
    case chat(accountId: String, chatId: String, child: ChatPage? = nil)
    /// EXP-825: the composer page, with a play button's preselection (`seed`)
    /// or an empty seed (#EXP-2109: the New popup's Chat).
    case agent(accountId: String, seed: AgentComposerSeed)
    /// The Settings tab.
    case settings
    case serverDetail(accountId: String)
    /// NEW-67: the account's Settings → Voice page (`voice.json`).
    case voiceSettings(accountId: String)
    /// #EXP-1815: the account's Settings › Notifications (the source switches).
    case notificationSettings(accountId: String)
    /// #EXP-2109: the account's Settings › Security.
    case securitySettings(accountId: String)
    /// #EXP-1987: the account's Settings › Assistant (the viewer's ONE
    /// Assistant).
    case assistantSettings(accountId: String)
    /// #EXP-1381 (settings-nav.json): a team settings row → that category's
    /// OWN page.
    case teamSettingsSection(accountId: String, teamId: String, section: TeamSettingsSection)
    /// #VAPP-185: Settings › MCP › Activity (settings-mcp.json `activityPage`).
    case mcpActivity(accountId: String, teamId: String)
    /// #EXP-1616 (repo-page.json): Settings › Repositories › a repo's page.
    case repo(accountId: String, teamId: String, repositoryId: String, isOwner: Bool)
    case invite(token: String)
    case syncDebug
    /// About (EXP-262): the app's version surface, pushed from Settings →
    /// General. Third-party licences are one push further so the notice blob
    /// never weighs down the settings screen.
    case about
    case thirdPartyLicenses

    /// #EXP-2109 (chats.json `shell.pages`): the page this route is, if it
    /// is one of the bar's; nil = a detail: no bar.
    var phonePage: Chats.Shell.Page? {
        switch self {
        case .board: .board
        case .teamSettings, .teamSettingsSection, .prompt, .skill, .mcpActivity, .repo: .teamSettings
        case .devices: .devices
        case .serverDetail, .voiceSettings, .notificationSettings, .securitySettings, .assistantSettings,
             .syncDebug, .about, .thirdPartyLicenses: .settingsPage
        case .chats: .chats
        case .settings: .settings
        default: nil
        }
    }

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
        await openingNeedsYou(
            rootId: row.rootId, messageId: row.messageId, opens: row.opens,
            accountId: accountId, deps: deps
        )
    }

    /// #EXP-1830: a needs-you widget row (its link carries the row's ids).
    @MainActor static func openingNeedsYou(
        rootId: String, messageId: String, opens: NeedsYou.Opens, accountId: String,
        deps: AppDependencies
    ) async -> AppRoute {
        switch opens {
        case .plan: .chat(accountId: accountId, chatId: rootId, child: .plan(messageId))
        case .card: .chat(accountId: accountId, chatId: rootId, child: .card(messageId))
        case .results: .chat(accountId: accountId, chatId: rootId, child: .results)
        case .chat: await openingSession(rootId, accountId: accountId, deps: deps)
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

/// #EXP-2109 (chats.json `shell.tabs`): the bar's root pages — ROOT siblings
/// on phones (New is no page: it opens its popup). The selected one IS the
/// navigation stack's root, so switching swaps the root (no push animation,
/// no back chevron, no swipe-back) and the path holds only the pushes made
/// inside it. A tab's pushes are parked while another tab is up and handed
/// back on return. The app opens on Chats.
enum MainTab: Hashable {
    case chats, teams, assistant, settings

    var page: Chats.Shell.Page {
        switch self {
        case .chats: .chats
        case .teams: .teams
        case .assistant: .assistant
        case .settings: .settings
        }
    }

    /// nil = New.
    init?(_ tab: Chats.Shell.Tab) {
        switch tab {
        case .chats: self = .chats
        case .teams: self = .teams
        case .assistant: self = .assistant
        case .settings: self = .settings
        case .new: return nil
        }
    }
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
            HomeWidgetsPublisher.shared.refresh(auth: deps.auth, db: deps.db)
            deps.steerSessions.removeAll()
            // NEW-69: a call belongs to the account that started it.
            voiceCall.end()
        }
        .onChange(of: deps.auth.authenticatedAccountIds) { old, new in
            // #EXP-1830: the widgets follow the active account; signed out = their signed-out line.
            HomeWidgetsPublisher.shared.refresh(auth: deps.auth, db: deps.db)
            deps.steerSessions.removeAll()
            // #EDE-114: the chat stores follow the signed-in accounts (one no
            // longer signed in is closed, its files kept: an expiry; a
            // deliberate sign-out deleted them already).
            ChatStoreHub.shared.sync(accounts: AppDependencies.chatStoreAccounts(deps.auth))
            systemSearch?.refresh()
        }
        .task {
            HomeWidgetsPublisher.shared.refresh(auth: deps.auth, db: deps.db)
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
        // FEED-272: the cover is the Voice tab; the Chat tab is the agent's
        // chat itself (its header = the call's bar), opened once the cover
        // is gone when none shows.
        .background {
            let shown = voiceCall.view != nil && !voiceCall.minimized && voiceCall.tab == .voice
            Color.clear
                .fullScreenCover(
                    isPresented: Binding(get: { shown }, set: { if !$0, voiceCall.tab == .voice { voiceCall.minimize() } }),
                    onDismiss: {
                        Task { @MainActor in
                            await Task.yield()
                            if voiceCall.chatMissing, let agentId = voiceCall.agentId {
                                deps.deepLinkBus.navigateToLaunch(.shortcut(.agent(agentId)))
                            }
                        }
                    }
                ) {
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
        // #EXP-1830 (home-widgets.json TAPS): a widget's header or row —
        // exponential://issue/<id>, board/<id>, my-issues, needs-you, home,
        // session/<rootId>?opens=…&message=… — lands by THE back stack rule.
        if let launch = LaunchTarget(url: url) {
            deps.deepLinkBus.navigateToLaunch(launch)
            return
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
    /// app LANDS on Chats, so a cold start and an account switch
    /// (`.id(activeAccountId)` recreates this view) both open there.
    @State private var tab: MainTab = .chats
    // Typed path (not NavigationPath) so the tab bar can inspect the top route.
    // Only the pushes made INSIDE the current tab (issue, run, board list, the
    // New issue page…); a tab switch parks it in `savedPaths`.
    @State private var path: [AppRoute] = []
    /// Each OTHER tab's pushes, parked while it is not selected and handed
    /// back when it is (Android `restoreState` parity).
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
    /// EXP-1373: every unread notification the inbox can render: the page
    /// header's Inbox count reads them through `InboxRoute.inboxRowListed`
    /// (#EXP-1987), like the inbox's own list.
    @State private var unreadRows = UnreadRows()
    /// The account's resolvable drafts (the iPad sidebar's Drafts count).
    @State private var draftCount = 0
    /// EXP-1291: the session tree's lists — ONE observation for the Chats
    /// tab and its badge, the Teams pages and the composer.
    @State private var sessionLists = SessionListsModel()
    /// #EXP-2109: the Teams pages' tree (layout, agents, apps, owned teams).
    @State private var sidebarTree = SidebarTreeModel()
    /// The minute the nav badges read (a PR gets stuck with time).
    @State private var badgeNow = Date()
    /// The bar hides while the keyboard is up.
    @State private var keyboardVisible = false
    /// #EXP-2109: the bar's New popup is up.
    @State private var newMenuOpen = false
    /// A page of the Assistant's chat a link named before its tab mounted.
    @State private var assistantPage: ChatPage?
    /// #EXP-1674: every read-only markdown render's link tap = THE opener. One
    /// closure for the navigator's lifetime, like `pushRouteAction`.
    @State private var markdownLinks: MarkdownLinkHandler?
    /// A pick of another server's team rebuilds this navigator
    /// (`.id(activeAccountId)`); the new one lands on that team.
    @MainActor private static var landOnTeam: String?
    /// #NEW-12: regular width (iPad full screen, the ⅔ split) = the split
    /// layout (`TabletShell`); compact = the phone's.
    @Environment(\.horizontalSizeClass) private var sizeClass
    /// #NEW-12: the sidebar's open entry (follows `tab` on a landing).
    @State private var sidebar: SidebarItem = .chats
    /// #NEW-12: a list column row's open REPLACES the detail.
    @State private var listPushAction = PushRouteAction()
    /// #NEW-12: every sidebar pick (an overlaid sidebar folds on one).
    @State private var sidebarPicks = 0

    /// The size class mirrored into state: the push handlers are closures
    /// set once, and read it after the body that set them.
    @State private var splitActive = false

    private var isSplit: Bool { sizeClass == .regular }

    var body: some View {
        ZStack {
            AppBackground()

            if isSplit {
                tabletShell
            } else {
                NavigationStack(path: $path) {
                    tabRoot
                        .phoneDockInset(dockInset(for: tab.page))
                        // Every root states its bar visible explicitly — the
                        // roots swap inside ONE stack (EXP-1358: Chats wears the
                        // same inline bar as the others again).
                        .toolbar(tab == .assistant ? .hidden : .visible, for: .navigationBar)
                        // EXP-1554: every pushed page's back = the bare arrow;
                        // the draft page draws its own (the discard guard).
                        .navigationDestination(for: AppRoute.self) { route in
                            destination(for: route)
                                .phoneDockInset(dockInset(for: route.phonePage))
                                .bareBackButton(enabled: !route.isIssueDraft)
                        }
                }
            }
        }
        .background { if isSplit { tabletShortcuts } }
        .onChange(of: isSplit, initial: true) { _, split in splitActive = split }
        .onChange(of: tab) { _, target in
            // #NEW-12: a landing on a phone root lights its sidebar entry.
            newMenuOpen = false
            if sidebar.tab != target { sidebar = SidebarItem(target, assistantId: sessionLists.assistant?.agentId) }
        }
        .onChange(of: path) { _, _ in newMenuOpen = false }
        // #EXP-1815: the screen's thread (`willPresent` stays quiet on it).
        .onChange(of: viewingRoute, initial: true) { _, route in
            ViewingThread.shared.publish(route, deps: deps)
        }
        .environment(teamState)
        .environment(sessionLists)
        .environment(sidebarTree)
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
        // #FEED-232 (nav-badges.json `appIconCases`): the OS app icon = the
        // account's Needs you total; 0 clears it.
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                badgeNow = Date()
            }
        }
        .onChange(of: appIconBadgeCount, initial: true) { _, count in
            UNUserNotificationCenter.current().setBadgeCount(count)
        }
        // #EXP-1830: the Needs you widget's rows (every team), newest first.
        .onChange(of: widgetNeedsYou, initial: true) { _, rows in
            HomeWidgetsPublisher.shared.setNeedsYou(rows, accountId: deps.auth.activeAccountId)
        }
        .onAppear {
            if markdownLinks == nil {
                markdownLinks = LinkOpener.surfaceHandler(accountId: nil, baseURL: nil, teamId: nil, deps: deps)
            }
            if let teamId = Self.landOnTeam {
                Self.landOnTeam = nil
                tab = .teams
                sidebarTree.revealTeam = teamId
            }
            listPushAction.setHandler { route in
                if let item = sidebarItem(for: route) {
                    selectSidebar(item)
                } else {
                    navigate { path = [route] }
                }
            }
            pushRouteAction.setHandler { openRoute($0) }
            if boardLoader == nil {
                boardLoader = MultiAccountBoardLoader(auth: deps.auth, db: deps.db)
                shareTargets = ShareTargetsPublisher(auth: deps.auth, db: deps.db)
                shareTargets?.refresh()
            }
            startObserving()
            if teamState.teams.isEmpty {
                Task { await deps.syncManager.initialSync() }
            }
        }
        .onChange(of: deps.auth.accounts) { _, _ in
            boardLoader?.refresh()
            shareTargets?.refresh()
        }
        // Defense-in-depth against a split binding. `.id(activeAccountId)` on
        // this navigator (AppNavigator) normally recreates the whole view on an
        // account switch, resetting @State and re-running startObserving(). If
        // that recreation is ever skipped (e.g. an account activated while a
        // cover is presented), the environment accountId + tRPC re-reads flip to
        // the new account while these observations keep streaming the OLD
        // account's pool — the "wrong account's data" bug. Rebind explicitly:
        // cancel, clear state, re-observe the new active pool.
        .onChange(of: deps.auth.activeAccountId) { _, _ in
            stopObserving()
            teamState.teams = []
            teamState.boards = []
            startObserving()
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
                        openRoute(await AppRoute.openingInbox(sessionId, card: card, accountId: accountId, deps: deps))
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
        // #EXP-1638 / #EXP-1830: a pick from the app icon's long-press menu,
        // a widget tap, an App Intent.
        .onChange(of: deps.deepLinkBus.pendingLaunch) { _, target in
            if target != nil {
                navigate({ openLaunch() }, dropped: { _ = deps.deepLinkBus.consumeLaunch() })
            }
        }
        // A cold-launch New issue waits for the boards to arrive.
        .onChange(of: teamState.boards.map(\.id)) { _, _ in
            if deps.deepLinkBus.pendingLaunch == .shortcut(.newIssue) { openLaunch() }
        }
        // A team was deleted in-app (EXP-43): back to the Teams list so no
        // pushed view (its page, its settings) still targets it.
        .onReceive(NotificationCenter.default.publisher(for: .teamDeleted)) { _ in
            savedPaths = [:]
            tab = .teams
            path = []
        }
        // Drain links that arrived before this navigator mounted (cold launch).
        // They push ON TOP of Chats, where the app lands, so Back returns
        // there — deep links are the only cold-launch navigation.
        .task {
            if deps.deepLinkBus.pendingSessionId != nil || deps.deepLinkBus.pendingLinkLanding != nil
                || deps.deepLinkBus.pendingLaunch != nil {
                await awaitSessionLists()
            }
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
                openRoute(await AppRoute.openingInbox(sessionId, card: sessionCard, accountId: accountId, deps: deps))
            }
            if deps.deepLinkBus.pendingInbox { openInboxFromPush() }
            if deps.deepLinkBus.pendingDeviceRowId != nil { openDeviceFromLink() }
            if deps.deepLinkBus.pendingLinkLanding != nil { openLinkLanding() }
            if deps.deepLinkBus.pendingLaunch != nil { openLaunch() }
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
        // An OVERLAY over the stack; a page under the bar reserves its
        // clearance itself (`phoneDockInset`, inside the stack: an ancestor
        // inset never reliably reaches the pushed scrollables).
        .overlay(alignment: .bottom) {
            if showsDock, !tabBarChrome.suppressed {
                PhoneDock(
                    selected: litTab,
                    chatsBadge: sessionLists.chatsBadge,
                    teamsBadge: sidebarTree.teamsBadge(
                        teams: teamState.teams, teamState: teamState, lists: sessionLists,
                        me: deps.auth.userId, now: badgeNow
                    ),
                    assistantBadge: sessionLists.assistantBadge,
                    issueEnabled: composeTarget != nil,
                    newOpen: $newMenuOpen,
                    // EXP-1210: every entry SWITCHES the root (a re-tap pops
                    // back to it); EXP-1212: through the draft hold like
                    // every other path change.
                    onSelect: { tab in MainTab(tab).map(selectTab) },
                    onNew: { item in
                        switch item {
                        case .issue: newIssueAction?()
                        case .chat: openNewChat()
                        }
                    }
                )
                .ignoresSafeArea(.keyboard)
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

    // MARK: - Docked bar

    /// #EXP-2109 (`shell.pages`): the page on top; nil = a detail.
    private var topPage: Chats.Shell.Page? {
        guard let top = path.last else { return tab.page }
        return top.phonePage
    }

    /// The tab the page on top lights.
    private var litTab: Chats.Shell.Tab? { topPage?.tab }

    /// Only the listed pages wear the bar; it hides while the keyboard is up
    /// (the Assistant's composer, a settings field).
    private var showsDock: Bool {
        !isSplit && topPage?.bar == true && !keyboardVisible
    }

    private func dockInset(for page: Chats.Shell.Page?) -> Bool {
        page?.bar == true && !isSplit && !keyboardVisible && !tabBarChrome.suppressed
    }

    /// The New popup's Chat: the composer page, pushed (once).
    private func openNewChat() {
        guard let accountId = deps.auth.activeAccountId else { return }
        let route = AppRoute.agent(accountId: accountId, seed: .empty)
        guard path.last != route else { return }
        navigate { path.append(route) }
    }

    /// #EXP-1815: the screen on top (a tab root that is a chat, the split
    /// layout's chat / agent entry, when nothing is pushed over it).
    private var viewingRoute: AppRoute? {
        if let top = path.last { return top }
        guard let accountId = deps.auth.activeAccountId else { return nil }
        guard isSplit else {
            guard tab == .assistant, let assistant = sessionLists.assistant else { return nil }
            return .agentChannel(accountId: accountId, agentId: assistant.agentId)
        }
        switch sidebar {
        case let .chat(rootId): return .chat(accountId: accountId, chatId: rootId)
        case let .agentChannel(agentId): return .agentChannel(accountId: accountId, agentId: agentId)
        default: return nil
        }
    }

    /// #FEED-232: the page header's Inbox count: every team's unread rows.
    private var activeInboxUnreadCount: Int {
        unreadRows.rows.filter {
            InboxRoute.inboxRowListed(
                type: $0.type, rowTeamId: $0.teamId, issueId: $0.issueId, teamId: nil, teamIds: unreadRows.teamIds
            )
        }.count
    }

    /// #FEED-232: the app icon's number (`NavBadges.appIconBadge`): the
    /// signed-in account's Needs you total, waiting installs included.
    private var appIconBadgeCount: Int {
        NavBadges.appIconBadge([
            sessionLists.needsYouCounts(
                me: deps.auth.userId, now: badgeNow,
                installs: deps.installTrust.snapshot(deps.auth.activeAccountId ?? "").waitingRows
            ).total,
        ])
    }

    /// #EXP-1830 (home-widgets.json `needs-you`): THE Needs you rows over
    /// every team of the active account, newest first.
    private var widgetNeedsYou: [NeedsYou.Row] {
        sessionLists.needsYou(me: deps.auth.userId, now: badgeNow)
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
            assistantPage = nil
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

    /// Parks the current tab's pushes before a switch away from it.
    private func park() {
        savedPaths[tab] = path
    }

    /// The bar destination a route NAMES, if any — those routes never push
    /// (EXP-1210).
    private static func tab(for route: AppRoute) -> MainTab? {
        switch route {
        case .chats: .chats
        case .settings: .settings
        default: nil
        }
    }

    private struct AssistantLanding {
        let page: ChatPage?
    }

    /// #EXP-2109: the Assistant's conversation opens as its TAB, from
    /// anywhere; the page of it the route names (nil = its Conversation).
    private func assistantLanding(_ route: AppRoute) -> AssistantLanding? {
        guard !splitActive, let assistant = sessionLists.assistant else { return nil }
        switch route {
        case let .agentChannel(_, agentId, settings) where agentId == assistant.agentId && !settings:
            return AssistantLanding(page: nil)
        case let .chat(_, chatId, child) where chatId == sessionLists.agentConversations[assistant.agentId]:
            return AssistantLanding(page: child)
        default:
            return nil
        }
    }

    /// Opens a route: a bar destination (a link, an entity chip) and the
    /// Assistant's conversation switch to their tab, a chat already on the
    /// stack is popped back to (EXP-1284 T-N9), anything else pushes.
    private func openRoute(_ route: AppRoute) {
        // #NEW-12: the split layout's entries are sidebar picks.
        if splitActive, let item = sidebarItem(for: route) {
            selectSidebar(item)
        } else if let target = Self.tab(for: route) {
            selectTab(target)
        } else if let landing = assistantLanding(route) {
            navigate { landOnAssistant(landing.page) }
        } else if case let .chat(_, chatId, child) = route, let index = Self.chatIndex(chatId, in: path) {
            navigate { landOnChat(at: index, chatId: chatId, page: child) }
        } else {
            navigate { path.append(route) }
        }
    }

    /// A cold launch's target may be the Assistant: its tab is known once the
    /// lists' first emission landed (bounded: the target opens either way).
    private func awaitSessionLists() async {
        for _ in 0..<30 where !sessionLists.loaded {
            try? await Task.sleep(for: .milliseconds(100))
        }
    }

    private func landOnAssistant(_ page: ChatPage?) {
        assistantPage = page
        landOnRoot(of: .assistant)
        if let page, let rootId = sessionLists.assistant.flatMap({ sessionLists.agentConversations[$0.agentId] }) {
            chatLanding.land(chatId: rootId, page: page)
        }
    }

    /// The selected tab's screen — the navigation stack's ROOT.
    @ViewBuilder
    private var tabRoot: some View {
        let accountId = deps.auth.activeAccountId ?? ""
        switch tab {
        case .chats:
            ChatsView()
                .environment(\.accountId, accountId)
        case .teams:
            TeamsView(onSwitchAccount: { accountId, teamId in switchAccount(accountId, landingOn: teamId) })
                .environment(\.accountId, accountId)
        case .assistant:
            AssistantTabView(page: assistantPage)
                .environment(\.accountId, accountId)
                // A tab root: nothing to go back to.
                .environment(\.detailColumnRoot, true)
        case .settings:
            SettingsTabView()
                .environment(\.accountId, accountId)
        }
    }

    /// New issue's board: the board on screen, else the last used board, else
    /// the first board of the first team; nil = the viewer has no board.
    private var composeTarget: (accountId: String, boardId: String)? {
        guard let accountId = deps.auth.activeAccountId else { return nil }
        if case let .board(boardAccount, id)? = path.last { return (boardAccount, id) }
        if isSplit, case let .board(id) = sidebar { return (accountId, id) }
        if let last = SharedBoardMirror.readLastUsed(), last.accountId == accountId,
           teamState.boards.contains(where: { $0.id == last.boardId }) {
            return (accountId, last.boardId)
        }
        return teamState.firstBoard.map { (accountId, $0.id) }
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
                draftCount: draftCount,
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
                    ? AnyView(destination(for: route)
                        .bareBackButton(enabled: !route.isIssueDraft)
                        .environment(\.pageHeaderAsRoot, false))
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

    /// The New issue page (`composeTarget`).
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
        case .settings: .settings
        case let .agent(_, seed) where seed == .empty: .agent
        case .needsYou: .needsYou
        case .devices: .devices
        case let .board(_, id): .board(id)
        default: nil
        }
    }

    /// A sidebar pick: the entry's columns, the detail cleared (through the
    /// draft hold like every other path change).
    private func selectSidebar(_ item: SidebarItem) {
        navigate {
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
        case .settings:
            return AnyView(SettingsTabView().environment(\.accountId, accountId))
        case let .board(boardId):
            return AnyView(BoardPageView(boardId: boardId).id(boardId).environment(\.accountId, accountId))
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
        case .needsYou, .chats, .drafts, .settings, .board:
            return nil
        }
    }

    @ViewBuilder
    private func destination(for route: AppRoute) -> some View {
        switch route {
        case .search:
            SearchView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case let .teamSettings(accountId, teamId):
            TeamSettingsView(teamId: teamId)
                .id(teamId)
                .environment(\.accountId, accountId)
        case let .devices(accountId):
            AgentsView()
                .environment(\.accountId, accountId)
        case let .prompt(accountId, teamId, id):
            PromptSettingsPage(teamId: teamId, promptId: id)
                .environment(\.accountId, accountId)
        case let .skill(accountId, _, id):
            SkillSettingsPage(skillId: id)
                .environment(\.accountId, accountId)
        case let .agentChannel(accountId, agentId, settings):
            AgentChatRoute(agentId: agentId, settings: settings)
                .id(agentId)
                .environment(\.accountId, accountId)
        case let .app(accountId, appId, screen):
            AppEditorScreen(appId: appId, screen: screen)
                .id(appId)
                .environment(\.accountId, accountId)
        case let .appRun(accountId, appId, screen):
            AppRunScreen(appId: appId, screen: screen)
                .id(appId)
                .environment(\.accountId, accountId)
        case let .callView(accountId, sessionId):
            CallViewScreen(sessionId: sessionId)
                .id(sessionId)
                .environment(\.accountId, accountId)
        case let .ciRun(accountId, runId, job):
            CiRunScreen(runId: runId, requestedJob: job)
                .id(runId)
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
            BoardPageView(boardId: id)
                .id(id)
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
            AgentPageView(seed: seed)
                .environment(\.accountId, accountId)
        case .settings:
            SettingsTabView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case let .serverDetail(accountId):
            ServerDetailView(accountId: accountId)
        case let .voiceSettings(accountId):
            VoiceSettingsPage(accountId: accountId)
                .environment(\.accountId, accountId)
        case let .notificationSettings(accountId):
            NotificationSettingsPage(accountId: accountId)
                .environment(\.accountId, accountId)
        case let .securitySettings(accountId):
            SecuritySettingsPage(accountId: accountId)
                .environment(\.accountId, accountId)
        case let .assistantSettings(accountId):
            AssistantSettingsPage()
                .environment(\.accountId, accountId)
        case let .teamSettingsSection(accountId, teamId, section):
            TeamSettingsPage(teamId: teamId, section: section)
                .environment(\.accountId, accountId)
        case let .mcpActivity(accountId, teamId):
            McpActivityPage(teamId: teamId)
                .environment(\.accountId, accountId)
        case let .repo(accountId, teamId, repositoryId, isOwner):
            RepoPageView(teamId: teamId, repositoryId: repositoryId, isOwner: isOwner)
                .id(repositoryId)
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
                }
            } catch {}
        }
        let projTask = Task { @MainActor in
            do {
                for try await proj in projObs.removeDuplicates().values(in: pool) {
                    teamState.boards = proj
                }
            } catch {}
        }
        // Unread notifications drive the page header's inbox count. The synced issue
        // ids ride along because the dot must count ONLY rows the inbox can
        // render and clear (InboxViewModel.isRenderable, REV-15): delivered
        // notification rows outlive membership, so after leaving a team its
        // unread rows keep syncing while their issues drop out of the local
        // store — a raw count would light the dot forever over an inbox that
        // shows "All caught up".
        let notifObs = ValueObservation.tracking { db -> UnreadRows in
            // `InboxViewModel.isRenderable` in SQL: an issue row needs its
            // issue synced; issue-less rows are agent messages and blocked
            // runs. The team = the row's own, else its issue's board's.
            let rows = try Row.fetchAll(db, sql: """
                SELECT "n"."type" AS "type", "n"."issue_id" AS "issue_id",
                       COALESCE("n"."team_id", "b"."team_id") AS "team_id"
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
                ]).map {
                    UnreadRows.Row(type: $0["type"], teamId: $0["team_id"], issueId: $0["issue_id"])
                }
            return UnreadRows(rows: rows, teamIds: Set(try String.fetchAll(db, sql: #"SELECT "id" FROM "teams""#)))
        }
        let notifTask = Task { @MainActor in
            do {
                for try await next in notifObs.values(in: pool) {
                    unreadRows = next
                }
            } catch {}
        }
        let draftsObs = ValueObservation.tracking { db in
            try IssueDraftQueries.resolved(db: db).count
        }
        let draftsTask = Task { @MainActor in
            do {
                for try await count in draftsObs.removeDuplicates().values(in: pool) {
                    draftCount = count
                }
            } catch {}
        }
        // EXP-1291: the session tree's lists; #EXP-2109: the Teams pages'.
        sessionLists.bind(
            accountId: deps.auth.activeAccountId ?? "", userId: deps.auth.userId, db: deps.db
        )
        sidebarTree.bind(accountId: deps.auth.activeAccountId ?? "", userId: deps.auth.userId, db: deps.db)
        observationTasks = [wsTask, projTask, notifTask, draftsTask]
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

    /// EXP-801: land on the Inbox for the push's recipient — it renders the
    /// ACTIVE account, so a push for another signed-in account switches first
    /// and leaves the tap PENDING (`.id(activeAccountId)` recreates this
    /// navigator and its cold-launch `.task` drains it).
    private func openInboxFromPush() {
        let accountId = issueAccountId(forUserId: deps.deepLinkBus.pendingInboxUserId)
        if !accountId.isEmpty, accountId != deps.auth.activeAccountId {
            deps.auth.switchAccount(id: accountId)
            return
        }
        _ = deps.deepLinkBus.consumeInbox()
        // EXP-1373: the Inbox is a page pushed over the Chats tab.
        landOnRoot(of: .chats)
        path.append(.myWork)
    }

    /// #EXP-1534 (inbox-route.json): land on the Devices page (Settings ›
    /// Devices) and hand it the devices ROW id whose
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
        if case .devices? = path.last { return }
        landOnRoot(of: .settings)
        path.append(.devices(accountId: accountId))
    }

    /// #EXP-1638 / #EXP-1830 (app-shortcuts.json OPENING): open what an
    /// app-icon shortcut, a widget tap or an App Intent names, through the
    /// openers the app already has, with THE back stack rule: whatever was
    /// showing drops back to the Chats root, then the target is pushed
    /// (`launch`), so Back returns to Chats; the target already on
    /// top is not pushed twice. A chat that is gone opens the start screen.
    /// New issue opens on `composeTarget`
    /// (the page's board picker moves it); with no board synced yet it waits
    /// for the boards, at most `shortcutBoardWait`, then drops.
    private func openLaunch() {
        let bus = deps.deepLinkBus
        guard let target = bus.pendingLaunch else { return }
        if target == .shortcut(.newIssue), composeTarget == nil {
            Task { @MainActor in
                try? await Task.sleep(for: Self.shortcutBoardWait)
                if bus.pendingLaunch == .shortcut(.newIssue) { _ = bus.consumeLaunch() }
            }
            return
        }
        _ = bus.consumeLaunch()
        let accountId = deps.auth.activeAccountId ?? ""
        switch target {
        case .home:
            landOnRoot(of: .chats)
        case .shortcut(.newIssue):
            guard let board = composeTarget else { return }
            launch(.issueDraft(
                accountId: board.accountId,
                draftId: UUID().uuidString.lowercased(),
                boardId: board.boardId
            ))
        case .shortcut(.newChat):
            bus.pendingComposerFocus = true
            launch(.agent(accountId: accountId, seed: .empty))
        case .shortcut(.search):
            launch(.search)
        case let .shortcut(.chat(sessionId)):
            landOnRoot(of: .chats)
            Task { @MainActor in
                let exists = (try? await deps.db.pool(forAccountId: accountId).read { db in
                    try Bool.fetchOne(db, sql: "SELECT EXISTS(SELECT 1 FROM \"sessions\" WHERE \"id\" = ?)", arguments: [sessionId])
                }) ?? nil
                guard exists == true else { return }
                launch(await AppRoute.openingInbox(sessionId, accountId: accountId, deps: deps))
            }
        case let .shortcut(.agent(agentId)):
            launch(.agentChannel(accountId: accountId, agentId: agentId))
        case let .issue(issueId):
            landOnRoot(of: .chats)
            appendIssueRoute(accountId: accountId, issueId: issueId)
        case let .board(boardId):
            launch(.board(accountId: accountId, id: boardId))
        case .myIssues:
            // The last board's own chip.
            guard let board = composeTarget else {
                landOnRoot(of: .chats)
                return
            }
            bus.pendingMyIssues = true
            launch(.board(accountId: board.accountId, id: board.boardId))
        case .needsYou:
            launch(.needsYou(accountId: accountId))
        case let .needsYouRow(rootId, messageId, opens):
            landOnRoot(of: .chats)
            Task { @MainActor in
                launch(await AppRoute.openingNeedsYou(
                    rootId: rootId, messageId: messageId, opens: opens,
                    accountId: accountId, deps: deps
                ))
            }
        }
    }

    /// #EXP-1830 THE BACK STACK RULE: the Chats root under `route` (a board's
    /// is the Teams root; the Assistant's conversation is its tab); `route`
    /// already on top of it is left alone.
    private func launch(_ route: AppRoute) {
        if splitActive, let item = sidebarItem(for: route) {
            selectSidebar(item)
            return
        }
        if let landing = assistantLanding(route) {
            landOnAssistant(landing.page)
            return
        }
        let root: MainTab = if case .board = route { .teams } else { .chats }
        if tab == root, path == [route] { return }
        if case .issueDraft = route, tab == root, path.count == 1, path.first?.isIssueDraft == true { return }
        landOnRoot(of: root)
        path = [route]
    }

    private static let shortcutBoardWait: Duration = .seconds(15)

    /// #EXP-1674: land where a link resolved to. Another signed-in account
    /// switches first and leaves the landing PENDING (`.id(activeAccountId)`
    /// recreates this navigator and its cold-launch `.task` drains it).
    private func openLinkLanding() {
        let bus = deps.deepLinkBus
        guard let landing = bus.pendingLinkLanding else { return }
        if !landing.accountId.isEmpty, landing.accountId != deps.auth.activeAccountId {
            deps.auth.switchAccount(id: landing.accountId)
            return
        }
        _ = bus.consumeLinkLanding()
        switch landing.destination {
        case .home:
            landOnRoot(of: .chats)
        case let .tab(target):
            landOnRoot(of: target)
        case let .push(route, target):
            // #EXP-1674: a link from a running app pushes: back returns to it.
            if let target, !Self.opensOverRun(route, on: path) { landOnRoot(of: target) }
            // EXP-1288: a chat already on the stack is popped back to.
            openRoute(route)
        }
    }

    /// Another server's team: activates that account, whose navigator lands on
    /// the team's page.
    private func switchAccount(_ accountId: String, landingOn teamId: String) {
        Self.landOnTeam = teamId
        let account = deps.auth.accounts.first { $0.id == accountId }
        BlockingFeedback.shared.show(
            FeedbackStates.Blocking.switchAccount(account?.userEmail ?? account?.displayName ?? "")
        ) {
            deps.auth.switchAccount(id: accountId)
        }
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
        sessionLists.stop()
        sidebarTree.stop()
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

/// #EXP-1987: the unread rows the Inbox count reads (`inboxRowListed`'s
/// inputs) and the synced teams (never a personal team).
struct UnreadRows: Equatable, Sendable {
    struct Row: Equatable, Sendable {
        let type: String
        let teamId: String?
        let issueId: String?
    }

    var rows: [Row] = []
    var teamIds: Set<String> = []
}
