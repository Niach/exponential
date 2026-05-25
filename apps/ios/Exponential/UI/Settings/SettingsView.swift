import SwiftUI

struct SettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(WorkspaceState.self) private var workspaceState
    @State private var workspaceLoader: MultiAccountWorkspaceLoader?
    @State private var pendingWorkspaceId: String?
    @State private var showAddServer = false

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    serversSection
                    workspacesSection
                    generalSection
                    if deps.auth.isAdmin {
                        adminSection
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 96)
            }
        }
        .navigationTitle("Settings")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .navigationDestination(item: $pendingWorkspaceId) { workspaceId in
            WorkspaceSettingsView(workspaceId: workspaceId)
        }
        .onAppear {
            if workspaceLoader == nil {
                workspaceLoader = MultiAccountWorkspaceLoader(auth: deps.auth)
            }
        }
        .onChange(of: deps.auth.accounts) { _, _ in
            workspaceLoader?.refresh()
        }
        .fullScreenCover(isPresented: $showAddServer) {
            InstanceView(showCancel: true) {
                showAddServer = false
            }
        }
    }

    private var serversSection: some View {
        sectionStack(title: "Servers") {
            VStack(spacing: 6) {
                ForEach(deps.auth.accounts) { account in
                    NavigationLink(value: AppRoute.serverDetail(accountId: account.id)) {
                        serverRow(account)
                    }
                    .buttonStyle(.plain)
                }
                Button {
                    showAddServer = true
                } label: {
                    HStack(spacing: 12) {
                        Image(systemName: "plus.circle")
                            .font(.body)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 22)
                        Text("Add server")
                            .font(.body)
                            .foregroundStyle(.white)
                        Spacer()
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)
                    .glassRow()
                }
                .buttonStyle(.plain)
            }
        }
    }

    @ViewBuilder
    private func serverRow(_ account: ServerAccount) -> some View {
        HStack(spacing: 12) {
            Image(systemName: "server.rack")
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(account.displayHost)
                    .font(.body)
                    .foregroundStyle(.white)
                if account.token == nil {
                    Text("Signed out")
                        .font(.caption)
                        .foregroundStyle(.orange.opacity(0.85))
                } else if let email = account.userEmail, !email.isEmpty {
                    Text(email)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            Spacer()
            Image(systemName: "chevron.right")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .glassRow()
    }

    private var workspacesSection: some View {
        sectionStack(title: "Workspaces") {
            let groups = workspaceLoader?.groups ?? []
            if groups.isEmpty {
                Text("No workspaces synced yet.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 4)
            } else {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(groups) { group in
                        workspaceGroupBlock(group)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func workspaceGroupBlock(_ group: ServerWorkspaceGroup) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(group.hostname)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 4)

            VStack(spacing: 6) {
                ForEach(group.workspaces) { workspace in
                    Button {
                        handleWorkspaceTap(accountId: group.accountId, workspaceId: workspace.id)
                    } label: {
                        HStack(spacing: 12) {
                            WorkspaceAvatar(workspace: workspace, size: 22)
                            Text(workspace.name)
                                .font(.body)
                                .foregroundStyle(.white)
                            Spacer()
                            Image(systemName: "chevron.right")
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 12)
                        .glassRow()
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    /// Open WorkspaceSettings for any workspace on any server. The current
    /// WorkspaceSettingsView reads from the active account's pool, so we
    /// silently switch the active account first if needed.
    /// Same-server taps push the route directly via `pendingWorkspaceId`
    /// (used as a NavigationDestination trigger below); cross-server taps
    /// stash the workspaceId in WorkspaceState and let the rebuilt
    /// MainNavigator's onAppear push the route once the new pool is live.
    private func handleWorkspaceTap(accountId: String, workspaceId: String) {
        if accountId == deps.auth.activeAccountId {
            pendingWorkspaceId = workspaceId
        } else {
            workspaceState.pendingWorkspaceSettingsIdAfterSwitch = workspaceId
            try? deps.db.open(accountId: accountId)
            deps.auth.switchAccount(id: accountId)
        }
    }

    private var generalSection: some View {
        sectionStack(title: "General") {
            VStack(spacing: 6) {
                NavigationLink(value: AppRoute.integrations) {
                    settingsRow(icon: "puzzlepiece.extension", title: "Integrations")
                }
                .buttonStyle(.plain)

                if let url = feedbackUrl() {
                    Button {
                        UIApplication.shared.open(url)
                    } label: {
                        settingsRow(icon: "envelope", title: "Send feedback")
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    // The web `/feedback` route redirects to the workspace+project both
    // slugged "feedback". Mobile opens that URL on the configured instance
    // so feedback lands in the same shared workspace the web app uses.
    private func feedbackUrl() -> URL? {
        guard let baseUrl = deps.auth.instanceUrl else { return nil }
        return URL(string: "\(baseUrl)/w/feedback/projects/feedback")
    }

    private var adminSection: some View {
        sectionStack(title: "Admin") {
            VStack(spacing: 6) {
                NavigationLink(value: AppRoute.adminUsers) {
                    settingsRow(icon: "person.2", title: "Users")
                }
                .buttonStyle(.plain)

                NavigationLink(value: AppRoute.adminWorkspaces) {
                    settingsRow(icon: "rectangle.stack", title: "Workspaces")
                }
                .buttonStyle(.plain)
            }
        }
    }

    @ViewBuilder
    private func sectionStack<Content: View>(title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 4)
            content()
        }
    }

    private func settingsRow(icon: String, title: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 22)
            Text(title)
                .font(.body)
                .foregroundStyle(.white)
            Spacer()
            Image(systemName: "chevron.right")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .glassRow()
    }
}
