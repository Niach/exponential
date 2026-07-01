import ExpCore
import ExpUI
import GRDB
import SwiftUI

struct ProjectRef: Hashable {
    let accountId: String
    let projectId: String
}

struct IssueRef: Hashable {
    let accountId: String
    let issueId: String
}

struct WorkspaceRef: Hashable {
    let accountId: String
    let workspaceId: String
}

/// The main three-column shell: project sidebar | issue list | issue detail.
/// Read-only for A2; selection drives the list and detail columns.
struct MacShell: View {
    @Environment(MacAppDependencies.self) private var deps
    @State private var projectLoader: MultiAccountProjectLoader?
    @State private var selectedProject: ProjectRef?
    @State private var issuePath: [IssueRef] = []
    @State private var settingsTarget: WorkspaceSettingsTarget?
    @State private var createProjectTarget: WorkspaceSettingsTarget?
    @State private var showIntegrations = false
    @State private var showCreateWorkspace = false
    @State private var showInbox = false
    @State private var ensuredDefault = false
    // The dedicated Preview pane (build/run/embed the selected run target). A
    // resizable trailing pane, separate from the bottom terminal dock (which
    // stays for build/run logs).
    @State private var showPreview = false
    @State private var previewWidth: CGFloat = 420
    // The workspace the sidebar is currently scoped to (web shows ONE workspace's
    // projects at a time, switched via the dropdown — not every workspace flat).
    @State private var selectedWorkspace: WorkspaceRef?

    private var activeAccount: ServerAccount? {
        deps.auth.accounts.first { $0.id == deps.auth.activeAccountId }
    }

    // The (account, workspace-block) the sidebar shows: the explicit switcher
    // choice, else the workspace of the selected project, else the first synced
    // workspace. The block carries that workspace's projects.
    private var activeWorkspaceBlock: (accountId: String, block: WorkspaceBlock)? {
        let groups = projectLoader?.groups ?? []
        if let sel = selectedWorkspace {
            for group in groups where group.accountId == sel.accountId {
                for block in group.workspaceBlocks where block.workspace.id == sel.workspaceId {
                    return (group.accountId, block)
                }
            }
        }
        if let sel = selectedProject {
            for group in groups where group.accountId == sel.accountId {
                for block in group.workspaceBlocks where block.projects.contains(where: { $0.id == sel.projectId }) {
                    return (group.accountId, block)
                }
            }
        }
        if let group = groups.first, let block = group.workspaceBlocks.first {
            return (group.accountId, block)
        }
        return nil
    }

    private var activeWorkspace: (accountId: String, workspace: WorkspaceEntity)? {
        activeWorkspaceBlock.map { ($0.accountId, $0.block.workspace) }
    }

    // The full ProjectEntity for the current selection (needed to bind the
    // preview controller — repo + previewConfig mirror live on the row).
    private var selectedProjectEntity: ProjectEntity? {
        guard let sel = selectedProject else { return nil }
        for group in (projectLoader?.groups ?? []) where group.accountId == sel.accountId {
            for block in group.workspaceBlocks {
                if let project = block.projects.first(where: { $0.id == sel.projectId }) {
                    return project
                }
            }
        }
        return nil
    }

    // Equatable digest of the preview-relevant project fields, so `.onChange`
    // can rebind when the repo link / previewConfig mirror syncs in.
    private var selectedProjectPreviewKey: String? {
        guard let project = selectedProjectEntity else { return nil }
        return "\(project.id)|\(project.githubRepo ?? "")|\(project.previewConfig ?? "")"
    }

    /// (Re)bind the preview controller to the current selection. Idempotent; a
    /// no-op selection still resolves to nothing.
    private func bindPreview() {
        guard let project = selectedProjectEntity, let sel = selectedProject else {
            deps.previewController.shutdown()
            return
        }
        deps.previewController.bind(accountId: sel.accountId, project: project)
    }

    var body: some View {
        VStack(spacing: 0) {
            splitView
            terminalDockView
        }
        .overlay(alignment: .bottom) {
            MacToastOverlay(center: deps.toastCenter)
                .padding(.bottom, deps.terminalDock.isMounted && deps.terminalDock.isExpanded ? deps.terminalDock.dockHeight + 30 : 0)
        }
    }

