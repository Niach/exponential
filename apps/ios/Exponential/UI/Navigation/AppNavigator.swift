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
    @State private var path = NavigationPath()
    @State private var workspaceState = WorkspaceState()
    @State private var projectLoader: MultiAccountProjectLoader?
    @State private var observationTasks: [Task<Void, Never>] = []
    @State private var syncing = false

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
            if let issueId = deps.deepLinkBus.consume() {
                path.append(AppRoute.issue(accountId: deps.auth.activeAccountId ?? "", id: issueId))
            }
            if let token = deps.deepLinkBus.consumeInvite() {
                path.append(AppRoute.invite(token: token))
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) { syncBanner }
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
        observationTasks = [wsTask, projTask]
    }

    private func handleProjectTap(accountId: String, projectId: String) {
        // Remember the opened project so the Share Extension defaults its picker to it.
        SharedProjectMirror.writeLastUsed(accountId: accountId, projectId: projectId)
        path.append(AppRoute.project(accountId: accountId, id: projectId))
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
    }
}

extension Notification.Name {
    static let oauthTokenReceived = Notification.Name("oauthTokenReceived")
}
