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
    /// Team actions (EXP-253) — its own tab since EXP-686.
    case actions
    /// SLOP-2: one action's page (Prompt · Triggers · Runs) — pushed from an
    /// Actions row or an `action` entity ref, opened on `tab`.
    case action(accountId: String, id: String, tab: ActionPageTab = .prompt)
    /// My Work (EXP-58): Inbox + My Issues merged behind one destination.
    /// Nothing external ever landed on the old inbox route — notification
    /// taps deep-link straight to the issue.
    case myWork
    /// Reviews (EXP-147): the open-PR list, its own tab beside My Work —
    /// no longer a segment inside it.
    case reviews
    case board(accountId: String, id: String)
    case issue(accountId: String, id: String)
    /// EXP-933: an issue's Work screen opened on a given face — an agent's
    /// targeted message (inbox row or push) lands on its Results.
    case issueFace(accountId: String, id: String, face: WorkFaceKind)
    /// EXP-1170: the New issue page — a DRAFT in the issue face's layout.
    /// Every opener mints `draftId` at TAP time (a fresh lowercase uuid; the
    /// Drafts list passes the row's own), so the route names the draft it
    /// autosaves to. `statusId` pre-picks a status; `parentId` files it as a
    /// sub-issue of that issue (the face's Sub-issues `+`, EXP-1097), which
    /// writes no draft row. Creating replaces this route with the issue it
    /// filed.
    case issueDraft(
        accountId: String,
        draftId: String,
        boardId: String,
        statusId: String? = nil,
        parentId: String? = nil
    )
    /// The live agent-session (steering) screen — pushed from the Agents tab
    /// or the issue detail's coding card. A pushed destination (EXP-221), not
    /// a fullScreenCover, so it gets the native back button + swipe-back.
    case agentSession(accountId: String, sessionId: String)
    /// EXP-1194: a run's OWN issue-less pull request (`RunChangesView`), what
    /// a Reviews → Agent runs row opens, like an issue row opens its issue's
    /// `.issueFace(…, face: .guide)`.
    case runChanges(accountId: String, sessionId: String)
    /// EXP-825: the team's Agent page — the ONE launcher (composer + the
    /// caller's Running/Recent sessions), a pushed detail. Every play button
    /// lands here with a `seed`; the Chat FAB with an empty one.
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
/// only the pushes made inside it. `agent` = the chat arm's Agent page, the
/// screen the app opens on. A tab's pushes are parked while another tab is
/// up and handed back on return, EXCEPT the Agent tab's: it always lands on
/// its root (Android parity: `popUpTo(agent-tab)` never restores it).
enum MainTab: Hashable {
    case agent, issues, inbox, devices, reviews, actions

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
            // EXP-825: the Agent page under a signed-in account on the link's
            // host; a foreign host falls back to the Safari sheet.
            if let account = accountsOnHost(of: url).first {
                deps.deepLinkBus.navigateToAgent(teamSlug: teamSlug, accountId: account.id)
            } else {
                deps.deepLinkBus.openExternal(url)
            }
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