    private var splitView: some View {
        NavigationSplitView {
            sidebar
                .navigationSplitViewColumnWidth(min: 220, ideal: 280)
        } detail: {
            // Two-pane layout mirroring the web app: a project sidebar + a content
            // area that navigates from the issue list into a full issue detail
            // (push), rather than an always-present third detail column. A dedicated
            // Preview pane can be opened to the trailing edge (resizable).
            HStack(spacing: 0) {
                NavigationStack(path: $issuePath) {
                    Group {
                        if showInbox {
                            MacInboxView(
                                accountId: activeAccount?.id ?? "",
                                onOpenIssue: { issueId in
                                    if let aid = activeAccount?.id {
                                        issuePath.append(IssueRef(accountId: aid, issueId: issueId))
                                    }
                                }
                            )
                        } else if let selectedProject {
                            MacIssueListView(
                                accountId: selectedProject.accountId,
                                projectId: selectedProject.projectId
                            )
                            .id(selectedProject)
                        } else {
                            emptyState
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
                .frame(maxWidth: .infinity)
                if showPreview, selectedProject != nil {
                    previewPane
                }
            }
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        showPreview.toggle()
                    } label: {
                        Label("Preview", systemImage: showPreview ? "play.rectangle.fill" : "play.rectangle")
                    }
                    .help(showPreview ? "Hide preview" : "Show preview")
                    .disabled(selectedProject == nil)
                }
            }
        }
        .onAppear {
            if projectLoader == nil {
                projectLoader = MultiAccountProjectLoader(auth: deps.auth, db: deps.db)
            }
            // Make sure a fresh account lands in a usable workspace (idempotent
            // server-side) so the sidebar is never empty after first sign-in.
            if !ensuredDefault, let id = activeAccount?.id {
                ensuredDefault = true
                Task {
                    try? await deps.workspacesApi.ensureDefault(accountId: id)
                    // Re-establish the observation so a just-created default workspace
                    // surfaces as soon as Electric syncs it, not only on the next tick.
                    projectLoader?.refresh()
                }
            }
        }
        .onChange(of: deps.auth.accounts) { _, _ in projectLoader?.refresh() }
        // Switching projects returns to that project's list (pop any open detail).
        // The preview is per-project — rebind (tears the old one down).
        .onChange(of: selectedProject) { _, new in
            issuePath.removeAll()
            if new != nil { showInbox = false }
            bindPreview()
        }
        // The project row may sync in (repo / previewConfig) after selection;
        // rebinding when those fields resolve keeps the picker current. Tracked
        // via an equatable key since ProjectEntity isn't Equatable.
        .onChange(of: selectedProjectPreviewKey) { _, _ in bindPreview() }
        // Bind on first open of the pane (the project was already selected).
        .onChange(of: showPreview) { _, shown in if shown { bindPreview() } }
        // A selection from the previous account points at another account's DB
        // pool — clear it so the list/detail never query the wrong account.
        .onChange(of: deps.auth.activeAccountId) { _, _ in
            selectedProject = nil
            selectedWorkspace = nil
            showInbox = false
            issuePath.removeAll()
            deps.previewController.shutdown()
        }
        .sheet(item: $settingsTarget) { target in
            MacWorkspaceSettingsView(target: target)
                .environment(deps)
                .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showIntegrations) {
            if let account = activeAccount {
                MacIntegrationsView(accountId: account.id)
                    .environment(deps)
                    .preferredColorScheme(.dark)
            }
        }
        .sheet(item: $createProjectTarget) { target in
            MacCreateProjectView(accountId: target.accountId, workspaceId: target.workspaceId) { newId in
                selectedProject = ProjectRef(accountId: target.accountId, projectId: newId)
            }
            .environment(deps)
            .preferredColorScheme(.dark)
        }
        .sheet(isPresented: $showCreateWorkspace) {
            if let account = activeAccount {
                MacCreateWorkspaceView(accountId: account.id)
                    .environment(deps)
                    .preferredColorScheme(.dark)
            }
        }
    }

    // MARK: - Preview pane

    // A resizable trailing pane (separate from the bottom terminal dock). The
    // draggable divider on its leading edge adjusts the width; the controller is
    // bound to the selected project by bindPreview().
    @ViewBuilder
    private var previewPane: some View {
        HStack(spacing: 0) {
            Divider()
                .frame(width: 6)
                .contentShape(Rectangle())
                .onHover { inside in
                    if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
                }
                .gesture(
                    DragGesture()
                        .onChanged { value in
                            previewWidth = min(900, max(300, previewWidth - value.translation.width))
                        }
                )
            MacPreviewPane(controller: deps.previewController)
                .frame(width: previewWidth)
        }
    }

    // MARK: - Terminal dock

