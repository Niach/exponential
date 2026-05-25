import Foundation

@Observable
final class WorkspaceState {
    var workspaces: [WorkspaceEntity] = []
    var projects: [ProjectEntity] = []
    var activeWorkspaceId: String?

    // Set just before a cross-server account switch from the Home tree when
    // the user taps a project on a different server. MainNavigator pushes
    // .project(id:) onto its NavigationPath as soon as the new active
    // account's MainNavigator instance appears, then clears the field.
    var pendingProjectIdAfterSwitch: String?

    // Same idea for Settings → Workspaces → tap a workspace on a different
    // server: pre-set the workspaceId then switchAccount; MainNavigator's
    // onAppear pushes .workspaceSettings after the rebuild.
    var pendingWorkspaceSettingsIdAfterSwitch: String?

    var activeWorkspace: WorkspaceEntity? {
        workspaces.first { $0.id == activeWorkspaceId } ?? workspaces.first
    }

    var filteredProjects: [ProjectEntity] {
        guard let wsId = activeWorkspace?.id else { return [] }
        return projects
            .filter { $0.workspaceId == wsId && $0.archivedAt == nil }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }
}
