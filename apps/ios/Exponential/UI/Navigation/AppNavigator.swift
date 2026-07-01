import ExpUI
import ExpCore
import SwiftUI
import GRDB

enum AppRoute: Hashable {
    case home
    case inbox
    case project(accountId: String, id: String)
    case issue(accountId: String, id: String)
    case settings
    case serverDetail(accountId: String)
    case workspaceSettings(accountId: String, workspaceId: String)
    case integrations
    case invite(token: String)
    case syncDebug
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
                HomeView(
                    syncing: syncing,
                    onProjectTap: { accountId, projectId in
                        handleProjectTap(accountId: accountId, projectId: projectId)
                    },
                    projectLoader: projectLoader
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
        .task {
            var deepLinked = false
            if let issueId = deps.deepLinkBus.consume() {
                path.append(AppRoute.issue(accountId: deps.auth.activeAccountId ?? "", id: issueId))
                deepLinked = true
            }
            if let token = deps.deepLinkBus.consumeInvite() {
                path.append(AppRoute.invite(token: token))
                deepLinked = true
            }
            if !deepLinked {
                await openLastProjectIfAvailable()
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) { syncBanner }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if showsTabBar {
                MobileTabBar(
                    homeActive: path.isEmpty,
                    inboxActive: isOnInbox,
                    unreadCount: unreadCount,
                    showsCompose: resolvedComposeTarget != nil,
                    onHome: { path = [] },
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

    /// The bar floats only over the top-level surfaces (Home, Inbox, project
    /// lists); detail and settings screens get the full height back.
    private var showsTabBar: Bool {
        guard let top = path.last else { return true }
        if case .inbox = top { return true }
        if case .project = top { return true }
        return false
    }

    private var isOnInbox: Bool {
        if case .inbox = path.last { return true }
        return false
    }

    /// Compose targets the project being viewed, else the last-opened project.
    private var resolvedComposeTarget: ComposeTarget? {
        if case let .project(accountId, id)? = path.last {
            return ComposeTarget(accountId: accountId, projectId: id)
        }
        if let last = SharedProjectMirror.readLastUsed(),
           deps.auth.accounts.contains(where: { $0.id == last.accountId && $0.token != nil }) {
            return ComposeTarget(accountId: last.accountId, projectId: last.projectId)
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
        case .home:
            HomeView(
                syncing: syncing,
                onProjectTap: { accountId, projectId in
                    handleProjectTap(accountId: accountId, projectId: projectId)
                },
                projectLoader: projectLoader
            )
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
        case .integrations:
            IntegrationsView()
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
        observationTasks = [wsTask, projTask, notifTask]
    }

    private func handleProjectTap(accountId: String, projectId: String) {
        // Remember the opened project so the Share Extension defaults its picker
        // to it and a fresh launch can land back in it.
        SharedProjectMirror.writeLastUsed(accountId: accountId, projectId: projectId)
        path.append(AppRoute.project(accountId: accountId, id: projectId))
    }

    // Fresh starts land in the last-opened project, with Home left beneath in
    // the navigation stack. One-shot per process so account switches (which
    // remount MainNavigator via .id) don't re-trigger it; deep links win. The
    // stored project must belong to a signed-in account and still exist locally
    // un-archived — otherwise the launch falls back to Home.
    @MainActor private static var didAutoOpenLastProject = false

    @MainActor
    private func openLastProjectIfAvailable() async {
        guard !MainNavigator.didAutoOpenLastProject else { return }
        MainNavigator.didAutoOpenLastProject = true
        guard path.isEmpty,
              let last = SharedProjectMirror.readLastUsed(),
              let account = deps.auth.accounts.first(where: { $0.id == last.accountId }),
              account.token != nil,
              let pool = try? deps.db.pool(forAccountId: last.accountId)
        else { return }
        let projectId = last.projectId
        let project = try? await pool.read { db in
            try ProjectEntity.fetchAll(db).first { $0.id == projectId }
        }
        guard let project, project.archivedAt == nil else { return }
        path.append(AppRoute.project(accountId: last.accountId, id: projectId))
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
    }
}

extension Notification.Name {
    static let oauthTokenReceived = Notification.Name("oauthTokenReceived")
}