    @ViewBuilder
    private var terminalDockView: some View {
        let dock = deps.terminalDock
        if dock.isMounted {
            VStack(spacing: 0) {
                Divider()
                HStack(spacing: 8) {
                    Button { dock.toggleExpanded() } label: {
                        Image(systemName: dock.isExpanded ? "chevron.down" : "chevron.up")
                    }
                    .buttonStyle(.borderless)
                    .help(dock.isExpanded ? "Collapse" : "Expand")
                    Image(systemName: "terminal").font(.caption).foregroundStyle(.secondary)
                    Text(dock.title.isEmpty ? "Terminal" : dock.title)
                        .font(.caption.weight(.medium)).lineLimit(1)
                    Spacer()
                    Button { dock.close() } label: { Image(systemName: "xmark") }
                        .buttonStyle(.borderless)
                        .help("Close the terminal")
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                .background(.bar)
                .contentShape(Rectangle())
                // Drag the header to resize the dock.
                .gesture(
                    DragGesture()
                        .onChanged { v in
                            dock.dockHeight = min(700, max(160, dock.dockHeight - v.translation.height))
                        }
                )
                if dock.isExpanded {
                    TerminalDockHost(dock: dock)
                        .frame(height: dock.dockHeight)
                }
            }
        }
    }

    // MARK: - Empty state

    @ViewBuilder
    private var emptyState: some View {
        if (projectLoader?.groups ?? []).isEmpty {
            ContentUnavailableView {
                Label("No projects yet", systemImage: "folder.badge.plus")
            } description: {
                Text("Create a workspace and a project to get started.")
            } actions: {
                Button("New Workspace") { showCreateWorkspace = true }
                if let aw = activeWorkspace {
                    Button("New Project") {
                        createProjectTarget = WorkspaceSettingsTarget(accountId: aw.accountId, workspaceId: aw.workspace.id)
                    }
                }
            }
        } else {
            ContentUnavailableView("Select a project", systemImage: "folder")
        }
    }

    @ViewBuilder
    private var sidebar: some View {
        List(selection: $selectedProject) {
            Section {
                Button {
                    showInbox = true
                    selectedProject = nil
                    issuePath.removeAll()
                } label: {
                    Label("Inbox", systemImage: "tray")
                        .foregroundStyle(showInbox ? Color.accentColor : Color.primary)
                }
                .buttonStyle(.plain)
            }
            if let (accountId, block) = activeWorkspaceBlock {
                Section("Projects") {
                    if block.projects.isEmpty {
                        Text("No projects yet")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(block.projects) { project in
                            projectRow(project)
                                .tag(ProjectRef(accountId: accountId, projectId: project.id))
                        }
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .top) { sidebarHeader }
        .safeAreaInset(edge: .bottom) { sidebarFooter }
    }

    // Workspace name is rendered as a PLAIN view (not inside a Menu label) — a
    // borderlessButton menu collapses a rich label to its smallest content, which
    // truncated the name to a single letter. The menu hangs off the chevron only.
    @ViewBuilder
    private var sidebarHeader: some View {
        HStack(spacing: 6) {
            if let aw = activeWorkspace {
                WorkspaceAvatar(workspace: aw.workspace, size: 18)
            }
            Text(activeWorkspace?.workspace.name ?? "Workspaces")
                .font(.subheadline.weight(.semibold))
                .lineLimit(1)
                .truncationMode(.tail)
            Menu {
                workspaceMenuItems
            } label: {
                Image(systemName: "chevron.up.chevron.down").font(.caption2).foregroundStyle(.secondary)
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            Spacer(minLength: 4)
            Button {
                if let aw = activeWorkspace {
                    createProjectTarget = WorkspaceSettingsTarget(accountId: aw.accountId, workspaceId: aw.workspace.id)
                }
            } label: {
                Image(systemName: "plus")
            }
            .buttonStyle(.borderless)
            .disabled(activeWorkspace == nil)
            .help("New project")
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }

    @ViewBuilder
    private var workspaceMenuItems: some View {
        ForEach(projectLoader?.groups ?? []) { group in
            ForEach(group.workspaceBlocks) { block in
                Button {
                    // Scope the sidebar to this workspace; clear the open project so
                    // the content resets to that workspace's empty state.
                    selectedWorkspace = WorkspaceRef(accountId: group.accountId, workspaceId: block.workspace.id)
                    selectedProject = nil
                } label: {
                    if activeWorkspace?.workspace.id == block.workspace.id {
                        Label(block.workspace.name, systemImage: "checkmark")
                    } else {
                        Text(block.workspace.name)
                    }
                }
            }
        }
        Divider()
        Button { showCreateWorkspace = true } label: { Label("New workspace", systemImage: "plus") }
        if let aw = activeWorkspace {
            Button {
                settingsTarget = WorkspaceSettingsTarget(accountId: aw.accountId, workspaceId: aw.workspace.id)
            } label: {
                Label("Workspace Settings…", systemImage: "gearshape")
            }
        }
    }

    // Identity name + email are PLAIN views (same reason as the header); the
    // account menu hangs off the trailing ellipsis only.
    @ViewBuilder
    private var sidebarFooter: some View {
        if let account = activeAccount {
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
                Spacer(minLength: 4)
                Menu {
                    Button { showIntegrations = true } label: {
                        Label("Integrations", systemImage: "puzzlepiece.extension")
                    }
                    Button { openFeedback() } label: { Label("Send feedback", systemImage: "envelope") }
                    Divider()
                    Button(role: .destructive) { signOut(account) } label: {
                        Label("Sign out", systemImage: "rectangle.portrait.and.arrow.right")
                    }
                } label: {
                    Image(systemName: "ellipsis").font(.body).foregroundStyle(.secondary)
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
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

