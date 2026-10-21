import Combine
import ExpUI
import ExpCore
import SwiftUI
import GRDB

enum AppRoute: Hashable {
    /// Search (EXP-686): a pushed detail off the board header's search
    /// button — no longer a tab of its own.
    case search
    case agents
    /// NEW-2: one prompt's page in Settings → Prompts (`id` nil = New
    /// prompt) — pushed from a Prompts row or a `prompt` entity ref.
    case prompt(accountId: String, teamId: String, id: String?)
    /// NEW-2/NEW-3: a team agent's page = the CALLER's DM with it (the
    /// definition's id; the conversation is resolved — or, none yet, opened
    /// by the first message — on the page).
    case agentChannel(accountId: String, agentId: String)
    /// My Work (EXP-58): Inbox + My Issues merged behind one destination.
    case myWork
    /// EXP-1291: the Threads tab (it replaced Reviews) — the agents, then
    /// the thread roots Needs you · Running · Recent.
    case threads
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
    /// `threadId` (a root session's id) files it INTO that thread.
    case issueDraft(
        accountId: String,
        draftId: String,
        boardId: String,
        statusId: String? = nil,
        parentId: String? = nil,
        threadId: String? = nil
    )
    /// EXP-1291: ONE session's raw work (`AgentSessionScreen` → today's
    /// steer view) — a session card's tap, a blocked-session push. A pushed
    /// destination (EXP-221), so it gets the native back + swipe-back.
    case agentSession(accountId: String, sessionId: String)
    /// EXP-1291: a THREAD = a root session (`threadId` = its id): a thread,
    /// or a team agent's channel — `ThreadScreen`, its strip as the pager.
    /// `child` opens it on one page (nil = the Conversation).
    case thread(accountId: String, threadId: String, child: ThreadPage? = nil)
    /// EXP-825: Home's composer pushed with a preselection (a play button's
    /// `seed`); an empty seed IS the Home tab.
    case agent(accountId: String, seed: AgentComposerSeed)
    case settings
    case serverDetail(accountId: String)
    case teamSettings(accountId: String, teamId: String)
    case invite(token: String)
    case syncDebug
    /// About (EXP-262): the app's version surface, pushed from Settings →
    /// General. Third-party licences are one push further so the notice blob
    /// never weighs down the settings screen.
    case about
    case thirdPartyLicenses
}

