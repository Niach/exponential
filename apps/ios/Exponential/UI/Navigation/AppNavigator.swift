import SwiftUI
import GRDB

enum AppRoute: Hashable {
    case home
    case project(id: String)
    case issue(id: String)
    case settings
    case serverDetail(accountId: String)
    case workspaceSettings(workspaceId: String)
    case integrations
    case adminUsers
    case adminWorkspaces
    case invite(token: String)
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
        .transaction { $0.animation = nil }
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
                // Per-account DB: an empty workspace table on mount means we haven't
                // synced this server yet. Show a loading indicator while shapes catch up.
                syncing = true
                Task {
                    await deps.syncManager.initialSync()
                    syncing = false
                }
            }
            // Consume any cross-server project-tap target left for us by the
            // previous MainNavigator instance (before this account-switch
            // rebuild). Push the project route now that the new account's
            // pool is live.
            if let pendingProjectId = workspaceState.pendingProjectIdAfterSwitch {
                workspaceState.pendingProjectIdAfterSwitch = nil
                path.append(AppRoute.project(id: pendingProjectId))
            }
            // Same handoff for Settings → Workspaces → cross-server tap.
            if let pendingWorkspaceSettingsId = workspaceState.pendingWorkspaceSettingsIdAfterSwitch {
                workspaceState.pendingWorkspaceSettingsIdAfterSwitch = nil
                path.append(AppRoute.settings)
                path.append(AppRoute.workspaceSettings(workspaceId: pendingWorkspaceSettingsId))
            }
        }
        .onChange(of: deps.auth.accounts) { _, _ in
            projectLoader?.refresh()
        }
        .onDisappear { stopObserving() }
        .onOpenURL { url in
            handleDeepLink(url)
        }
        .onChange(of: deps.deepLinkBus.pendingIssueId) { _, issueId in
            if let issueId {
                path.append(AppRoute.issue(id: issueId))
                _ = deps.deepLinkBus.consume()
            }
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
        case let .project(id):
            IssueListView(projectId: id)
        case let .issue(id):
            IssueDetailView(issueId: id)
        case .settings:
            SettingsView()
        case let .serverDetail(accountId):
            ServerDetailView(accountId: accountId)
        case let .workspaceSettings(workspaceId):
            WorkspaceSettingsView(workspaceId: workspaceId)
        case .integrations:
            IntegrationsView()
        case .adminUsers:
            AdminUsersView()
        case .adminWorkspaces:
            AdminWorkspacesView()
        case let .invite(token):
            InviteAcceptView(token: token)
        }
    }

    private func handleDeepLink(_ url: URL) {
        // Handle exp://oauth-return#token=...
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
        // Handle exp://issue/<issueId>
        if url.host == "issue", let issueId = url.pathComponents.dropFirst().first {
            path.append(AppRoute.issue(id: String(issueId)))
        }
        // Handle exp://invite/<token>
        if url.host == "invite", let token = url.pathComponents.dropFirst().first {
            path.append(AppRoute.invite(token: String(token)))
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
        if accountId == deps.auth.activeAccountId {
            // Same-server tap: just navigate.
            path.append(AppRoute.project(id: projectId))
        } else {
            // Cross-server tap: stash the project id, swap pool + active
            // account, and let the rebuilt MainNavigator's onAppear push the
            // route once it's mounted under the new .id(activeAccountId).
            workspaceState.pendingProjectIdAfterSwitch = projectId
            try? deps.db.pool(forAccountId: accountId)
            deps.auth.switchAccount(id: accountId)
        }
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
    }
}

extension Notification.Name {
    static let oauthTokenReceived = Notification.Name("oauthTokenReceived")
}
