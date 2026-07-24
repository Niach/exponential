import Combine
import ExpUI
import ExpCore
import SwiftUI
import GRDB

enum AppRoute: Hashable {
    case search
    case agents
    /// Team actions (EXP-253, view + run only) — pushed from the Agents tab's
    /// toolbar entry; NOT helpdesk-gated.
    case actions
    /// My Work (EXP-58): Inbox + My Issues merged behind one destination.
    /// Nothing external ever landed on the old inbox route — notification
    /// taps deep-link straight to the issue.
    case myWork
    /// Reviews (EXP-147): the open-PR list, its own tab beside My Work —
    /// no longer a segment inside it.
    case reviews
    /// Support (EXP-180): the team helpdesk inbox, its own tab — shown only
    /// while the active team's synced `helpdesk_enabled` flag is on.
    case support
    case board(accountId: String, id: String)
    case issue(accountId: String, id: String)
    /// One support ticket's conversation (EXP-180 helpdesk) — pushed from the
    /// My Work Support segment or a support_reply push tap.
    case supportThread(accountId: String, threadId: String)
    /// The dedicated per-issue diff page (EXP-34) — pushed from the issue
    /// detail's Changes card.
    case changes(accountId: String, issueId: String)
    /// The live agent-session (steering) screen — pushed from the Agents tab
    /// or the issue detail's coding card. A pushed destination (EXP-221), not
    /// a fullScreenCover, so it gets the native back button + swipe-back.
    case agentSession(accountId: String, sessionId: String)
    case settings
    case serverDetail(accountId: String)
    case teamSettings(accountId: String, teamId: String)
    case invite(token: String)
    case syncDebug
}

/// The board the Issues tab is currently showing. May belong to a
/// non-active account — the switcher sheet spans every signed-in server.
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

    private struct ExternalUrl: Identifiable {
        let url: URL
        var id: String { url.absoluteString }
    }

    var body: some View {
        Group {
            if UpdateGate.shared.upgrade != nil {
                // Client-version gate (EXP-104): the server 426'd this build.
                // Blocks the entire app ahead of every other state — sync loops
                // have already stopped; nothing below is reachable until the
                // user updates and relaunches.
                UpdateRequiredView()
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
    }

    private func handleDeepLink(_ url: URL) {
        // Universal links (EXP-92): https app.exponential.at issue/invite URLs
        // land here too (SwiftUI lifecycle delivers them to onOpenURL).
        if url.scheme == "https" || url.scheme == "http" {
            handleWebLink(url)
            return
        }
        // exponential://oauth-return#token=...
        if url.host == "oauth-return", let fragment = url.fragment {
            let params = fragment.split(separator: "&").reduce(into: [String: String]()) { dict, pair in
                let parts = pair.split(separator: "=", maxSplits: 1)
                if parts.count == 2 {
                    dict[String(parts[0])] = String(parts[1])
                }
            }
            if let token = params["token"] {
                NotificationCenter.default.post(name: .oauthTokenReceived, object: nil, userInfo: ["token": token])
            }
        }
        // exponential://github-connected — the GitHub App install flow finished (fired
        // by the server's post-install page). The in-app install surface
        // (ASWebAuthenticationSession) normally consumes this as its callback;
        // this path covers installs that finish in an external browser. The
        // repo picker listens and re-queries.
        if url.host == "github-connected" {
            NotificationCenter.default.post(name: .githubConnected, object: nil)
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
        case nil:
            // Shouldn't happen (the AASA claims only the two parsed shapes),
            // but never swallow a link the user tapped.
            deps.deepLinkBus.openExternal(url)
        }
    }

    private func resolveWebIssueLink(url: URL, teamSlug: String, identifier: String) {
        // Signed-in accounts on the link's instance — active account first,
        // then most recently used (multi-account devices can hold several
        // accounts on the same host).
        let host = url.host
        let candidates = deps.auth.accounts
            .filter { $0.token != nil && URL(string: $0.instanceUrl)?.host == host }
            .sorted { a, b in
                if a.id == deps.auth.activeAccountId { return true }
                if b.id == deps.auth.activeAccountId { return false }
                return a.lastUsedAt > b.lastUsedAt
            }
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
            // synced yet — one sync pass, then retry before giving up.
            await deps.syncManager.initialSync()
            if let hit = resolve() {
                deps.deepLinkBus.navigateToIssue(hit.issueId, accountId: hit.accountId)
            } else {
                deps.deepLinkBus.openExternal(url)
            }
        }
    }
}

