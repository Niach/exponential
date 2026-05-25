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
            if !deps.auth.hasInstance {
                InstanceView(showCancel: deps.auth.isAddingServer) {
                    deps.auth.cancelAddServer()
                }
            } else if !deps.auth.isAuthenticated {
                LoginView()
            } else {
                // Keying off the active account id forces SwiftUI to tear down and
                // rebuild MainNavigator (and its GRDB ValueObservations) when the
                // user switches between accounts. The DB pool itself is swapped by
                // SyncManager before this id changes.
                MainNavigator()
                    .id(deps.auth.activeAccountId ?? "none")
            }
        }
        .transaction { $0.animation = nil } // Prevent auth transitions from affecting child navigation
    }
}

struct MainNavigator: View {
    @Environment(AppDependencies.self) private var deps
    @State private var path = NavigationPath()
    @State private var workspaceState = WorkspaceState()
    @State private var workspaceLoader: MultiAccountWorkspaceLoader?
    @State private var projectLoader: MultiAccountProjectLoader?
    @State private var showWorkspaceSwitcher = false
    @State private var observationTask: Task<Void, Never>?
    @State private var syncing = false

    var workspaceSheetHeight: CGFloat {
        let header: CGFloat = 56
        let rowHeight: CGFloat = 44
        let groupHeader: CGFloat = 36
        let bottomPadding: CGFloat = 24
        let groups = workspaceLoader?.groups ?? []
        let rowCount = groups.reduce(0) { $0 + $1.workspaces.count }
        let groupCount = groups.count
        let estimated = header + CGFloat(groupCount) * groupHeader + CGFloat(max(rowCount, 1)) * rowHeight + bottomPadding
        return min(estimated, 480)
    }

    var body: some View {
        ZStack {
            AppBackground()

            NavigationStack(path: $path) {
                HomeView(
                    syncing: syncing,
                    onWorkspaceTap: { showWorkspaceSwitcher = true },
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
        .sheet(isPresented: $showWorkspaceSwitcher) {
            SidebarView(
                groups: workspaceLoader?.groups ?? [],
                activeAccountId: deps.auth.activeAccountId,
                activeWorkspaceId: workspaceState.activeWorkspaceId,
                onSelectWorkspace: { accountId, workspaceId in
                    handleWorkspacePick(accountId: accountId, workspaceId: workspaceId)
                }
            )
            .presentationBackground(.ultraThinMaterial)
            .presentationDetents([.height(workspaceSheetHeight), .medium])
            .presentationDragIndicator(.visible)
        }
        .onAppear {
            if workspaceLoader == nil {
                workspaceLoader = MultiAccountWorkspaceLoader(auth: deps.auth)
            }
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
            workspaceLoader?.refresh()
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
                onWorkspaceTap: { showWorkspaceSwitcher = true },
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
        observationTask = Task {
            let wsObs = ValueObservation.tracking { db in
                try WorkspaceEntity.fetchAll(db)
            }
            let projObs = ValueObservation.tracking { db in
                try ProjectEntity.fetchAll(db)
            }
            Task {
                for try await ws in wsObs.values(in: deps.db.dbPool) {
                    await MainActor.run {
                        workspaceState.workspaces = ws
                        // Mirror the active account's workspaces into the
                        // cross-server loader so the picker's grouped list
                        // includes the current server without us opening a
                        // second DatabasePool on the same read-write file.
                        workspaceLoader?.setActiveAccountWorkspaces(ws)
                        // Cross-server pick from the picker pre-set this id
                        // before auth.switchAccount; promote it now that the
                        // new DB's workspaces have arrived.
                        if let pending = workspaceState.pendingWorkspaceIdAfterSwitch,
                           ws.contains(where: { $0.id == pending }) {
                            workspaceState.activeWorkspaceId = pending
                            workspaceState.pendingWorkspaceIdAfterSwitch = nil
                        } else if workspaceState.activeWorkspaceId == nil, let first = ws.first {
                            workspaceState.activeWorkspaceId = first.id
                        }
                    }
                }
            }
            Task {
                for try await proj in projObs.values(in: deps.db.dbPool) {
                    await MainActor.run { workspaceState.projects = proj }
                }
            }
        }
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
            try? deps.db.open(accountId: accountId)
            deps.auth.switchAccount(id: accountId)
        }
    }

    private func handleWorkspacePick(accountId: String, workspaceId: String) {
        showWorkspaceSwitcher = false
        if accountId == deps.auth.activeAccountId {
            workspaceState.activeWorkspaceId = workspaceId
        } else {
            // Tell WorkspaceState which workspace to land on once SyncManager
            // has swapped the DB and MainNavigator has rebuilt under the new
            // .id(activeAccountId).
            workspaceState.pendingWorkspaceIdAfterSwitch = workspaceId
            // Swap the DB pool *before* the auth change so the rebuilt
            // MainNavigator binds its ValueObservation to the new account's
            // file. SyncManager polls auth state every 500ms, which otherwise
            // leaves a window where the new UI's observation captures the
            // still-pointing-at-the-old-account pool, reads the previous
            // account's workspaces, and stashes them under the new activeId —
            // making both groups in the picker look identical.
            try? deps.db.open(accountId: accountId)
            deps.auth.switchAccount(id: accountId)
        }
    }

    private func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
    }
}

extension Notification.Name {
    static let oauthTokenReceived = Notification.Name("oauthTokenReceived")
}