    private func resolveWebIssueLink(url: URL, teamSlug: String, identifier: String) {
        let candidates = accountsOnHost(of: url)
        guard !candidates.isEmpty else {
            deps.deepLinkBus.openExternal(url)
            return
        }
        Task { @MainActor in
            func resolve() -> (issueId: String, accountId: String)? {
                for account in candidates {
                    if let issueId = IssueRefLookup.resolve(
                        identifier: identifier,
                        teamSlug: teamSlug,
                        db: deps.db,
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
    @Environment(\.scenePhase) private var scenePhase
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
    /// EXP-1212: a New issue page with content holds every path change below.
    @State private var draftLeaveGuard = IssueDraftLeaveGuard()
    @State private var boardLoader: MultiAccountBoardLoader?
    @State private var observationTasks: [Task<Void, Never>] = []
    @State private var syncing = false
    @State private var unreadCount = 0
    @State private var agentsRunning = false
    // EXP-214: any live session's desktop-written `needs_input` flag (agent
    // parked on a plan-approval / question picker) — escalates the Agents
    // dot to amber.
    @State private var agentsNeedInput = false
    // EXP-214/1244: the Reviews tab's dot inputs — the SAME rows the Reviews
    // screen feeds `ReviewsQueue.build`: every issue with an open pr_state or
    // any pr_url, every run with a pr_url (any state links its PR).
    @State private var observedPrIssues: [IssueEntity] = []
    @State private var observedPrSessions: [CodingSessionEntity] = []
    // Raw observed running-session rows — cached so the liveness ticker can
    // recompute `agentsRunning` between sync deltas (EXP-153).
    @State private var observedSessions: [CodingSessionEntity] = []
    @State private var currentBoard: CurrentBoardRef?
    // EXP-400: the current board resolved before the boards observation
    // delivered rows, so the active-team alignment couldn't look up its
    // team yet — re-run it on the next boards emission.
    @State private var pendingTeamAlign = false
    /// The Agent tab hides the floating bar while the keyboard is up.
    @State private var keyboardVisible = false
    var body: some View {
        ZStack {
            AppBackground()

            NavigationStack(path: $path) {
                tabRoot
                    .navigationDestination(for: AppRoute.self) { destination(for: $0) }
            }
        }
        .environment(teamState)
        .environment(tabBarChrome)
        // EXP-698 r5: the issue list's rows are plain Buttons now (a
        // NavigationLink in a List row draws the system disclosure OUTSIDE
        // the glass card), so they push through this instead.
        .environment(\.pushRoute, pushRouteAction)
        .environment(\.issueDraftLeaveGuard, draftLeaveGuard)
        .environment(\.accountId, deps.auth.activeAccountId ?? "")
        .onAppear {
            pushRouteAction.setHandler { route in
                // EXP-1210: a bar destination is never pushed — a link to one
                // (an entity chip, a getting-started card) switches to it.
                if let target = Self.tab(for: route) {
                    selectTab(target)
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
        // The Agents dots are team-scoped like the Reviews one,
        // but they're cached @State (the liveness ticker needs the raw rows)
        // rather than computed — so a team switch has to re-filter them here.
        .onChange(of: teamState.activeTeamId) { _, _ in
            recomputeAgentDots()
        }
        .onDisappear { stopObserving() }
        .onChange(of: deps.deepLinkBus.pendingIssueId) { _, issueId in
            if let issueId {
                // Universal links (EXP-92) resolve the account directly (URL
                // host match); push taps only know the recipient's userId.
                let accountId = deps.deepLinkBus.pendingIssueAccountId
                    ?? issueAccountId(forUserId: deps.deepLinkBus.pendingIssueUserId)
                let face = deps.deepLinkBus.pendingIssueFace
                let replacesTop = deps.deepLinkBus.pendingIssueReplacesTop
                _ = deps.deepLinkBus.consume()
                navigate {
                    appendIssueRoute(
                        accountId: accountId, issueId: issueId, face: face, replacingTop: replacesTop
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
        // A session_blocked push tap (EXP-980): open the RUN that hit the
        // rate limit, under the recipient's account.
        .onChange(of: deps.deepLinkBus.pendingSessionId) { _, sessionId in
            if let sessionId {
                let accountId = issueAccountId(forUserId: deps.deepLinkBus.pendingSessionUserId)
                _ = deps.deepLinkBus.consumeSession()
                navigate {
                    path.append(AppRoute.agentSession(accountId: accountId, sessionId: sessionId))
                }
            }
        }
        // EXP-825: a `/t/{team}/agent` universal link.
        .onChange(of: deps.deepLinkBus.pendingAgentTeamSlug) { _, slug in
            if slug != nil {
                navigate({ openAgentFromLink() }, dropped: { _ = deps.deepLinkBus.consumeAgent() })
            }
        }
        // A team was deleted in-app (EXP-43): back to the landing tab (Agent)
        // so no pushed view (team settings, server detail) still targets it.
        .onReceive(NotificationCenter.default.publisher(for: .teamDeleted)) { _ in
            savedPaths = [:]
            tab = .agent
            path = []
        }
        // Drain links that arrived before this navigator mounted (cold launch).
        // They push ON TOP of the Agent tab the app lands on, so Back returns
        // there — deep links are the only cold-launch navigation.
        .task {
            let pendingAccountId = deps.deepLinkBus.pendingIssueAccountId
            let userId = deps.deepLinkBus.pendingIssueUserId
            let face = deps.deepLinkBus.pendingIssueFace
            if let issueId = deps.deepLinkBus.consume() {
                let accountId = pendingAccountId ?? issueAccountId(forUserId: userId)
                appendIssueRoute(accountId: accountId, issueId: issueId, face: face)
            }
            if let token = deps.deepLinkBus.consumeInvite() {
                path.append(AppRoute.invite(token: token))
            }
            let sessionUserId = deps.deepLinkBus.pendingSessionUserId
            if let sessionId = deps.deepLinkBus.consumeSession() {
                let accountId = issueAccountId(forUserId: sessionUserId)
                path.append(AppRoute.agentSession(accountId: accountId, sessionId: sessionId))
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
                    actionsActive: tab == .actions,
                    myWorkActive: tab == .inbox,
                    reviewsActive: tab == .reviews,
                    unreadCount: unreadCount,
                    agentsRunning: agentsRunning,
                    agentsNeedInput: agentsNeedInput,
                    reviewsOpen: reviewsNav.dot,
                    showsReviews: reviewsNav.shows,
                    // The launcher capsule (chat | new issue) rides every
                    // bar-visible surface (EXP-827/EXP-973); only a team with
                    // no board leaves the New-issue arm inert.
                    composeEnabled: composeTarget != nil,
                    // EXP-1210: every entry SWITCHES the root (a re-tap pops
                    // back to it); EXP-1212: through the draft hold like
                    // every other path change. EXP-1187: Actions is a
                    // top-level tab; Settings is the Issues header's gear only.
                    onIssues: { selectTab(.issues) },
                    onDevices: { selectTab(.devices) },
                    onActions: { selectTab(.actions) },
                    onMyWork: { selectTab(.inbox) },
                    onReviews: { selectTab(.reviews) },
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
                    // EXP-825: the chat arm SWITCHES to the Agent root (the
                    // empty-seed page, a bar-visible root) — never a push.
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
        // EXP-1105: Reviews exists only until yolo mode hides it (the flag
        // flips on, or the last open PR in a yolo team merges) — then land
        // back on Issues instead of stranding a tab-less screen.
        // EXP-1244: the dot's openPulls half — a team older than 60 s
        // refetches when the team set changes and when the app comes back.
        .task(id: PullsNavKey(accountId: deps.auth.activeAccountId ?? "", teamIds: teamState.teams.map(\.id).sorted())) {
            await refreshStalePulls()
        }
        .onChange(of: scenePhase) { _, phase in
            guard phase == .active else { return }
            Task { await refreshStalePulls() }
        }
        .onChange(of: reviewsNav.shows) { _, shown in
            guard !shown else { return }
            savedPaths[.reviews] = nil
            if tab == .reviews {
                tab = .issues
                path = savedPaths.removeValue(forKey: .issues) ?? []
            }
        }
    }

    // MARK: - Tab bar

    /// The bar floats only over the tab ROOTS (EXP-1210) and pushed board
    /// lists; detail and settings screens — Search among them since EXP-686,
    /// and a SEEDED Agent page pushed by a play button — get the full height
    /// back. On the Agent root the bar also stands down while the keyboard is
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

    /// Parks the current tab's pushes before a switch away from it; the
    /// Agent tab drops them (it always lands on its root).
    private func park() {
        savedPaths[tab] = tab.keepsPushes ? path : nil
    }

    /// The bar destination a route NAMES, if any — those routes never push
    /// (EXP-1210); an empty-seed Agent page is the Agent root itself.
    private static func tab(for route: AppRoute) -> MainTab? {
        switch route {
        case .agents: return .devices
        case .actions: return .actions
        case .myWork: return .inbox
        case .reviews: return .reviews
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
        case .reviews:
            ReviewsView()
                .environment(\.accountId, accountId)
        case .actions:
            ActionsListView()
                .environment(\.accountId, accountId)
        }
    }

    /// EXP-1244: the Reviews tab's dot + presence = `ReviewsQueue.nav` over
    /// the SAME queue the Reviews screen lists (every member team, the
    /// app-wide openPulls store), fixture `_navDoc`: the dot = anything
    /// queued; the tab hides only while every team runs in yolo mode
    /// (EXP-1105: PRs auto-merge) and nothing is queued.
    private var reviewsNav: ReviewsQueue.Nav {
        let accountId = deps.auth.activeAccountId ?? ""
        let teamIds = teamState.teams.map(\.id)
        let queue = ReviewsQueue.build(
            teamIds: teamIds,
            boards: teamState.boards,
            issues: observedPrIssues,
            sessions: observedPrSessions,
            pulls: deps.openPulls.pulls(accountId: accountId, teamIds: teamIds)
        )
        return ReviewsQueue.nav(yolo: teamState.teams.map(\.yoloMode), count: queue.count)
    }

    private func refreshStalePulls() async {
        let teamIds = teamState.teams.map(\.id)
        guard let accountId = deps.auth.activeAccountId, !teamIds.isEmpty else { return }
        await deps.openPulls.refresh(
            accountId: accountId, teamIds: teamIds, api: deps.repositoriesApi
        )
    }

    /// Compose targets the board in view: a pushed board list wins, and every
    /// other bar-visible surface (Issues root, Devices, Actions, Inbox,
    /// Reviews) falls back to the CURRENT board — the one the Issues
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
        if case let .issueDraft(_, draftId, _, _, _)? = path.last {
            let held = IssueDraftLeaveGuard.Held(
                proceed: {
                    if case let .issueDraft(_, top, _, _, _)? = path.last, top == draftId {
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

    @ViewBuilder
    private var healthBanner: some View {
        // Only the ACTIVE account's health — a signed-out/failing OTHER account
        // must never flash the banner while the active account syncs fine.
        let health = SyncDebug.shared.health(forAccountId: deps.auth.activeAccountId)
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

    @ViewBuilder
    private func destination(for route: AppRoute) -> some View {
        switch route {
        case .search:
            SearchView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case .agents:
            AgentsView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case .actions:
            ActionsListView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case let .action(accountId, id, tab):
            ActionDetailView(actionId: id, initialTab: tab)
                .environment(\.accountId, accountId)
        case .myWork:
            MyWorkView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case .reviews:
            ReviewsView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case let .board(accountId, id):
            IssueListView(boardId: id)
                .environment(\.accountId, accountId)
        case let .issue(accountId, id):
            // EXP-893: the Work screen on its Issue face.
            WorkScreen(subject: .issue(id: id))
                .environment(\.accountId, accountId)
        case let .issueFace(accountId, id, face):
            WorkScreen(subject: .issue(id: id), initialFace: face)
                .environment(\.accountId, accountId)
        case let .issueDraft(accountId, draftId, boardId, statusId, parentId):
            IssueDraftPageView(
                draftId: draftId,
                boardId: boardId,
                statusId: statusId,
                parentId: parentId,
                onCreated: { createdId in
                    replaceTopRoute(with: .issue(accountId: accountId, id: createdId))
                },
                onClose: {
                    // Only THIS page: a link may have pushed over it.
                    if case let .issueDraft(_, top, _, _, _)? = path.last, top == draftId {
                        path.removeLast()
                    }
                }
            )
            .environment(\.accountId, accountId)
        case let .agentSession(accountId, sessionId):
            // EXP-893: the Work screen on its Run face.
            WorkScreen(subject: .session(id: sessionId))
                .environment(\.accountId, accountId)
        case let .runChanges(accountId, sessionId):
            RunChangesView(sessionId: sessionId)
                .environment(\.accountId, accountId)
        case let .agent(accountId, seed):
            // Always a pushed detail (a play button's seed): the Agent TAB is
            // the stack's root (`tabRoot`, EXP-1210).
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
                for try await ws in wsObs.values(in: pool) {
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
                for try await proj in projObs.values(in: pool) {
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
        // shows "You're all caught up".
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
        // Live coding sessions drive the Agents tab's dot — running AND in_review
        // (the "agent finished, look at it" signal counts too, EXP-194).
        // OWN sessions only, matching the owner-only Agents list: a teammate's
        // session must not light a dot over a screen that shows nothing.
        // Captured here because startObserving() re-runs on account switch.
        let ownUserId = deps.auth.userId
        let sessionObs = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter([
                    DomainContract.codingSessionStatusRunning,
                    DomainContract.codingSessionStatusInReview,
                ].contains(Column("status")))
                .fetchAll(db)
        }
        let sessionTask = Task { @MainActor in
            do {
                for try await sessions in sessionObs.values(in: pool) {
                    // Cached own-only (not team-filtered): a team switch
                    // re-filters these without waiting for a sync delta.
                    observedSessions = CodingSessionOwnership.own(sessions, userId: ownUserId)
                    recomputeAgentDots()
                }
            } catch {}
        }
        // GRDB only re-fires on writes — a minute clock clears the dot once a
        // phantom row's liveness window elapses without any sync delta.
        let livenessTask = Task { @MainActor in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                guard !Task.isCancelled else { return }
                recomputeAgentDots()
            }
        }
        // The Reviews tab's dot (EXP-214/1244) — the Reviews screen's own
        // observations: open issues are entries, every pr_url links a PR.
        let prIssueObs = ValueObservation.tracking { db in
            try IssueEntity
                .filter(Column("pr_state") == DomainContract.prStateOpen || Column("pr_url") != nil)
                .fetchAll(db)
        }
        let prIssueTask = Task { @MainActor in
            do {
                for try await issues in prIssueObs.values(in: pool) {
                    observedPrIssues = issues
                }
            } catch {}
        }
        // EXP-734: an action or chat run's own PR links no issue — it lives on
        // the session row; any run's pr_url links its PR (EXP-1244).
        let prSessionObs = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("pr_url") != nil)
                .fetchAll(db)
        }
        let prSessionTask = Task { @MainActor in
            do {
                for try await sessions in prSessionObs.values(in: pool) {
                    observedPrSessions = sessions
                }
            } catch {}
        }
        observationTasks = [
            wsTask, projTask, notifTask, sessionTask, livenessTask, prIssueTask,
            prSessionTask,
        ]
    }

    /// The Agents tab's dots, from the cached own sessions: live in ANY
    /// member team (EXP-1186) — the Agent page lists every team's runs now,
    /// so the dot counts what it shows; a teammate's run never lights it.
    /// Heartbeat-stale rows don't light it either (EXP-153). Re-run on every
    /// session emission, on the liveness tick, and whenever the active team
    /// changes.
    private func recomputeAgentDots() {
        let mine = observedSessions.filter {
            CodingSessionOwnership.isOwn($0, userId: deps.auth.userId)
        }
        agentsRunning = mine.contains { CodingSessionLiveness.isLive($0) }
        // EXP-1184: the display rule — needs input wins on every live status
        // (an open PR included), so the amber dot means "a live agent wants
        // you".
        agentsNeedInput = mine.contains {
            CodingSessionLiveness.isLive($0)
                && CodingSessionDisplayState.of(session: $0, prState: nil) == .needsInput
        }
        // EXP-1075: the same live/own rule per team — the board switcher's
        // per-team dot. `observedSessions` is already own-only; liveByTeam
        // filters again anyway.
        teamState.liveRunsByTeam = CodingSessionOwnership.liveByTeam(
            observedSessions, userId: deps.auth.userId
        )
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
        currentBoard = CurrentBoardRef(accountId: accountId, boardId: boardId)
        // EXP-400: the tab bar's team-scoped surfaces (Reviews, the
        // Agents start pool, Actions) key off the active team — without a
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
    /// `replacingTop` (a Stack card member tap) swaps an issue page on top in
    /// place, so Back returns to where the stack was opened from.
    private func appendIssueRoute(
        accountId: String, issueId: String, face: WorkFaceKind = .issue, replacingTop: Bool = false
    ) {
        let route = face == .issue
            ? AppRoute.issue(accountId: accountId, id: issueId)
            : AppRoute.issueFace(accountId: accountId, id: issueId, face: face)
        switch path.last {
        case .issue?, .issueFace?:
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

    /// EXP-825: land on the linked team's Agent page — switch account first
    /// when the link's host matched another signed-in one (the page renders
    /// the ACTIVE team), point the active team at the slug when it synced,
    /// and switch to the Agent TAB (the link carries no seed).
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

/// EXP-1244: the tab bar's openPulls refresh key — the account + team set.
private struct PullsNavKey: Hashable {
    let accountId: String
    let teamIds: [String]
}
