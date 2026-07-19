import ExpUI
import ExpCore
import SwiftUI

private struct WorkspaceNavTarget: Hashable {
    let accountId: String
    let workspaceId: String
}

struct SettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @State private var workspaceLoader: MultiAccountWorkspaceLoader?
    @State private var pendingWorkspace: WorkspaceNavTarget?
    @State private var showAddServer = false
    // The account that was active before Add-server opened, so a cancelled /
    // incomplete add can be rolled back off the tokenless pending account.
    @State private var previousActiveAccountId: String?

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    serversSection
                    workspacesSection
                    generalSection
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 96)
            }
        }
        .navigationTitle("Settings")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .navigationDestination(item: $pendingWorkspace) { target in
            WorkspaceSettingsView(workspaceId: target.workspaceId)
                .environment(\.accountId, target.accountId)
        }
        .onAppear {
            if workspaceLoader == nil {
                workspaceLoader = MultiAccountWorkspaceLoader(auth: deps.auth, db: deps.db)
            }
        }
        .onChange(of: deps.auth.accounts) { _, _ in
            workspaceLoader?.refresh()
        }
        .fullScreenCover(isPresented: $showAddServer) {
            InstanceView(showCancel: true) {
                // Add-server activates a tokenless pending account the moment a
                // URL is entered (setInstanceUrl → upsertAndActivate). If the
                // user backs out before login resolves a token, remove that
                // half-added pending and roll back to the account that was
                // active before — so the app is never left pointed at a
                // tokenless server, and a cancelled add leaves no phantom entry
                // (which would also wrongly mark the cloud as already-added).
                // (The core wrong-account symptom is separately covered by
                // MainNavigator's activeAccountId rebind; fully decoupling
                // activation from setInstanceUrl would require threading the URL
                // through resolveActiveAccount — deferred.)
                if deps.auth.token == nil,
                   let prev = previousActiveAccountId,
                   let stranded = deps.auth.activeAccountId,
                   stranded != prev {
                    deps.auth.removeAccount(id: stranded)
                    deps.auth.switchAccount(id: prev)
                }
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
                    previousActiveAccountId = deps.auth.activeAccountId
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
                Text(account.displayName)
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
        sectionStack(title: "Teams") {
            let groups = workspaceLoader?.groups ?? []
            if groups.isEmpty {
                Text("No teams synced yet.")
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

    private func handleWorkspaceTap(accountId: String, workspaceId: String) {
        pendingWorkspace = WorkspaceNavTarget(accountId: accountId, workspaceId: workspaceId)
    }

    private var generalSection: some View {
        sectionStack(title: "General") {
            VStack(spacing: 6) {
                NavigationLink(value: AppRoute.syncDebug) {
                    settingsRow(icon: "arrow.triangle.2.circlepath", title: "Sync diagnostics")
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
