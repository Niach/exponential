import ExpUI
import ExpCore
import SwiftUI
import GRDB

enum AppRoute: Hashable {
    case search
    case agents
    case inbox
    case project(accountId: String, id: String)
    case issue(accountId: String, id: String)
    case settings
    case serverDetail(accountId: String)
    case workspaceSettings(accountId: String, workspaceId: String)
    case invite(token: String)
    case syncDebug
}

/// The project the Issues tab is currently showing. May belong to a
/// non-active account — the switcher sheet spans every signed-in server.
struct CurrentProjectRef: Hashable {
    let accountId: String
    let projectId: String
}

struct AppNavigator: View {
    @Environment(AppDependencies.self) private var deps

    var body: some View {
        Group {
            if deps.auth.accounts.isEmpty {
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
                // backfills the flag for users who already have a project in a
                // non-public workspace, and OnboardingView re-reads the session
                // on appear so stale accounts dismiss themselves.
                OnboardingView()
                    .id(deps.auth.activeAccountId ?? "none")
            } else {
                MainNavigator()
                    .id(deps.auth.activeAccountId ?? "none")
            }
        }
        // URL handling lives at the ROOT view (mounted from first render), so a
        // cold launch via exp:// lands in the bus even before MainNavigator
        // exists; MainNavigator drains the bus when it appears.
        .onOpenURL { url in
            handleDeepLink(url)
        }
        .transaction { $0.animation = nil }
    }

    private func handleDeepLink(_ url: URL) {
        // exp://oauth-return#token=...
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
        // exp://issue/<issueId>
        if url.host == "issue", let issueId = url.pathComponents.dropFirst().first {
            deps.deepLinkBus.navigateToIssue(String(issueId))
        }
        // exp://invite/<token>
        if url.host == "invite", let token = url.pathComponents.dropFirst().first {
            deps.deepLinkBus.navigateToInvite(String(token))
        }
    }
}

struct MainNavigator: View {
    @Environment(AppDependencies.self) private var deps
    // Typed path (not NavigationPath) so the tab bar can inspect the top route.
    @State private var path: [AppRoute] = []
    @State private var workspaceState = WorkspaceState()
    @State private var projectLoader: MultiAccountProjectLoader?
    @State private var observationTasks: [Task<Void, Never>] = []
    @State private var syncing = false
    @State private var unreadCount = 0
    @State private var agentsRunning = false
    @State private var currentProject: CurrentProjectRef?
    @State private var composeTarget: ComposeTarget?

    private struct ComposeTarget: Identifiable {
        let accountId: String
        let projectId: String
        var id: String { "\(accountId)/\(projectId)" }
    }

    var body: some View {
        ZStack {
            AppBackground()

            NavigationStack(path: $path) {
                IssuesHomeView(
                    syncing: syncing,
                    currentProject: currentProject,
                    projectLoader: projectLoader,
                    onSelectProject: { accountId, projectId in
                        selectProject(accountId: accountId, projectId: projectId)
                    }
                )
                .navigationDestination(for: AppRoute.self) { destination(for: $0) }
            }
        }
        .environment(workspaceState)
        .environment(\.accountId, deps.auth.activeAccountId ?? "")
        .onAppear {
            if projectLoader == nil {
                projectLoader = MultiAccountProjectLoader(auth: deps.auth, db: deps.db)
            }
            startObserving()
            resolveCurrentProject()
            if workspaceState.workspaces.isEmpty {
                syncing = true
                Task {
                    await deps.syncManager.initialSync()
                    syncing = false
                }
            }
        }
        .onChange(of: deps.auth.accounts) { _, _ in
            projectLoader?.refresh()
        }
        // Any change to the available (signed-in, non-archived) projects
        // re-validates the Issues tab's current project.
        .onChange(of: availableProjectKeys) { _, _ in
            resolveCurrentProject()
        }
        .onDisappear { stopObserving() }
        .onChange(of: deps.deepLinkBus.pendingIssueId) { _, issueId in
            if let issueId {
                path.append(AppRoute.issue(accountId: deps.auth.activeAccountId ?? "", id: issueId))
                _ = deps.deepLinkBus.consume()
            }
        }
        .onChange(of: deps.deepLinkBus.pendingInviteToken) { _, token in
            if let token {
                path.append(AppRoute.invite(token: token))
                _ = deps.deepLinkBus.consumeInvite()
            }
        }
        // Drain links that arrived before this navigator mounted (cold launch).
        // The Issues tab already lands in the last-used project, so there is no
        // auto-push anymore — deep links are the only cold-launch navigation.
        .task {
            if let issueId = deps.deepLinkBus.consume() {
                path.append(AppRoute.issue(accountId: deps.auth.activeAccountId ?? "", id: issueId))
            }
            if let token = deps.deepLinkBus.consumeInvite() {
                path.append(AppRoute.invite(token: token))
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) { syncBanner }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if showsTabBar {
                MobileTabBar(
                    issuesActive: path.isEmpty,
                    searchActive: isOnSearch,
                    agentsActive: isOnAgents,
                    inboxActive: isOnInbox,
                    unreadCount: unreadCount,
                    agentsRunning: agentsRunning,
                    showsCompose: resolvedComposeTarget != nil,
                    onIssues: { path = [] },
                    onSearch: { if !isOnSearch { path = [.search] } },
                    onAgents: { if !isOnAgents { path = [.agents] } },
                    onInbox: { if !isOnInbox { path = [.inbox] } },
                    onCompose: { composeTarget = resolvedComposeTarget }
                )
            }
        }
        .sheet(item: $composeTarget) { target in
            CreateIssueSheet(projectId: target.projectId, onCreated: {})
                .environment(\.accountId, target.accountId)
                .presentationBackground(.ultraThinMaterial)
        }
    }