struct MainNavigator: View {
    @Environment(AppDependencies.self) private var deps
    // Typed path (not NavigationPath) so the tab bar can inspect the top route.
    @State private var path: [AppRoute] = []
    @State private var teamState = TeamState()
    @State private var boardLoader: MultiAccountBoardLoader?
    @State private var observationTasks: [Task<Void, Never>] = []
    @State private var syncing = false
    @State private var unreadCount = 0
    // Raw observed notification rows — cached so the Support tab's unread dot
    // can recompute against the ACTIVE team when the selection changes
    // without a new sync delta.
    @State private var observedNotifications: [NotificationEntity] = []
    @State private var agentsRunning = false
    // EXP-214: any live session's desktop-written `needs_input` flag (agent
    // parked on a plan-approval / question picker) — escalates the Agents
    // dot to amber.
    @State private var agentsNeedInput = false
    // EXP-214: open-PR issues (non-archived) — the Reviews tab's green dot,
    // scoped to the active team via `reviewsOpen`.
    @State private var observedOpenPrIssues: [IssueEntity] = []
    // Raw observed running-session rows — cached so the liveness ticker can
    // recompute `agentsRunning` between sync deltas (EXP-153).
    @State private var observedSessions: [CodingSessionEntity] = []
    @State private var currentBoard: CurrentBoardRef?
    @State private var composeTarget: ComposeTarget?

    private struct ComposeTarget: Identifiable {
        let accountId: String
        let boardId: String
        var id: String { "\(accountId)/\(boardId)" }
    }