/// EXP-1210: the bottom bar's destinations — ROOT siblings on phones. The
/// selected one IS the navigation stack's root, so switching swaps the root
/// (no push animation, no back chevron, no swipe-back) and the path holds
/// only the pushes made inside it. `agent` = the chat arm's Home, the
/// screen the app opens on. A tab's pushes are parked while another tab is
/// up and handed back on return, EXCEPT Home's: it always lands on its root
/// (Android parity: `popUpTo(agent-tab)` never restores it).
enum MainTab: Hashable {
    case agent, issues, inbox, devices, threads

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
        }
        .onChange(of: deps.auth.authenticatedAccountIds) { _, _ in
            deps.steerSessions.removeAll()
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
        .onPreferenceChange(ToastTopInsetKey.self) { height in
            MainActor.assumeIsolated { toastBannerHeight = height }
        }
        .environment(\.toaster, toaster)
        // EXP-1275 (D8 BLOCKING): ONE overlay over the whole window, above
        // the `.id(activeAccountId)` screens so an account switch or a
        // sign-out never tears it down mid-wait.
        .blockingOverlayHost()
        // EXP-1275: the styleguide suite's Feedback states screen (capture
        // only, `-uiTesting -uiTestingStyleguide feedback-states`).
        .fullScreenCover(isPresented: .constant(UITestingStyleguide.requested == "feedback-states")) {
            FeedbackStatesScreen()
        }
        // EXP-1259 D12-D14: the run-thread cards (capture only,
        // `-uiTestingStyleguide work-card|script-card|comment-call-card`).
        .fullScreenCover(isPresented: .constant(
            UITestingStyleguide.requested.map(ThreadCardsScreen.kinds.contains) == true
        )) {
            ThreadCardsScreen(kind: UITestingStyleguide.requested ?? "work-card")
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
        // exponential://invite/<token>
        if url.host == "invite", let token = url.pathComponents.dropFirst().first {
            deps.deepLinkBus.navigateToInvite(String(token))
        }
    }

    /// A universal link (EXP-92). Issue links resolve locally (identifier →
    /// synced issue id) under a signed-in account matching the URL's host;
    /// anything unresolvable falls back to the in-app Safari sheet.
    private func handleWebLink(_ url: URL) {
        switch WebLinks.parse(url) {
        case .invite(let token):
            deps.deepLinkBus.navigateToInvite(token)
        case .issue(let teamSlug, _, let identifier):
            resolveWebIssueLink(url: url, teamSlug: teamSlug, identifier: identifier)
        case .agent(let teamSlug):
            // EXP-825: Home under a signed-in account on the link's host; a
            // foreign host falls back to the Safari sheet.
            if let account = accountsOnHost(of: url).first {
                deps.deepLinkBus.navigateToAgent(teamSlug: teamSlug, accountId: account.id)
            } else {
                deps.deepLinkBus.openExternal(url)
            }
        case .thread(let teamSlug, let threadId):
            // EXP-1288: the Thread screen, the in-app `AppLink.thread` path.
            guard let holder = sessionLinkAccount(url: url, teamSlug: teamSlug, sessionId: threadId) else {
                deps.deepLinkBus.openExternal(url)
                return
            }
            deps.deepLinkBus.navigateToThread(threadId, accountId: holder.id)
        case .session(let teamSlug, let sessionId, let guide):
            // EXP-1291: a root → its thread (the Guide with `?view=guide`),
            // a child → its raw session.
            guard let holder = sessionLinkAccount(url: url, teamSlug: teamSlug, sessionId: sessionId) else {
                deps.deepLinkBus.openExternal(url)
                return
            }
            deps.deepLinkBus.navigateToSessionLink(sessionId, account: holder, guide: guide, db: deps.db)
        case nil:
            // Shouldn't happen (the AASA claims only the parsed shapes), but
            // never swallow a link the user tapped.
            deps.deepLinkBus.openExternal(url)
        }
    }

    /// Signed-in accounts on the link's instance — active account first,
    /// then most recently used (multi-account devices can hold several
    /// accounts on the same host).
    private func accountsOnHost(of url: URL) -> [ServerAccount] {
        let host = url.host
        let activeId = deps.auth.activeAccountId
        return deps.auth.accounts
            .filter { $0.token != nil && URL(string: $0.instanceUrl)?.host == host }
            .sorted { a, b in
                if a.id == activeId { return true }
                if b.id == activeId { return false }
                return a.lastUsedAt > b.lastUsedAt
            }
    }

    /// EXP-1288: a thread or session link opens under the signed-in account
    /// on the link's host that holds the session (else the one in the link's
    /// team, else the first); nil = a foreign host (the Safari sheet).
    private func sessionLinkAccount(url: URL, teamSlug: String, sessionId: String) -> ServerAccount? {
        let candidates = accountsOnHost(of: url)
        guard let first = candidates.first else { return nil }
        let store = deps.db
        func holds(_ account: ServerAccount, _ query: (Database) throws -> Bool) -> Bool {
            guard let pool = try? store.pool(forAccountId: account.id) else { return false }
            return (try? pool.read { try query($0) }) ?? false
        }
        return candidates.first { account in
            holds(account) { try SessionQueries.session(db: $0, id: sessionId) != nil }
        } ?? candidates.first { account in
            holds(account) { try TeamEntity.filter(Column("slug") == teamSlug).fetchCount($0) > 0 }
        } ?? first
    }

    private func resolveWebIssueLink(url: URL, teamSlug: String, identifier: String) {
        let candidates = accountsOnHost(of: url)
        guard !candidates.isEmpty else {
            deps.deepLinkBus.openExternal(url)
            return
        }
        let store = deps.db
        Task { @MainActor in
            func resolve() -> (issueId: String, accountId: String)? {
                for account in candidates {
                    if let issueId = IssueRefLookup.resolve(
                        identifier: identifier,
                        teamSlug: teamSlug,
                        db: store,
                        accountId: account.id
                    ) {
                        return (issueId, account.id)
                    }
                }
                return nil
            }
            if let hit = resolve() {
                deps.deepLinkBus.navigateToIssue(hit.issueId, accountId: hit.accountId)
                return
            }
            // Cold launch / brand-new issue: the row may simply not have
            // synced yet — one sync pass, then a bounded poll before giving
            // up. Opening the link activated the scene, so the wake kick has
            // just restarted the pipelines and a fresh row typically lands
            // within a couple of seconds. Worst case this adds ~4s before the
            // Safari bounce, on a path that was already failing.
            await deps.syncManager.initialSync()
            for _ in 0..<8 {
                if let hit = resolve() {
                    deps.deepLinkBus.navigateToIssue(hit.issueId, accountId: hit.accountId)
                    return
                }
                try? await Task.sleep(for: .milliseconds(500))
            }
            deps.deepLinkBus.openExternal(url)
        }
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
    /// EXP-1284 (T11/T-N9): lands a thread page already on the stack.
    @State private var threadLanding = ThreadLanding()
    /// EXP-1212: a New issue page with content holds every path change below.
    @State private var draftLeaveGuard = IssueDraftLeaveGuard()
    @State private var boardLoader: MultiAccountBoardLoader?
    @State private var observationTasks: [Task<Void, Never>] = []
    @State private var syncing = false
    @State private var unreadCount = 0
    /// EXP-1272: the Inbox tab's dot also lights while a draft resolves: the
    /// SAME rows the inbox's Drafts segment lists (account-wide, like it).
    /// The team id of every resolvable draft (one entry per draft); the
    /// Inbox dot counts the active team's (EXP-1272, web parity).
    @State private var draftTeamIds: [String] = []
    /// EXP-1291: the session tree's lists — ONE observation for the Threads
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
    var body: some View {
        ZStack {
            AppBackground()

            NavigationStack(path: $path) {
                tabRoot
                    // Every root states its bar visible explicitly — the
                    // roots swap inside ONE stack (EXP-1358: Threads wears the
                    // same inline bar as the others again).
                    .toolbar(.visible, for: .navigationBar)
                    .navigationDestination(for: AppRoute.self) { destination(for: $0) }
            }
        }
        .environment(teamState)
        .environment(sessionLists)
        .environment(tabBarChrome)
        // EXP-698 r5: the issue list's rows are plain Buttons now (a
        // NavigationLink in a List row draws the system disclosure OUTSIDE
        // the glass card), so they push through this instead.
        .environment(\.pushRoute, pushRouteAction)
        .environment(\.threadLanding, threadLanding)
        .environment(\.issueDraftLeaveGuard, draftLeaveGuard)
        .environment(\.accountId, deps.auth.activeAccountId ?? "")
        .onAppear {
            pushRouteAction.setHandler { route in
                // EXP-1210: a bar destination is never pushed — a link to one
                // (an entity chip, a getting-started card) switches to it.
                if let target = Self.tab(for: route) {
                    selectTab(target)
                } else if case let .thread(_, threadId, child) = route,
                          let index = Self.threadIndex(threadId, in: path) {
                    // EXP-1284 (T-N9): never a second page of the same
                    // thread — pop back to it and land the child there.
                    navigate { landOnThread(at: index, threadId: threadId, page: child) }
                } else {
                    navigate { path.append(route) }
                }
            }
            if boardLoader == nil {
                boardLoader = MultiAccountBoardLoader(auth: deps.auth, db: deps.db)
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
        // EXP-1075: the board switcher's per-team dots ride TeamState.
        .onChange(of: sessionLists.liveByTeam, initial: true) { _, live in
            teamState.liveRunsByTeam = live
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
        // A session_blocked push tap (EXP-980): open the SESSION that hit
        // the rate limit, under the recipient's account.
        .onChange(of: deps.deepLinkBus.pendingSessionId) { _, sessionId in
            if let sessionId {
                let accountId = issueAccountId(forUserId: deps.deepLinkBus.pendingSessionUserId)
                _ = deps.deepLinkBus.consumeSession()
                navigate {
                    path.append(AppRoute.agentSession(accountId: accountId, sessionId: sessionId))
                }
            }
        }
        // EXP-1284: a tapped thread link opens the thread (a root session).
        .onChange(of: deps.deepLinkBus.pendingThreadId) { _, threadId in
            if threadId != nil, let target = deps.deepLinkBus.consumeThread() {
                // EXP-1288: like `pushRoute` — a thread already on the stack
                // is popped back to, never a second page of it.
                navigate {
                    if let index = Self.threadIndex(target.threadId, in: path) {
                        landOnThread(at: index, threadId: target.threadId, page: target.page)
                    } else {
                        path.append(AppRoute.thread(
                            accountId: target.accountId, threadId: target.threadId, child: target.page
                        ))
                    }
                }
            }
        }
        // EXP-825: a `/t/{team}/agent` universal link.
        .onChange(of: deps.deepLinkBus.pendingAgentTeamSlug) { _, slug in
            if slug != nil {
                navigate({ openAgentFromLink() }, dropped: { _ = deps.deepLinkBus.consumeAgent() })
            }
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
            if let sessionId = deps.deepLinkBus.consumeSession() {
                let accountId = issueAccountId(forUserId: sessionUserId)
                path.append(AppRoute.agentSession(accountId: accountId, sessionId: sessionId))
            }
            if let target = deps.deepLinkBus.consumeThread() {
                path.append(AppRoute.thread(
                    accountId: target.accountId, threadId: target.threadId, child: target.page
                ))
            }
            if deps.deepLinkBus.pendingInbox { openInboxFromPush() }
            if deps.deepLinkBus.pendingAgentTeamSlug != nil { openAgentFromLink() }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            syncBanner
                // EXP-1031: the top-hung toast stack sits below the banner.
                .background(GeometryReader { proxy in
                    Color.clear.preference(key: ToastTopInsetKey.self, value: proxy.size.height)
                })
        }
        // Attached as an OVERLAY, not a safeAreaInset (EXP-36): an ancestor
        // inset outside the NavigationStack never reliably reaches the pushed
        // scrollables' content insets, so each bar-visible scrollable reserves
        // its own clearance via `.tabBarBottomInset()` instead — one source of
        // truth, no double-inset.
        .overlay(alignment: .bottom) {
            if showsTabBar && !tabBarChrome.suppressed {
                MobileTabBar(
                    agentActive: tab == .agent,
                    issuesActive: tab == .issues,
                    devicesActive: tab == .devices,
                    myWorkActive: tab == .inbox,
                    threadsActive: tab == .threads,
                    unreadCount: unreadCount,
                    hasDrafts: teamState.activeTeamId.map { draftTeamIds.contains($0) } ?? false,
                    threadsRunning: sessionLists.threadsRunning,
                    threadsNeedYou: sessionLists.needsYou,
                    threadsUnread: sessionLists.threadsUnread,
                    // The launcher capsule (chat | new issue) rides every
                    // bar-visible surface (EXP-827/EXP-973); only a team with
                    // no board leaves the New-issue arm inert.
                    composeEnabled: composeTarget != nil,
                    // EXP-1210: every entry SWITCHES the root (a re-tap pops
                    // back to it); EXP-1212: through the draft hold like
                    // every other path change. NEW-7: the Actions tab is
                    // gone (prompts = Settings → Prompts and the `/` menu);
                    // Settings is the Issues header's gear only.
                    onIssues: { selectTab(.issues) },
                    onDevices: { selectTab(.devices) },
                    onMyWork: { selectTab(.inbox) },
                    onThreads: { selectTab(.threads) },
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
        case .agents: return .devices
        case .myWork: return .inbox
        case .threads: return .threads
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
        case .inbox:
            MyWorkView()
                .environment(\.accountId, accountId)
        case .devices:
            AgentsView()
                .environment(\.accountId, accountId)
        case .threads:
            ThreadsView()
                .environment(\.accountId, accountId)
        }
    }

    /// Compose targets the board in view: a pushed board list wins, and every
    /// other bar-visible surface (Issues root, Devices, Inbox,
    /// Threads) falls back to the CURRENT board — the one the Issues
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

    /// EXP-1284: the LAST stack position showing `threadId`, if any.
    private static func threadIndex(_ threadId: String, in path: [AppRoute]) -> Int? {
        path.lastIndex { route in
            if case let .thread(_, id, _) = route { return id == threadId }
            return false
        }
    }

    /// EXP-1284 (T11/T-N9): pop everything above the thread page at `index`
    /// and land it on `page` (nil = its Conversation).
    private func landOnThread(at index: Int, threadId: String, page: ThreadPage?) {
        if index + 1 < path.count {
            path.removeSubrange((index + 1)...)
        }
        threadLanding.land(threadId: threadId, page: page ?? .conversation)
    }

    /// EXP-1284 (T11/T-N7): the New issue page filed into a thread — it
    /// leaves the stack (its OWN exit, never held), and the thread shows the
    /// new issue's tab: the page below when it is on the stack, else a fresh
    /// thread page.
    private func landAfterThreadCreate(accountId: String, threadId: String, issueId: String) {
        if case .issueDraft = path.last { path.removeLast() }
        if let index = Self.threadIndex(threadId, in: path) {
            landOnThread(at: index, threadId: threadId, page: .issue(issueId))
        } else {
            path.append(.thread(accountId: accountId, threadId: threadId, child: .issue(issueId)))
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
        case let .agentChannel(accountId, agentId):
            AgentChannelScreen(agentId: agentId)
                .id(agentId)
                .environment(\.accountId, accountId)
        case .myWork:
            MyWorkView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case .threads:
            ThreadsView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
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
        case let .issueDraft(accountId, draftId, boardId, statusId, parentId, threadId):
            IssueDraftPageView(
                draftId: draftId,
                boardId: boardId,
                statusId: statusId,
                parentId: parentId,
                threadId: threadId,
                onCreated: { createdId in
                    if let threadId {
                        // EXP-1284 (T11/T-N7): filed INTO a thread = back on
                        // that thread with the new issue's tab selected.
                        landAfterThreadCreate(accountId: accountId, threadId: threadId, issueId: createdId)
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
        case let .thread(accountId, threadId, child):
            // `.id(threadId)`: a root swapped in place rebuilds rather than
            // keeping the old one's state.
            ThreadScreen(rootId: threadId, initialPage: child ?? .conversation)
                .id(threadId)
                .environment(\.accountId, accountId)
        case let .agent(accountId, seed):
            // Always a pushed detail (a play button's seed): Home is the
            // stack's root (`tabRoot`, EXP-1210).
            AgentPageView(seed: seed)
                .environment(\.accountId, accountId)
        case .settings:
            SettingsView()
        case let .serverDetail(accountId):
            ServerDetailView(accountId: accountId)
        case let .teamSettings(accountId, teamId):
            TeamSettingsView(teamId: teamId)
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
                    if teamState.activeTeamId == nil, let first = ws.first {
                        teamState.activeTeamId = first.id
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
        let notifObs = ValueObservation.tracking { db -> ([NotificationEntity], Set<String>) in
            let notifications = try NotificationEntity.fetchAll(db)
            let issueIds = try Set(String.fetchAll(db, sql: "SELECT \"id\" FROM \"issues\""))
            return (notifications, issueIds)
        }
        let notifTask = Task { @MainActor in
            do {
                for try await (notifications, issueIds) in notifObs.values(in: pool) {
                    unreadCount = notifications.filter {
                        $0.readAt == nil && InboxViewModel.isRenderable($0, issueIds: issueIds)
                    }.count
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
        // EXP-1291: the session tree's lists (the Threads tab + its dot,
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

    /// EXP-801: land on My Work's Inbox segment for the push's recipient —
    /// My Work renders the ACTIVE account, so a push for another signed-in
    /// account switches first (the issue routes carry their account instead).
    private func openInboxFromPush() {
        let (_, userId) = deps.deepLinkBus.consumeInbox()
        let accountId = issueAccountId(forUserId: userId)
        if !accountId.isEmpty, accountId != deps.auth.activeAccountId {
            deps.auth.switchAccount(id: accountId)
        }
        // MyWorkView persists its segment in AppStorage — point it at Inbox.
        UserDefaults.standard.set("inbox", forKey: "myWorkSegment")
        landOnRoot(of: .inbox)
    }

    /// EXP-825: land on Home in the linked team — switch account first when
    /// the link's host matched another signed-in one (Home starts in the
    /// ACTIVE team), point the active team at the slug when it synced, and
    /// switch to the Home TAB (the link carries no seed).
    private func openAgentFromLink() {
        guard let slug = deps.deepLinkBus.pendingAgentTeamSlug,
              let accountId = deps.deepLinkBus.pendingAgentAccountId else { return }
        if accountId != deps.auth.activeAccountId {
            // Left PENDING on purpose: `.id(activeAccountId)` recreates this
            // navigator and its cold-launch `.task` drains the link under the
            // right account.
            deps.auth.switchAccount(id: accountId)
            return
        }
        _ = deps.deepLinkBus.consumeAgent()
        if let team = teamState.teams.first(where: { $0.slug == slug }) {
            teamState.activeTeamId = team.id
        }
        landOnRoot(of: .agent)
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
