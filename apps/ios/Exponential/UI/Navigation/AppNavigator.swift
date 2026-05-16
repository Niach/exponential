import SwiftUI
import GRDB

enum AppRoute: Hashable {
    case home
    case project(id: String)
    case issue(id: String)
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
                InstanceView()
            } else if !deps.auth.isAuthenticated {
                LoginView()
            } else {
                MainNavigator()
            }
        }
        .transaction { $0.animation = nil } // Prevent auth transitions from affecting child navigation
    }
}

struct MainNavigator: View {
    @Environment(AppDependencies.self) private var deps
    @State private var selectedTab: BottomTab = .projects
    @State private var projectsPath = NavigationPath()
    @State private var settingsPath = NavigationPath()
    @State private var workspaceState = WorkspaceState()
    @State private var showWorkspaceSwitcher = false
    @State private var observationTask: Task<Void, Never>?
    @State private var syncing = false

    private var workspaceSheetHeight: CGFloat {
        let header: CGFloat = 56
        let rowHeight: CGFloat = 44
        let bottomPadding: CGFloat = 24
        let count = max(workspaceState.workspaces.count, 1)
        return min(header + CGFloat(count) * rowHeight + bottomPadding, 320)
    }

    var body: some View {
        ZStack {
            AppBackground()

            switch selectedTab {
            case .projects:
                NavigationStack(path: $projectsPath) {
                    HomeView(syncing: syncing)
                        .navigationDestination(for: AppRoute.self) { destination(for: $0) }
                }
            case .settings:
                NavigationStack(path: $settingsPath) {
                    SettingsView()
                        .navigationDestination(for: AppRoute.self) { destination(for: $0) }
                }
            }
        }
        .environment(workspaceState)
        .safeAreaInset(edge: .bottom) {
            BottomBar(
                selectedTab: $selectedTab,
                workspace: workspaceState.activeWorkspace,
                onWorkspaceTap: { showWorkspaceSwitcher = true }
            )
        }
        .sheet(isPresented: $showWorkspaceSwitcher) {
            SidebarView(
                workspaces: workspaceState.workspaces,
                activeWorkspaceId: workspaceState.activeWorkspaceId,
                onSelectWorkspace: { id in
                    workspaceState.activeWorkspaceId = id
                    showWorkspaceSwitcher = false
                }
            )
            .presentationBackground(.ultraThinMaterial)
            .presentationDetents([.height(workspaceSheetHeight), .medium])
            .presentationDragIndicator(.visible)
        }
        .onAppear {
            startObserving()
            if workspaceState.workspaces.isEmpty {
                syncing = true
                Task {
                    try? deps.db.clearAllData()
                    await deps.syncManager.initialSync()
                    syncing = false
                }
            }
        }
        .onDisappear { stopObserving() }
        .onOpenURL { url in
            handleDeepLink(url)
        }
        .onChange(of: deps.deepLinkBus.pendingIssueId) { _, issueId in
            if let issueId {
                selectedTab = .projects
                projectsPath.append(AppRoute.issue(id: issueId))
                _ = deps.deepLinkBus.consume()
            }
        }
    }

    @ViewBuilder
    private func destination(for route: AppRoute) -> some View {
        switch route {
        case .home:
            HomeView(syncing: syncing)
        case let .project(id):
            IssueListView(projectId: id)
        case let .issue(id):
            IssueDetailView(issueId: id)
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
            selectedTab = .projects
            projectsPath.append(AppRoute.issue(id: String(issueId)))
        }
        // Handle exp://invite/<token>
        if url.host == "invite", let token = url.pathComponents.dropFirst().first {
            selectedTab = .projects
            projectsPath.append(AppRoute.invite(token: String(token)))
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
                        if workspaceState.activeWorkspaceId == nil, let first = ws.first {
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

    private func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
    }
}

extension Notification.Name {
    static let oauthTokenReceived = Notification.Name("oauthTokenReceived")
}
