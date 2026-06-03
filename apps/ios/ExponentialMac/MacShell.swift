import ExpCore
import ExpUI
import SwiftUI

struct ProjectRef: Hashable {
    let accountId: String
    let projectId: String
}

struct IssueRef: Hashable {
    let accountId: String
    let issueId: String
}

/// The main three-column shell: project sidebar | issue list | issue detail.
/// Read-only for A2; selection drives the list and detail columns.
struct MacShell: View {
    @Environment(MacAppDependencies.self) private var deps
    @State private var projectLoader: MultiAccountProjectLoader?
    @State private var selectedProject: ProjectRef?
    @State private var issuePath: [IssueRef] = []
    @State private var settingsTarget: WorkspaceSettingsTarget?
    @State private var showAdmin = false
    @State private var showIntegrations = false

    private var activeAccount: ServerAccount? {
        deps.auth.accounts.first { $0.id == deps.auth.activeAccountId }
    }

    var body: some View {
        NavigationSplitView {
            sidebar
                .navigationSplitViewColumnWidth(min: 220, ideal: 280)
        } detail: {
            // Two-pane layout mirroring the web app: a project sidebar + a content
            // area that navigates from the issue list into a full issue detail
            // (push), rather than an always-present third detail column.
            NavigationStack(path: $issuePath) {
                Group {
                    if let selectedProject {
                        MacIssueListView(
                            accountId: selectedProject.accountId,
                            projectId: selectedProject.projectId
                        )
                        .id(selectedProject)
                    } else {
                        ContentUnavailableView("Select a project", systemImage: "folder")
                    }
                }
                .navigationDestination(for: IssueRef.self) { ref in
                    MacIssueDetailView(
                        accountId: ref.accountId,
                        issueId: ref.issueId,
                        onDelete: { issuePath.removeAll { $0 == ref } }
                    )
                }
            }
        }
        .onAppear {
            if projectLoader == nil {
                projectLoader = MultiAccountProjectLoader(auth: deps.auth, db: deps.db)
            }
        }
        .onChange(of: deps.auth.accounts) { _, _ in projectLoader?.refresh() }
        // Switching projects returns to that project's list (pop any open detail).
        .onChange(of: selectedProject) { _, _ in issuePath.removeAll() }
        // A selection from the previous account points at another account's DB
        // pool — clear it so the list/detail never query the wrong account.
        .onChange(of: deps.auth.activeAccountId) { _, _ in
            selectedProject = nil
            issuePath.removeAll()
        }
        .sheet(item: $settingsTarget) { target in
            MacWorkspaceSettingsView(target: target)
                .environment(deps)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showAdmin) {
            if let account = activeAccount {
                MacAdminView(accountId: account.id)
                    .environment(deps)
                    .preferredColorScheme(.dark)
            }
        }
        .sheet(isPresented: $showIntegrations) {
            if let account = activeAccount {
                MacIntegrationsView(accountId: account.id)
                    .environment(deps)
                    .preferredColorScheme(.dark)
            }
        }
    }

    @ViewBuilder
    private var sidebar: some View {
        List(selection: $selectedProject) {
            ForEach(projectLoader?.groups ?? []) { group in
                Section(group.hostname) {
                    ForEach(group.workspaceBlocks) { block in
                        workspaceHeader(block.workspace, accountId: group.accountId)
                        ForEach(block.projects) { project in
                            projectRow(project)
                                .tag(ProjectRef(accountId: group.accountId, projectId: project.id))
                        }
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) { sidebarFooter }
    }

    @ViewBuilder
    private var sidebarFooter: some View {
        if let account = activeAccount {
            Menu {
                if deps.auth.isAdmin {
                    Button { showAdmin = true } label: { Label("Admin", systemImage: "shield") }
                }
                Button { showIntegrations = true } label: {
                    Label("Integrations", systemImage: "puzzlepiece.extension")
                }
                Button { openFeedback() } label: { Label("Send feedback", systemImage: "envelope") }
                Divider()
                Button(role: .destructive) { signOut(account) } label: {
                    Label("Sign out", systemImage: "rectangle.portrait.and.arrow.right")
                }
            } label: {
                HStack(spacing: 8) {
                    Text((account.userEmail ?? account.displayName).prefix(1).uppercased())
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.white)
                        .frame(width: 24, height: 24)
                        .background(Accent.indigo.opacity(0.7))
                        .clipShape(Circle())
                    VStack(alignment: .leading, spacing: 1) {
                        Text(account.displayName).font(.caption.weight(.medium)).lineLimit(1)
                        if let email = account.userEmail, !email.isEmpty {
                            Text(email).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                    Spacer()
                    Image(systemName: "chevron.up.chevron.down").font(.caption2).foregroundStyle(.tertiary)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .contentShape(Rectangle())
            }
            .menuStyle(.borderlessButton)
            .padding(8)
        }
    }

    private func openFeedback() {
        guard let base = deps.auth.instanceUrl,
              let url = URL(string: "\(base)/w/feedback/projects/feedback") else { return }
        Platform.open(url)
    }

    private func signOut(_ account: ServerAccount) {
        let id = account.id
        // Tear sync down first (it still references the token + DB pool), then
        // remove the account so we never yank state out from under the sync task.
        Task {
            await deps.syncManager.signOut(accountId: id)
            deps.db.closePool(forAccountId: id)
            deps.auth.removeAccount(id: id)
        }
    }

    private func workspaceHeader(_ workspace: WorkspaceEntity, accountId: String) -> some View {
        HStack(spacing: 6) {
            WorkspaceAvatar(workspace: workspace, size: 16)
            Text(workspace.name)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(.top, 4)
        .contentShape(Rectangle())
        .contextMenu {
            Button("Workspace Settings…") {
                settingsTarget = WorkspaceSettingsTarget(accountId: accountId, workspaceId: workspace.id)
            }
        }
    }

    private func projectRow(_ project: ProjectEntity) -> some View {
        HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 3)
                .fill(Color(hex: project.color) ?? .gray)
                .frame(width: 10, height: 10)
            Text(project.name)
            Spacer()
            Text(project.prefix)
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
    }
}

