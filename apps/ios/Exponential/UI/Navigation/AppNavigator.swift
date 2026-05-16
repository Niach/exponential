import SwiftUI

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
    @State private var path = NavigationPath()

    var body: some View {
        NavigationStack(path: $path) {
            HomeView()
                .navigationDestination(for: AppRoute.self) { route in
                    switch route {
                    case .home:
                        HomeView()
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
        }
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
}

extension Notification.Name {
    static let oauthTokenReceived = Notification.Name("oauthTokenReceived")
}