    var body: some View {
        ZStack {
            AppBackground()

            NavigationStack(path: $path) {
                IssuesHomeView(
                    syncing: syncing,
                    currentBoard: currentBoard,
                    boardLoader: boardLoader,
                    onSelectBoard: { accountId, boardId in
                        selectBoard(accountId: accountId, boardId: boardId)
                    }
                )
                .navigationDestination(for: AppRoute.self) { destination(for: $0) }
            }
        }
        .environment(teamState)
        .environment(\.accountId, deps.auth.activeAccountId ?? "")
        .onAppear {
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
        // Any change to the available (signed-in, non-archived) boards
        // re-validates the Issues tab's current board.
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
                appendIssueRoute(accountId: accountId, issueId: issueId)
                _ = deps.deepLinkBus.consume()
            }
        }
        .onChange(of: deps.deepLinkBus.pendingInviteToken) { _, token in
            if let token {
                path.append(AppRoute.invite(token: token))
                _ = deps.deepLinkBus.consumeInvite()
            }
        }
        // A support_reply push tap (EXP-180): open the ticket's conversation
        // under the recipient's account (helpdesk pushes carry no issue keys).
        .onChange(of: deps.deepLinkBus.pendingSupportThreadId) { _, threadId in
            if let threadId {
                let accountId = issueAccountId(forUserId: deps.deepLinkBus.pendingSupportThreadUserId)
                path.append(AppRoute.supportThread(accountId: accountId, threadId: threadId))
                _ = deps.deepLinkBus.consumeSupportThread()
            }
        }
        // A team was deleted in-app (EXP-43): pop to root so no pushed
        // view (team settings, server detail) still targets it.
        .onReceive(NotificationCenter.default.publisher(for: .teamDeleted)) { _ in
            path = []
        }
        // Drain links that arrived before this navigator mounted (cold launch).
        // The Issues tab already lands in the last-used board, so there is no
        // auto-push anymore — deep links are the only cold-launch navigation.
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
            let supportUserId = deps.deepLinkBus.pendingSupportThreadUserId
            if let threadId = deps.deepLinkBus.consumeSupportThread() {
                let accountId = issueAccountId(forUserId: supportUserId)
                path.append(AppRoute.supportThread(accountId: accountId, threadId: threadId))
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) { syncBanner }
        // Attached as an OVERLAY, not a safeAreaInset (EXP-36): an ancestor
        // inset outside the NavigationStack never reliably reaches the pushed
        // scrollables' content insets, so each bar-visible scrollable reserves
        // its own clearance via `.tabBarBottomInset()` instead — one source of
        // truth, no double-inset.
        .overlay(alignment: .bottom) {
            if showsTabBar {
                MobileTabBar(
                    issuesActive: path.isEmpty,
                    searchActive: isOnSearch,
                    agentsActive: isOnAgents,
                    myWorkActive: isOnMyWork,
                    reviewsActive: isOnReviews,
                    supportActive: isOnSupport,
                    unreadCount: unreadCount,
                    agentsRunning: agentsRunning,
                    agentsNeedInput: agentsNeedInput,
                    reviewsOpen: reviewsOpen,
                    showsSupport: helpdeskEnabled,
                    supportUnread: supportUnread,
                    showsCompose: resolvedComposeTarget != nil,
                    onIssues: { path = [] },
                    onSearch: { if !isOnSearch { path = [.search] } },
                    onAgents: { if !isOnAgents { path = [.agents] } },
                    onMyWork: { if !isOnMyWork { path = [.myWork] } },
                    onReviews: { if !isOnReviews { path = [.reviews] } },
                    onSupport: { if !isOnSupport { path = [.support] } },
                    onCompose: { composeTarget = resolvedComposeTarget }
                )
            }
        }
        // The Support tab exists only while the flag is on — if it flips off
        // (team switch, feature disabled) while the Support surface is up,
        // land back on Issues instead of stranding a tab-less screen.
        .onChange(of: helpdeskEnabled) { _, enabled in
            if !enabled {
                path.removeAll { $0 == .support }
            }
        }
        .sheet(item: $composeTarget) { target in
            CreateIssueSheet(boardId: target.boardId, onCreated: {})
                .environment(\.accountId, target.accountId)
                .presentationBackground(.ultraThinMaterial)
        }
    }

    // MARK: - Tab bar

    /// The bar floats only over the top-level surfaces (Issues root, Search,
    /// Agents, My Work, Reviews, pushed board lists); detail and settings
    /// screens get the full height back.
    private var showsTabBar: Bool {
        guard let top = path.last else { return true }
        switch top {
        case .search, .agents, .myWork, .reviews, .support, .board:
            return true
        default:
            return false
        }
    }

    /// Support (EXP-180) gets a tab only while the active team's synced
    /// `teams.helpdesk_enabled` flag is on.
    private var helpdeskEnabled: Bool {
        teamState.activeTeam?.helpdeskEnabled == true
    }

    /// Unread helpdesk activity in the ACTIVE team lights the Support tab's
    /// dot (EXP-182): issue-less support_reply rows carry a synced team_id —
    /// the same rule the inbox's per-team Support groups use.
    private var supportUnread: Bool {
        guard let teamId = teamState.activeTeamId else { return false }
        return observedNotifications.contains {
            $0.type == DomainContract.notificationTypeSupportReply
                && $0.issueId == nil
                && $0.teamId == teamId
                && $0.readAt == nil
        }
    }

    /// Any open PR in the ACTIVE team lights the Reviews tab's green dot
    /// (EXP-214) — the same open-PR set the Reviews screen lists, scoped
    /// through the already-observed boards like `supportUnread`.
    private var reviewsOpen: Bool {
        guard let teamId = teamState.activeTeamId else { return false }
        let teamBoardIds = Set(
            teamState.boards.filter { $0.teamId == teamId }.map(\.id)
        )
        return observedOpenPrIssues.contains { teamBoardIds.contains($0.boardId) }
    }

    private var isOnMyWork: Bool {
        if case .myWork = path.last { return true }
        return false
    }

    private var isOnSupport: Bool {
        if case .support = path.last { return true }
        return false
    }

    private var isOnReviews: Bool {
        if case .reviews = path.last { return true }
        return false
    }

    private var isOnSearch: Bool {
        if case .search = path.last { return true }
        return false
    }

    private var isOnAgents: Bool {
        if case .agents = path.last { return true }
        return false
    }

    /// Compose targets the board in view: a pushed board list wins,
    /// otherwise the Issues tab root composes into its current board. The
    /// other surfaces (Search, Agents, My Work, Reviews) hide the button —
    /// creating an issue without a board context is ambiguous.
    private var resolvedComposeTarget: ComposeTarget? {
        if case let .board(accountId, id)? = path.last {
            return ComposeTarget(accountId: accountId, boardId: id)
        }
        if path.isEmpty, let current = currentBoard {
            return ComposeTarget(accountId: current.accountId, boardId: current.boardId)
        }
        return nil
    }

    /// Thin status banner when live sync is degraded (offline / expired session).
    @ViewBuilder
    private var syncBanner: some View {
        // Only the ACTIVE account's health — a signed-out/failing OTHER account
        // must never flash the banner while the active account syncs fine.
        let health = SyncDebug.shared.health(forAccountId: deps.auth.activeAccountId)
        if health != .ok {
            HStack(spacing: 6) {
                Image(systemName: health == .unauthorized ? "person.crop.circle.badge.exclamationmark" : "wifi.slash")
                    .font(.caption2)
                Text(health == .unauthorized
                    ? "Session expired — sign in again to keep syncing"
                    : "Can't reach the server — showing cached data")
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
        case .myWork:
            MyWorkView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case .reviews:
            ReviewsView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case .support:
            SupportView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case let .board(accountId, id):
            IssueListView(boardId: id)
                .environment(\.accountId, accountId)
        case let .issue(accountId, id):
            IssueDetailView(issueId: id)
                .environment(\.accountId, accountId)
        case let .supportThread(accountId, threadId):
            SupportThreadView(threadId: threadId)
                .environment(\.accountId, accountId)
        case let .changes(accountId, issueId):
            ChangesView(issueId: issueId)
                .environment(\.accountId, accountId)
        case let .agentSession(accountId, sessionId):
            AgentSessionRouteView(sessionId: sessionId)
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
                }
            } catch {}
        }
        // Unread notifications drive the tab bar's inbox dot.
        let notifObs = ValueObservation.tracking { db in
            try NotificationEntity.fetchAll(db)
        }
        let notifTask = Task { @MainActor in
            do {
                for try await notifications in notifObs.values(in: pool) {
                    observedNotifications = notifications
                    unreadCount = notifications.filter { $0.readAt == nil }.count
                }
            } catch {}
        }
        // Live coding sessions drive the Agents tab's dot — running AND in_review
        // (the "agent finished, look at it" signal counts too, EXP-194).
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
                    observedSessions = sessions
                    // Heartbeat-stale rows don't light the dot (EXP-153).
                    agentsRunning = sessions.contains { CodingSessionLiveness.isLive($0) }
                    agentsNeedInput = sessions.contains {
                        CodingSessionLiveness.isLive($0) && $0.needsInput
                    }
                }
            } catch {}
        }
        // GRDB only re-fires on writes — a minute clock clears the dot once a
        // phantom row's liveness window elapses without any sync delta.
        let livenessTask = Task { @MainActor in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                guard !Task.isCancelled else { return }
                agentsRunning = observedSessions.contains { CodingSessionLiveness.isLive($0) }
                agentsNeedInput = observedSessions.contains {
                    CodingSessionLiveness.isLive($0) && $0.needsInput
                }
            }
        }
        // Open PRs light the Reviews tab's green dot (EXP-214) — mirrors the
        // Reviews screen's observation (open pr_state, non-archived).
        let openPrObs = ValueObservation.tracking { db in
            try IssueEntity
                .filter(Column("pr_state") == DomainContract.prStateOpen)
                .fetchAll(db)
        }
        let openPrTask = Task { @MainActor in
            do {
                for try await issues in openPrObs.values(in: pool) {
                    observedOpenPrIssues = issues.filter { $0.archivedAt == nil }
                }
            } catch {}
        }
        observationTasks = [wsTask, projTask, notifTask, sessionTask, livenessTask, openPrTask]
    }

    // MARK: - Current board (Issues tab)

    /// Every selectable board across all signed-in servers, as
    /// `accountId/boardId` keys. `MultiAccountBoardLoader` already limits
    /// this to non-archived boards of signed-in accounts, so key membership
    /// doubles as validity.
    private var availableBoardKeys: [String] {
        (boardLoader?.groups ?? []).flatMap { group in
            group.teamBlocks.flatMap { block in
                block.boards.map { "\(group.accountId)/\($0.id)" }
            }
        }
    }

    /// Resolution order: keep a still-valid selection → last-used board →
    /// first board of the first team (active account sorts first) →
    /// none (empty state, switcher disabled).
    private func resolveCurrentBoard() {
        let available = Set(availableBoardKeys)
        if let current = currentBoard,
           available.contains("\(current.accountId)/\(current.boardId)") {
            return
        }
        if let last = SharedBoardMirror.readLastUsed(),
           available.contains("\(last.accountId)/\(last.boardId)") {
            currentBoard = CurrentBoardRef(accountId: last.accountId, boardId: last.boardId)
            return
        }
        if let group = boardLoader?.groups.first,
           let board = group.teamBlocks.first?.boards.first {
            currentBoard = CurrentBoardRef(accountId: group.accountId, boardId: board.id)
            return
        }
        currentBoard = nil
    }

    private func selectBoard(accountId: String, boardId: String) {
        // Remember the choice so the Share Extension defaults its picker to it
        // and the next launch lands back in it.
        SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: boardId)
        currentBoard = CurrentBoardRef(accountId: accountId, boardId: boardId)
    }

    /// Push the issue detail from a deep-link/push tap. Shared by both drain
    /// paths (EXP-172). No explicit sync kick: IssueDetailView observes GRDB
    /// live, and a just-created issue (e.g. a fresh support ticket) arrives over
    /// the running Electric long-poll — on a cold start the initial sync is
    /// already in flight by the time this route lands, so there is nothing
    /// useful to await here (initialSync only passively polls the active
    /// account's teams table; it starts no shape fetch).
    private func appendIssueRoute(accountId: String, issueId: String) {
        path.append(AppRoute.issue(accountId: accountId, id: issueId))
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
    static let oauthTokenReceived = Notification.Name("oauthTokenReceived")
    /// `exponential://github-connected` arrived — a GitHub App install just
    /// completed.
    static let githubConnected = Notification.Name("githubConnected")
    /// A team was deleted in-app (EXP-43) — MainNavigator pops to root so
    /// no pushed view still targets the deleted team.
    static let teamDeleted = Notification.Name("teamDeleted")
}
