import ExpUI
import ExpCore
import SwiftUI

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
                setUpOnWebHint
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                switcherControl
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

    // MARK: - Empty state

    // Projects (and workspaces) are created on the web or desktop app — the
    // mobile app is a companion. When there's nothing to show yet, point the
    // user there instead of offering a create button.
    private var setUpOnWebHint: some View {
        VStack(spacing: 12) {
            Image(systemName: "tray")
                .font(.title2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No projects yet")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Create your first project on the web or desktop app.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
            if let host = instanceHost {
                Text(host)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassRow()
            }
        }
        .padding(.horizontal, 40)
    }

    private var instanceHost: String? {
        guard let base = deps.auth.instanceUrl,
              let url = URL(string: base) else { return nil }
        return url.host ?? base
    }
}