    // MARK: - Tab bar

    /// The bar floats only over the top-level surfaces (Issues root, Search,
    /// Agents, Inbox, pushed project lists); detail and settings screens get
    /// the full height back.
    private var showsTabBar: Bool {
        guard let top = path.last else { return true }
        switch top {
        case .search, .agents, .inbox, .project:
            return true
        default:
            return false
        }
    }

    private var isOnInbox: Bool {
        if case .inbox = path.last { return true }
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

    /// Compose targets the project in view: a pushed project list wins,
    /// otherwise the Issues tab root composes into its current project. The
    /// other surfaces (Search, Agents, Inbox) hide the button — creating an
    /// issue without a project context is ambiguous.
    private var resolvedComposeTarget: ComposeTarget? {
        if case let .project(accountId, id)? = path.last {
            return ComposeTarget(accountId: accountId, projectId: id)
        }
        if path.isEmpty, let current = currentProject {
            return ComposeTarget(accountId: current.accountId, projectId: current.projectId)
        }
        return nil
    }

    /// Thin status banner when live sync is degraded (offline / expired session).
    @ViewBuilder
    private var syncBanner: some View {
        let health = SyncDebug.shared.health
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
        case .inbox:
            InboxView()
                .environment(\.accountId, deps.auth.activeAccountId ?? "")
        case let .project(accountId, id):
            IssueListView(projectId: id)
                .environment(\.accountId, accountId)
        case let .issue(accountId, id):
            IssueDetailView(issueId: id)
                .environment(\.accountId, accountId)
        case .settings:
            SettingsView()
        case let .serverDetail(accountId):
            ServerDetailView(accountId: accountId)
        case let .workspaceSettings(accountId, workspaceId):
            WorkspaceSettingsView(workspaceId: workspaceId)
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
            try WorkspaceEntity.fetchAll(db)
        }
        let projObs = ValueObservation.tracking { db in
            try ProjectEntity.fetchAll(db)
        }

        let wsTask = Task { @MainActor in
            do {
                for try await ws in wsObs.values(in: pool) {
                    workspaceState.workspaces = ws
                    if workspaceState.activeWorkspaceId == nil, let first = ws.first {
                        workspaceState.activeWorkspaceId = first.id
                    }
                }
            } catch {}
        }
        let projTask = Task { @MainActor in
            do {
                for try await proj in projObs.values(in: pool) {
                    workspaceState.projects = proj
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
                    unreadCount = notifications.filter { $0.readAt == nil }.count
                }
            } catch {}
        }
        // Running coding sessions drive the Agents tab's green dot.
        let sessionObs = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("status") == DomainContract.codingSessionStatusRunning)
                .fetchAll(db)
        }
        let sessionTask = Task { @MainActor in
            do {
                for try await sessions in sessionObs.values(in: pool) {
                    agentsRunning = !sessions.isEmpty
                }
            } catch {}
        }
        observationTasks = [wsTask, projTask, notifTask, sessionTask]
    }

    // MARK: - Current project (Issues tab)

    /// Every selectable project across all signed-in servers, as
    /// `accountId/projectId` keys. `MultiAccountProjectLoader` already limits
    /// this to non-archived projects of signed-in accounts, so key membership
    /// doubles as validity.
    private var availableProjectKeys: [String] {
        (projectLoader?.groups ?? []).flatMap { group in
            group.workspaceBlocks.flatMap { block in
                block.projects.map { "\(group.accountId)/\($0.id)" }
            }
        }
    }

    /// Resolution order: keep a still-valid selection → last-used project →
    /// first project of the first workspace (active account sorts first) →
    /// none (empty state, switcher disabled).
    private func resolveCurrentProject() {
        let available = Set(availableProjectKeys)
        if let current = currentProject,
           available.contains("\(current.accountId)/\(current.projectId)") {
            return
        }
        if let last = SharedProjectMirror.readLastUsed(),
           available.contains("\(last.accountId)/\(last.projectId)") {
            currentProject = CurrentProjectRef(accountId: last.accountId, projectId: last.projectId)
            return
        }
        if let group = projectLoader?.groups.first,
           let project = group.workspaceBlocks.first?.projects.first {
            currentProject = CurrentProjectRef(accountId: group.accountId, projectId: project.id)
            return
        }
        currentProject = nil
    }

    private func selectProject(accountId: String, projectId: String) {
        // Remember the choice so the Share Extension defaults its picker to it
        // and the next launch lands back in it.
        SharedProjectMirror.writeLastUsed(accountId: accountId, projectId: projectId)
        currentProject = CurrentProjectRef(accountId: accountId, projectId: projectId)
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
    }
}

extension Notification.Name {
    static let oauthTokenReceived = Notification.Name("oauthTokenReceived")
}
