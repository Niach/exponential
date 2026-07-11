import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// Root of the Issues tab: the issue list of the current project, with an
/// inline project switcher in the navigation bar (project name + up/down
/// chevron → `ProjectSwitcherSheet`). Replaces the old Projects overview as
/// the app's home — switching projects swaps the list in place, no push.
struct IssuesHomeView: View {
    let syncing: Bool
    let currentProject: CurrentProjectRef?
    let projectLoader: MultiAccountProjectLoader?
    let onSelectProject: (_ accountId: String, _ projectId: String) -> Void

    @Environment(AppDependencies.self) private var deps
    @State private var showSwitcher = false
    @State private var preparingCreate = false
    @State private var createTarget: CreateTarget?

    private struct CreateTarget: Identifiable {
        let accountId: String
        let workspaceId: String
        var id: String { "\(accountId)/\(workspaceId)" }
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let current = currentProject {
                IssueListView(projectId: current.projectId)
                    .environment(\.accountId, current.accountId)
                    // Remount on switch so the list view model rebinds to the
                    // selected project (it captures projectId at creation).
                    .id(current)
            } else if syncing {
                VStack(spacing: 12) {
                    ProgressView()
                        .tint(.white)
                    Text("Syncing...")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            } else {
                emptyStateHint
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                switcherControl
            }
            // Releases (EXP-56): the tab bar is full, so the workspace's
            // releases live behind a nav-bar action on the Issues screen.
            if let workspaceRef = currentWorkspaceRef {
                ToolbarItem(placement: .topBarTrailing) {
                    releasesButton(workspaceRef)
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                settingsButton
            }
        }
        .sheet(isPresented: $showSwitcher) {
            ProjectSwitcherSheet(
                projectLoader: projectLoader,
                currentProject: currentProject,
                onSelect: { accountId, projectId in
                    showSwitcher = false
                    onSelectProject(accountId, projectId)
                }
            )
            .presentationDetents([.medium, .large])
            .presentationDragIndicator(.visible)
            .presentationBackground(.ultraThinMaterial)
        }
        .sheet(item: $createTarget) { target in
            CreateProjectSheet(
                accountId: target.accountId,
                workspaceId: target.workspaceId,
                onCreated: { projectId in onSelectProject(target.accountId, projectId) }
            )
            .presentationBackground(.ultraThinMaterial)
        }
    }

    // MARK: - Switcher control

    private var hasAnyProjects: Bool {
        !(projectLoader?.groups ?? []).isEmpty
    }

    private var currentProjectName: String? {
        guard let current = currentProject else { return nil }
        for group in projectLoader?.groups ?? [] where group.accountId == current.accountId {
            for block in group.workspaceBlocks {
                if let project = block.projects.first(where: { $0.id == current.projectId }) {
                    return project.name
                }
            }
        }
        return nil
    }

    /// One tappable control: current project name + the combobox-style
    /// up/down chevron. Disabled until there is anything to switch to.
    private var switcherControl: some View {
        Button {
            showSwitcher = true
        } label: {
            HStack(spacing: 5) {
                Text(currentProjectName ?? "Projects")
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!hasAnyProjects)
        .opacity(hasAnyProjects ? 1 : 0.5)
        .accessibilityLabel("Switch project")
    }

    private var settingsButton: some View {
        NavigationLink(value: AppRoute.settings) {
            Image(systemName: "gearshape")
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 32, height: 32)
                .contentShape(Circle())
        }
    }

    /// (accountId, workspaceId) of the CURRENT project — the workspace whose
    /// releases the toolbar action opens. Resolved from the loader tree, which
    /// spans every signed-in server.
    private var currentWorkspaceRef: (accountId: String, workspaceId: String)? {
        guard let current = currentProject else { return nil }
        for group in projectLoader?.groups ?? [] where group.accountId == current.accountId {
            for block in group.workspaceBlocks
            where block.projects.contains(where: { $0.id == current.projectId }) {
                return (current.accountId, block.workspace.id)
            }
        }
        return nil
    }

    private func releasesButton(_ ref: (accountId: String, workspaceId: String)) -> some View {
        NavigationLink(value: AppRoute.releases(accountId: ref.accountId, workspaceId: ref.workspaceId)) {
            Image(systemName: "shippingbox")
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 32, height: 32)
                .contentShape(Circle())
        }
        .accessibilityLabel("Releases")
    }

    // MARK: - Empty state

    // Nothing synced yet — offer to create the first project inline (a project
    // is backed by a GitHub repo, connected in the create sheet).
    private var emptyStateHint: some View {
        VStack(spacing: 12) {
            Image(systemName: "tray")
                .font(.title2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No projects yet")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Create your first project to get started.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)

            Button {
                Task { await beginCreateProject() }
            } label: {
                HStack(spacing: 6) {
                    if preparingCreate {
                        ProgressView().controlSize(.small).tint(.white)
                    } else {
                        Image(systemName: "plus")
                            .font(.caption.weight(.semibold))
                    }
                    Text("Create project")
                        .font(.subheadline.weight(.medium))
                }
                .foregroundStyle(.white)
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .glassButton()
            }
            .buttonStyle(.plain)
            .disabled(preparingCreate)
        }
        .padding(.horizontal, 40)
    }

    /// Resolve (creating if needed) the default workspace, then open the create
    /// sheet targeting it.
    private func beginCreateProject() async {
        guard !preparingCreate, let accountId = deps.auth.activeAccountId else { return }
        preparingCreate = true
        defer { preparingCreate = false }
        if let workspace = try? await deps.workspacesApi.ensureDefault(accountId: accountId) {
            // If the workspace isn't in the local synced set, ensureDefault
            // just CREATED it — the membership change rotates every shape's
            // server-derived where clause, and the in-flight live long-polls
            // would keep the OLD scope for up to ~60s, so the project created
            // next would "show up nowhere". Relaunch the pipeline so the fresh
            // scope syncs in seconds (EXP-46; same drain-lag gap as EXP-43).
            var alreadySynced = false
            if let pool = try? deps.db.pool(forAccountId: accountId) {
                alreadySynced = (try? await pool.read { db in
                    try WorkspaceEntity.fetchOne(db, key: workspace.id) != nil
                }) ?? false
            }
            if !alreadySynced {
                await deps.syncManager.restartPipeline(accountId: accountId)
            }
            createTarget = CreateTarget(accountId: accountId, workspaceId: workspace.id)
        }
    }
}
